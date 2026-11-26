# ADR 0001：SSH 会话模型——每个会话一个 PTY 子进程

- 状态：已采纳（第一阶段 spike 结论；第二阶段补充真彩探测、加固与复测）
- 日期：2026-11-22
- 范围：`crates/coffee-sshd`、`crates/coffee-host`、`deploy/`

## 背景

ROAST/TTY 通过 SSH 免密码提供一个 nsh（0.6.1，Rust 模式）写的终端咖啡馆。需要决定一个 SSH
会话如何承载一份 TUI：

1. **每会话一个子进程**：`coffee-sshd`（russh）为每个会话分配真实 PTY，启动 `coffee-tui`
   （nsh `AppView` + crossterm，与本地开发用的是同一个二进制），把 PTY 的两端接到 SSH 通道。
2. **进程内多 `AppView`**：在 sshd 进程里为每个会话建一个 `AppView`，自己把字节流接到 ratatui
   后端。
3. **OpenSSH `ForceCommand`**：系统 sshd 负责认证与 PTY，`ForceCommand coffee-tui`。

预设的取舍规则：**每会话常驻内存约 60 MB 以内、空闲 CPU 接近 0 → 保留方案 1**。

## 决策

保留方案 1：每个 SSH 会话一个 `coffee-tui` 子进程，跑在自己的 PTY 上。实测（见下）远低于阈值。

理由：

- nsh 0.6 的 `AppView` 带全局状态（`coffee_app::configure` 是进程级 `OnceLock`，nsh 的主题、
  引擎线程、信号处理也按“一个进程一个终端”设计），进程内多实例需要改框架或冒共享状态的险；
  子进程天然隔离，一个会话崩溃不影响别人。
- resize（SIGWINCH）、输入解析、终端模式都交给 crossterm 与内核 PTY，和本地终端里完全相同，
  本地开发看到的就是线上看到的。
- 回收简单：会话结束 = 杀进程组。
- 方案 3 需要系统 sshd、系统账号和 PAM 配置，免密码登录（任意公钥 / 无公钥）在 OpenSSH 里要
  `PermitEmptyPasswords` + 空密码账号之类的危险配置，也拿不到“公钥指纹 → 会话”的直通，否决。

## 度量（20 个并发会话）

环境：macOS（Apple Silicon，Darwin 27.2），release 构建，真实 `coffee-tui`（当前占位首页），
russh 客户端 + `vt100` 终端模拟器，160×50，无公钥登录；度量代码是
`crates/coffee-sshd/tests/e2e.rs` 的 `measure_twenty_sessions`（`--ignored`）。

服务器为独立运行的 release `coffee-sshd`（`COFFEE_SPIKE_ADDR`/`COFFEE_SPIKE_PID`），测试客户端也是
release 构建。连续两轮：

| 指标                                                  | 第 1 轮                        | 第 2 轮                     |
| ----------------------------------------------------- | ------------------------------ | --------------------------- |
| 启动延迟（连接 → 首帧含 `◆ROAST/TTY`，20 个同时发起） | p50 57 ms · p95/最大 162 ms    | p50 42 ms · p95/最大 155 ms |
| 每个 `coffee-tui` 常驻内存（RSS）                     | 15.7–16.0 MB，均值 15.8 MB     | 15.7–16.0 MB，均值 15.9 MB  |
| 每个 `coffee-tui` 空闲 CPU（10 秒窗口）               | 均值 0.08 %，最大 0.10 %       | 均值 0.05 %，最大 0.10 %    |
| `coffee-sshd` 自身                                    | RSS 7.6 MB，空闲 CPU 0.00 %    | RSS 7.8 MB，空闲 CPU 0.00 % |
| 20 个客户端同时断开                                   | 88 ms 内全部子进程退出，0 残留 | 82 ms 内全部退出，0 残留    |

（`ps` 的 CPU 时间精度为 10 ms，10 秒窗口里 0.10 % 就是“1 个刻度”，即测量下限。）

