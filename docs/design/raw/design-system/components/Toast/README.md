# Toast

右下角的短暂通知（窗口内 right 20px、bottom 46px，即状态栏上方），360px 宽。

## 结构
- `.t-toast`：`surface` 底、`radius-8`、14px 16px、`shadow-toast`；左边 22px 圆形图标（`ink` 字 800 12px），右边两行：标题 700 `fg` + 12px `mute` 副标题（单行省略）。
- 三种：
  - 成功（默认）：`✓`，图标 `acc2`，边框 `acc2-line`。
  - 提示 `--info`：`i`，图标 `acc`，边框 `dim`。
  - 警告 `--warn`：`!`，图标 `danger`，边框 `danger-line`。
- 进场：opacity 220ms + transform 320ms cubic-bezier(.2,.9,.25,1.15)（带一点回弹）。

## 文案
标题说发生了什么（4–8 字），副标题给细节或下一步按键：“已加入购物车 / 耶加雪菲 孔加 · 250g · 手冲 × 1”，“请先选择收货地址 / 在地址本里按 ⏎ 指定”。

## 消费方提供
`text`、`sub`、`kind`（ok / info / warn）；同时只显示一条，新的替换旧的。`role="status"`。
