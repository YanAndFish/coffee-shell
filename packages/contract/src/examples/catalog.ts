// The eight seeded beans (docs/design/data.md), as the API returns them. Prices are the
// table of data.md#单价表 (standard bag; gift +¥20); the server tests recompute them.
import type { PriceCell, ProductDetail, ProductList, ProductSummary } from '../catalog.ts'
import type { Weight } from '../sku.ts'
import { PACKS, WEIGHTS } from '../sku.ts'

interface Bean extends ProductSummary {
  code: string
  farm: string
  altitude: string
  variety: string
  harvest: string
  brew: string
  soldOut: Weight[]
  /** Standard-bag unit price of 100g, 250g, 500g, 1kg (data.md). */
  unit: [number, number, number, number]
}

const BEANS: Bean[] = [
  {
    id: 'yirga',
    code: 'YIR',
    name: '耶加雪菲 孔加',
    subtitle: 'Ethiopia · Yirgacheffe Konga G1',
    origin: '埃塞俄比亚',
    region: '耶加雪菲 · 孔加',
    farm: '孔加合作社',
    altitude: '1,950–2,200 m',
    variety: '埃塞原生种',
    process: '水洗',
    harvest: '2025/26',
    roast: 1,
    tag: '新产季',
    notes: ['茉莉', '柠檬', '佛手柑', '红茶'],
    flavor: [8, 7, 4, 2, 7, 9],
    score: 87.5,
    description: '高海拔小农水洗批次，干香是一把新鲜茉莉。入口柠檬与佛手柑的酸质明亮干净，冷却后浮出红茶般的回甘，适合手冲慢慢喝。',
    price250: 98,
    stock: 42,
    defaultWeight: '250g',
    brew: '1:15 · 92°C · 2′30″',
    soldOut: [],
    unit: [44, 98, 186, 353],
  },
  {
    id: 'pinkb',
    code: 'PNK',
    name: '慧兰 粉红波旁',
    subtitle: 'Colombia · Huila Pink Bourbon',
    origin: '哥伦比亚',
    region: '慧兰 · 皮塔利托',
    farm: '小农合作批次',
    altitude: '1,700–1,850 m',
    variety: '粉红波旁',
    process: '水洗',
    harvest: '2025/26',
    roast: 2,
    tag: '店长推荐',
    notes: ['荔枝', '蔓越莓', '蜂蜜', '可可'],
    flavor: [7, 8, 5, 2, 8, 8],
    score: 88,
    description: '粉红波旁的甜感是它最大的特点。荔枝与蔓越莓的果汁感包裹着蜂蜜甜，收尾带一点干净的可可，冷热都好喝。',
    price250: 118,
    stock: 26,
    defaultWeight: '250g',
    brew: '1:15 · 91°C · 2′20″',
    soldOut: ['1kg'],
    unit: [53, 118, 224, 425],
  },
  {
    id: 'nyeri',
    code: 'NYR',
    name: '涅里 AA 双洗',
    subtitle: 'Kenya · Nyeri AA Double Washed',
    origin: '肯尼亚',
    region: '涅里 · 卡拉蒂纳',
    farm: '处理站批次',
    altitude: '1,700–1,900 m',
    variety: 'SL28 / SL34',
    process: '双重水洗',
    harvest: '2025',
    roast: 2,
    tag: null,
    notes: ['黑醋栗', '西柚', '番茄', '红糖'],
    flavor: [9, 6, 6, 3, 7, 7],
    score: 87,
    description: '典型的肯尼亚明亮酸质：黑醋栗与西柚汁的多汁感，带一点番茄般的鲜咸。中段红糖甜感撑起结构，做冰手冲也很出彩。',
    price250: 108,
    stock: 18,
    defaultWeight: '250g',
    brew: '1:16 · 93°C · 2′40″',
    soldOut: [],
    unit: [49, 108, 205, 389],
  },
  {
    id: 'geisha',
    code: 'GSH',
    name: '波奎特 瑰夏',
    subtitle: 'Panama · Boquete Geisha Washed',
    origin: '巴拿马',
    region: '奇里基 · 波奎特',
    farm: '精品庄园批次',
    altitude: '1,600–1,800 m',
    variety: '瑰夏 Geisha',
    process: '水洗',
    harvest: '2025/26',
    roast: 1,
    tag: '限量',
    notes: ['茉莉', '白桃', '柑橘', '蜂蜜'],
    flavor: [7, 8, 3, 1, 9, 10],
    score: 90,
    description: '每批只烘几公斤的限量瑰夏。花香极为饱满，白桃与柑橘层层展开，口感轻盈如茶，余韵是长长的蜂蜜甜。',
    price250: 268,
    stock: 6,
    defaultWeight: '250g',
    brew: '1:16 · 92°C · 2′15″',
    soldOut: ['500g', '1kg'],
    unit: [121, 268, 509, 965],
  },
  {
    id: 'yunnan',
    code: 'YUN',
    name: '保山 小粒 日晒',
    subtitle: 'China · Yunnan Baoshan Natural',
    origin: '中国云南',
    region: '保山 · 潞江坝',
    farm: '本地庄园',
    altitude: '1,100–1,400 m',
    variety: '卡蒂姆',
    process: '日晒',
    harvest: '2025/26',
    roast: 3,
    tag: null,
    notes: ['红枣', '黑巧', '莓果', '焦糖'],
    flavor: [5, 7, 6, 4, 6, 6],
    score: 85,
    description: '来自干热河谷的日晒批次。红枣和莓果的发酵甜感很讨喜，中烘之后黑巧与焦糖撑起厚度，加奶也不违和。',
    price250: 68,
    stock: 64,
    defaultWeight: '250g',
    brew: '1:15 · 90°C · 2′30″',
    soldOut: [],
    unit: [31, 68, 129, 245],
  },
  {
    id: 'antigua',
    code: 'ATG',
    name: '安提瓜 火山',
    subtitle: 'Guatemala · Antigua',
    origin: '危地马拉',
    region: '安提瓜谷地',
    farm: '庄园批次',
    altitude: '1,500–1,700 m',
    variety: '波旁 / 卡杜拉',
    process: '水洗',
    harvest: '2025',
    roast: 4,
    tag: null,
    notes: ['黑巧', '坚果', '烟熏', '焦糖'],
    flavor: [4, 6, 7, 5, 6, 6],
    score: 84.5,
    description: '火山土壤带来的厚实质感。黑巧与烤坚果为主调，带一丝温和的烟熏，酸度低、口感圆润，适合法压或摩卡壶。',
    price250: 78,
    stock: 30,
    defaultWeight: '250g',
    brew: '1:14 · 89°C · 2′40″',
    soldOut: [],
    unit: [35, 78, 148, 281],
  },
  {
    id: 'mandh',
    code: 'MDH',
    name: '曼特宁 G1',
    subtitle: 'Indonesia · Sumatra Mandheling',
    origin: '印尼',
    region: '苏门答腊 · 多巴湖',
    farm: '小农批次',
    altitude: '1,200–1,500 m',
    variety: '混合品种',
    process: '湿刨',
    harvest: '2025',
    roast: 5,
    tag: null,
    notes: ['草本', '黑巧', '雪松', '糖浆'],
    flavor: [2, 5, 9, 7, 8, 5],
    score: 84,
    description: '湿刨法造就的厚重醇度：草本、雪松与黑巧克力，糖浆般的口感久久不散。深烘下苦而不焦，是重口味爱好者的老朋友。',
    price250: 72,
    stock: 40,
    defaultWeight: '250g',
    brew: '1:14 · 88°C · 3′00″',
    soldOut: [],
    unit: [32, 72, 137, 259],
  },
  {
    id: 'night',
    code: 'NGT',
    name: '「夜班」意式拼配',
    subtitle: 'House Espresso Blend',
    origin: '拼配',
    region: '巴西 × 哥伦比亚 × 埃塞',
    farm: 'ROAST/TTY 烘焙室',
    altitude: '—',
    variety: '多产区拼配',
    process: '日晒 + 水洗',
    harvest: '季节轮换',
    roast: 4,
    tag: '招牌',
    notes: ['可可', '榛子', '焦糖', '太妃'],
    flavor: [4, 8, 7, 4, 7, 6],
    score: 85.5,
    description: '为深夜写代码的人调的拼配。巴西打底给出榛子与可可，哥伦比亚补甜，一点埃塞带来尾段果香，做拿铁奶感很足。',
    price250: 88,
    stock: 120,
    defaultWeight: '250g',
    brew: '18g → 36g · 28s',
    soldOut: [],
    unit: [40, 88, 167, 317],
  },
]

