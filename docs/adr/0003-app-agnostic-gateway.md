# ADR 0003：与应用无关的终端网关——按 SSH 用户名路由

- 状态：已采纳
- 日期：2026-11-26
- 范围：`crates/tty-gateway`、`crates/tty-gateway-testkit`、`crates/tty-host`、`crates/coffee-tui`、`deploy/`、`scripts/`

[ADR 0001](0001-ssh-session-model.md) 定下“每个 SSH 会话一个 PTY 子进程”，[ADR 0002](0002-phase-2-architecture.md)
在其上做出了演示店。本篇把这两层终端基建改成**与应用无关**：网关像 NGINX 反向代理一样，在同一个端口上按
SSH 用户名启动不同的终端程序；宿主成为能跑任何 nsh 应用的库；咖啡馆退化为“一份配置 + 一个很薄的入口二进制”。
ADR 0001 的会话模型、认证方式与度量结论不变，变的是名字、配置方式和几处健壮性。

## 背景

原来的 `crates/coffee-sshd` 与 `crates/coffee-host` 写死为咖啡馆服务：

- 网关只会启动 `COFFEE_TUI_BIN` 一个程序；SSH 用户名被丢弃；横幅、版本串、中文提示与 `COFFEE_*` 变量都写在
  代码里；调优靠十来个 `COFFEE_SSH_*` 环境变量。
- 宿主直接依赖 `coffee_app`：终端模式、色深、Ctrl+C、剪贴板这些通用部分和咖啡馆的钉色、会话环境混在一起。
- 勘察还发现几处健壮性缺口：russh 的 handler 里 `await` 有界队列（子进程不读输入时整条连接卡死）、fork/exec
  与重试的睡眠占着 handler、不是所有结束路径都回收进程组、客户端的 EOF 没人处理、没有会话最长时长、
  停机时直接丢下会话、宿主引擎线程的 panic 可能被当成正常退出。

想要的是：同一个端口后面可以挂第二个应用（第一个例子就是组件画廊），换应用不改网关代码，网关的行为由一个
可检查、可重载的文件决定。

## 决策

| Crate                                    | 产物                      | 内容                                                                                                 |
| ---------------------------------------- | ------------------------- | ---------------------------------------------------------------------------------------------------- |
| `crates/tty-gateway`（原 `coffee-sshd`） | lib + bin `tty-gateway`   | SSH 网关，不含任何咖啡馆知识                                                                         |
| `crates/tty-gateway-testkit`（新）       | lib（仅 dev 依赖）        | russh 测试客户端 + `vt100` 终端 + 进程内 `TestServer` + 探针程序                                     |
| `crates/tty-host`（新）                  | lib                       | 通用 nsh 0.6.1 终端宿主：`terminal.rs`、`color.rs`、`clipboard.rs`、`interrupt.rs` 与运行循环        |
| `crates/coffee-tui`（原 `coffee-host`）  | bin `coffee-tui` + 小 lib | 咖啡馆入口：`session_env_from_process`、画廊开关、`Root`、`coffee_app::configure`、钉色 `palette.rs` |

咖啡馆的网关配置是 `deploy/gateway.toml`，`pnpm dev`、Docker 镜像与 `scripts/e2e` 共用这一份。

不变：二进制名 `coffee-tui`（`target/debug/coffee-tui` 的所有用法照旧）；应用的输入契约 `COFFEE_API_URL`、
`COFFEE_INTERNAL_KEY`、`COFFEE_SSH_FINGERPRINT`、`COFFEE_CLIENT_IP`、`COFFEE_COLORS`、`COFFEE_UI_GALLERY`；
根脚本名；认证方式；每会话一个进程；`apps/tui` 与它的冻结接口。

去掉：`COFFEE_SSH_ADDR`、`COFFEE_SSH_HOST_KEY`、`COFFEE_TUI_BIN`、`COFFEE_SSH_MAX_SESSIONS`、
`COFFEE_SSH_MAX_SESSIONS_PER_IP`、`COFFEE_SSH_CONNECTIONS_PER_MINUTE`、`COFFEE_SSH_IDLE_TIMEOUT`、
`COFFEE_SSH_AUTH_TIMEOUT`、`COFFEE_SSH_LOG`。它们成了配置键或 `TTY_GATEWAY_*`（见第 2 节）。compose 发布端口用的
`COFFEE_SSH_PORT` 保留。

## 1. 路由模型

