# {TICKER} — Equity Signal Monitor
**As of:** {YYYY-MM-DD} · **Price:** ${PRICE}

**Investor question:** {one-line restatement of what the reader actually asked}

## Decision Card

| Field | Value |
|---|---|
| Monitor Status | {Review Now / Watch Closely / No Material Change / Insufficient Evidence} |
| Signal Direction | {Positive / Neutral / Negative} |
| Thesis Impact | {Positive / Neutral / Negative} |
| Signal Quality | {Strong / Mixed / Thin} |
| Urgency | {High / Medium / Low} |
| Confidence | {High / Medium / Low} |
| New Money Action* | {Starter Only / Wait / Avoid for now / n/a} |
| Existing Holder Action* | {Hold/Do not add / Trim / Exit / n/a} |

**One-line rationale:** {one sentence explaining why the badge is the right calibrated signal call now.}

**Main Risk To The Signal:** {the single most important risk or evidence gap that could blunt, reverse, or overstate the signal.}

**Not Personalized Advice:** This is a public-market signal assessment, not personalized financial advice.

## Why Now

{2-4 sentences. Explain why the signal matters now, or why there is no material signal now. If the signal is weak or incomplete, name the exact missing fact or unresolved variable.}

## Dual-Horizon Framing

### Near-Term Timing View (next 1-2 quarters)
{2-4 sentences. Name the dominant catalyst and explain the near-term setup without collapsing into generic caution.}

### Long-Term Ownership View (multi-year)
{2-4 sentences. Structural drivers, competitive position, or capital structure. Keep this separate from the near-term signal.}

## Verified Facts

- {Reported figure with units} ([{Source name}]({URL}))
- {Guidance figure or management comment} ([{Source name}]({URL}))
- {Industry or competitor datapoint} ([{Source name}]({URL}))
- {Disclosure or risk factor from a filing} ([SEC EDGAR]({URL}))

> Aim for >=3 distinct sources in a full monitor, spanning >=2 tiers, with at least one primary source.

## Derived Metrics

| Metric | Value | Formula / Inputs |
|---|---|---|
| {Core valuation or setup metric} | {VALUE} | {Formula / Inputs} |
| {Growth or margin metric} | {VALUE} | {Formula / Inputs} |
| {Balance-sheet or cash-flow metric} | {VALUE} | {Formula / Inputs or "Not reliably derivable"} |

> If your inputs match the canonical schema in `references/data-adapters.md`, run `scripts/compute_metrics.py` to generate this block.

## Scenarios

### Bull Case
**Trigger:** {explicit numeric condition}. {Explain what becomes true if the bullish signal is confirmed.}

### Base Case
**Trigger:** {explicit numeric range or expected path}. {Explain the most likely read-through.}

### Bear Case
**Invalidation:** {explicit numeric condition}. {Explain what breaks the thesis or weakens the signal materially.}

## What Would Change The View

- **Upgrade / Review Now:** {numeric threshold or verified event that would justify a more urgent review.}
- **Downgrade / De-risk:** {numeric threshold or verified event that would worsen the signal materially.}
- **Invalidation:** {specific fact pattern that would break the current framing.}

## Judgment

*Inference, {High / Medium / Low} confidence.* {One short paragraph. Explain the calibrated signal call, the biggest reason confidence is not higher, and keep any remaining caution specific rather than generic.}

---

### Author checklist (delete before publishing)

- [ ] I chose exactly one badge from `Review Now / Watch Closely / No Material Change / Insufficient Evidence`.
- [ ] I stated `Signal Direction`, `Thesis Impact`, `Signal Quality`, `Urgency`, and `Confidence`.
- [ ] `Why Now` is specific and evidence-based.
- [ ] I named the `Main Risk To The Signal`.
- [ ] `What Would Change The View` contains explicit thresholds or concrete events.
- [ ] I avoided generic `wait/hold/be careful/it depends` filler.
- [ ] I kept any action rows secondary rather than as the headline answer.
- [ ] The compliance sentence appears once and stays concise.
