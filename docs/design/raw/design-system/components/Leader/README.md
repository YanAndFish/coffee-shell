# Leader

键 ······ 值：像目录或小票那样用点线把左右两端连起来。用于豆子档案和订单回执。

## 结构
`.t-leader`（flex，baseline 对齐，间距 8px，行高 27px，13px）：
- `.t-leader__k`：键，`mute`，不收缩。回执里商品名改为 `fg` 700，后面再跟一个 `mute` 的规格“250g · 手冲 · 标准袋 × 1”。
- `.t-leader__dots`：弹性填充，1px dotted `line-strong` 下边线，上移 4px 贴近文字基线；`aria-hidden`。
- `.t-leader__v`：值，`fg`，不收缩；金额带两位小数。

## 配套
上方用 `.t-heading`（“── 豆子档案”）；回执的分组之间用 `line-strong` 虚线（`.t-rule-dashed--strong`），最后一行是“实付”+ 28px/800 `acc` 金额。

## 消费方提供
`{k, v}` 列表；栏宽由外层决定（详情页档案栏 250px）。
