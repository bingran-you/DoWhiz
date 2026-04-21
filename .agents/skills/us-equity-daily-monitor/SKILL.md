---
name: us-equity-daily-monitor
description: One-off investment research for a single U.S. stock or single U.S. ETF. Use this skill whenever the user asks to analyze a ticker, do deep research on one stock or ETF, ask whether now is a good time to buy, request an investment view, or wants a decision-useful Buy / Wait / Sell memo with explicit horizon, timing, scenario analysis, invalidation criteria, and auditable derived metrics. Keep the scope to one U.S. stock or one U.S. ETF, use current public sources, separate facts from judgment, and do not present personalized investment advice, trade execution, portfolio construction, recurring automation, or dashboard behavior.
---

# U.S. Equity Investment Skill

This skill is for one-off investment decision support on one U.S. public-market asset.

Technical compatibility note: the skill id remains `us-equity-daily-monitor` in this pass to avoid broader rename churn.

Use it for:

- one U.S. stock
- one U.S. ETF

This skill is for public-information-based research only. It is not a broker, auto-trader, portfolio manager, scheduler, dashboard, or personalized advisory system.

## Core job

Return one structured investment memo that helps the user decide what to do now.

The memo must:

- identify the asset and the decision frame correctly
- return exactly one final rating: `Buy`, `Wait`, or `Sell`
- state the horizon as `Short-term`, `Medium-term`, or `Long-term`
- mark whether the objective and horizon were `stated` or `inferred`
- separate `Verified facts`, `Derived metrics`, and `Inference / judgment`
- include `Bull case`, `Base case`, and `Bear case`
- include an explicit timing view with one current action: `Buy now`, `Starter only`, `Wait`, or `Avoid for now`
- state what would justify adding and what would invalidate the thesis
- lower confidence when the evidence is incomplete, conflicting, or messy
- name the public sources behind the main claims

This skill should not behave like an investor-relations summary. It should behave like a falsifiable investment memo.

## Hard boundaries

- Use only public information.
- Do not imply access to non-public information, channel checks, privileged order flow, or private models.
- Do not present yourself as a licensed investment adviser or as giving personalized regulated advice.
- Do not place trades or imply that a trade will be placed automatically.
- Do not expand into options, crypto, global equities, portfolio allocation, or multi-asset comparison.
- Do not invent unavailable price levels, earnings dates, holdings data, filing details, valuation inputs, or "changed vs yesterday" baselines.
- Do not bluff derived metrics. If the inputs are not accessible, say so plainly.
- Do not add scheduler, automation, channel-delivery, or dashboard logic to the analysis.

## Source hierarchy

Prefer sources in this order:

1. Official company or fund materials, SEC filings, exchange notices, earnings materials, and fund sponsor or index-provider documents
2. Reliable market data and chart data for price, volume, trend, and session context
3. Reputable financial news coverage
4. Consensus analyst or sector commentary as context, never as the thesis by itself

If a source is stale, gated, ambiguous, or inaccessible, say so plainly and reduce conviction.

## Request framing

Before you analyze the asset, identify:

- ticker
- issuer or fund name
- whether it is a `Stock` or `ETF`
- market context: `Pre-market`, `Intraday`, `Post-close`, or `Market closed`
- research mode: `Quick analysis` or `Deep research`
- user objective: mark as `stated` or `inferred`
- horizon: `Short-term`, `Medium-term`, or `Long-term`, and mark as `stated` or `inferred`
- question type: `Long-term accumulation`, `Medium-term investment`, `Short-term trade`, or `Event-driven timing`

Use this inference logic:

- If the user gives an explicit holding period in days or weeks, that is usually `Short-term`.
- If the user gives an explicit holding period in months, that is usually `Medium-term`.
- If the user asks broadly whether an asset is worth buying and gives no tactical language, default to `Long-term (inferred)`.
- If the question centers on earnings, an FDA decision, a merger vote, or another catalyst window, classify it as `Event-driven timing` even if the user may hold longer afterward.

If the prompt is ambiguous or could map to multiple tickers, resolve the ambiguity before making claims.

## Research workflow

Work in this order:

1. Confirm the asset and whether it is a stock or ETF.
2. Determine market context: pre-market, intraday, post-close, or market-closed day.
3. Gather current public facts:
   - latest relevant news, earnings, guidance, contracts, policy, sector, or fund updates
   - price action, volume, trend, moving averages, momentum, and key levels when available
   - ownership or insider filings if relevant
   - for ETFs, objective, major exposures, concentration, sector or factor sensitivity, and rate sensitivity when relevant
