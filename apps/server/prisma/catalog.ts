// The eight beans of the design (`P` in docs/design/raw/canvas/Main.dc.html), in display
// order. `seedCatalog` inserts the missing ones and leaves existing rows (and their stock) alone.

import type { Weight } from '@coffee/contract'
import type { PrismaClient } from '../src/db.ts'
import { FlavorSchema, WeightSchema } from '@coffee/contract'
import { z } from 'zod'
import { toJsonColumn } from '../src/services/json-columns.ts'

export interface CatalogProduct {
  id: string
  code: string
  name: string
  subtitle: string
  origin: string
  region: string
  farm: string
  altitude: string
  variety: string
  process: string
  harvest: string
  roast: number
  tag: string | null
  notes: string[]
  flavor: number[]
  score: number
  basePrice: number
  stock: number
  brew: string
  description: string
  soldOutWeights: Weight[]
}

export const CATALOG: readonly CatalogProduct[] = [
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
    basePrice: 98,
    stock: 42,
    brew: '1:15 · 92°C · 2′30″',
    description: '高海拔小农水洗批次，干香是一把新鲜茉莉。入口柠檬与佛手柑的酸质明亮干净，冷却后浮出红茶般的回甘，适合手冲慢慢喝。',
    soldOutWeights: [],
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
    basePrice: 118,
    stock: 26,
    brew: '1:15 · 91°C · 2′20″',
    description: '粉红波旁的甜感是它最大的特点。荔枝与蔓越莓的果汁感包裹着蜂蜜甜，收尾带一点干净的可可，冷热都好喝。',
    soldOutWeights: ['1kg'],
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
    basePrice: 108,
    stock: 18,
    brew: '1:16 · 93°C · 2′40″',
    description: '典型的肯尼亚明亮酸质：黑醋栗与西柚汁的多汁感，带一点番茄般的鲜咸。中段红糖甜感撑起结构，做冰手冲也很出彩。',
    soldOutWeights: [],
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
    basePrice: 268,
    stock: 6,
    brew: '1:16 · 92°C · 2′15″',
    description: '每批只烘几公斤的限量瑰夏。花香极为饱满，白桃与柑橘层层展开，口感轻盈如茶，余韵是长长的蜂蜜甜。',
    soldOutWeights: ['500g', '1kg'],
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
    basePrice: 68,
    stock: 64,
    brew: '1:15 · 90°C · 2′30″',
    description: '来自干热河谷的日晒批次。红枣和莓果的发酵甜感很讨喜，中烘之后黑巧与焦糖撑起厚度，加奶也不违和。',
    soldOutWeights: [],
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
    basePrice: 78,
    stock: 30,
    brew: '1:14 · 89°C · 2′40″',
    description: '火山土壤带来的厚实质感。黑巧与烤坚果为主调，带一丝温和的烟熏，酸度低、口感圆润，适合法压或摩卡壶。',
    soldOutWeights: [],
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
    basePrice: 72,
    stock: 40,
    brew: '1:14 · 88°C · 3′00″',
    description: '湿刨法造就的厚重醇度：草本、雪松与黑巧克力，糖浆般的口感久久不散。深烘下苦而不焦，是重口味爱好者的老朋友。',
    soldOutWeights: [],
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
    basePrice: 88,
    stock: 120,
    brew: '18g → 36g · 28s',
    description: '为深夜写代码的人调的拼配。巴西打底给出榛子与可可，哥伦比亚补甜，一点埃塞带来尾段果香，做拿铁奶感很足。',
    soldOutWeights: [],
  },
]

const Notes = z.array(z.string().min(1))
const SoldOut = z.array(WeightSchema)

/** Inserts the catalog products that are missing; returns how many were created. */
export async function seedCatalog(prisma: PrismaClient): Promise<number> {
  let created = 0
  for (const [position, product] of CATALOG.entries()) {
    const { notes, flavor, soldOutWeights, ...columns } = product
    const data = {
      ...columns,
      position,
      notes: toJsonColumn(Notes, notes),
      flavor: toJsonColumn(FlavorSchema, flavor),
      soldOutWeights: toJsonColumn(SoldOut, soldOutWeights),
    }
    const existing = await prisma.product.findUnique({
      where: { id: product.id },
      select: { id: true },
    })
    if (existing)
      continue
    await prisma.product.create({ data })
    created++
  }
  return created
}