**决定**：一个端口；SSH **用户名**选路由。用户名在某条路由的 `users` 里（精确匹配；省略 `users` 时就是路由名；
同一个名字出现在两条路由里是配置错误）就走那条，否则走 `server.default_route`。咖啡馆的文件有两条：

| 路由      | 用户名                     | 程序                                | 说明                               |
| --------- | -------------------------- | ----------------------------------- | ---------------------------------- |
| `coffee`  | `coffee`、`shop`；默认路由 | `coffee-tui`                        | 商店；带内部密钥、指纹、客户端地址 |
| `gallery` | `gallery`                  | `coffee-tui`，`COFFEE_UI_GALLERY=1` | 组件画廊；不带内部密钥             |

所以 `ssh -p 2222 localhost`（本机用户名）、`ssh -p 2222 anything@localhost` 都还是商店，
`ssh -p 2222 gallery@localhost` 是画廊。

认证方式对所有路由相同，与 ADR 0001 一致：任意公钥都接受（指纹成为 `${fingerprint}`），没有公钥走零提示的
keyboard-interactive，`none` 仍然拒绝（否则带密钥的客户端不出示公钥，“记住我”拿不到指纹）。

**路由绑定在被接受的那次认证请求上**：SSH 允许客户端在每次认证尝试里换用户名，russh 每次回调都带着当次的
用户名。只有最终被接受的那次才算数，它的用户名决定路由，之后的 `shell` 请求就用这条路由。早于认证（比如按
第一次 `none` 探测）绑定会被换名绕过；晚于认证（等 `shell` 时再查）则要另存一份用户名，结果相同却多一个
出错的地方。日志在 `authenticated` 和 `session started` 两行里都带 `user` 与 `route`。

用户名只在“纯净”时才作为 `${user}` 交给程序：1–64 个 `A-Z a-z 0-9 . _ @ + -`。其他名字照样路由（落到默认
路由），但 `${user}` 视为无值，带它的环境变量整条省略——用户名是客户端随便写的，不能原样进环境。

**横幅只能是全局的**：russh 在发认证横幅（`authentication_banner`）时还不知道用户名，于是 `server.banner`
对所有路由相同。限额提示也一样：认证前只能检查全局与每地址的上限，路由级上限只在 `shell` 时才知道是哪条。
按路由的问候语以后可以在通道上发。

## 2. 配置模型与变量模板

**决定**：`tty-gateway --config <file> [--check]`；一个 TOML 文件，四张表 `[server]`、`[limits]`、`[messages]`、
`[routes.NAME]`，每张表都 `deny_unknown_fields`（拼错的键是错误，不是静默的默认值）。时长写成 `"500ms"`、
`"30s"`、`"15m"`、`"2h"` 或 `"0"`（`0` 在超时类键上表示“从不”）。网关自己只读四个环境变量：

| 变量                   | 作用                                                |
| ---------------------- | --------------------------------------------------- |
| `TTY_GATEWAY_CONFIG`   | 没有 `--config` 时的配置文件                        |
| `TTY_GATEWAY_ADDR`     | 覆盖 `server.listen`                                |
| `TTY_GATEWAY_HOST_KEY` | 覆盖 `server.host_key`                              |
| `TTY_GATEWAY_LOG`      | 日志过滤（否则 `RUST_LOG`，默认 `info,russh=warn`） |

两个覆盖项是为了让同一份文件在本机、容器和测试里都能用（地址与密钥路径因环境而异，其余不变）。全部键与默认值
见 [`deploy/README.md`](../../deploy/README.md)；代码里的默认是中性的：版本串 `SSH-2.0-tty_gateway`、无横幅、英文
提示。咖啡馆的品牌（`SSH-2.0-ROAST_TTY`、演示横幅、中文提示）都在 `deploy/gateway.toml` 里。

提示文案（`no_pty`、`unavailable`、`crashed`、`idle`、`time_limit`、`full`、`full_per_ip`、`full_route`、
`shutting_down`）可全局设、可按路由覆盖：路由自己的 → `[messages]` → 默认。

**会话环境**从空开始：网关自己的 `PATH`、`HOME`、`TMPDIR`、`USER`、`LOGNAME`，客户端的 `TERM`（非法值换成
`xterm-256color`），`LANG`（客户端的 UTF-8 locale，否则 `C.UTF-8`），客户端发来且 `accept_env` 允许的变量
（默认 `LANG`、`LC_*`、`COLORTERM`），最后是路由的 `env` 表——类似 NGINX 的 `proxy_set_header`。它的值是模板：

