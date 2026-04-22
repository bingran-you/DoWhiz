---
name: us-equity-daily-monitor
description: One-off investment research for a single U.S. stock or single U.S. ETF. Use this skill whenever the user asks to analyze one ticker, asks whether now is a good time to buy, asks about buying before earnings, requests deep research on one U.S. stock or ETF, or wants a clear Buy / Wait / Sell view. Return a decision-useful memo with exact visible labels for Rating, Horizon, Confidence, Timing Verdict, Verified Facts, Derived Metrics, Bull Case, Base Case, Bear Case, Add Criteria, Invalidation Criteria, Biggest Near-Term Risk, and Biggest Long-Term Strength.
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

Return one final user-visible investment memo that helps the user decide what to do now.

This memo must not collapse into generic stock commentary. It must separate:

- verified facts
- derived metrics
- inference or judgment

It must also give:

- one final `Buy`, `Wait`, or `Sell` rating
- one direct timing verdict
- explicit bull, base, and bear cases
- explicit add criteria
- explicit invalidation criteria

## Hard output contract

The final user-visible artifact must contain these exact visible labels.

For email replies, write semantic HTML in `reply_email_draft.html` and keep these labels visible in the HTML body.

For chat replies, keep the same visible labels in markdown or plain text.

Do not rename, collapse, or replace them with softer alternatives.

Required labels:

- `Rating`
- `Horizon`
- `Confidence`
- `Timing Verdict`
- `Verified Facts`
- `Derived Metrics`
- `Bull Case`
- `Base Case`
- `Bear Case`
- `Add Criteria`
- `Invalidation Criteria`
- `Biggest Near-Term Risk`
- `Biggest Long-Term Strength`

## Request framing

Before making claims, identify:

- ticker
- company or fund name
- `Stock` or `ETF`
- research mode: `Quick analysis` or `Deep research`
- user objective: mark `stated` or `inferred`
- horizon: `Short-term`, `Medium-term`, or `Long-term`, and mark `stated` or `inferred`
- question type: `Long-term accumulation`, `Medium-term investment`, `Short-term trade`, or `Event-driven timing`

Inference rules:

- days or weeks usually map to `Short-term`
- months usually map to `Medium-term`
- broad "is this worth buying?" questions usually map to `Long-term (inferred)`
- earnings or catalyst timing questions map to `Event-driven timing`

If the prompt is ambiguous or points to multiple possible tickers, resolve the ambiguity before giving the memo.

## Facts, metrics, and judgment

### Verified Facts

Only sourced facts belong here.

Examples:

- latest reported revenue, EPS, cash, debt, margin, or guidance
- next earnings date
- major concentration or regulatory facts
- recent price or trend context when available from accessible public data

Do not mix opinion into this section.

### Derived Metrics

This section is mandatory even when the conclusion is that a metric is not reliably derivable.

Rules:

- show the metric name
- show the formula
- show the inputs used
- show the result, or say what missing input prevents a reliable result

Examples:

- trailing GAAP P/E = share price / trailing diluted EPS
- forward P/E = share price / forward EPS consensus
- net cash or net debt = cash and equivalents - total debt
- FCF margin = trailing free cash flow / trailing revenue

If the metric is approximate, mark it clearly as approximate.

### Inference / Judgment

Keep judgment separate from fact.

Make it clear when you are interpreting mixed evidence, valuation stretch, event risk, or poor timing.

## Timing and scenarios

### Timing Verdict

Give one direct action from this set:

- `Buy now`
- `Starter only`
- `Wait`
- `Avoid for now`

Do not hide behind vague language such as:

- "good company"
- "buy in tranches"
- "wait and see"
- "not a good all-in buy"

Translate vague language into an explicit `Timing Verdict`, `Add Criteria`, and `Invalidation Criteria`.

### Scenario Analysis

This section is mandatory and must include:

- `Bull Case`
- `Base Case`
- `Bear Case`

Do not invent precise price targets unless the supporting math is shown.

## Wrong-premise handling

If the user states a factual premise and trusted public sources conflict with it, correct the premise explicitly in the final memo before proceeding.

Do not silently absorb a wrong earnings date, filing date, or guidance number.

## Confidence

Use only:

- `Low`
- `Medium`
- `High`

Lower confidence when data is incomplete, conflicting, stale, messy, or heavily catalyst-dependent.

## Exact final structure

Use this exact visible structure and keep the labels exactly as written.

### Request Framing
- `Ticker`: ...
- `Name`: ...
- `Type`: `Stock` or `ETF`
- `Research Mode`: `Quick analysis` or `Deep research`
- `User Objective`: ... `(<stated or inferred>)`
- `Horizon`: `Short-term`, `Medium-term`, or `Long-term` `(<stated or inferred>)`
- `Question Type`: `Long-term accumulation`, `Medium-term investment`, `Short-term trade`, or `Event-driven timing`

### Final Recommendation
- `Rating`: `Buy`, `Wait`, or `Sell`
- `Horizon`: `Short-term`, `Medium-term`, or `Long-term` `(<stated or inferred>)`
- `Confidence`: `Low`, `Medium`, or `High`
- `Timing Verdict`: `Buy now`, `Starter only`, `Wait`, or `Avoid for now`
- `Add Criteria`: ...
- `Invalidation Criteria`: ...
- `Biggest Near-Term Risk`: ...
- `Biggest Long-Term Strength`: ...

### Verified Facts
- 4 to 8 concise bullets with sourced facts only

### Derived Metrics
- 2 to 4 concise bullets in the form `Metric: formula = result`
- or explicit `Not reliably derivable from accessible public data today: ...`

### Inference / Judgment
- 2 to 5 concise bullets

### Scenario Analysis
- `Bull Case`: ...
- `Base Case`: ...
- `Bear Case`: ...

### Sources
- name the primary public sources used
- list official or primary sources first

### Disclaimer
`Public-information-based research only, not personalized investment advice or trade execution.`

## Email formatting guidance

For email replies:

- use semantic HTML such as `<h2>`, `<p>`, `<ul>`, `<li>`, and `<strong>`
- keep the contract labels visible as text inside the HTML body
- do not hide the labels inside images or attachments
- do not replace the labeled structure with one prose paragraph

## Failure mode to avoid

This is bad:

`NVIDIA is a great company, but I would wait and buy in tranches after earnings.`

This is acceptable only when translated into the exact labeled memo with a clear `Rating`, `Timing Verdict`, `Add Criteria`, and `Invalidation Criteria`.
