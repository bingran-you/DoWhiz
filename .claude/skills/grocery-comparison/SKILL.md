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

## Culinary Knowledge (核心差异化)

**This is the key differentiator**: understand what dish the user wants to make, and recommend the RIGHT ingredient, not just the cheapest.

### 菜肴 → 食材要求映射

当用户提到想做某道菜时，要理解这道菜对食材的具体要求：

| 菜肴 | 推荐部位/食材 | 关键要求 | 不推荐 |
|------|-------------|---------|--------|
| **糖醋小排** | 肋排/小排 (spare ribs) | 小块、带少量肉、骨肉比例均衡 | 大骨头、纯瘦肉 |
| **排骨汤** | 筒骨/大骨 (pork neck bones, soup bones) | 大块、骨髓丰富、适合长时间炖 | 小排（肉多骨少，不出味） |
| **红烧排骨** | 肋排 | 中等大小、肥瘦相间 | 太大或太小的块 |
| **红烧肉** | 五花肉 (pork belly) | 层次分明、皮肉脂比例好 | 太瘦的部位 |
| **回锅肉** | 后臀尖/二刀肉 | 肥三瘦七、连皮带肉 | 纯瘦肉、五花（太肥） |
| **水煮肉片** | 里脊/梅花肉 | 瘦肉为主、易切薄片 | 带筋的部位 |
| **火锅肥牛** | 肥牛卷 (beef for hotpot) | 薄切、脂肪花纹 | 牛排级别（太厚） |
| **牛肉面** | 牛腩/牛腱 (beef shank, brisket) | 带筋、炖后软糯 | 里脊（太柴） |
| **清蒸鱼** | 鲈鱼/鳜鱼/石斑 | 鲜活、1-1.5磅 | 冷冻鱼（口感差） |
| **酸菜鱼** | 草鱼/黑鱼片 | 肉厚、易片 | 带刺多的鱼 |

### 文化口味偏好

根据用户的文化背景，理解他们可能的口味偏好：

| 背景 | 典型口味偏好 | 注意事项 |
|------|------------|---------|
| **中国大陆** | 美式甜品太甜、偏好鲜味和咸鲜、重视食材新鲜度 | Hershey's 巧克力偏甜、American cakes 糖霜过重 |
| **北方人** | 偏咸、喜面食、口味重 | 南方菜可能觉得淡 |
| **南方人 (粤/闽)** | 偏清淡、重视食材原味、甜品偏清甜 | 川菜太辣、北方菜太咸 |
| **川渝** | 能吃辣、麻辣口味、重油 | 不辣的菜可能觉得没味道 |
| **台湾/港澳** | 口味介于大陆南北之间、接受度较广 | 对食材品质要求高 |
| **韩国** | 喜辣、发酵食品、烤肉文化 | 泡菜是刚需 |
| **日本** | 清淡、鲜味为主、注重摆盘 | 对新鲜度要求极高 |
| **东南亚** | 酸辣甜复合、椰奶、鱼露 | 香料是刚需 |
| **ABC/华裔** | 可能偏美式口味、也可能保留家庭传统 | 需要具体了解 |

### 推荐策略

当用户说 "我想买排骨" 时，不要直接比价，先问/判断：
1. **做什么菜？** → 决定推荐哪个部位
2. **几个人吃？** → 决定推荐多少量
3. **有时间处理吗？** → 决定推荐整块还是切好的

**示例对话：**
```
用户: 我想买排骨
Agent: 你是想做什么菜呢？
- 如果是糖醋小排、红烧排骨，推荐买肋排 (spare ribs)，Weee 和 168 都有切好的小块
- 如果是煲汤，推荐买筒骨/大骨 (soup bones)，168 的更便宜而且新鲜

你住在安娜堡对吧？168 Asian Mart 开车约 25 分钟，他们的排骨 $3.99/lb 是附近最便宜的。
```

### 美式产品避雷

中国用户常见的"踩坑"产品，可以主动提醒：