改进前的对照：nsh 的 `run_app` 每 16 ms 轮询一次 crossterm（因为引擎线程无法唤醒 crossterm），
同样的度量得到**每会话空闲 CPU 0.49 %**（20 个会话合计约 10 % 个核）。宿主改为在 stdin 与一个
自管道上 `poll`——管道由 nsh 的 wake 回调（`AppOptions::wake`）和 SIGWINCH 写入，最长 250 ms
醒一次检查信号/挂断——之后降到测量下限。debug 构建的同一测试：RSS 约 16 MB、启动 p50 约 850 ms
（debug 下 russh 的加密很慢），不作为结论依据。另：macOS 上二进制重新编译后首次启动会被系统扫描，
多出 1 秒以上，Linux 容器里没有这个现象。

结论：每会话内存与空闲 CPU 都远低于“≈60 MB、接近 0”的阈值 → **保留每会话一个进程**。

### 第二阶段复测（2026-11-22，当前应用 + 真实后端）

同一测试，同一台机器（8 核），release `coffee-sshd` + release `coffee-tui`（商品列表首页、HTTP 客户端），
后端为 `deploy/server.Dockerfile` 的运行布局在本机组装（`node dist/main.mjs`，SQLite，迁移+种子数据，
带内部密钥），`COFFEE_SPIKE_WAIT='耶加雪菲 孔加'` 另计“商品出现在屏幕上”的时间。先连一次预热（见上
“首次启动被系统扫描”）。连续两轮：

| 指标                                                  | 第 1 轮                        | 第 2 轮                      |
| ----------------------------------------------------- | ------------------------------ | ---------------------------- |
| 启动延迟（连接 → 首帧含 `◆ROAST/TTY`，20 个同时发起） | p50 75 ms · p95/最大 101 ms    | p50 125 ms · p95/最大 146 ms |
| 连接 → 后端的商品列表出现在屏幕上                     | p50 86 ms · p95/最大 104 ms    | p50 134 ms · p95/最大 151 ms |
| 每个 `coffee-tui` 常驻内存（RSS）                     | 19.8–20.3 MB，均值 20.1 MB     | 14.9–16.3 MB，均值 15.4 MB   |
| 每个 `coffee-tui` 空闲 CPU（10 秒窗口）               | 均值 0.00 %，最大 0.00 %       | 均值 0.01 %，最大 0.10 %     |
| `coffee-sshd` 自身                                    | RSS 7.5 MB，空闲 CPU 0.00 %    | RSS 7.0 MB，空闲 CPU 0.00 %  |
| 20 个客户端同时断开                                   | 12 ms 内全部子进程退出，0 残留 | 13 ms 内全部退出，0 残留     |

RSS 两轮之差来自 macOS 的内存压缩/回收时机（同一二进制、同一页面），按上限 20 MB 规划：64 个会话约
1.3 GB + sshd。空闲 CPU 仍在测量下限。结论不变：**保留每会话一个进程**。

### 第二阶段完成后复测（2026-11-22，六个画面全部落地）

同一测试（已改为新流程：新连接先到登录页，首帧以演示提示 `演示环境` 为准；设了 `COFFEE_SPIKE_WAIT` 时每个会话
以访客身份走一遍：`Esc` → `g` → 列表 → `⏎` 第一款的详情 → `a` 加入 → `2` 购物车，之后停在购物车上测内存与空闲
CPU）。同一台机器（Apple M1，8 核，macOS 27.2），release `coffee-sshd` + release `coffee-tui`，后端为
`node dist/main.mjs`（`pnpm -F @coffee/server build`）+ 新迁移、新种子的 SQLite，带内部密钥；先连一次预热。复现：
起后端与 release sshd（`COFFEE_SSH_MAX_SESSIONS_PER_IP=64`、`COFFEE_SSH_CONNECTIONS_PER_MINUTE=0`），然后
`COFFEE_SPIKE_ADDR=127.0.0.1:<端口> COFFEE_SPIKE_PID=<sshd pid> COFFEE_SPIKE_WAIT='耶加雪菲 孔加' cargo test --release
-p coffee-sshd --test e2e measure_twenty_sessions -- --ignored --nocapture`。