| 变量                                | 值                                                 |
| ----------------------------------- | -------------------------------------------------- |
| `${user}`                           | 纯净的 SSH 用户名（见第 1 节）                     |
| `${route}`                          | 路由名                                             |
| `${fingerprint}`                    | `SHA256:…`，只在验证过公钥之后有值                 |
| `${client_ip}`                      | 客户端地址                                         |
| `${env:NAME}` / `${env:NAME:-默认}` | 网关自己的环境变量，**读文件时**解析；空值视为无值 |

**任何一个变量无值，这一条就整条省略**，而不是给一个空字符串：没有公钥的会话里根本没有
`COFFEE_SSH_FINGERPRINT`（应用据此判断“无法记住登录”），网关环境里没有 `COFFEE_INTERNAL_KEY` 时会话里也没有。
路由 `env` 里点了名的变量，客户端永远不能用 `SendEnv` 伪造，即使这一次它被省略了。

**为什么密钥不进文件**：ADR 0002 的安全边界是内部密钥——拿到它就能伪造任意公钥指纹（记住我）与客户端地址
（限流）。配置文件要提交、要进镜像、要被 `--check` 打印，不能装密钥。所以文件里只写
`COFFEE_INTERNAL_KEY = "${env:COFFEE_INTERNAL_KEY}"`，值留在网关的环境里（`pnpm dev` 每次随机生成，compose 要求
你提供）；而且它**只到达点名它的路由**——画廊路由没有这一行，画廊进程里就没有密钥。`--check` 的摘要与
`Debug` 输出只列变量名，从不列值。

**`--check`**：读文件、校验、打印路由摘要（监听地址、限额、每条路由的用户名、命令、工作目录、变量名），配置错误
时错误信息点出键名、退出码 1。命令文件不存在**只是警告**：程序可能稍后才构建或挂载，而 e2e 的 `gateway`
流程恰好要测“程序缺失”时客户端看到什么。裸程序名（`coffee-tui`）每次启动时先在网关可执行文件旁边找，再找
`PATH`，所以 `target/debug/` 与镜像的 `/usr/local/bin/` 都不用写绝对路径。

## 3. 重载

**决定**：SIGHUP 重新读取并校验文件。成功则换上新快照（`tokio::sync::watch<Arc<Snapshot>>`）：**新连接**用新的
路由、限额与文案；每个连接在 accept 时拿到当时的快照并一直用它，所以**进行中的会话不受影响**。失败则记一行
错误、保留旧配置——一个手误不会让网关停下。`listen` 与 `host_key` 的变化只告警：换监听地址或主机密钥需要重启
（主机密钥变了，所有客户端都会报警，不应该悄悄发生）。

限额计数器（`Limits` / `Gate`）常驻，上限按每次调用传入：重载改上限不会把正在计的会话数与令牌桶清零，调低上限
只影响之后的新会话。

## 4. 优雅停机

**决定**：SIGTERM / SIGINT 时：停止 accept → 通知所有连接 → 每个会话收到 `shutting_down` 提示 → 向每个程序的
进程组发 SIGHUP → 等 `server.shutdown_grace`（默认 2 秒）→ 仍在的 SIGKILL → 带超时的 `disconnect` → 强关
socket → 等所有任务结束。总时长不超过 grace + 1 秒。

**为什么**：以前停机直接丢下会话，客户端的终端可能停在备用屏、raw 模式里，用户看不到原因。现在 `coffee-tui`
收到 SIGHUP 会恢复终端再退出，客户端看到一句说明。时长上限低于 `scripts/dev.mjs` 与 e2e harness 等子进程的
5 秒、Docker `stop` 的 10 秒，所以不会被外层强杀打断。调大 `shutdown_grace` 时要留在这些上限之下。

## 5. 健壮性

- **输入不阻塞**：russh 的 `data()` / `window_change_request()` 回调不再 `await` 有界队列（子进程不读输入时，
  整条连接的事件循环会卡在那里，连断开都处理不了）。输入进一个按**字节预算 1 MiB** 计的队列，**溢出就结束会话**：
  一个不读输入的程序或一个灌数据的客户端，与其无限缓冲或丢字节（丢字节会把转义序列切碎），不如明确结束。
  resize 用 `watch`（只留最新的尺寸，中间的不重要）；向 PTY 写入的任务用可取消的 `write` 与 resize 一起 `select!`。
