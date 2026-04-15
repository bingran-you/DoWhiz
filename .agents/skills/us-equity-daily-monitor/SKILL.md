---
name: us-equity-daily-monitor
description: Daily monitoring and email-ready trading notes for U.S. stocks and ETFs. Use this skill whenever the user asks to track or watch a ticker, follow BE or INTC or any other U.S. equity, prepare a daily trading rating, explain what changed versus yesterday, summarize public news or filings, review insider or major-holder activity, or analyze price action, support or resistance, moving averages, momentum, volume, sentiment, event impact, key levels, or risk or reward for a trade decision. Also use it for Buy or Hold or Sell style ratings and for Buy or Wait or Trim or Sell action calls. Separate facts from interpretation, use current market data before making market claims, and do not present this as personalized investment advice or autonomous execution.
---

# U.S. Equity Daily Monitor

Use this skill to produce a disciplined daily note for one U.S. equity or ETF.

This skill is for research, monitoring, and communication quality. It is not a scheduler and it is not an execution engine.

## Core job

Produce one clear daily view that:

- explains what happened
- explains what matters now
- translates professional trading logic into plain language
- states uncertainty honestly
- gives an action-oriented but non-hyped conclusion

## Boundaries

- Use only public information.
- Do not imply access to non-public information, channel checks, or privileged order flow.
- Do not present yourself as a licensed investment adviser or promise returns.
- Do not place trades or imply that a trade will be placed automatically.
- Do not overstate conviction when the signal set is mixed.

## Source hierarchy

Prefer sources in this order:

1. Official company releases, SEC filings, exchange notices, and earnings materials
2. Reliable market data and chart data for price, volume, and trend context
3. Reputable financial news coverage
4. Consensus analyst or sector commentary as context, not as the thesis by itself

If a source is stale, ambiguous, or inaccessible, say so plainly.

## Interpret filings correctly

Do not collapse all ownership signals into one bucket.

- `Form 4`: insider ownership changes. Treat this as insider activity, not institutional positioning.
- `Schedule 13D` or `13G`: beneficial ownership disclosures for large holders. Treat these as major-holder positioning or control-relevant updates.
- `Form 13F`: lagged quarterly institutional holdings disclosure. Do not frame this as same-day active buying or selling.

When a filing is material but lagged, say both things:

1. what the filing shows
2. why it may not reflect today's live positioning

## Handle time of day correctly

Before writing the note, determine which market context applies:

- **Pre-market**: before the regular session opens. Use the prior close plus pre-market context if available.
- **Intraday**: during the regular session. Do not describe the current daily candle as a confirmed end-of-day close.
- **Post-close**: after the regular session ends. You may discuss the completed daily candle and close.
- **Market closed day**: weekend or holiday. Do not fabricate a live session; give a carry-forward watchlist view instead.

If the user says "9:00 AM America/Los_Angeles", treat that as a market-status check first, not a fixed assumption that the market is still closed.

## Required thinking process

Build the note in this order:

1. Confirm the ticker and company.
2. Gather current public facts:
   - latest relevant news
   - earnings, guidance, policy, contracts, or sector updates
   - ownership or insider filings if any
   - current price action and volume context
3. Separate **facts** from **interpretation**.
4. Build the technical view:
   - trend
   - support and resistance
   - relative volume
   - candlestick context
   - moving averages
   - momentum
5. Build the event view:
   - catalyst
   - sentiment
   - event impact
   - risk or reward asymmetry
6. Compare with the prior note if one exists.
7. Assign rating, confidence, and final action.
8. Write the note in plain language.

## If prior-day context is missing

Never invent "what changed vs yesterday."

If no prior report, prior close summary, or prior stored note is available, say:

- that there is no reliable prior-note baseline
- what changed versus the latest available public facts instead

## Rating rubric

Use these labels consistently:

- `Strong Buy`: multiple aligned bullish signals, favorable risk or reward, and no major near-term contradiction
- `Buy`: positive setup with some risks or less-than-perfect alignment
- `Hold`: mixed or balanced setup; evidence does not justify a directional call
- `Sell`: bearish setup or deteriorating thesis, but not a panic scenario
- `Strong Sell`: strongly negative setup with multiple aligned bearish signals or major thesis break

Do not force a bullish or bearish rating when `Hold` is the honest answer.

## Confidence rubric

- `High`: several independent signals align and the main uncertainty is modest
- `Medium`: the thesis is plausible but key signals are mixed or incomplete
- `Low`: the setup is noisy, event-driven, or too uncertain for conviction

Confidence is about signal quality, not about sounding authoritative.

## Output format

When the user wants an email-ready daily note, use this structure:

**Subject**

`[TICKER] Daily Trading Rating - [DATE]`

**Body**

1. `Rating`: one of the five rating labels
2. `Confidence`: High, Medium, or Low
3. `One-sentence reason`
4. `What happened`
5. `Top 3 bullish factors`
6. `Top 3 bearish factors`
7. `Key price levels to watch`
8. `What changed vs yesterday`
9. `Risks`
10. `Final action`: Buy, Wait, Trim, Sell, or No action
11. `Key sources`

## Writing style

- Sound precise, calm, and evidence-based.
- Explain jargon in plain English.
- Keep the conclusion concise enough for email.
- Use short sentences for the final recommendation.
- If there is little new news, say that and still provide a technical update.

## What to avoid

- Do not confuse a lagged 13F with same-day live buying.
- Do not describe an intraday candle as a fully confirmed daily close.
- Do not let analyst price targets dominate the conclusion.
- Do not present every insider buy as bullish or every insider sale as bearish without context.
- Do not hide uncertainty.

## Automation handoff

This skill drafts the analysis and the email-ready content.

Scheduling, recurring execution, and actual email delivery belong to the automation or scheduler layer. If used inside an automated workflow, return content that is ready to send, but do not mix scheduling rules into the analytical conclusion.