| 指标                                      | 走完 列表→详情→购物车（第 1 轮） | 同（第 2 轮）                | 只停在登录页                |
| ----------------------------------------- | -------------------------------- | ---------------------------- | --------------------------- |
| 连接 → 首帧（登录页，20 个同时发起）      | p50 74 ms · p95/最大 160 ms      | p50 63 ms · p95/最大 91 ms   | p50 80 ms · p95/最大 95 ms  |
| 连接 → 商品列表（含 `Esc`、`g` 两次往返） | p50 267 ms · p95/最大 325 ms     | p50 231 ms · p95/最大 263 ms | —                           |
| 每个 `coffee-tui` 常驻内存（RSS）         | 35.1–37.1 MB，均值 36.1 MB       | 35.2–37.7 MB，均值 36.4 MB   | 22.5–22.9 MB，均值 22.8 MB  |
| 每个 `coffee-tui` 空闲 CPU（10 秒窗口）   | 均值 0.03 %，最大 0.10 %         | 均值 0.06 %，最大 0.10 %     | 均值 0.38 %，最大 0.50 %    |
| `coffee-sshd` 自身                        | RSS 7.4 MB，空闲 CPU 0.10 %      | RSS 7.6 MB，空闲 CPU 0.10 %  | RSS 7.5 MB，空闲 CPU 0.30 % |
| 20 个客户端同时断开                       | 14 ms 内全部子进程退出，0 残留   | 13 ms 内全部退出，0 残留     | 16 ms 内全部退出，0 残留    |

读法：

- 内存随用过的画面增长（详情的风味雷达、购物车、各 store 的数据），走完一圈约 **36 MB**，比上一轮只看列表时的
  15–20 MB 高，但仍远低于 60 MB 的阈值。按 **40 MB/会话** 规划：64 个会话约 2.6 GB + sshd。
- 停在购物车上的空闲 CPU 仍在测量下限；**停在登录页**时每会话约 0.4 %（20 个会话合计约 8 % 个核），来自登录页的
  蒸汽动画（离开登录页、停在购物车上即回到下限），sshd 为转发这些帧也有 0.3 %。这是有意的动效，不改变结论，
  但公开部署时若大量连接停在登录页，可考虑让动画在无输入一段时间后停下（已列为对界面的建议）。
- 结论不变：**保留每会话一个进程**。

断开行为：客户端断开（TCP 关闭）后 sshd 立即向子进程**进程组**发 SIGHUP，`coffee-tui` 收到后
恢复终端并退出；1 秒内未退出则 SIGKILL（`KILL_GRACE`），所以 2 秒内必然回收。e2e 测试
`a_disconnect_ends_the_real_tui_within_two_seconds` 与 `a_disconnect_ends_the_child_within_two_seconds`
固定了这一点。网络静默中断（没有 FIN）时依赖 SSH keepalive：30 秒一次、3 次无响应断开，即最长约
2 分钟后回收——这不在“2 秒”承诺之内。

## 终端宿主（`coffee-host`）加固

- **等待**：不用 nsh `run_app` 的 16 ms 轮询，自己写了等价的主循环（同样只用公开 API），在
  stdin 与自管道（nsh wake 回调 + SIGWINCH）上 `poll`，最长 250 ms；钉色和 Ctrl+C 提示也在这个
  循环里画（渲染之后、ratatui diff 之前）。焦点回来时整屏重画，等同于 `draw_terminal` 的 repaint。
- **信号**：SIGHUP / SIGTERM / SIGINT 只置位标志（`signal-hook`），信号会打断 `poll`，主循环随即退出，
  恢复终端（离开备用屏、关鼠标/粘贴/焦点/kitty 键盘、显示光标、退出 raw 模式）。处理器在进程
  结束前不卸载，避免恢复途中第二个 SIGHUP 以默认动作杀掉进程。
- **终端消失**：每次轮询检查 stdin/stdout 的 HUP/ERR/NVAL；macOS 的 `poll` 对主端已关闭的 PTY
  可能不报 HUP，另用“可读但 FIONREAD 为 0 = EOF”判断（与 nsh-tui-host 相同）。
- **发现并绕过的缺陷**：crossterm 0.29 默认（mio）的输入读取在 PTY 挂断后会在
  `read` 返回 0/EIO 时**死循环、100% CPU 且永不返回**——引导阶段的宿主遇到断线会留下满载孤儿
  进程（SIGHUP 也救不了，因为循环里不检查标志）。`coffee-host` 显式依赖 crossterm 的
  `use-dev-tty` 特性（基于 `poll` 的读取器，nsh-tui-host 也这样做），实测挂断后约 30 ms 正常退出。