function bean(id: string): Bean {
  const found = BEANS.find(b => b.id === id)
  if (!found)
    throw new Error(`no bean ${id}`)
  return found
}

function summary(b: Bean): ProductSummary {
  return {
    id: b.id,
    name: b.name,
    subtitle: b.subtitle,
    origin: b.origin,
    region: b.region,
    process: b.process,
    roast: b.roast,
    tag: b.tag,
    notes: b.notes,
    flavor: b.flavor,
    score: b.score,
    description: b.description,
    price250: b.price250,
    stock: b.stock,
    defaultWeight: b.defaultWeight,
  }
}

/** Standard-bag unit price of `id` at `weight` (data.md#单价表). */
export function unitPriceOf(id: string, weight: Weight, gift = false): number {
  return bean(id).unit[WEIGHTS.indexOf(weight)]! + (gift ? 20 : 0)
}

function detail(id: string, stock?: number): ProductDetail {
  const b = {
    ...bean(id),
    ...(stock === undefined
      ? {}
      : { stock }),
  }
  const maxQuantity = Math.min(9, b.stock)
  const prices: PriceCell[] = WEIGHTS.flatMap(weight => PACKS.map((pack) => {
    const unitPrice = unitPriceOf(id, weight, pack === 'GIFT')
    return {
      weight,
      pack,
      unitPrice,
      totals: Array.from({ length: maxQuantity }, (_, i) => unitPrice * (i + 1)),
    }
  }))
  return {
    ...summary(b),
    code: b.code,
    farm: b.farm,
    altitude: b.altitude,
    variety: b.variety,
    harvest: b.harvest,
    brew: b.brew,
    maxQuantity,
    weights: WEIGHTS.map(weight => ({ weight, available: !b.soldOut.includes(weight) })),
    prices,
  }
}

