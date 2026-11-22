# Panel

界面的基本容器，像 TUI 里带标题的框：标题直接“压”在上边框上，用 `bg` 底色把边框挖断。

## 结构
- `.t-panel`：`surface` 底、1px `line` 边框、`radius-8`，顶部内边距 ≥22px 给标题留位（常用 24px 20px 18px）。
- `.t-panel__legend`：`top:-10px; left:14px`，左右 8px 内边距，`bg` 底，`legend` 字样（12px/700/0.06em/20px）`acc` 色。文字写成“中文 ENGLISH”：`筛选 FILTER`、`规格 SKU`、`结算 CHECKOUT`；带计数时用“购物车 · 3 件”“商品列表 · 8 款 · 推荐”。成功回执的标题用 `.t-panel__legend--ok`（`acc2`）。
- `.t-panel--card`：页面中央的独立卡片（登录、回执、访客提示），`radius-10`、内边距 32px，标题 left 18px。

## 配套
- `.t-inset`：面板里的凹陷信息块，`bg` 底、`line` 边框、`radius-6`、12px 14px——收货地址、冲煮建议、统计格。
- `.t-heading`：小节标题，`── ` 开头，12px/700 `acc`，下距 8px：`── 豆子档案`、`── 风味指标`。
- `.t-rule-dashed`：价格合计前的 1px 虚线 `line`；回执里用 `--strong`（`line-strong`）。

## 消费方提供
标题文字、内容、面板在栅格中的位置。页面用 CSS grid 排面板，间距 `space-14`（如列表页 `220px 1fr 340px`）。

## 不要
- 不要给面板加阴影或左侧色条；层级只靠边框和 `bg`/`surface` 两级底色。