- **写出不阻塞**：stdin/stdout 设为非阻塞，写满时以 16 ms 为步长等待，并在挂断或收到信号时放弃，
  客户端不读数据也卡不住进程。
- **启动查询**：OSC 10/11、kitty 键盘协议、DECRQM 2026（同步输出）和 DA1（最后发，所有终端都会
  回答，回答到即结束等待；上限 1.5 秒，SSH 下需要一个往返）。支持则启用 kitty 键盘协议
  （Apple Terminal、VS Code、VTE、tmux < 3.3 除外）与同步输出（整帧写出、不撕裂）。
- **剪贴板**：OSC 52 写到用户本机终端，不回退到服务器的系统剪贴板。
- **色深**（第二阶段实现，`crates/coffee-host/src/color.rs`，单元测试逐行覆盖下表）：按顺序，第一条
  适用的规则决定：

  | #   | 条件                                                    | 色深                                      |
  | --- | ------------------------------------------------------- | ----------------------------------------- |
  | 1   | 服务器 `COFFEE_COLORS=truecolor/24bit/256/16/mono/none` | 照办（终端的回答也不改它）                |
  | 2   | `NO_COLOR` 非空                                         | 单色                                      |
  | 3   | `COLORTERM` 为 `truecolor`/`24bit`                      | 真彩                                      |
  | 4   | 真彩品牌（见下）                                        | 真彩                                      |
  | 5   | `TERM` 含 `256color`                                    | 256；终端答了 XTGETTCAP `RGB`/`Tc` → 真彩 |
  | 6   | `TERM=dumb`                                             | 单色                                      |
  | 7   | 其余                                                    | 16                                        |

  真彩品牌：`TERM` 含 `kitty`/`ghostty`/`alacritty`/`foot`/`wezterm`/`rio` 或以 `-direct` 结尾；本地运行时另认
  `TERM_PROGRAM`（iTerm.app、ghostty、WezTerm、vscode、WarpTerminal、rio、alacritty）与 `KITTY_WINDOW_ID`、
  `ALACRITTY_WINDOW_ID`、`WEZTERM_EXECUTABLE`、`GHOSTTY_RESOURCES_DIR`、`WT_SESSION`。

  OpenSSH 默认不转发 `COLORTERM`（macOS 自带配置只 `SendEnv LANG LC_*`），而 `TERM` 原样到达，所以第 4 条
  让 kitty、Ghostty、foot、Alacritty、WezTerm、rio 用原版 `ssh` 也是真彩（同 nsh-tui-host 0.6.1 的
  `truecolor_brand`；`TERM_PROGRAM` 等经 SSH 到不了，只对本地运行有用）。
  第 5 条只在“回答能改变结果”时才问：在已有的启动查询里、DA1 之前加两个 `XTGETTCAP` 请求
  （`DCS + q 524742 ST` 问 `RGB`、`DCS + q 5463 ST` 问 `Tc`，分开发，因为 xterm 遇到第一个不认识的名字就
  停）。终端按序回答，答复先于 DA1 到达，所以**不增加等待**（DA1 一到即结束，上限仍是 1.5 秒）；只有
  完整的 `DCS 1 + r … ST` 才算数，`0 + r`、半截或沉默都维持 256 色——只会升级、不会降级。只问声称
  `256color` 的终端：这类现代终端按 VT 状态机吞掉不认识的 DCS，不会把字节画到屏幕上；`TERM=linux`
  （Linux 控制台不解析 DCS）这类 16 色终端从不问。e2e 测试固定了这些：`xterm-kitty` 且无 `COLORTERM` →
  只有 `38;2;`/`48;2;`（底色 `48;2;20;17;14`）；`xterm-256color` 不回答 → 只有 `38;5;`/`48;5;`，且确实发了
  查询；`xterm-256color` 回答 `RGB` → 真彩，屏幕上和应用输入里都没有回答的残留。
  **未实现 DECRQSS 往返**（“设一个 24 位 SGR 再 `DCS $ q m ST` 读回”）：它要改终端的 SGR 状态并在之后
  复位，多一段输出；支持 DECRQSS 但只有 256 色的终端会回报量化后的值，判断要比对数值；而 XTGETTCAP 已覆盖
  会回答这类查询的终端（xterm、kitty、foot、WezTerm 等，各终端未在此逐一实测）。回答慢于 1.5 秒（极慢的
  链路）时，迟到的回答会被当作按键送进应用——这是已有启动查询（OSC 10/11 等）同样的风险，不是新问题。
  其余手段不变：用户可 `ssh -o SendEnv=COLORTERM`（`pnpm ssh` 已带上），运维可设服务器端
  `COFFEE_COLORS=truecolor` 强制真彩（不支持 24 位色的终端会因此显示错色，只在确知客户端时用）。客户端
  无法经 SSH 设 `COFFEE_COLORS`：sshd 只接受 `LANG`、`LC_*`、`COLORTERM`。

