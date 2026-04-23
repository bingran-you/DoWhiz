---
name: us-equity-daily-monitor
description: One-off investment research for a single U.S. stock or single U.S. ETF. Use this skill whenever the user asks to analyze one ticker, asks whether now is a good time to buy, asks about buying before earnings, requests deep research on one U.S. stock or ETF, or wants a decision-useful investment memo. Return a scan-first decision memo with exact visible labels for Decision Card, New Money Action, Existing Holder Action, Near-Term Timing View, Long-Term Ownership View, Confidence, What Is Priced In, Verified Facts, Derived Metrics, Expectations, Bull Case, Base Case, Bear Case, Upgrade / Add Triggers, Invalidation Criteria, Opportunity-Cost / Peer Check, Source Notes, and Disclaimer.
---

# U.S. Equity Decision Memo

Use this skill for one U.S. public stock or one U.S. ETF.

Do not use it for:

- crypto
- options
- portfolio allocation
- multi-asset screeners
- automated trading
- recurring monitoring

## Core job

Return one final user-visible investment memo that helps the user decide what to do now with the least possible cognitive load.

The memo must:

- front-load the decision
- separate `New Money Action` from `Existing Holder Action`
- separate `Near-Term Timing View` from `Long-Term Ownership View`
- separate verified fact from derived metric from judgment
- explain what appears priced in
- explain what would upgrade the view, keep it in wait mode, or break the thesis
- make the evidence easy to click without pushing everything into a generic appendix

This skill must not collapse into generic stock commentary or a long reading-heavy research email.

## Hard output contract

The final user-visible artifact must contain these exact visible labels.

For email replies, write semantic HTML in `reply_email_draft.html` and keep these labels visible in the HTML body.

For chat replies, keep the same visible labels in markdown or plain text.

Do not rename, collapse, or replace them with softer alternatives.

Required headings and labels:

- `Request Framing`
- `Decision Card`
- `New Money Action`
- `Existing Holder Action`
- `Near-Term Timing View`
- `Long-Term Ownership View`
- `Confidence`
- `One-Line Rationale`
- `Why in 3 bullets`
- `What Is Priced In`
- `What Keeps This From Being Stronger`
- `What Would Change The View`
- `Trigger Block`
- `Upgrade / Add Triggers`
- `Stay Wait Unless`
- `Invalidation Criteria`
- `Verified Facts`
- `Derived Metrics`
- `Expectations`
- `What the Next Catalyst Must Show`
- `What Could Disappoint Even If Fundamentals Are Fine`
- `Opportunity-Cost / Peer Check`
- `Inference / Judgment`
- `Bull Case`
- `Base Case`
- `Bear Case`
- `Source Notes`
- `Disclaimer`

## Recommendation rules

### Two action tracks are mandatory

The memo must answer both:

- `New Money Action`: one of `Buy`, `Wait`, `Starter Only`, or `Avoid for now`
- `Existing Holder Action`: one of `Hold`, `Add`, `Trim`, `Exit`, or `Hold / Do not add`

Do not answer a new-money question with only `Hold`.

### Dual-horizon framing is the default

If the user does not give a clear horizon, default to both:

- `Near-Term Timing View`
- `Long-Term Ownership View`

Do not force an arbitrary exact horizon such as "6 months" unless the prompt or evidence clearly supports it.

If the user gives a specific horizon, keep the dual-horizon view anyway when it helps explain why short-term timing and long-term ownership differ.

### Trigger-based verdicts

Every actionable view must include:

- `Upgrade / Add Triggers`
- `Stay Wait Unless`
- `Invalidation Criteria`

Do not use vague wording such as:

- "watch for a pullback"
- "wait for more clarity"
- "buy in tranches"

unless it is translated into explicit conditions.

## Request framing

Before making claims, identify:

