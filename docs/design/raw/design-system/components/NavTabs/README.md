# NavTabs

顶栏主导航：四个固定宽 116px 的标签，每个前面是数字键帽，按 `1`–`4` 直接跳转。

## 规则
- 标签：56px 高（`h-header`），14px；当前页 `fg` 700，其余 `mute` 400；`aria-current="page"`。
- 下划线 `.t-nav__bar`：116×2px `acc`，贴在顶栏下边线上（bottom:-1px），切换时 `translateX(index × 116px)` 滑过去，280ms cubic-bezier(.2,.8,.2,1)。
- 购物车标签后跟数量胶囊 `.t-count`（见 Tag）。
- 顺序固定：1 选购 · 2 购物车 · 3 地址本 · 4 我的。详情页属于“选购”。

## 消费方提供
标签列表 `{key, label, badge?}`、当前索引、跳转回调。