## 调色板钉色结果

nsh 的 TUI 把应用颜色映射成 ANSI 语义色（`nsh-platform-tui` 的 `palette.rs`），真实颜色由用户的
终端主题决定，设计稿的琥珀 `#f0b45a`、薄荷 `#8cc7a1` 等因此无法保证。`TerminalCapabilities`
的钩子不够：给了 `background_color` 后 nsh 改走“表面阶梯”，主按钮变成背景与琥珀的混色
`rgb(130,99,52)`、次要文字变成 `rgb(128,123,114)`，都偏离设计值。

所以宿主**不**把终端底色交给 nsh（让它画“无表面”外观：强调色为 ANSI 色名、次要文字为
`DIM`、不透明强调色填充保留精确 RGB、卡片填充为 `DarkGray`），再在每帧渲染后、diff 之前对
ratatui 缓冲做一次钉色（`coffee_host::palette::Pin`）：

| nsh 画出                                   | 钉成（`apps/tui/theme.json` 暗色）                                          |
| ------------------------------------------ | --------------------------------------------------------------------------- |
| 背景 `Reset`                               | `background #14110e`                                                        |
| 背景 `DarkGray`（卡片/弹层/边框填充）      | `card #1b1713`                                                              |
| `Yellow`/`LightYellow`                     | `primary #f0b45a`                                                           |
| `Green`/`LightGreen`                       | `success #8cc7a1`                                                           |
| `Red`/`LightRed`                           | `destructive #f08a6c`                                                       |
| 文字 `Reset` + `DIM`                       | `muted-foreground #a39684`（去掉 DIM）                                      |
| 文字 `Reset`                               | `foreground #ece4d6`；在琥珀/薄荷/珊瑚填充上为 `primary-foreground #1a140d` |
| 无表面外观的悬停/按下带（黑上 10%/18% 白） | `background` 向 `foreground` 混合 10%/18%                                   |

各色深的实测（`crates/coffee-host/tests/pin_render.rs` 用真实 nsh 渲染探针组件后钉色；
SSH 下的原始输出由 e2e 与 PTY 驱动脚本抓取）：

| 色深      | 结果                                                                                                                                                                                                 |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| truecolor | 全部 token 精确还原：正文 `38;2;236;228;214` / 底 `48;2;20;17;14`、次要 `38;2;163;150;132`、琥珀 `240,180,90`、薄荷 `140,199,161`、珊瑚 `240,138,108`、主按钮 `#1a140d` on `#f0b45a`、卡片 `#1b1713` |
| 256       | 取 xterm 立方体/灰阶（16–255，终端主题不改这些）中“redmean”距离最近者：底 233、正文 254、次要 246、琥珀 215、薄荷 115；永不输出 0–15                                                                 |
| 16        | 不钉，交给终端主题：琥珀 = `Yellow`，次要 = `DIM`，主按钮 = `Yellow` + 反显                                                                                                                          |
| mono      | 不钉、不输出任何颜色：次要 = `DIM`，状态靠反显/粗体/下划线与字形表达                                                                                                                                 |

