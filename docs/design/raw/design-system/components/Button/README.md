# Button

按钮的文字永远以它的快捷键开头：`⏎ 查看详情`、`a 加入购物车`、`Esc 取消`、`d 删除`。鼠标和键盘是同一套操作。

## 变体
| 类 | 用途 | 样式 |
|---|---|---|
| `.t-btn--primary` | 每屏唯一的主操作 | `acc` 底、`ink` 字、800，无边框 |
| `.t-btn--outline` | 与主操作并列的第二操作（立即购买） | `acc` 边框与文字、700 |
| `.t-btn`（默认） | 次级操作 | 透明底、`line` 边框、`fg` 字 |
| `.t-btn--danger` | 退出登录这类破坏性操作 | `danger-line` 边框、`danger` 字 |
| `.t-btn--undo` | 删除后的撤销条 | 虚线 `danger-line`、12px |
| `.t-btn--green` | 按口味推荐的商品跳转 | `acc2-line` 边框、`acc2` 字、36px |
| `.t-btn--link` | 面板内的文字链接（“3 更换 →”） | 无边框、`acc` 字 |
| `.t-btn--add` | 网格末尾的“新增”占位卡 | 虚线 `line-strong`、176px 高、`acc` 的大“+” |

## 尺寸
`--lg` 48px（提交，15px/800；结算按钮 50px），默认 44px，`h-36` 用于空状态与推荐，`--sm` 32px（卡片内操作，`radius-5`，12px），`--xs` 28px（返回）。默认圆角 `radius-6`。

## 消费方提供
文字（以快捷键开头）、点击回调；按钮内可嵌 Kbd：在主按钮里用 `.t-kbd--on-acc`。

## 状态
- 悬停：`filter: brightness(1.22)`，不换颜色。
- 焦点：1px `acc` 虚线外框，偏移 2px。
- 禁用：文字改 `disabled`，光标 not-allowed。
- 提交中：按钮被同高的进度条替换（见 Meter）。

## 不要
- 不要一屏放两个 `primary`。
- 不要用图标代替快捷键前缀。
