---
name: us-equity-daily-monitor
description: One-off investment research for a single U.S. stock or single U.S. ETF. Use this skill whenever the user asks to analyze a ticker, do deep research on one stock or ETF, ask whether it is worth buying now, request an investment view, or wants a structured Buy / Wait / Sell answer grounded in public information. Keep the scope to one U.S. stock or one U.S. ETF, use current public sources, separate facts from interpretation, and do not present personalized investment advice, auto-execution, portfolio construction, recurring automation, or dashboard behavior.
---

# Stock Investment Skill

This is the first-version stock investment research workflow for one U.S. public-market asset.

Technical compatibility note: the skill id remains `us-equity-daily-monitor` in this round to avoid broader rename churn. A path or id cleanup can happen later as a dedicated follow-up.

Use it for one-off analysis of:

- one U.S. stock
- one U.S. ETF

This skill is for public-information-based research only. It is not a broker, auto-trader, scheduler, dashboard, or delivery system.

## Core job

Produce one clear investment research answer that:

- identifies the asset correctly
- explains the current setup in plain language
- separates facts from interpretation
- weighs bullish and bearish evidence honestly
- returns exactly one rating: `Buy`, `Wait`, or `Sell`
- stays usable for both quick analysis and deeper one-off research

## Hard boundaries

- Use only public information.
- Do not imply access to non-public information, channel checks, or privileged order flow.
- Do not present yourself as a licensed investment adviser or as giving personalized regulated advice.
- Do not place trades or imply that a trade will be placed automatically.
- Do not expand into options, crypto, global equities, portfolio allocation, or multi-asset comparison.
- Do not invent unavailable price levels, holdings data, filing details, or "changed vs yesterday" baselines.
- Do not add scheduler, automation, channel-delivery, or dashboard logic to the analysis.

## Source hierarchy

Prefer sources in this order:

1. Official company or fund materials, SEC filings, exchange notices, earnings materials, and fund sponsor or index-provider documents
2. Reliable market data and chart data for price, volume, trend, and session context
3. Reputable financial news coverage
4. Consensus analyst or sector commentary as context, never as the thesis by itself

If a source is stale, gated, ambiguous, or inaccessible, say so plainly and reduce conviction.

## Asset identification

Start by confirming:

- ticker
- issuer or fund name
- whether it is a stock or ETF
- whether the user wants a quick analysis or deep research answer

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
4. Separate facts from interpretation.
5. Build the case for both sides:
   - bullish factors
   - bearish factors
   - near-term risks and thesis-break conditions
6. Assign exactly one rating: `Buy`, `Wait`, or `Sell`.
7. Write the answer in the required structure.
8. Name or cite the public sources behind the main claims.

## Interpret filings correctly

Do not collapse all ownership signals into one bucket.

- `Form 4`: insider ownership changes. Treat this as insider activity, not institutional positioning.
- `Schedule 13D` or `13G`: beneficial ownership disclosures for large holders. Treat these as major-holder positioning or control-relevant updates.
- `Form 13F`: lagged quarterly institutional holdings disclosure. Do not frame this as same-day active buying or selling.

When a filing is material but lagged, say both things:

1. what the filing shows
2. why it may not reflect today's live positioning

Do not turn one filing into the whole thesis without broader context.

## Handle time of day correctly

Before writing the answer, determine which market context applies:

- **Pre-market**: before the regular session opens. Use the prior close plus pre-market context if available.
- **Intraday**: during the regular session. Do not describe the current daily candle as a confirmed end-of-day close.
- **Post-close**: after the regular session ends. You may discuss the completed daily candle and close.
- **Market closed day**: weekend or holiday. Do not fabricate a live session; give a carry-forward watchlist view instead.

If the user says "9:00 AM America/Los_Angeles", treat that as a market-status check first, not as a fixed assumption that the market is still closed.

## If prior-day context is missing

Never invent "what changed vs yesterday."

If the user asks for a yesterday comparison and no reliable prior report, prior close summary, or stored note is available, say:

- that there is no reliable prior-note baseline
- what changed versus the latest accessible public baseline instead

## Rating semantics

Use only these labels:

- `Buy`: evidence is sufficiently favorable for a constructive stance right now, even if risks remain
- `Wait`: setup is mixed, incomplete, extended, or not attractive enough for action right now
- `Sell`: evidence suggests avoiding, reducing, or exiting because the setup or thesis is deteriorating or broken

If signals are mixed, `Wait` is often the honest answer.

The formal `## Rating` section must contain exactly one standalone label: `Buy`, `Wait`, or `Sell`.

Never reintroduce legacy formal labels such as `Strong Buy`, `Hold`, `Trim`, `No action`, or `Strong Sell`.

Do not add a second formal action taxonomy such as `Action`, `Final action`, `Trade plan`, or similar.

Do not output a formal `Confidence` label or section by default unless a future version explicitly adds it back. Express uncertainty in the summary, why-now explanation, and risks instead.

## Quick analysis versus deep research

- For a quick request such as "Analyze NVDA", keep each section concise and decision-useful.
- For a deep-research request such as "Deep research BE", use the same section order but provide broader synthesis across filings, news, price action, and risks.
- Deep research should go deeper than a headline recap, but it should still stay readable and structured.

## Required output format

Always use this section order:

Use these headers as written. Do not add formal sections such as `Confidence`, `Action`, or `Final action`.

## Asset identified
- `Ticker`: ...
- `Name`: ...
- `Type`: `Stock` or `ETF`
- `Market context`: `Pre-market`, `Intraday`, `Post-close`, or `Market closed`
- `Research mode`: `Quick analysis` or `Deep research`

## Summary
- `Facts`: 2 to 4 sentences on the most relevant current public facts
- `Interpretation`: 1 to 3 sentences on what those facts mean now

## Rating
`Buy` or `Wait` or `Sell`

## Why now
Explain why the current timing, catalyst path, valuation backdrop, fund-exposure backdrop, or technical setup supports the rating now.

## Bullish factors
- 2 to 5 concise bullets

## Bearish factors
- 2 to 5 concise bullets

## Risks
- 2 to 5 concise bullets, including what could break the thesis or invalidate the rating

## Key levels to watch
- Include support, resistance, trend, or trigger levels when relevant and supported by accessible market data
- If levels are not available or not meaningful, say so instead of inventing them

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

## Out of scope

Automation, recurring execution, channel delivery, dashboards, and broker actions belong to other layers. This skill returns the research answer only.