已知局限：`soft #d9cfbf` 这类接近前景的中性色被 nsh 归为默认前景，只能还原成 `foreground`；
`card`、`popover`、`border` 作为填充时 nsh 都画成 `DarkGray`，统一钉成 `card`；没有可识别
语义色名的颜色（图片、canvas）原样通过。nsh 升级若改了映射，`pin_render.rs` 会失败提醒。

## Ctrl+C 策略：连按两次退出

nsh 默认“有选区则复制，否则立即退出”。经 SSH 时 raw 模式下 Ctrl+C 只是一个按键，一次误触就
会断开整个会话（访客的购物车也随之丢失）。三个选项：

| 选项                     | 问题                                               |
| ------------------------ | -------------------------------------------------- |
| nsh 默认（一次即退出）   | 误触代价太高                                       |
| 忽略                     | 用户在终端里的肌肉记忆失效，会以为程序卡死         |
| **连按两次退出（采纳）** | 失去“Ctrl+C 复制选区”（可用应用自己的复制/OSC 52） |

实现：宿主拦截 Ctrl+C（按下/重复/释放都不交给应用），第一次在底行居中显示
`再按一次 Ctrl+C 退出`（反显，钉色后为琥珀底深色字），2 秒内再按一次退出（退出码 0，SSH 通道
随之以状态 0 关闭）；超时或按了别的键则取消。e2e 测试覆盖了这一行为。应用若要自己画这条提示
（与状态栏统一），见下方“对其他切片的请求”。

## 认证（对原版 OpenSSH 客户端零配置免密码）

- 服务器只公布 `publickey,keyboard-interactive`。OpenSSH 先发 `none`，立即被拒（不计时惩罚），
  拿到方法列表。
- **任意公钥都接受**，并记录其 SHA256 指纹（`SHA256:…`），作为 `COFFEE_SSH_FINGERPRINT` 传给子进程。
  不接受 `none` 是有意的：一旦 `none` 成功，带密钥的客户端根本不会出示公钥，“记住我”就拿不到指纹。
- 没有公钥（或 `-o PubkeyAuthentication=no`）时走 **keyboard-interactive，零个问题直接成功**：
  客户端不显示任何提示。
- 认证横幅：`◆ROAST/TTY 终端咖啡馆 / 演示环境 · 不是真实商店 · 不收款、不发货 · 请勿使用真实密码、地址或电话`
  （第二阶段有了账号与地址本，多提醒地址和电话）。横幅里没有版本号、主机名或其他内部信息；
  SSH 版本串是固定的 `SSH-2.0-ROAST_TTY`。
- 已验证：原版 `ssh`（OpenSSH_10.5p1）默认身份（RSA 密钥）与 `-o PubkeyAuthentication=no -o IdentitiesOnly=yes`
  两种方式都无提示进入。
- 局限：如果用户默认密钥是**带口令且未载入 agent** 的，OpenSSH 会在签名前询问口令——服务器
  无法避免（接受公钥必须先验证签名）。直接回车跳过即可落到 keyboard-interactive。

## 会话边界与限额

- 接受 `pty-req`、`window-change`、`shell`，以及安全的 `env`（`LANG`、`LC_*`、`COLORTERM`，
  名字/值字符白名单）；拒绝 `exec`、子系统（sftp）、X11、agent 转发、端口转发（本地/远程）。
  每个连接只允许一个会话通道；没有 `pty-req` 的 shell 会收到“请用 `ssh -t`”并以状态 1 结束。
- 子进程环境从空开始：`PATH/HOME/TMPDIR/USER/LOGNAME`（继承自 sshd）、`TERM`（客户端，非法值
  替换为 `xterm-256color`）、`LANG`（客户端的 UTF-8 locale，否则 `C.UTF-8`）、客户端的
  `LC_*`/`COLORTERM`、`COFFEE_API_URL`、`COFFEE_INTERNAL_KEY`（若配置）、`COFFEE_SSH_FINGERPRINT`
  （仅当公钥验证过）、`COFFEE_CLIENT_IP`、`COFFEE_COLORS`（若服务器配置）。
- 限额：`COFFEE_SSH_MAX_SESSIONS`（64）、`COFFEE_SSH_MAX_SESSIONS_PER_IP`（8）。名额在 **shell 请求时**
  才占（第一阶段在 TCP accept 时就占，正在登录的连接也占名额）。超限的连接仍能看到横幅里的说明，并在
  通道里收到“会话已满/已达上限”后以状态 1 关闭；连接数超过 `2 × 上限 + 16` 时在握手前直接丢弃。