| 产品类型 | 问题 | 推荐替代 |
|---------|------|---------|
| 美式蛋糕 (Walmart/Kroger bakery) | 糖霜太甜、奶油假 | 85°C、Paris Baguette、或自己做 |
| Hershey's 巧克力 | 有酸味（butyric acid）、太甜 | Lindt、Ghirardelli、或亚洲品牌 |
| American cheese (Kraft singles) | 口感像塑料、不是真芝士 | 真正的 cheddar、mozzarella |
| Wonderbread 白面包 | 太软太甜 | 亚洲面包店、或自己买吐司 |
| 美式香肠 (hot dog) | 调味不同、太咸 | 亚超的台湾香肠、或自己买肉馅 |
| Spam | 太咸 | 梅林午餐肉 (亚超有) |

---

## Data Source Availability Status (Tested 2026-04-08)

| 数据源 | 状态 | 抓取方法 | 备注 |
|--------|------|----------|------|
| **Weee** | ✅ 可用 | browser-use | 成功抓取产品名称、价格、单位 |
| **Yami** | ✅ 可用 | browser-use | 已验证，价格/产品信息可抓取 |
| **Google Maps** | ✅ 可用 | browser-use | 成功获取店铺评分、评论、营业时间 |
| **Kroger** | ✅ 可用 | 官方API | 已配置 OAuth2，支持产品搜索+店铺查询+价格 |
| **H Mart** | ⚠️ 部分可用 | browser-use | 主页可访问，搜索功能需进一步测试 |
| **Sam's Club** | ⚠️ CAPTCHA | cookies + browser-use | 有 `.samsclub_cookies.json`，但 headless 被检测，需 human_approval_gate |
| **Costco** | ❌ 被限制 | 需登录+cookie | headless browser 被 block |
| **小红书** | ❌ IP blocked | 需 Azure Asia | Error 300012 "IP at risk"，需中国 IP |

### Cookies 文件位置 (Staging/Production)
```
~/server/DoWhiz/DoWhiz_service/.samsclub_cookies.json  # Sam's Club 会员登录
~/server/DoWhiz/DoWhiz_service/.notion_cookies_liuxt.json  # Notion (测试用)
```

### 小红书访问方案 (TODO)

小红书对非中国 IP 有严格限制。解决方案：

**方案 1: Azure Asia VM (推荐)**
```bash
# 在 Azure East Asia / China North 创建小型 VM
# 专门用于抓取小红书
# 通过 API 调用或 SSH tunnel 访问
```

**方案 2: 代理服务**
- 使用中国代理 IP 池
- 或用户自己的 VPN 账号

**方案 3: 用户自助**
- 让用户自己搜索小红书，提供关键词
- 用户粘贴相关笔记链接，agent 解析

**推荐抓取策略：**
1. **首选**: Weee + Yami + Google Maps (直接 browser-use)
2. **需 API**: Kroger (`grocery_cli kroger search`)
3. **需 cookies**: Sam's Club (有 cookies，但需处理 CAPTCHA)
4. **需特殊处理**: 小红书 (Azure Asia VM 或用户自助)

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

If preferences not found or incomplete, you have two options:
1. **Ask in conversation** - If you need critical info (like location), ask directly
2. **Point to questionnaire** - For comprehensive setup: `https://dowhiz.com/grocery/onboarding`

Use your judgment: if user's question can be answered with partial info, answer first and suggest completing preferences later.

---

## Response Requirements

### 1. Purchase Links (REQUIRED)

Every product recommendation MUST include clickable purchase links:

| 渠道 | 链接获取方法 | 格式 |
|------|------------|------|
| **Weee** | browser-use 抓取搜索结果 | `[产品名](https://www.sayweee.com/en/product/...)` |
| **Yami** | browser-use 抓取 | `[产品名](https://www.yamibuy.com/en/p/...)` |
| **Kroger** | `grocery_cli kroger search` 返回的 URL | `[产品名](https://www.kroger.com/p/...)` |
| **H Mart** | 搜索页链接 | `[Search H Mart](https://www.hmart.com/search?q=...)` |
| **实体店** | Google Maps | `[店名](https://maps.google.com/?q=...)` |