4. If the user states a factual premise and trusted sources conflict, correct the premise explicitly before proceeding.
5. Populate `Verified facts` with sourced facts only.
6. Populate `Derived metrics` with explicit formulas or explicit "not reliably derivable" statements.
7. Write `Inference / judgment` as interpretation, not disguised fact.
8. Build `Scenario analysis`.
9. Build `Timing / execution`.
10. End with `Final recommendation`, `Sources`, and `Disclaimer`.

## Verified facts

Only sourced facts belong here.

Good examples:

- latest reported revenue, EPS, margins, or ETF sponsor facts
- cash, debt, dilution, buybacks, or expense ratio
- guidance, backlog, major concentration, or regulatory facts
- next earnings date or status
- recent price action, volume, and market context from accessible market data

Rules:

- Do not put adjectives like "great", "cheap", "high quality", or "crowded" here unless they are direct source language and clearly quoted as such.
- If two trusted sources conflict, say that they conflict and reduce confidence.
- If a fact is unavailable, say it is unavailable rather than filling the gap with inference.

## Derived metrics

This section is mandatory even if the conclusion is that a metric is not reliably derivable.

Rules:

- Prefer 2 to 4 decision-useful metrics when the inputs are accessible.
- Show the metric name, the formula, the inputs, and the result.
- If the inputs come from different timestamps or require approximation, mark the result with `~` and explain briefly.
- If a metric cannot be derived reliably, write `Not reliably derivable from accessible public data today:` and name the missing input.
- Never state a derived metric without either showing the arithmetic or explicitly stating why it could not be derived.

Useful stock examples:

- trailing GAAP P/E = share price / trailing diluted EPS
- forward P/E = share price / forward EPS consensus
- net cash or net debt = cash and equivalents - total debt
- FCF margin = trailing free cash flow / trailing revenue
- EV/revenue = enterprise value / revenue

Useful ETF examples:

- top-10 concentration = weight of top 10 holdings / total portfolio
- premium or discount to NAV when relevant
- yield or duration-based sensitivity when the inputs are accessible

## Inference / judgment

This section is where you interpret the facts.

Rules:

- Make it obvious that this is judgment, not sourced fact.
- Distinguish business quality from timing quality.
- Distinguish a good asset from a good entry point.
- If the evidence is mixed, say why that leads to `Wait` or `Starter only`.
- If data is messy or incomplete, say exactly what that does to confidence.

## Scenario analysis

This section is mandatory.

Include:

- `Bull case`: what has to go right and why upside exists
- `Base case`: the most likely path from the current public facts
- `Bear case`: what disappoints or breaks and why downside risk exists

Use drivers such as demand, margins, regulation, concentration, funding, valuation, or ETF exposure that are supported by the available evidence.

Do not invent precise price targets unless the supporting math is shown.

## Timing / execution

This section is mandatory.

You must give one direct current action from this set:

- `Buy now`
- `Starter only`
- `Wait`
- `Avoid for now`

Then state:

- why now or why not now
- what would justify adding
- what would invalidate the thesis or setup

Use these field names exactly in the output:

- `Current action`
- `Why now`
- `Add criteria`
- `Invalidation criteria`

Do not hide behind vague phrases such as:

- "great company"
- "buy in tranches"
- "not ideal for an all-in buy"

Convert that language into an explicit rating, an explicit entry approach, and explicit add or invalidation criteria.

## Confidence

Use `Low`, `Medium`, or `High`.

Base confidence on:

- evidence quality
- data completeness
- source quality
- internal consistency

Lower confidence when:

- the company is messy, early-stage, or highly event-driven
- the public data is stale, conflicting, or incomplete
- key metrics cannot be derived reliably
- the thesis depends heavily on one near-term catalyst

## Rating semantics

Use only these final labels:

- `Buy`
- `Wait`
- `Sell`

Use `Buy` when the evidence is sufficiently favorable for a constructive stance now, even if risks remain.

Use `Wait` when the setup is mixed, incomplete, extended, or unattractive enough that the honest answer is no action now.

Use `Sell` when the evidence suggests avoiding, reducing, or exiting because the setup or thesis is deteriorating or broken. Keep the language non-personalized.

Distinguish the final rating from the entry approach:

- `Buy` + `Buy now` means constructive and timing is acceptable now
- `Buy` + `Starter only` means constructive, but valuation, event risk, or setup argues against full sizing now
- `Wait` + `Wait` means no action now
- `Sell` + `Avoid for now` means avoid initiating or stay away until the thesis changes

Never use legacy formal labels such as `Strong Buy`, `Hold`, `Trim`, `No action`, or `Strong Sell`.

## Interpret filings correctly

Do not collapse all ownership signals into one bucket.

- `Form 4`: insider ownership changes. Treat this as insider activity, not institutional positioning.
- `Schedule 13D` or `13G`: beneficial ownership disclosures for large holders. Treat these as major-holder positioning or control-relevant updates.
- `Form 13F`: lagged quarterly institutional holdings disclosure. Do not frame this as same-day active buying or selling.

When a filing is material but lagged, say both:

1. what the filing shows
2. why it may not reflect today's live positioning

## Handle time of day correctly

Before writing the answer, determine which market context applies:

- **Pre-market**: before the regular session opens. Use the prior close plus pre-market context if available.
- **Intraday**: during the regular session. Do not describe the current daily candle as a confirmed end-of-day close.
- **Post-close**: after the regular session ends. You may discuss the completed daily candle and close.
- **Market closed**: weekend or holiday. Do not fabricate a live session; give a carry-forward watchlist view instead.

If the user gives a local time, resolve market status from that time instead of guessing.

## If prior-day context is missing

Never invent "what changed vs yesterday."

If the user asks for a yesterday comparison and no reliable prior report, prior close summary, or stored note is available, say:

- that there is no reliable prior-note baseline
- what changed versus the latest accessible public baseline instead

## Required output format

Always use this exact section order and these headers as written.

Never collapse, rename, or restyle these sections for short prompts, tactical prompts, or event-driven prompts.

Do not substitute headings such as `Asset`, `Call`, `Bottom line`, `Action now`, `Why not now`, or any other custom layout.

Even when the user asks a short question like "Is NVDA a buy this week for a 3-month position?", still return the exact `##` section headers below.

## Request framing
- `Ticker`: ...
- `Name`: ...
- `Type`: `Stock` or `ETF`
- `Market context`: `Pre-market`, `Intraday`, `Post-close`, or `Market closed`
- `Research mode`: `Quick analysis` or `Deep research`
- `User objective`: ... `(<stated or inferred>)`
- `Horizon`: `Short-term`, `Medium-term`, or `Long-term` `(<stated or inferred>)`
- `Question type`: `Long-term accumulation`, `Medium-term investment`, `Short-term trade`, or `Event-driven timing`

## Verified facts
- 4 to 8 concise bullets with sourced facts only

## Derived metrics
- 2 to 4 concise bullets in the form `Metric: formula = result`
- or explicit `Not reliably derivable from accessible public data today: ...`

## Inference / judgment
- 2 to 5 concise bullets

## Scenario analysis
- `Bull case`: ...
- `Base case`: ...
- `Bear case`: ...

## Timing / execution
- `Current action`: `Buy now`, `Starter only`, `Wait`, or `Avoid for now`
- `Why now`: ...
- `Add criteria`: ...
- `Invalidation criteria`: ...

## Final recommendation
- `Rating`: `Buy`, `Wait`, or `Sell`
- `Horizon`: `Short-term`, `Medium-term`, or `Long-term` `(<stated or inferred>)`
- `Confidence`: `Low`, `Medium`, or `High`
- `Entry approach`: `Buy now`, `Starter only`, `Wait`, or `Avoid for now`
- `Add criteria`: ...
- `Invalidation criteria`: ...
- `Biggest near-term risk`: ...
- `Biggest long-term strength`: ...

## Sources
- Name the main public sources used
- Put official and primary sources first

## Disclaimer
`Public-information-based research only, not personalized investment advice or trade execution.`

## Writing style

- Sound precise, calm, and evidence-based.
- Explain jargon in plain language.
- Keep the answer structured and easy to scan.
- Avoid hype, memes, and certainty theater.
- For ETFs, do not write as though the fund were an operating company.
- For stocks, do not let analyst price targets or a single headline dominate the thesis.
- When the evidence is weak, say so directly instead of smoothing it over.

## Out of scope

Automation, recurring execution, channel delivery, dashboards, and broker actions belong to other layers. This skill returns the research memo only.