- 握手前的门（第二阶段）：同一地址最多 `2 × COFFEE_SSH_MAX_SESSIONS_PER_IP` 个打开的连接，新连接
  `COFFEE_SSH_CONNECTIONS_PER_MINUTE`（30，令牌桶，可一次用满；0 关闭）。超出的连接在握手前直接关闭，
  同一地址连续被拒只记一行日志。
- 时限：从 accept 到会话开始（握手、登录、shell 请求）共 `COFFEE_SSH_AUTH_TIMEOUT`（30 秒）。第一阶段只在
  握手完成后计时、且只看“是否登录”：不发 SSH 版本行的连接会**永远**挂着（`russh` 在
  `inactivity_timeout: None` 时读版本行不设时限），登录后不开 shell 的连接也永远挂着——一个地址开
  `2 × 64 + 16` 个这样的连接就能让所有人连不上。e2e `a_silent_connection_is_cut_off_at_the_login_deadline`
  固定了这一点。
- 空闲：`COFFEE_SSH_IDLE_TIMEOUT`（第二阶段默认 900 秒，原 1800）内无输入则关闭会话（状态 124，提示
  “空闲超过 N 分钟”）。**从会话程序第一次输出开始计时**，输入重新计时（见下“空闲计时的缺陷”）。
- 会话程序非 0 退出（崩溃、被信号杀死、报错退出）：sshd 先发一段终端复位（弹出 kitty 键盘标志、关鼠标/
  粘贴/焦点报告、结束同步更新、离开备用屏、显示光标、复位颜色），再显示
  “ROAST/TTY 出错了，会话已结束（代码 N）。请重新连接；如果一直这样，请稍后再试。”，以该代码关闭通道，
  日志 `warn`。正常退出（0）不提示。
- 网关停止（SIGTERM / Ctrl+C）：不再接受新连接；每个会话的程序收到 SIGTERM，自己恢复客户端终端（离开
  备用屏等）后退出（1 秒内不退则 SIGKILL，并补发上面的终端复位），它最后的输出先发完，再显示
  “ROAST/TTY 服务正在停止，连接已关闭。请稍后重新连接。”，以状态 143 关闭通道，然后断开连接；还在登录的
  连接直接断开。全部连接结束（至多约 3.5 秒）后进程退出。此前进程直接退出，客户端停在备用屏里。e2e
  `a_stopping_server_restores_the_client_terminal_before_it_closes` 固定了这一点。

### 空闲计时的缺陷（第二阶段修复）

`an_idle_session_is_closed`（空闲 1 秒）在满载的 `pnpm verify` 中偶发失败：会话在探针子进程打印任何
东西之前就被当作空闲关闭。根因不在测试：空闲时钟的起点是 TCP accept（`Connection::new` 里初始化
`last_input`），而 debug 构建里 russh 的握手与登录本身就要数百毫秒，机器繁忙时超过 1 秒，于是会话
一启动就已“空闲”超时，子进程刚 spawn 就收到 SIGHUP。生产环境同理：登录慢于空闲时限的客户端（时限
设得很小时）会被立刻踢掉。修复：时钟从会话程序第一次输出开始（之前不会判空闲），输入重新计时。
新测试 `a_slow_login_does_not_count_as_idle`（登录后停 1.5 倍时限再开 shell）在旧逻辑下失败、新逻辑下
通过；`an_idle_session_is_closed` 改为等第一行输出并断言关闭不早于时限。同类时间假设的排查：测试客户端
原来只在同一条消息里看到 `ESC [ c` 才回答 DA1（被拆到两条消息时，宿主要多等 1.5 秒），现在按累计输出
计数回答；`alive()` 先用 `kill(pid, 0)` 判断（不用等一次 `ps`，繁忙时 `ps` 本身可能要上百毫秒，计入
“2 秒内退出”的测量）；其余测试的等待都是“直到条件成立，上限 10/15 秒”，没有固定睡眠。

### 公开演示的加固审查（第二阶段）

