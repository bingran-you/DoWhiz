# DoWhiz 产品策略讨论记录

**日期**: 2026-03-31  
**参与者**: 产品团队 + Claude AI  
**目的**: 讨论用户场景和市场定位

---

## 背景问题

最初考虑用**大学生小组作业场景**验证产品，但 survey 结果显示同学们可能只想尝试而不愿付费。

---

## 讨论要点

### 1. 大学生场景的问题

**核心问题**: 痛点不够"痛"到愿意付费
- 作业做不好最多分数低，没有真金白银的损失
- 预算有限
- 有免费替代品（直接用 ChatGPT/Claude）

### 2. 最初建议：瞄准人类 VA 替代市场

**人类 VA (Virtual Assistant) 市场概况**:
- 2026年市场规模: 约 $50-65亿美元
- 2035年预计: $420亿+
- 年增长率: 22-24% CAGR

**主要用户群体**:
| 用户群体 | 占比 |
|---------|------|
| 创业者/Entrepreneur | 28% |
| 咨询顾问 | 14% |
| 教练/Coach | 9% |
| 高管/Executive | 42% 使用VA管理日常事务 |

**定价参考**:
| 服务商 | 价格 |
|-------|------|
| BELAY（美国高端） | ~$42/小时 |
| Time Etc | $35-38/小时 |
| 菲律宾VA | $5-15/小时 |

**最初的差异化假设**:
- DoWhiz = "AI-powered VA"
- 价格是人类VA的1/10
- 24/7在线，不会忘事
- 主动跟进（proactive messaging）

---

### 3. 关键质疑（团队提出）

**质疑1**: "主动跟进"不就是 cron job 吗？竞品做不到吗？

**质疑2**: 用 AI 的人已经习惯 Claude，不会换到 DoWhiz；不用 AI 的人会被市场淘汰，也不需要我们

**质疑3**: 有些场景需要真人 VA 出面（电话、线下），AI 无法取代

---

### 4. 竞品调研结果（重要）

**市场上已有成熟的 AI assistant 竞品**:

| 产品 | 定位 | 价格 | 关键能力 |
|-----|------|------|---------|
| **Lindy.ai** | No-code AI workflow | $50/月 | 邮件/日历/CRM整合，定时任务，SMS/iMessage交互 |
| **OpenClaw** | 开源AI agent | 按API计费（$5-50/月） | **有heartbeat scheduler**，定期wake up执行任务 |
| **Motion** | "AI Employee SuperApp" | $19-34/月 | 已有"Alfred"(EA)、"Chip"(销售)等AI角色，$550M估值 |
| **Reclaim AI** | 日程优化 | 有免费版 | 专注Focus Time和会议安排 |
| **April** (YC) | Voice AI EA | - | 语音管理邮件和日历 |

**结论**: 
- OpenClaw 确实有 heartbeat scheduler，可以定期 wake up 检查任务
- "Proactive cron job" **不是护城河**
- Lindy.ai 已在做类似的事，$50/月，有 VC backing

---

### 5. 现实约束

团队提出的约束条件：
1. **没有完成第一轮融资**，没钱做 marketing
2. **产品特性不足以融资**（VC 已经 AI exhausted，看了1000个"ChatGPT for X"）
3. **国内市场付费意愿低**（千问、元宝在烧钱，但用户主要"图一乐"）

**这条路基本堵死**: VC融资 → 做通用产品 → 烧钱获客

---

### 6. 垂直场景分析

**高付费意愿的垂直领域**:

| 垂直场景 | 为什么付费意愿高 | 但是... |
|---------|----------------|--------|
| 物业管理 | 省人力成本可算ROI，EliseAI证明市场 | 竞品多（MagicDoor, AppFolio, EliseAI） |
| 法律/合规 | 高客单价，合规刚需 | 需要行业背景，获客难 |
| 医疗 | 高客单价，文档需求大 | 监管严，准入门槛高 |
| 招聘/猎头 | 痛点真实（83%的HR AI成熟度低） | 竞品超多 |

**问题**: 这些都需要**行业 knowhow + 获客渠道**，不是技术能解决的。

---

## 建议的方向

### 核心问题

在决定方向之前，需要回答：**团队有什么独特的资源？**

| 资源类型 | 如果有... | 可能的方向 |
|---------|----------|-----------|
| 行业背景 | 有人做过物业/猎头/法律？ | 深耕那个垂直 |
| 渠道关系 | 认识某个垂直的KOL/群主？ | 借他们的渠道 |
| 技术壁垒 | 有什么别人复制不了的？ | 围绕它建产品 |
| 已有客户 | 有人愿意付钱让你帮忙？ | 先服务后产品化 |

### 如果以上都没有：Bootstrapping 路线

1. **找1个具体的人**，问他："你每周花最多时间在什么重复性工作上？"
2. **为他做一个定制方案**，收 $200-500/月
3. **复制到类似的人**，手动获客（LinkedIn、冷邮件）
4. **积累10个付费客户后**，再考虑产品化和融资

**关键转变**: 从问"什么垂直场景值得做"到问"我认识的人里，谁最可能为我的帮助付$100/月？"

---

## 待讨论问题

1. 团队是否有特定的行业背景或渠道资源？
2. 是否已经有潜在的付费客户（哪怕只有1个）？
3. 是否考虑 pivot 到纯咨询/服务模式，先赚钱再产品化？
4. 当前的 runway 还能支撑多久？这决定了能承受多少试错。

---

## 参考资料

### 市场数据
- [Virtual Assistant Market Size & Growth, Forecast 2035](https://www.businessresearchinsights.com/market-reports/virtual-assistant-market-111910)
- [2026 Virtual Assistant Industry Report - Wishup](https://www.wishup.co/blog/virtual-assistant-industry-report/)
- ["AI Inside" Opens New Markets for Vertical SaaS - a16z](https://a16z.com/vsaas-vertical-saas-ai-opens-new-markets/)

### 竞品
- [Lindy.ai](https://www.lindy.ai) - No-code AI workflow, $50/月
- [OpenClaw](https://github.com/openclaw) - 开源 AI agent，有 heartbeat scheduler
- [Motion](https://www.usemotion.com) - AI Employee SuperApp, $550M 估值
- [Reclaim AI](https://reclaim.ai) - 日程优化

### 垂直场景
- [How Agentic AI Can Reshape Real Estate - McKinsey](https://www.mckinsey.com/industries/real-estate/our-insights/how-agentic-ai-can-reshape-real-estates-operating-model)
- [AI in Recruiting 2026: What Actually Works](https://dishertalent.com/blog/ai-in-recruiting-2026/)
- [Best AI-Powered Property Management Tools 2026](https://www.showdigs.com/property-managers/the-best-ai-powered-property-management-tools)

---

*文档由 Claude AI 整理，基于 2026-03-31 产品策略讨论*
