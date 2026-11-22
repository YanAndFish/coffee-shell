# ADR 0002：第二阶段的架构决定

- 状态：已采纳（第二阶段落地后补记）
- 日期：2026-11-22
- 范围：`packages/contract`、`crates/coffee-api`、`apps/server`、`apps/tui`、`crates/coffee-host`、`scripts/e2e`

第二阶段把第一阶段的 SSH 外壳（[ADR 0001](0001-ssh-session-model.md)）做成了一家能注册、加购、结算的演示店。
产品规则（U1–U7）与接口层的细则（P-…）记在 [`docs/design/decisions.md`](../design/decisions.md)；本篇只记
**跨层的架构决定**：它们决定代码放在哪里、谁对什么负责，改动其中任何一条都会牵动多个目录。

## 1. 契约先行，夹具是跨语言的唯一事实

**决定**：`/v1` 接口只在一处定义——`packages/contract` 的 zod schema 与 `defineEndpoint`。它生成
`openapi.json` 和 `fixtures/v1/**/*.json`（两者都提交；`check:generated` 在过期时失败），各方都对这些
生成物测试，而不是各自描述接口：

| 一方                 | 怎么用契约                                                                                                                              |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/server`        | 路由是一行 `route(router, deps, endpoint, handler)`：鉴权、请求头、参数与请求体校验都由端点定义完成；测试里再用成功 schema 校验每个回答 |
| `crates/coffee-api`  | 手写的 serde DTO；测试把 `fixtures/v1` 里**每一个**文件往返一遍，没有映射的夹具直接失败                                                 |
| `apps/tui`           | 夹具传输（下文第 5 节）直接回放这些文件；`stores` 只认 `coffee_api` 的类型                                                              |
| `apps/server` 的测试 | `test/fixtures.test.ts` 用真实服务端重放夹具描述的场景，证明夹具就是服务端会给的回答                                                    |

错误码、HTTP 状态与唯一的中文文案在同一张表（`ERRORS`），TUI 只按 `code` 分支、按 `details` 拼句。

**为什么**：TypeScript 与 Rust 之间没有共享类型系统；手写两份接口说明必然漂移。让“契约生成的 JSON”
成为两种语言共同的测试数据，漂移就会在某一侧的测试里失败，而不是在线上。代价是改接口必须按固定顺序走
（契约 → 生成 → DTO → 服务端 → TUI，见 `.claude/skills/add-api-endpoint`），并提交生成物。

## 2. 金额规则只在服务端

**决定**：单价、优惠、运费、合计、免运费进度、详情页每个数量的小计（`PriceCell.totals`）全部由服务端
（`apps/server/src/services/pricing.ts`，整数元、整数运算的四舍五入）算出；接口里的金额都是整数元；
TUI 只格式化（`pages::format`），连“单价 × 数量”都不做。结算在一个事务里按当前基价重算并扣库存
（P-order-4），可带 `expectedTotal` 防止价格在用户眼前变化后成交（`409 price_changed`）。

**为什么**：同一规则写两遍（TS 与 Rust）迟早不一致，而用户看到的金额与实际成交额不一致是最糟的
错误。TUI 每次改动都要一次往返，但演示店的延迟在本机或同一主机内，可以接受。端到端测试
（`scripts/e2e` 的 `money` 流程）用**独立实现**的同一套规则检查界面上显示的数字，而不是复用服务端代码。

## 3. 账号在应用里，SSH 公钥只用于“记住我”

**决定**：SSH 层不认证任何人（ADR 0001：任意公钥或无公钥都能进来）；账号是应用内的用户名 + 密码
（argon2id），会话令牌只在 `coffee-tui` 进程内存里。`coffee-sshd` 把验证过的公钥指纹作为
`COFFEE_SSH_FINGERPRINT` 交给会话，TUI 以 `X-SSH-Fingerprint` 转发；后端只因为同时要求内部密钥
（`X-Internal-Key`）才信任这个头。勾选“记住我”的**密码**登录把指纹绑定到账号 30 天，之后同一公钥的新连接
直接恢复登录；退出登录（需要确认）删除绑定；没有公钥的连接不能记住任何东西。

**为什么**：要求 SSH 公钥认证会把没有密钥或不想交出密钥的访客挡在门外；只靠公钥又无法在别的机器上
登录同一个账号。两者结合：门永远开着，账号跨机器可用，常用的机器免密。威胁模型是演示：密码可以
随便取，界面始终提醒不要用真实密码（U2、U3、P-auth-1…6）。

## 4. 颜色经“载体”送达，宿主再钉回 token

**决定**：nsh 0.6.1 在终端里只画 ANSI 语义（黄、绿、红、默认前景 + DIM…），画不出设计稿的颜色值。
每个颜色 token 因此搭一个能原样穿过 nsh 映射的**载体**（ANSI 通道，可能带斜体标记；底色用正好落在
xterm 色立方上的 `carrier-*` 颜色），`coffee-host` 在每帧渲染后、diff 之前把载体钉回 token
（`crates/coffee-host/src/palette.rs`），真彩下精确、256 色下取最近的立方格、16 色与单色下交给终端主题与
字形。载体表只有一处：`apps/tui/src/ui/theme.rs`（`INKS`、`FILLS`），钉色从它与 `theme.json` 构建。
页面只能经 `ui::label` / `strong` / `toned` 或带 `Tone` / `Fill` 的组件着色。色深的判断（`COFFEE_COLORS`、
`COLORTERM`、终端品牌、XTGETTCAP）见 ADR 0001。

**为什么**：改 nsh 不在范围内；`TerminalCapabilities` 的钩子会把颜色混成别的值（ADR 0001“调色板钉色结果”）。
载体 + 钉色让设计稿的颜色在真彩终端上逐位准确，又不丢掉 16 色与单色下的可读性（状态始终是字形 + 属性，
不只靠颜色）。代价：新颜色必须先在载体表里有一行；nsh 升级若改了映射，`crates/coffee-host/tests/pin_render.rs` 会失败。

## 5. 夹具传输：没有后端也能开发界面

**决定**：`COFFEE_API_URL=fixture:<场景>`（或存储键 `coffee.transport`）让 `coffee-tui` 不连网络，
直接用契约夹具回答（`apps/tui/src/api/fixtures.rs` 的 `FixtureSet`：`guest`、`member`、`empty`、`new`、
`problem`、`expired`、`member-error`、`slow`、`down`、`error`），页眉显示 `◇ 样例数据`。回答是静态的：
改动会得到对应的夹具回答，但不被记住。`COFFEE_UI_GALLERY=1` 另有一个组件画廊。

**为什么**：界面工作者不需要起后端、数据库与 sshd；界面测试（`nsh test`）不依赖网络与时间；每个错误与
边界状态（缺货、过期、500、断网、慢响应）都能稳定复现。夹具来自契约，所以界面看到的和真实服务端
回答同构（第 1 节）。真实链路由 `pnpm test:e2e:real` 覆盖（第 7 节）。

## 6. 每个画面一个测试文件

**决定**：界面测试按画面分文件：`apps/tui/<Name>.test.tsx`（`Login`、`Catalog`、`Detail`、`Cart`、
`Addresses`、`Profile`；`App.test.tsx` 是外壳，`Gallery.test.tsx` 是组件），共享 `apps/tui/testkit.ts`
（`start`、`enterAsGuest`、`logInAsMember`、`startOnClock` + `runUntil`）。每个文件必须以桌面尺寸打开应用
（`platform: 'linux'`，`width: cols × 8`，`height: rows × 16`）：无头默认是手机，快捷键不会触发。
vitest 不收这些文件（`vitest.config.ts`）。

**为什么**：并行的画面工作者各自拥有自己的页面目录与测试文件，不会在同一个大文件里冲突；一个画面的
回归只需要跑它自己的文件。代价：外壳与跨画面流程（登录 → 合并 → 结算）在单个画面测试里只能部分覆盖，
这由第 7 节的端到端流程补上。

## 7. 端到端：真实链路、读屏驱动

**决定**：`scripts/e2e`（Python 标准库，复用 `scripts/pty-run.py` 的终端模拟器）对**真实**后端（每个流程一份
新迁移、新种子的 SQLite 副本）、`coffee-sshd` 与系统自带的 `ssh` 客户端跑完整流程：`main`（注册、筛选搜索、
访客加购被结算拦下、登录合并、地址校验、优惠码、结算回执、我的订单、公钥恢复、确认退出）、`money`、
`stock`（缺货、库存上限、两人抢最后几件、错误码、登录限流）、`session`（无公钥、结算中途断开、后端重启、
缩放）、`gateway`（连接时后端不在、会话程序缺失）。每一步都**等屏幕上出现的东西**，焦点与选择按键直到屏幕
确认到位为止（有上限），从不按固定次数或固定时长；超时就带着最后一屏失败。每个流程结束时检查没有遗留的
`coffee-tui`、日志里没有密码、令牌与内部密钥。`pnpm test:e2e:real` 本地约 2 分钟，CI 的 `e2e` 任务在 Linux 上
跑同一套；`deploy/smoke-test.sh` 用其中的 `smoke` 流程对 compose 起的两个镜像买一单。

**为什么**：契约与夹具证明各层对接口的理解一致，但证明不了“一个人用原版 ssh 真的能买到东西”。读屏驱动
（而不是坐标或固定按键次数）让界面的布局与焦点细节可以继续改进，而测试只在行为真的变了时失败。

## 后果

- 接口改动的成本集中在契约一侧（生成、提交、四处测试），换来跨语言不漂移。
- TUI 不能离线算价：后端不可用时，购物车与详情显示错误状态与重试，而不是旧数字。
- 安全边界在内部密钥：泄露它等于可以伪造任意公钥指纹（记住我）与客户端地址（限流）。它只在 sshd 与后端
  的环境里，从不记日志（`scripts/e2e` 每个流程都检查）。
- 颜色、夹具、每画面一测与端到端驱动都假定 nsh 0.6.1 的行为；升级 nsh 时先跑 `pin_render.rs`、
  `pnpm test:tui` 与 `pnpm test:e2e:real`。