**示例输出:**
```markdown
## 五花肉比价

| 渠道 | 价格 | 购买链接 |
|------|------|---------|
| [168 Asian Mart](https://maps.google.com/?q=168+Asian+Mart+Madison+Heights+MI) | $3.99/lb | 实体店 |
| [Weee](https://www.sayweee.com/en/product/12345) | $5.49/lb | [立即购买](https://www.sayweee.com/en/product/12345) |
| [Costco](https://maps.google.com/?q=Costco+Ann+Arbor) | $3.29/lb | 需会员 |
```

### 2. Questionnaire Link (回复结尾)

在回复结尾附上问卷链接，让用户选择是否完善偏好:

```markdown
---
💡 完成偏好问卷可获得更精准推荐: https://dowhiz.com/grocery/onboarding
```

### 3. Preference Updates (Agent Judgment)

当用户在对话中透露偏好信息时，**自主判断**是否值得保存:

- 明确的偏好 (如 "我不吃猪肉") → 保存到 memory
- 一次性需求 (如 "今天想吃辣的") → 不需要保存
- 位置信息 (如 "我住安娜堡") → 保存到 memory

保存格式: 更新用户 memory 文件的 `## Grocery Preferences` 部分

---

## Conversational Preference Collection

如果用户没有完整偏好且你需要关键信息才能回答，可以在对话中自然地询问。

**原则:**
- 不要一次问太多问题
- 只问回答当前问题必需的信息
- 让用户选择是回答还是跳过

**示例:**
```
用户: 排骨哪里买便宜？
Agent: 为了给你推荐最近的店，请问你住在哪个城市/zip code？
       (或者直接告诉我你方便去的超市，我帮你比价)
```

---

## Legacy: Detailed Questionnaire Reference

以下问题仅供参考，用于理解完整的偏好维度。实际收集通过 UI 问卷完成。

**可收集的偏好维度:**
- 位置: city, zip_code
- 交通: has_car, max_drive_minutes
- 购物方式: online/offline/both
- 会员: costco, sams_club, kroger_plus
- 常买品类: meat, seafood, vegetables, snacks, hotpot...
- 口味: sweetness_tolerance, spice_tolerance, american_sweets_opinion
- 饮食限制: vegetarian, no_pork, lactose_intolerant, gluten_free
- 预算: price_first, value_focused, quality_first

详见 `website/src/components/intake/GroceryPreferencesQuestionnaire.jsx`

---

## Shopping Habits Reference

以下内容描述用户可能的购物习惯，用于理解上下文:

**交通选项:**
- 没有车，靠公共交通/走路
- 有车，15分钟内
- 有车，30分钟内
- 有车，1小时内也可以

---

## User Preferences Schema

偏好数据由 UI 问卷收集并存储在后端。Agent 通过 `grocery_cli preferences get <user_id>` 读取。

**Memory 文件格式示例:**
```markdown
## Grocery Preferences

### Profile
- background: chinese_mainland
- city: Ann Arbor, MI
- zip_code: 48109
- household_size: 2

### Transportation
- has_car: true
- max_drive_minutes: 30
- prefers: both

### Memberships
- costco: true
- sams_club: false
- kroger_plus: true

### Main Categories
- primary: [meat, vegetables, snacks, hotpot]
- secondary: [condiments, instant_noodles]

### Taste Profile
- sweetness_tolerance: 2  # 1-5，美式甜品太甜
- spice_tolerance: 4  # 能吃辣
- american_sweets: too_sweet

### Exclusions
- dietary: []  # 无饮食限制

### Budget & Priorities
- budget_mindset: value_focused  # price_first/value_focused/quality_first
- priority_order: [price, quality, convenience, variety]

### Known Baselines (user-reported prices)
| Item | Store | Price | Unit |
|------|-------|-------|------|
| 五花肉 | 168 Asian Mart | $3.99 | /lb |
| 老干妈 | 168 Asian Mart | $3.29 | 280g |
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