- **启动不占 handler**：`shell_request` 只建输入通道并 spawn 会话任务；名额（`Slot`）在任务里获取并由任务持有，
  fork/exec 与重试的睡眠不再占着连接的 handler。
- **进程组**：每个会话程序是自己进程组的组长；**所有**结束路径（正常退出、断开、空闲、时限、溢出、停机、
  `Drop` 守卫）都对进程组 SIGHUP → 宽限 → SIGKILL，程序自己启动的子进程一起回收。
- **EOF = 挂断**：客户端的 `channel_eof`（例如 `ssh -tt … </dev/null`）按挂断处理，会话随即结束；以前它会让
  会话挂到空闲超时。
- **会话最长时长**：`limits.max_session_time`（可按路由覆盖，默认 `0` 不限），到时以状态 124 结束并显示
  `time_limit`。ADR 0001 列为“尚未做”的这一项由此关闭：一直有输入的会话也不能永远占着名额。
- **路由级上限**：`routes.NAME.max_sessions`，在全局与每地址上限之外；满了显示 `full_route`，不影响其他路由。
- **超时与 `TCP_NODELAY`**：结束路径上所有对 `Handle` 的调用（发提示、`exit_status`、`close`、`disconnect`）都带
  超时，一个不读数据的客户端拖不住回收；accept 之后 `set_nodelay(true)`（russh `Config.nodelay` 对 `run_stream`
  不起作用，按键回显因此多等一个 Nagle 周期）；任务边界记录 panic。

## 6. 上游抽象：只做本地命令

**决定**：路由连接到一个 `Upstream`，目前只有一种 `Upstream::Command(CommandSpec)`：本地程序 + 参数 + 工作目录，
跑在 PTY 上。会话循环只认 `Upstream::start → Running::{split, wait, stop}`。

**为什么只做本地命令**：这是唯一的需求，也是 ADR 0001 实测过的模型。SSH 上游（转发到另一台 sshd）要处理
上游的主机密钥、认证与通道语义；TCP 上游（telnet 风格）要处理终端尺寸与模式的协商——都是不小的新面，没有
用户时不值得。抽象留好了口子：新增一种上游只需实现 `start` 与 `Running` 的三个方法，路由、限额、重载、
停机都不用改。

## 7. 宿主拆分：`tty-host` 与 `coffee-tui`

**决定**：通用部分抽成 `tty-host`，咖啡馆只提供参数：

```rust
#[non_exhaustive]
pub struct HostOptions {
  pub program: String,              // 日志与提示里的程序名
  pub root: Root,                   // nsh 根组件
  pub ui_sets: Vec<Registry>,
  pub color_mode: Option<String>,   // 强制色深（咖啡馆传 COFFEE_COLORS）
  pub interrupt_hint: String,       // “再按一次 Ctrl+C 退出”
  pub frame_filter: Option<FrameFilterFactory>,
  pub quiet_panics: bool,
}
pub trait FrameFilter { fn apply(&mut self, buffer: &mut Buffer); fn owns_colors(&self) -> bool { true } }
pub type FrameFilterFactory = Box<dyn FnOnce(ColorDepth) -> Box<dyn FrameFilter>>;
pub fn run(options: HostOptions) -> io::Result<()>
```

`tty-host` 负责 ADR 0001“终端宿主加固”里的一切：自己的 `poll` 主循环、信号、挂断检测、`use-dev-tty`、非阻塞
写出、启动查询、OSC 52、色深判断（`crates/tty-host/src/color.rs`）、Ctrl+C 连按两次。`coffee-tui` 是咖啡馆的薄
入口：从进程环境读 `SessionEnv`（`session_env_from_process`）、画廊开关、`Root`、调用 `coffee_app::configure`，
再交给 `tty_host::run`。

**钉色留在咖啡馆**：`palette.rs` 的 `Pin` 实现 `FrameFilter`，在每帧渲染后、diff 之前把载体换回 token（ADR 0002
第 4 节）。它依赖 `apps/tui/src/ui/theme.rs` 的载体表与 `theme.json`，这是一家店的设计决定，不是终端的通用能力；
别的应用可以不要过滤器，或带自己的。`owns_colors()` 在所有色深都为 `true`，快照与 `tests/pin_render.rs` 不变
（它们一起搬到 `crates/coffee-tui`）。