- ticker
- company or fund name
- `Stock` or `ETF`
- research mode: `Quick analysis` or `Deep research`
- user objective: mark `stated` or `inferred`
- question type: `Long-term accumulation`, `Medium-term investment`, `Short-term trade`, or `Event-driven timing`
- horizon basis:
  - use the user-stated horizon when available
  - otherwise say that the note defaults to dual-horizon framing because the user did not specify one

Inference rules:

- days or weeks usually matter most for `Near-Term Timing View`
- months usually matter for both `Near-Term Timing View` and `Long-Term Ownership View`
- broad "is this worth buying?" questions usually need a long-term ownership answer plus a separate near-term timing answer
- earnings or catalyst questions require an explicit expectations layer

If the prompt is ambiguous or points to multiple possible tickers, resolve the ambiguity before giving the memo.

## Facts, metrics, and judgment

### Verified Facts

Only sourced facts belong here.

Examples:

- latest reported revenue, EPS, cash, debt, margin, or guidance
- next earnings date
- major concentration or regulatory facts
- recent price context from a quote or reference source

Rules:

- 3 to 6 concise bullets
- each material factual bullet must include at least one clickable evidence chip
- do not mix opinion into this section

### Derived Metrics

This section is mandatory even when the conclusion is that a metric is not reliably derivable.

Rules:

- show the metric name
- show the formula
- show the inputs used
- show the result, or say what missing input prevents a reliable result
- each material metric bullet must include at least one clickable evidence chip

Examples:

- trailing GAAP P/E = share price / trailing diluted EPS
- forward P/E = share price / forward EPS consensus
- net cash or net debt = cash and equivalents - total debt
- FCF margin = trailing free cash flow / trailing revenue

If the metric is approximate, mark it clearly as approximate.

### Inference / Judgment

Keep judgment separate from fact.

Use cited evidence to support the reasoning, but do not pretend the judgment itself is a directly sourced fact.

## Expectations and opportunity cost

### Expectations

This section is mandatory for "is now a good time to buy?" or catalyst-timing questions.

It must answer:

- `What Is Priced In`
- `What the Next Catalyst Must Show`
- `What Could Disappoint Even If Fundamentals Are Fine`

### Opportunity-Cost / Peer Check

This section is mandatory and concise.

It must answer one of these clearly:

- why this stock versus a key peer or alternative
- why this stock versus the index
- why this stock versus doing nothing / holding cash

Do not omit opportunity cost just because the company is high quality.

## Scenario analysis

This section is mandatory and must include:

- `Bull Case`
- `Base Case`
- `Bear Case`

Do not invent precise price targets unless the supporting math is shown.

## Confidence

Use only:

- `Low`
- `Medium`
- `High`

Lower confidence when data is incomplete, conflicting, stale, messy, or heavily catalyst-dependent.

## Citation and source policy

### Clickable evidence is mandatory for material factual and derived claims

Do not cite every sentence.

Do cite every material factual claim and every material derived-metric claim with clickable source links at the bullet or paragraph level.

Use compact evidence chips such as:

```html
<div class="dw-evidence-row">
  <a class="dw-evidence-chip" data-source-tier="primary" href="https://...">IR</a>
  <a class="dw-evidence-chip" data-source-tier="independent" href="https://...">Reuters</a>
  <a class="dw-evidence-chip" data-source-tier="reference" href="https://...">Quote</a>
</div>
```

Rules:

- use real clickable links
- keep chips compact and close to the claim they support
- use at least one `primary` chip somewhere in the note
- use at least one `independent` chip somewhere in the note when relevant
- use at least one `reference` chip for price or quote cross-checking when relevant
- do not use the appendix as the only place where sources appear

### Tiered source policy

Use a mix of source types.

Tier 1 `primary`:

- company investor relations pages
- earnings releases
- SEC filings such as `10-K`, `10-Q`, `8-K`, `20-F`
- official transcripts or prepared remarks when available
- official exchange or company filings

Tier 2 `independent`:

- Reuters
- Bloomberg
- WSJ
- FT
- AP when relevant
- other reputable independent reporting when justified

