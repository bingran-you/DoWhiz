---
name: grocery-comparison
description: Smart grocery price comparison agent for Asian international students. Compares prices across Weee, Asian markets (H Mart, 168, etc.), and mainstream stores (Kroger, Costco, Sam's Club, Aldi). Considers user taste preferences, distance, and cultural factors. Use when user asks to compare grocery prices, find best deals, or get shopping recommendations.
---

# Smart Grocery Comparison Agent

This skill helps users (primarily Asian international students) find the best value groceries by comparing prices across multiple channels while considering personal preferences.

## Core Principles

1. **Not just cheapest, but best value for YOU** - Consider taste, distance, delivery options
2. **Cultural awareness** - "American cakes are too sweet" is a valid preference
3. **Multi-channel** - Online (Weee, Yami) + Physical stores (H Mart, 168, Costco, Kroger)
4. **Explain trade-offs** - "$2 cheaper at 168 but 25 min drive vs Weee delivers free"

---

## Data Source Availability Status (Tested 2026-04-02)

| 数据源 | 状态 | 抓取方法 | 备注 |
|--------|------|----------|------|
| **Weee** | **可用** | browser-use | 成功抓取产品名称、价格、单位 |
| **Google Maps** | **可用** | browser-use | 成功获取店铺评分、评论、营业时间 |
| **H Mart** | 部分可用 | browser-use | 主页可访问，搜索功能需进一步测试 |
| **Yami** | 待测试 | browser-use | 应该可用，类似Weee |
| **小红书** | 被限制 | browser-use | 非中国IP被block，需VPN/代理 |
| **Kroger** | **可用** | 官方API | 已配置 OAuth2，支持产品搜索+店铺查询+价格 |
| **Costco** | 被限制 | 需登录+cookie | headless browser被block，需会员登录 |
| **Sam's Club** | 被限制 | 需登录+cookie | 有人机验证，需会员登录后导出cookie |

**推荐抓取策略：**
1. **首选**: Weee + Google Maps (可直接browser-use)
2. **需API**: Kroger (申请developer API key)
3. **需特殊处理**: Costco (用户提供cookie)、小红书 (中国代理)

---

## UI Questionnaire Component

问卷 UI 组件位于 `website/src/components/intake/GroceryPreferencesQuestionnaire.jsx`

**功能特性：**
- 6步向导式流程
- 单选/多选按钮组
- 滑块控件（甜度、辣度）
- 拖拽排序（优先级）
- 响应式设计

**使用方式：**
```jsx
import GroceryPreferencesQuestionnaire from './components/intake/GroceryPreferencesQuestionnaire';

<GroceryPreferencesQuestionnaire
  onComplete={(formData) => {
    // 保存到用户memory
    console.log('User preferences:', formData);
  }}
  onCancel={() => {
    // 取消问卷
  }}
  initialData={{}} // 可传入已有偏好数据
/>
```

---

## Quick Start: Check User Preferences First

**ALWAYS check user preferences before giving grocery recommendations:**

```bash
# Get user's saved preferences (zip code, stores, dietary restrictions, etc.)
grocery_cli preferences get <user_id>
```

If no preferences found, direct user to the onboarding questionnaire:
- Web UI: `/grocery/onboarding?user_id=<user_id>`
- Or run the conversational onboarding below

---

## User Onboarding Questionnaire

When a user first asks for grocery help and has no preference data in memory, run through this onboarding flow. This can be done conversationally (via email/chat) or through a UI questionnaire.

### Onboarding Flow (Conversational)

**Step 1: Basic Profile**
```
你好！我是你的智能买菜助手。为了给你更好的推荐，我想先了解一下你的情况：

1. 你的文化背景是？
   - 中国大陆
   - 台湾/港澳
   - ABC/华裔美国人
   - 韩国/日本
   - 东南亚
   - 其他亚裔
   - 非亚裔但喜欢亚洲食品

2. 你住在哪个城市/zip code？
   (用于计算到各超市的距离)

3. 你一般是为几个人买菜？
   - 1人（自己）
   - 2人（情侣/室友）
   - 3-4人（小家庭）
   - 5人以上（大家庭）
```

**Step 2: Shopping Habits**
```
4. 你有车吗？最远愿意开多久去买菜？
   - 没有车，靠公共交通/走路
   - 有车，15分钟内
   - 有车，30分钟内
   - 有车，1小时内也可以

5. 你更喜欢？
   - 网购送货（Weee/Instacart）
   - 线下超市
   - 两者都可以

6. 你有这些会员卡吗？（多选）
   - [ ] Costco
   - [ ] Sam's Club
   - [ ] Kroger Plus Card
   - [ ] 其他: ___
```

**Step 3: Main Shopping Categories**
```
7. 你主要买哪些品类？（多选，我会针对你选的品类问更细的问题）
   - [ ] 肉类（猪/牛/羊/鸡）
   - [ ] 海鲜
   - [ ] 蔬菜
   - [ ] 水果
   - [ ] 零食饮料
   - [ ] 火锅/烧烤食材
   - [ ] 调味料/酱料
   - [ ] 速食/方便面
   - [ ] 奶制品/鸡蛋
   - [ ] 面包/烘焙
```

**Step 4: Category-Specific Preferences (根据Step 3动态生成)**

如果选了**肉类**：
```
关于肉类：
- 你更常买哪种肉？猪肉 / 牛肉 / 鸡肉 / 羊肉
- 有宗教/饮食限制吗？（如不吃猪肉）
- 你会自己处理生肉吗？还是更喜欢买切好的？
- 对肉的分量有要求吗？（例如：一次只买1-2lb，或者可以批量买冷冻）
```

如果选了**零食**：
```
关于零食：
- 口味偏好？咸口 / 甜口 / 辣口 / 都喜欢
- 有特别喜欢的品牌吗？（如旺旺、乐事、百草味等）
- 有特别排斥的品牌/口味吗？
- ABC/华裔：你习惯中式零食还是美式零食？
```

如果选了**蔬菜**：
```
关于蔬菜：
- 你常买哪些蔬菜？（中式特有的如韭菜、空心菜还是常见蔬菜）
- 对新鲜度要求高吗？（愿意为更新鲜的多跑一趟店吗）
- 有机蔬菜重要吗？
```

如果选了**火锅食材**：
```
关于火锅：
- 多久吃一次火锅？
- 喜欢什么锅底？（麻辣/清汤/番茄/菌菇）
- 喜欢的火锅品牌？（海底捞/小龙坎/德庄等）
```

**Step 5: Taste Profile**
```
8. 口味偏好：
- 甜度接受度？1-5（1=很淡，5=很甜）
  美式甜品对你来说通常：太甜 / 刚好 / 可以更甜
- 辣度接受度？1-5（1=不能吃辣，5=越辣越好）
- 咸度偏好：偏淡 / 正常 / 偏咸

9. 有什么特别排斥的食物/品牌吗？
   例如：Kraft芝士、美式糖果、某些调味品等

10. 有什么饮食限制吗？
   - 无
   - 素食/纯素
   - 不吃猪肉（宗教原因）
   - 乳糖不耐受
   - 麸质过敏
   - 其他过敏: ___
```

**Step 6: Budget & Priorities**
```
11. 买菜预算大概是？
   - 能省则省，价格最重要
   - 性价比优先，质量也要看
   - 质量优先，价格其次
   - 不太在意价格

12. 以下因素对你的重要程度排序：
   - 价格便宜
   - 产品新鲜/质量好
   - 距离近/方便
   - 选择多样
   - 有特定想买的品牌
```

### Onboarding Result → Memory Storage

完成问卷后，生成并保存到用户memory：

```markdown
## Grocery Preferences (Generated from Onboarding 2026-04-02)

### Profile
- background: chinese_mainland  # chinese_mainland/taiwan_hk/abc/korean/japanese/southeast_asian/other
- city: Ann Arbor, MI
- zip_code: 48109
- household_size: 2  # 为2人购物

### Transportation
- has_car: true
- max_drive_minutes: 30
- prefers: both  # online/offline/both

### Memberships
- costco: true
- sams_club: false
- kroger_plus: true

### Main Categories
- primary: [meat, vegetables, snacks, hotpot]
- secondary: [condiments, instant_noodles]

### Category Preferences

#### Meat
- preferred: [pork, chicken]  # 常买猪肉和鸡肉
- avoid: []  # 无忌口
- processing: prefer_precut  # 喜欢买切好的
- bulk_buy: no  # 不喜欢批量买

#### Snacks
- taste: salty  # 偏咸口
- favorite_brands: [旺旺, 乐事, 百草味]
- avoid_brands: [某品牌]
- style: chinese  # 中式零食为主

#### Vegetables
- types: [chinese_special, common]  # 买中式特色菜和普通蔬菜
- freshness_priority: high
- organic: not_important

#### Hotpot
- frequency: monthly
- base_flavor: [mala, tomato]
- favorite_brands: [海底捞, 小龙坎]

### Taste Profile
- sweetness_tolerance: 2  # 1-5，美式甜品太甜
- spice_tolerance: 4  # 能吃辣
- saltiness: normal

### Exclusions
- brands: [Kraft cheese, Hershey's]
- products: [American-style cakes, overly sweet desserts]
- dietary: []  # 无饮食限制

### Budget & Priorities
- budget_sensitivity: value_focused  # price_first/value_focused/quality_first/price_insensitive
- priority_order: [price, quality, convenience, variety]

### Notes
- 自动生成于 2026-04-02
- 用户可随时说"更新我的买菜偏好"来修改
```

### Quick Onboarding (Minimal Version)

如果用户不想回答那么多问题，提供简化版：

```
快速设置（3个问题）：

1. 你的zip code？
2. 有Costco会员卡吗？有/没有
3. 有什么不吃的吗？（留空表示无）

好的！我会根据你的位置推荐附近的超市。之后如果你想设置更详细的偏好，
随时说"完善我的买菜偏好"就可以继续。
```

---

## User Preferences Schema

Store user preferences in their memory file. Expected format:

```markdown
## Grocery Preferences

### Location
- zip_code: 48109
- address: Ann Arbor, MI

### Taste Preferences
- avoid: ["American-style sweets (too sweet)", "Kraft cheese"]
- prefer: ["Chinese-style pastries", "Asian brands"]
- dietary: ["no pork"] (optional)

### Shopping Habits
- has_car: true
- max_drive_time: 30 minutes
- preferred_stores: ["168 Asian Mart", "Weee", "Costco"]
- costco_membership: true
- sams_membership: false

### Known Baselines (user-reported prices)
Last updated: 2026-04-01
| Item | Store | Price | Unit | Notes |
|------|-------|-------|------|-------|
| 五花肉 Pork Belly | 168 Asian Mart | $3.99 | /lb | Fresh, good quality |
| 五花肉 Pork Belly | H Mart | $4.49 | /lb | |
| 老干妈 Lao Gan Ma | 168 Asian Mart | $3.29 | 280g | |
| 旺旺仙贝 Want Want | Weee | $4.99 | 472g | Often on sale |
```

## Data Sources (Priority Order)

### Tier 1: API/Automated (Most Reliable)

#### Kroger API (via grocery_cli)

**IMPORTANT**: Use `grocery_cli` for Kroger queries - it handles OAuth automatically.

```bash
# Search for products (with prices at nearest store)
grocery_cli kroger search "pork belly" --location 48109 --limit 5

# Find nearby Kroger stores
grocery_cli kroger stores 48109 --radius 15 --limit 5

# Get user's saved preferences
grocery_cli preferences get <user_id>
```

**Example output:**
```
Kroger Products for "pork belly":

1. Kroger® Fresh Natural Pork Belly Boneless
   Brand: Kroger
   Size: 1.5 lb   Price: $12.99

2. Smithfield Fresh Pork Belly
   Brand: Smithfield
   Size: 2 lb   Price: $15.49 (Sale: $12.99)
```

**Available data**: Product name, brand, price, sale price, size, store location

#### Weee (via browser-use)
```bash
# Weee doesn't have public API, use browser automation
IN_DOCKER=true browser-use open "https://www.sayweee.com/en/search?keyword=pork%20belly"
browser-use state  # Get product listings
browser-use screenshot /tmp/weee_search.png
```

**Parsing Weee results**: Look for price patterns like `$X.XX` and unit patterns like `/lb`, `/ea`, `/pack`

### Tier 2: Browser Scraping (Medium Reliability)

#### H Mart Online
```bash
IN_DOCKER=true browser-use open "https://www.hmart.com/search?q=pork+belly"
browser-use state
```

#### Yami (Yamibuy)
```bash
IN_DOCKER=true browser-use open "https://www.yamibuy.com/en/search?keywords=pork+belly"
browser-use state
```

#### Costco (Requires login)
```bash
# Check if user has saved Costco cookies
if [ -f ~/.costco_cookies.json ]; then
  IN_DOCKER=true browser-use cookies import ~/.costco_cookies.json
fi
IN_DOCKER=true browser-use open "https://www.costco.com/CatalogSearch?keyword=pork+belly"
browser-use state
```

### Tier 3: Social Media & Reviews Mining (For Local Asian Markets)

For stores without online presence (168 Asian Mart, Way1/Huaxing, local markets), use social media and review platforms to gather price intelligence:

#### Google Maps Reviews
```bash
# Search for store and read reviews mentioning prices
IN_DOCKER=true browser-use open "https://www.google.com/maps/search/168+Asian+Mart+Madison+Heights+MI"
browser-use state  # Find the store listing
browser-use click <store_listing_index>  # Click to open details
browser-use scroll down  # Scroll to reviews
browser-use state  # Look for reviews mentioning prices

# Search pattern in reviews:
# - "prices are good/cheap/expensive"
# - "$X.XX" patterns
# - "cheaper than H Mart"
# - Comparisons with other stores
```

**What to extract from Google Maps:**
- Overall price sentiment (cheap/moderate/expensive)
- Specific price mentions in reviews
- Comparisons with other stores
- Recent review dates (prioritize recent ones)

#### 小红书 (Xiaohongshu) Price Discovery
```bash
# Search for store + location + price keywords
IN_DOCKER=true browser-use open "https://www.xiaohongshu.com/search_result?keyword=168亚洲超市+密歇根+价格"
browser-use state
browser-use screenshot /tmp/xhs_search.png

# Alternative searches:
# "密歇根 华人超市 价格"
# "Ann Arbor 买菜 攻略"
# "168超市 什么便宜"
# "美国亚超 比价"
```

**小红书搜索关键词模板：**
| 场景 | 关键词 |
|------|--------|
| 特定商品 | `{城市} {商品名} 价格` |
| 超市攻略 | `{城市} 华人超市 攻略` |
| 比价笔记 | `{超市名} vs {超市名}` |
| 省钱技巧 | `美国买菜 省钱` |
| 产品评价 | `{超市名} {商品名} 好吃吗` |
| 踩坑避雷 | `{超市名} 避雷 不要买` |
| 推荐必买 | `{超市名} 必买清单` |

**小红书内容解析要点 - 不只是价格！**

| 维度 | 要提取的信息 | 示例 |
|------|-------------|------|
| **价格** | 具体价格、单位、是否促销 | "$3.99/lb, 特价" |
| **分量/规格** | 包装大小、几人份 | "一盒够4个人吃火锅" |
| **品质评价** | 新鲜度、口感、质量 | "很新鲜"、"肉质紧实" |
| **加工难度** | 是否需要处理、骨头多少 | "骨头很大不好切"、"已经切好了" |
| **口味描述** | 甜/咸/辣、是否正宗 | "偏甜"、"和国内味道一样" |
| **性价比** | 相比其他店如何 | "比H Mart便宜但分量小" |
| **适合人群** | 适合谁、不适合谁 | "适合北方人口味"、"ABC可能觉得太咸" |
| **踩坑提醒** | 需要注意的问题 | "保质期短"、"需要提前解冻" |

**小红书"问一问"AI功能：**
```bash
# 小红书有AI问答功能，可以直接提问
IN_DOCKER=true browser-use open "https://www.xiaohongshu.com/search_result?keyword=山姆+五花肉"
browser-use state
# 找到"问一问"入口或搜索结果中的AI总结
# 可以直接问："山姆的五花肉怎么样？骨头多吗？"
```

**产品评价综合报告示例：**
```markdown
## 山姆 五花肉 详细评价

### 基本信息
- 价格: $3.29/lb (会员价)
- 规格: 整块约5-6lb，需要自己切
- 产地: 美国

### 用户评价摘要 (来自小红书 12篇笔记)
- **优点**: 
  - 价格便宜，性价比高
  - 肉质新鲜，脂肪层分明
- **缺点**:
  - 骨头比较大，需要自己处理
  - 整块太大，1-2人家庭吃不完
  - 需要自己切片，不如168切好的方便
  
### 适合人群
- ✅ 3人以上家庭
- ✅ 会做饭、有切肉工具的人
- ✅ 喜欢批量买然后冷冻的人
- ❌ 1-2人小家庭（分量太大）
- ❌ 不想自己处理的人

### 加工建议
- 买回来先切成小块分装冷冻
- 或者让山姆肉柜帮忙切（部分店提供）

### 综合推荐
如果你是3人以上家庭，有时间处理，山姆的五花肉性价比最高。
如果你是1-2人，建议去168买已切好的，虽然贵$0.70/lb但省事很多。
```

#### WeChat Articles & Groups (微信公众号/群聊截图)
```bash
# Search for WeChat articles about local grocery shopping
IN_DOCKER=true browser-use open "https://www.google.com/search?q=site:mp.weixin.qq.com+密歇根+华人超市+价格"
browser-use state
```

**微信渠道信息来源：**
- 当地华人公众号的超市攻略
- 留学生群里分享的价格截图
- 团购群的拼单信息

#### Reddit & Local Forums
```bash
# Search Reddit for local grocery discussions
IN_DOCKER=true browser-use open "https://www.reddit.com/r/AnnArbor/search/?q=asian+grocery+prices"
browser-use state

# Alternative: Google search with site filter
IN_DOCKER=true browser-use open "https://www.google.com/search?q=site:reddit.com+Ann+Arbor+168+Asian+Mart+prices"
```

#### Yelp Reviews
```bash
IN_DOCKER=true browser-use open "https://www.yelp.com/biz/168-asian-mart-madison-heights"
browser-use scroll down  # Scroll to reviews
browser-use state
```

### Tier 4: User-Reported Data (Baseline Prices)

For stores where social media mining doesn't yield results:

1. **Check user's baseline prices** in their memory file
2. **Ask user to report** if data is stale (>7 days)
3. **Offer to help track** via receipt scanning (future feature)

## Social Media Price Discovery Workflow

When a user asks about a store without online pricing (like 168 Asian Mart), follow this discovery workflow:

### Step 1: Quick Google Maps Check
```bash
# Fast sentiment check from reviews
IN_DOCKER=true browser-use open "https://www.google.com/maps/search/{store_name}+{city}+{state}"
browser-use state
# Click on store → scroll to reviews → extract price mentions
```

### Step 2: 小红书 Deep Dive (Best for Chinese stores)
```bash
# 小红书 is the BEST source for Chinese grocery price info
IN_DOCKER=true browser-use open "https://www.xiaohongshu.com/search_result?keyword={store_name}+{city}"
browser-use state
browser-use screenshot /tmp/xhs_results.png
```

**Parsing 小红书 Results:**
1. Look for posts with **价格/价签/账单** in title
2. Check images for price tag photos
3. Note the post date (ignore posts >6 months old)
4. Read comments for updated prices

### Step 3: Synthesize Price Intelligence

After gathering data, create a summary:

```markdown
## Price Intelligence: 168 Asian Mart (Madison Heights, MI)

### Data Sources
- Google Maps: 47 reviews mentioning prices (avg sentiment: "affordable")
- 小红书: 12 relevant posts (most recent: 2026-03-15)
- Yelp: 23 price mentions

### Price Findings
| Item | Reported Price | Source | Date |
|------|---------------|--------|------|
| 五花肉 | $3.99/lb | 小红书@密歇根吃货 | 2026-03-15 |
| 老干妈 | $3.29 | Google review | 2026-02-20 |
| 整体印象 | "比H Mart便宜20-30%" | Multiple sources | - |

### Confidence Level
- High: 多个来源一致的价格
- Medium: 单一来源但近期
- Low: 老旧数据或模糊描述

### Recommendation
基于社媒数据，168的价格整体比H Mart便宜，尤其是中国特色商品。
建议将此信息保存到你的memory作为baseline。
```

### Step 4: Update User Memory

If useful price data is found, offer to save to user's memory:

```markdown
我从小红书和Google Maps找到了168的一些价格信息：
- 五花肉 ~$3.99/lb
- 老干妈 ~$3.29
- 整体比H Mart便宜20-30%

要我帮你保存到memory作为baseline吗？下次比价可以直接用。
```

---

## Comparison Workflow

### Step 1: Understand the Request

Parse user query to extract:
- **Product**: What they want to buy (e.g., "五花肉", "pork belly", "老干妈")
- **Quantity**: How much (optional, affects bulk store recommendations)
- **Urgency**: Need it today vs can wait for delivery

Example queries:
- "帮我比一下五花肉哪里便宜"
- "Where should I buy pork belly?"
- "Weee上的旺旺和168比哪个划算"

### Step 2: Gather Price Data

```python
# Pseudocode for data gathering
async def gather_prices(product: str, user_prefs: dict):
    results = []
    
    # 1. Check user's baseline data first
    baseline = check_user_baselines(product, user_prefs)
    if baseline:
        results.extend(baseline)
    
    # 2. Query online sources
    if "Weee" in user_prefs.preferred_stores:
        weee_price = await scrape_weee(product)
        results.append(weee_price)
    
    if "Kroger" in user_prefs.preferred_stores:
        kroger_price = await query_kroger_api(product, user_prefs.zip_code)
        results.append(kroger_price)
    
    # 3. Browser scrape other online stores
    for store in ["H Mart", "Yami"]:
        if store in user_prefs.preferred_stores:
            price = await browser_scrape(store, product)
            results.append(price)
    
    return results
```

### Step 3: Normalize and Compare

**Unit normalization is critical:**

| Raw | Normalized |
|-----|-----------|
| $3.99/lb | $8.80/kg |
| $4.99/500g | $9.98/kg |
| $12.99/pack (2lb) | $6.50/lb = $14.33/kg |

```python
def normalize_price(price: float, unit: str) -> float:
    """Convert to $/kg for comparison"""
    conversions = {
        "lb": 2.20462,  # 1kg = 2.20462 lb
        "oz": 35.274,   # 1kg = 35.274 oz
        "g": 1000,      # 1kg = 1000g
        "kg": 1,
    }
    # Parse unit and convert
    ...
```

### Step 4: Apply User Preferences

**Scoring formula:**

```
score = base_price_score 
      + taste_penalty      # -100 if American-style and user avoids
      + distance_penalty   # -10 per 10 min drive over threshold
      + delivery_bonus     # +20 if delivers to user's zip
      + freshness_bonus    # +15 for fresh vs frozen
```

### Step 5: Generate Recommendation

**Output format:**

```markdown
## 五花肉 (Pork Belly) 比价结果

### 推荐: 168 Asian Mart
- 价格: $3.99/lb ($8.80/kg)
- 距离: 15分钟车程
- 优点: 最便宜，品质好，新鲜
- 缺点: 需要开车

### 其他选项:

| 渠道 | 价格 | 单价 | 配送 | 备注 |
|------|------|------|------|------|
| Weee | $5.49/lb | $12.10/kg | 免费送货 | 方便但贵$1.50/lb |
| H Mart | $4.49/lb | $9.90/kg | 需自取 | 20分钟车程 |
| Costco | $3.29/lb | $7.25/kg | 需自取 | 需买整块5lb+，会员价 |

### 建议
考虑到你住在Ann Arbor，168开车15分钟可以接受。如果你这周不想出门，Weee虽然贵$1.50/lb但免费送货，5lb以内差价不到$8，可能值得。

如果你要买大量(>5lb)，Costco最划算，但需要会员卡。
```

## Common Products (Chinese/English Mapping)

| 中文 | English | Category | Typical Stores |
|------|---------|----------|----------------|
| 五花肉 | Pork Belly | Meat | 168, H Mart, Costco, Weee |
| 猪蹄 | Pork Feet/Trotters | Meat | 168, H Mart (rare at Costco) |
| 老干妈 | Lao Gan Ma Chili | Condiment | 168, Weee, Yami, H Mart |
| 旺旺仙贝 | Want Want Rice Crackers | Snack | Weee, Yami, 168 |
| 康师傅方便面 | Master Kong Instant Noodles | Noodles | Weee, 168, Yami |
| 王老吉 | Wong Lo Kat Herbal Tea | Beverage | 168, Weee, H Mart |
| 豆腐 | Tofu | Produce | All stores |
| 韭菜 | Chinese Chives | Produce | 168, H Mart, 99 Ranch |
| 火锅底料 | Hot Pot Base | Condiment | Weee, 168, H Mart |

## Store Profiles

### Online Delivery

| Store | Delivery | Min Order | Coverage | Specialty |
|-------|----------|-----------|----------|-----------|
| Weee | Free >$35 | $35 | Major metros | Chinese + Pan-Asian groceries |
| Yami | $5.99 or free >$49 | None | Nationwide | Asian snacks, beauty |
| Instacart (Kroger) | $3.99+ | $10 | Nationwide | Mainstream groceries |
| Costco Delivery | Varies | $35 | Members only | Bulk items |

### Physical Stores (Michigan Ann Arbor Area)

| Store | Address | Drive Time | Best For |
|-------|---------|------------|----------|
| 168 Asian Mart | Madison Heights | ~25 min | Chinese groceries, best prices |
| H Mart | Troy | ~30 min | Korean + general Asian |
| Kroger | Multiple | 5-15 min | American groceries, good sales |
| Costco | Ann Arbor | 15 min | Bulk buying |
| Sam's Club | Ypsilanti | 20 min | Bulk buying |
| Aldi | Multiple | 5-10 min | Budget European goods |

## Handling Edge Cases

### Product not found online
```
我在Weee上没找到这个产品。根据你之前报告的价格，168有卖$X.XX。
你要我帮你记录一下其他店的价格吗？下次可以直接比较。
```

### Price data is stale
```
你上次记录168的价格是2周前了。这些价格可能有变化：
- 五花肉 $3.99/lb (记录于 2026-03-18)

你最近去过168吗？可以帮我更新一下价格。
```

### User asks about non-Asian product
```
鸡蛋在这些店的价格大概是：
- Costco: $X.XX/18个 (需会员)
- Aldi: $X.XX/12个 (最便宜非会员选项)
- Kroger: $X.XX/12个 (查看本周flyer可能有折扣)

注意：亚洲超市的鸡蛋通常比Costco/Aldi贵，除非你需要特定品种（如皮蛋、咸蛋）。
```

## Weekly Deals Integration

### Parsing Promotional Emails

If user forwards promotional emails to DoWhiz:

```python
# Extract deals from email body
def parse_promo_email(email_body: str, store: str):
    # Look for patterns like:
    # "Pork Belly $2.99/lb (reg $3.99)"
    # "五花肉 特价 $2.99/磅"
    deals = extract_deals(email_body)
    
    # Store in user's deal tracker
    for deal in deals:
        save_deal(user_id, store, deal.product, deal.sale_price, deal.valid_until)
```

### Proactive Recommendations

If user has subscribed to weekly recommendations:

```markdown
## 本周值得买 (Week of 2026-04-01)

基于你的偏好和各店促销:

1. **五花肉** - 168特价 $2.99/lb (原价$3.99，省25%)
   - 比Weee便宜$2.50/lb
   - 建议：多买点冻起来

2. **旺旺仙贝** - Weee买二送一
   - 相当于$3.33/包，比168便宜
   
3. **老干妈** - Yami有8折码 SPRING20
   - 但算上运费不如168划算，除非你要买其他东西凑$49免邮
```

## Receipt OCR Integration (Future)

When receipt scanning is available:

```bash
# User uploads receipt image
receipt_ocr --image /tmp/receipt.jpg --output json > /tmp/receipt_data.json

# Parse and update price database
update_prices_from_receipt /tmp/receipt_data.json --store "168 Asian Mart"
```

## Search Query Templates by Scenario

### Scenario 1: Unknown Store Price Level
```
Goal: Understand if a store is generally cheap or expensive

Google Maps: "{store_name} {city}" → read reviews for price sentiment
小红书: "{store_name} 价格 贵不贵"
Reddit: "site:reddit.com {city} {store_name} prices cheap expensive"
```

### Scenario 2: Specific Product Price
```
Goal: Find what a specific item costs at a store

小红书: "{store_name} {product_name_zh} 价格" (e.g., "168超市 五花肉 价格")
Google: "{store_name} {product_name} price reddit OR yelp"
```

### Scenario 3: Store Comparison
```
Goal: Compare two stores

小红书: "{store_A} vs {store_B}" or "{store_A} {store_B} 比较"
Reddit: "{city} {store_A} vs {store_B}"
Google: "site:reddit.com OR site:yelp.com {store_A} cheaper than {store_B}"
```

### Scenario 4: Best Store for a Category
```
Goal: Find which store is best for meat/produce/snacks

小红书: "{city} 买肉 哪家便宜" or "{city} 亚洲零食 推荐"
Reddit: "{city} best asian grocery for {category}"
```

### Scenario 5: Current Deals/Sales
```
Goal: Find current promotions

小红书: "{store_name} 打折 {current_month}" (e.g., "Weee 打折 四月")
Store's Instagram/Facebook (if they have one)
Local WeChat groups (ask user if they're in any)
```

## Data Freshness Guidelines

| Source | Typical Freshness | Trust Level |
|--------|-------------------|-------------|
| 小红书 posts <1 month | High | ⭐⭐⭐⭐ |
| 小红书 posts 1-6 months | Medium | ⭐⭐⭐ |
| Google reviews <3 months | High | ⭐⭐⭐⭐ |
| Google reviews 3-12 months | Medium | ⭐⭐⭐ |
| Reddit posts <6 months | Medium-High | ⭐⭐⭐ |
| Yelp reviews | Variable | ⭐⭐ |
| User's own baseline | Depends on date | ⭐⭐⭐⭐⭐ |

**Rule of thumb:** 
- Prices can change 5-15% over 6 months
- Sale items can change weekly
- Always note the date when reporting social media prices

## Handling No Results

If social media search yields no useful price data:

```markdown
我在小红书和Google Maps上没找到{store_name}的具体价格信息。

可能的原因：
1. 这家店比较小众，网上讨论少
2. 搜索关键词可能需要调整

建议：
1. 你下次去{store_name}的时候可以拍几张价签，我帮你记录
2. 或者你知道大概价格的话可以告诉我，我保存到你的memory
3. 我可以先用H Mart/Weee的价格作为参考（通常本地小店会便宜10-20%）

需要我用哪种方式帮你？
```

## Tips for Effective Use

1. **First time setup**: Ask user for zip code and store preferences
2. **Build baseline**: Encourage user to share prices they see in stores
3. **Track over time**: Prices fluctuate; historical data helps identify deals
4. **Consider total trip**: Factor in gas cost for distant stores
5. **Batch recommendations**: "If you're going to 168 anyway, also grab X, Y, Z"
6. **Social media first**: For local Asian markets, always check 小红书 first - it's the best source for Chinese grocery price info
7. **Save discoveries**: When social media yields good price data, offer to save to user's memory

## Example Conversations

### Basic Price Check
```
User: 老干妈在哪买便宜
Agent: [Checks Weee, user's 168 baseline, Yami]

老干妈 (280g装) 比价：
- 168 Asian Mart: $3.29 (你上次报告的价格)
- Weee: $3.99 
- Yami: $4.49

建议去168买，便宜$0.70。如果你这周有其他东西要在Weee买，可以顺便加购，差价不大。
```

### Complex Comparison
```
User: 我想买火锅食材，比较一下168和Weee

Agent: [Comprehensive comparison]

火锅食材比较 (168 vs Weee):

| 食材 | 168 | Weee | 推荐 |
|------|-----|------|------|
| 肥牛卷 | $8.99/lb | $9.49/lb | 168 |
| 羊肉卷 | $12.99/lb | $11.99/lb | Weee |
| 豆腐 | $1.99 | $2.49 | 168 |
| 金针菇 | $1.79 | $1.99 | 168 |
| 海底捞底料 | $5.99 | $5.49 | Weee |

总计 (估算一顿4人份):
- 168: ~$35 + 开车25分钟
- Weee: ~$38 + 免费配送

建议：如果你有时间去168，能省$3+。如果懒得出门，Weee贵不了多少，而且不用自己搬。
```

## Cleanup

Always close browser sessions after scraping:
```bash
browser-use close
```