**panic 处理**：nsh 引擎线程上的 panic 以前可能被当成“应用已结束”，进程以 0 退出，网关不显示崩溃提示。现在
panic hook 置位并请求停机，主循环以错误结束（非零退出码，网关显示 `crashed`），优先级高于“已结束”；只有
`AppError::is_fatal()` 的错误才结束循环，非致命错误不再把正常退出变成退出码 1。`quiet_panics`：`coffee-tui`
在有 `COFFEE_CLIENT_IP`（即经网关运行）时打开，终端上只打印一行中性提示，不把 panic 文案与源码路径泄给 SSH
客户端；本地直接运行时照常打印，方便调试。

## 8. 测试

- `crates/tty-gateway-testkit`：从原来的 `tests/support/mod.rs` 提出来的进程内 `TestServer`（临时端口、给定配置
  文本）、russh 客户端（可选公钥、可选用户名）、`vt100` 终端、探针程序（打印指定环境变量）。网关自己的测试与
  商店的 SSH 测试共用它。
- `crates/tty-gateway/tests`：只用探针程序与中性文案——认证、限额、空闲、崩溃、resize、路由（`routes.rs`：
  三种用户名、默认路由、密钥只到点名的路由、无公钥时省略指纹、客户端不能伪造路由变量、路由级上限、重载、
  `--check`），以及第 5 节的健壮性（卡住的子进程不阻塞关闭、输入洪泛、孙进程回收、EOF、时限、坏文件保留旧
  配置、停机在 grace + 1 秒内回收）。
- 依赖真实 `coffee-tui`、中文文案与 stub 后端的用例（以前在 `coffee-sshd/tests/e2e.rs`，过渡期在
  `tty-gateway/tests/shop.rs`）最终搬到 **`crates/coffee-tui/tests/ssh.rs`**：用 testkit、
  `CARGO_BIN_EXE_coffee-tui` 与仓库里的 `deploy/gateway.toml`，所以 `pnpm test` 不再需要先构建 `coffee-tui`。
  ADR 0001 的度量 `measure_twenty_sessions` 也在那里。
- `scripts/e2e` 用 `tomllib` 读 `deploy/gateway.toml`，只覆盖测试需要的两项；CI 对它跑 `--check`；
  `deploy/smoke-test.sh` 多一次 `ssh gallery@`。

## 后果

- 换一个终端应用、加一条路由、改限额或文案都只改配置文件，不改网关代码；文件可以先 `--check`，再 SIGHUP。
- 调优从环境变量搬到了文件：以前 `COFFEE_SSH_MAX_SESSIONS=…` 一行就能改，现在要编辑或挂载自己的
  `gateway.toml`（`deploy/README.md` 有 compose 示例）。在循环里连接的脚本要用 `connections_per_minute = 0`
  的配置，而不是 `COFFEE_SSH_CONNECTIONS_PER_MINUTE=0`。
- 应用的输入契约（`COFFEE_*`）没变，但它们现在**经路由的 `env` 表**到达：在 compose 或 `pnpm dev` 里设
  `COFFEE_COLORS`、`COFFEE_API_URL` 仍然有效，前提是文件里那条路由点了它们的名。
- 网关多了一层间接：一个会话收到什么环境，要看文件，而不是看代码。`--check` 的摘要就是为此准备的。
- 网关与宿主都不再认识咖啡馆，品牌文案只有 `deploy/gateway.toml` 与 `apps/tui` 两处。

## 已知缺口 / 不做

- **只有本地命令上游**：不做 SSH / TCP 上游（第 6 节）。
- **没有按路由的 `authorized_keys`**：所有路由都接受任意公钥或无公钥；“谁能进哪条路由”只靠用户名，用户名不是
  凭据。需要真正受限的路由时，要先加按路由的认证。
- **没有用户名通配**：只有精确匹配与默认路由，不支持 `*`、前缀或正则。
- **没有子进程 rlimit / cgroup**：进程组回收挡不住自己调用 `setsid` 的孙进程——它离开了进程组，网关回收不到。
  `coffee-tui` 不这样做；别的程序要靠容器整体的 `pids_limit` / `mem_limit`。
- **没有 metrics 端点**：只有日志。
- **横幅不能按路由**（第 1 节，russh 的限制）。
- **调优只在文件里**（见“后果”）。
- 未能从源码核实：OpenSSH 客户端在 PTY 会话上发送 EOF 的确切时机；由手动检查 `ssh -tt … </dev/null` 立即结束
  覆盖。
