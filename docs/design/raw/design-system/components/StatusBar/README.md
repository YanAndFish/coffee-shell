# StatusBar

窗口底部 30px（`h-statusbar`）的状态栏，像 vim/tmux 的模式行：左边是当前模式，中间是这一页可用的快捷键，右边是全局状态。

## 消费方提供
- `mode`：中文名 + 英文大写，如“选购 BROWSE”“详情 DETAIL”“购物车 CART”“地址本 ADDRESS”“我的 PROFILE”“登录 LOGIN”。放进 `.t-status__mode`（`acc` 底、`ink` 字、800，英文部分 500）。
- `hints`：本页 5–7 条 `{key, text}`，每条是一个 `.t-hint` 按钮（扁平键帽 `.t-kbd--flat` + 12px `mute` 文字）。**提示必须可点击**，点击等同按键。
- 右侧：鼠标追踪状态（`●` 开为 `acc2`，关为 `mute`）、购物车金额（数字用 `fg`）、`?` 帮助。

## 规则
- 背景 `surface`，上边线 `line`，12px。
- 切换页面时只替换提示内容，模式块宽度随文字变化。