| 项               | 现状                                                                                          |
| ---------------- | --------------------------------------------------------------------------------------------- |
| 每地址连接速率   | 已加：`COFFEE_SSH_CONNECTIONS_PER_MINUTE`（30），握手前拒绝                                   |
| 每地址打开的连接 | 已加：`2 × COFFEE_SSH_MAX_SESSIONS_PER_IP`（16）；总连接 `2 × 上限 + 16`                      |
| 登录时限         | 已改：accept → 会话开始整段 30 秒，含握手前（原先不发版本行的连接永不超时）                   |
| 名额             | 已改：shell 请求时才占；登录中的连接不占                                                      |
| 会话上限         | 64 / 每地址 8（不变）                                                                         |
| 空闲             | 已改：默认 900 秒，从首个输出算起                                                             |
| 横幅             | 演示声明 + 不要用真实密码、地址、电话；无版本/主机信息                                        |
| 日志             | 见下                                                                                          |
| 子进程崩溃       | 已加：终端复位 + 中文说明 + 非 0 状态（见上）                                                 |
| 容器             | uid 10001、`cap_drop: ALL`、`no-new-privileges`、`init`；后端不发布端口、在无出网的内部网络上 |

日志（`info`）：连接（地址）、登录（方式、公钥指纹——按设计）、会话（pid、`TERM`、尺寸、结束方式）、拒绝。
从不记内部密钥、环境变量值、输入内容；同一地址连续被拒只记一行。

尚未做（成本不低或需要取舍，列在这里）：

- **会话最长时长**：持续有输入的会话可以一直占着名额（每地址 8 个）。可加 `COFFEE_SSH_MAX_SESSION_TIME`。
- **全局握手速率 / 分布式来源**：每地址的限制挡不住大量不同地址；需要前置防火墙或代理（如 nftables
  `limit`、云负载均衡的连接限速）。russh 的密钥交换在 release 下约毫秒级，但仍是 CPU 开销。
- **按指纹/账号的限制**：公钥可随意生成，按指纹限流意义不大；账号层面的滥用（批量注册、刷单）属于后端。
- **子进程资源上限**：没有给每个 `coffee-tui` 设 RLIMIT（内存、CPU、文件数）或 cgroup；容器整体的
  `mem_limit`/`pids_limit` 由运维按“64 × 单会话内存”设。
- **Docker 本机连接共用一个地址**：userland proxy 下，从 Docker 主机本机连入的客户端都显示为网桥地址，
  共享一个地址的限额（外部客户端经 iptables DNAT 保留真实地址）。
- **日志量**：每个连接仍有 `connected`/`disconnected` 两行；洪泛时靠前置限速与日志轮转。

## 对其他切片的请求

- 根 `.gitignore` 加 `.data/`：`cargo run -p coffee-sshd` 默认把主机密钥生成在工作目录的
  `.data/ssh_host_ed25519_key`。
- TUI（`apps/tui`）：Ctrl+C 不再到达应用（宿主拦截），应用不要把 Ctrl+C 绑成任何命令；若想让
  “再按一次 Ctrl+C 退出”出现在自己的状态栏里，需要宿主与应用之间加一个约定（例如应用暴露一个
  信号/回调），目前由宿主覆盖在底行中央 2 秒。
- TUI：颜色请只用 `theme.json` 的 token，钉色按“ANSI 语义 → token”反查；自定义的饱和色会落到
  最近的色相（黄/绿/红会被钉成 primary/success/destructive），中性色会变成前景或次要文字。
- 根脚本 `pnpm dev`/`pnpm ssh`：`coffee-sshd` 需要先 `cargo build -p coffee-host`（它在自身旁边找
  `coffee-tui`）。PTY 驱动脚本若只发一次 `\x03` 期待退出，需要改为两次。

## 后果

- 每会话一个进程的内存与进程数随在线人数线性增长；限额默认 64，容器按“64 × 单会话内存 + sshd”
  规划。
- `coffee-tui` 必须与 `coffee-sshd` 一起发布（同一镜像，`COFFEE_TUI_BIN`）。
- 回退路线保留：若第二阶段复测超出阈值，先考虑进程内多 `AppView`，再考虑 `ForceCommand`。