Tier 3 `reference`:

- Yahoo Finance
- Nasdaq
- MarketWatch
- Investing.com
- other quote or reference sources when justified

Source rules:

- do not rely only on company-controlled sources
- do not let reference sites substitute for primary filings on core fundamentals
- when an external report is lower-confidence or indirect, say so
- if a user premise is wrong and a trusted source conflicts with it, correct the premise explicitly

## Exact final structure

Use this exact visible structure and keep the labels exactly as written.

### Request Framing
- `Ticker`: ...
- `Name`: ...
- `Type`: `Stock` or `ETF`
- `Research Mode`: `Quick analysis` or `Deep research`
- `User Objective`: ... `(<stated or inferred>)`
- `Horizon Basis`: ... `(<stated or inferred>)`
- `Question Type`: `Long-term accumulation`, `Medium-term investment`, `Short-term trade`, or `Event-driven timing`

### Decision Card
- `New Money Action`: `Buy`, `Wait`, `Starter Only`, or `Avoid for now`
- `Existing Holder Action`: `Hold`, `Add`, `Trim`, `Exit`, or `Hold / Do not add`
- `Near-Term Timing View`: ...
- `Long-Term Ownership View`: ...
- `Confidence`: `Low`, `Medium`, or `High`
- `One-Line Rationale`: ...

### Why in 3 bullets
- `What Is Priced In`: ...
- `What Keeps This From Being Stronger`: ...
- `What Would Change The View`: ...

### Trigger Block
- `Upgrade / Add Triggers`: ...
- `Stay Wait Unless`: ...
- `Invalidation Criteria`: ...

### Verified Facts
- 3 to 6 concise bullets with sourced facts only
- each bullet ends with a compact evidence row or inline evidence chips

### Derived Metrics
- 2 to 4 concise bullets in the form `Metric: formula = result`
- or explicit `Not reliably derivable from accessible public data today: ...`
- each bullet ends with a compact evidence row or inline evidence chips

### Expectations
- `What the Next Catalyst Must Show`: ...
- `What Could Disappoint Even If Fundamentals Are Fine`: ...

### Opportunity-Cost / Peer Check
- 2 to 4 concise bullets

### Inference / Judgment
- 2 to 5 concise bullets

### Scenario Analysis
- `Bull Case`: ...
- `Base Case`: ...
- `Bear Case`: ...

### Source Notes
- 3 to 8 concise bullets summarizing what each source contributed
- call out `primary`, `independent`, and `reference` source types visibly

### Disclaimer
`Public-information-based research only, not personalized investment advice or trade execution.`

## Email formatting guidance

For email replies:

- write semantic HTML and make the note scan-first
- use sections and cards, not one long prose block
- keep the answer visible in the first screen or two
- use compact `<ul>` blocks, short paragraphs, and clear subheads
- place evidence chips close to the claims they support
- use CSS classes when helpful:
  - `dw-investment-card`
  - `dw-investment-grid`
  - `dw-investment-list`
  - `dw-trigger-grid`
  - `dw-evidence-row`
  - `dw-evidence-chip`
  - `dw-source-note`
  - `dw-chart-card`
- do not hide the labels inside images or attachments
- do not rely on JavaScript or fragile interactivity

## Chart policy

Charts are optional, not mandatory.

Only include a chart if it directly supports the verdict and can be rendered safely in the final HTML email.

If included:

- limit to one or two charts
- prefer a simple static chart or table-style visual
- include a one-line caption explaining why it matters
- include a click-through link for fuller detail when helpful

If a clean chart is not available from accessible public data, omit it instead of faking precision.

## Failure modes to avoid

These are bad:

- `Great company, but not a good all-in buy.`
- `Wait until after earnings and see.`
- `Buy in tranches.`
- `Watch for a pullback.`

These are acceptable only when translated into the exact labeled memo with explicit actions, expectations, triggers, and evidence chips.
