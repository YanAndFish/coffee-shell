# Dialog

模态对话框（新增 / 编辑地址、快捷键帮助）。

## 结构
- 遮罩 `.t-scrim`：`scrim`，从标题栏下方（top 34px）铺满窗口，淡入 200ms。
- 面板 `.t-dialog`：560px（帮助 900px），`surface` 底、`dim` 边框（比普通面板亮一级）、`radius-10`、32px 28px 22px、`shadow-dialog`——这是界面里唯一真正有阴影的东西。
- 标题同 Panel，压在上边框（left 18px）。
- 底部右对齐两个 44px 按钮：`Esc 取消`（默认）+ `⏎ 保存`（主）。
- 出现：opacity 220ms + transform 320ms cubic-bezier(.2,.9,.25,1.15)。

## 快捷键帮助
三列，每列 `── 分组名`（`acc` 12px/700），条目是 64px 宽键帽 `.t-kbd--wide` + `soft` 说明；底部一条 `line` 分隔，左边说明鼠标操作，右边 `Esc 关闭`。

## 消费方提供
标题、表单内容、`aria-label`、取消 / 保存回调；打开时把焦点放到第一个输入框，Esc 关闭。