const facets: ProductList['facets'] = {
  total: 8,
  origins: BEANS.map(b => ({ origin: b.origin, count: 1 })),
}

/** `fixtures/v1/products.json`: no filters, recommended order. */
export const productListExample: ProductList = {
  items: BEANS.map(summary),
  facets,
}

/** `GET /v1/products?roast=light&sort=price_asc`: levels 1–2 by 250 g price. */
export const productListLightByPriceExample: ProductList = {
  items: ['yirga', 'nyeri', 'pinkb', 'geisha'].map(id => summary(bean(id))),
  facets,
}

/** `GET /v1/products?q=茉莉&sort=score`. */
export const productListSearchExample: ProductList = {
  items: ['geisha', 'yirga'].map(id => summary(bean(id))),
  facets,
}

/** `GET /v1/products?q=xyz`: no match (the facets still count everything). */
export const productListEmptyExample: ProductList = {
  items: [],
  facets,
}

/** `GET /v1/products/yirga`: every weight in stock. */
export const productDetailYirgaExample: ProductDetail = detail('yirga')

/** `GET /v1/products/pinkb`: 1kg sold out. */
export const productDetailPinkbExample: ProductDetail = detail('pinkb')

/** `GET /v1/products/nyeri`. */
export const productDetailNyeriExample: ProductDetail = detail('nyeri')

/** `GET /v1/products/geisha`: 6 left (`仅剩 6 份`), 500g and 1kg sold out. */
export const productDetailGeishaExample: ProductDetail = detail('geisha')

/** `GET /v1/products/yunnan`. */
export const productDetailYunnanExample: ProductDetail = detail('yunnan')

/** `GET /v1/products/antigua`. */
export const productDetailAntiguaExample: ProductDetail = detail('antigua')

/** `GET /v1/products/mandh`. */
export const productDetailMandhExample: ProductDetail = detail('mandh')

/** `GET /v1/products/night`: 120 in stock, the stepper still stops at 9. */
export const productDetailNightExample: ProductDetail = detail('night')

/**
 * `GET /v1/products/nyeri` once its stock ran out (`暂时缺货`): the stepper's bound is 0 and
 * the price cells have no totals; every weight is still "available" (sold-out weights are a
 * separate list).
 */
export const productDetailNoStockExample: ProductDetail = detail('nyeri', 0)

/**
 * The detail of every seeded bean, in display order: `fixtures/v1/catalog/product-<id>.json`
 * (each equal to what the server answers for the seeded catalog).
 */
export const productDetailExamples: readonly ProductDetail[] = [
  productDetailYirgaExample,
  productDetailPinkbExample,
  productDetailNyeriExample,
  productDetailGeishaExample,
  productDetailYunnanExample,
  productDetailAntiguaExample,
  productDetailMandhExample,
  productDetailNightExample,
]

/** A product as a cart line or order item needs it. */
export function beanFacts(id: string): { name: string, origin: string, roast: number, code: string, stock: number } {
  const b = bean(id)
  return {
    name: b.name,
    origin: b.origin,
    roast: b.roast,
    code: b.code,
    stock: b.stock,
  }
}
