---
name: us-equity-daily-monitor
description: Produce a one-page U.S. equity signal monitor for a single stock or ETF. Front-load exactly one monitor badge (`Review Now`, `Watch Closely`, `No Material Change`, or `Insufficient Evidence`), explain why the signal matters now, what changed or did not change in the thesis, the main risk to the signal, and what would change the view next. Use whenever the user asks whether a stock is a buy or sell, wants a single-name investment memo, asks for deep research or a high-level investment view on one company, wants a thesis review versus peers or sector expectations, asks what changed since a prior note, wants an earnings setup, or says "should I act" about one ticker. Keep it as public-market analysis rather than personalized portfolio advice. Do not use for portfolio optimization, multi-asset allocation, crypto, FX, fixed-income, or macro strategy notes.
---

# U.S. Equity Daily Monitor

A structured framework for one-page single-name U.S. equity signal monitors. The artifact should help a reader decide whether the stock-specific signal deserves immediate review, closer watching, or no thesis change, without collapsing into generic `wait/hold/be careful` filler.

## Core principles

1. **Front-load one concrete signal judgment.** Choose exactly one badge: `Review Now`, `Watch Closely`, `No Material Change`, or `Insufficient Evidence`.
2. **Separate company quality from signal actionability.** A strong company with no new thesis delta is still `No Material Change`. A weak company with a decisive negative break can still be `Review Now`.
3. **Require `Why Now`.** Every output explains why the signal matters now, or why there is no material signal now. "Stock is moving" is not enough.
4. **Tie uncertainty to a specific evidence gap.** Allowed: "Confidence is Medium because the margin guide still depends on an unproven product ramp." Not allowed: "Markets are uncertain, so be careful."
5. **Keep the monitor badge primary.** If the current product contract also expects `New Money Action` or `Existing Holder Action`, include them as a secondary translation layer. Do not let those rows replace the badge or drift into personalized portfolio advice.
6. **Quantify what would move the view.** Use explicit thresholds (`%`, `$`, `bps`, `x`, quarter counts) in the change-view section.
7. **Use peer context only when it sharpens the call.** A `2-5` name peer set is a lens for better judgment, not a mandatory sector report.
8. **Keep compliance concise.** Use exactly one short sentence such as: `This is a public-market signal assessment, not personalized financial advice.`

## Badge rubric

- `Review Now`: Material new evidence, a decisive catalyst, a sharp positive or negative thesis delta, or high urgency. This is the badge for "re-underwrite now," not necessarily "rush to trade."
- `Watch Closely`: A real signal exists, but the evidence is mixed, thin, incomplete, or not yet durable enough to count as a thesis change.
- `No Material Change`: The reviewed information does not materially alter the thesis or near-term setup. This is specific non-change, not hidden negativity.
- `Insufficient Evidence`: There is not enough reliable current evidence to judge. Name the missing evidence explicitly.

## Signal vocabulary

Use these controlled labels unless the user or product contract requires something narrower:

- `Signal Direction`: `Positive` | `Neutral` | `Negative`
- `Thesis Impact`: `Positive` | `Neutral` | `Negative`
- `Signal Quality`: `Strong` | `Mixed` | `Thin`
- `Urgency`: `High` | `Medium` | `Low`
- `Confidence`: `High` | `Medium` | `Low`

If the product contract expects action rows, keep them secondary and bounded:

- `New Money Action`: prefer `Starter Only` | `Wait` | `Avoid for now`
- `Existing Holder Action`: prefer `Hold/Do not add` | `Trim` | `Exit`

Avoid leading with `Buy`, `Hold`, or `Sell` as the main answer unless the surrounding product contract explicitly requires those labels.

## Workflow

1. **Restate the actual investor question** at the top so the monitor answers what was asked.
2. **Determine the monitor type before writing.**
   - Use the **full monitor** when the user asks for deep research, a full memo, or a fresh single-name view.
   - Use the **concise monitor** when the user asks "what changed," "since your last note," or "only tell me if I should act."
3. **Decide whether `Competitive / Peer Context` earns its keep.**
   - Include it when the user asks for deep research, a high-level investment view, a thesis review, sector-relative valuation context, or when competitor/substitute dynamics materially shape the current signal.
   - Usually skip it for narrow factual, latest-news, market-cap, CEO-comment, or single-event prompts unless the catalyst is directly peer-driven or clearly sector-wide.
   - If the user already asked for a direct comparison (for example `NVDA vs AMD`), keep the named comparison primary and do not add extra peers unless they materially change the thesis.
4. **Choose the badge first.** Decide whether the evidence supports `Review Now`, `Watch Closely`, `No Material Change`, or `Insufficient Evidence` before drafting prose.
5. **Compute derived metrics deterministically** when the full monitor needs them. Use `scripts/compute_metrics.py` if the inputs match [references/data-adapters.md](references/data-adapters.md); otherwise compute directly from [references/formulas.md](references/formulas.md).
6. **Draft from the template.** Copy [assets/memo-template.md](assets/memo-template.md) and fill every required field. Section structure is defined in [references/sections.md](references/sections.md).
7. **Cite cleanly.** Every material fact ends in a source chip. Aim for at least three distinct sources in a full monitor and at least two in a concise monitor. Rules live in [references/evidence.md](references/evidence.md).
8. **Name the main risk and what would change the view.** Caution is allowed only when the specific risk, threshold, or missing evidence is named.

## Output contract

Always include these fields:

- `As of`
- `Price`
- `Investor question`
- `Decision Card`
- `Monitor Status`
- `Signal Direction`
- `Thesis Impact`
- `Signal Quality`
- `Urgency`
- `Confidence`
- `One-line rationale`
- `Main Risk To The Signal`
- `Why Now`
- `What Would Change The View`
- `Not Personalized Advice`

When the current channel or product contract already expects action rows, also include:

- `New Money Action`
- `Existing Holder Action`

### Full monitor

Use this for deep-research or first-look requests. Include:

- `Decision Card`
- `Why Now`
- `Dual-Horizon Framing`
- `Competitive / Peer Context` (optional)
- `Verified Facts`
- `Derived Metrics`
- `Scenarios`
- `What Would Change The View`
- `Judgment`

### Concise monitor

Use this for delta checks and "should I act" requests. Include:

- `Decision Card`
- `Why Now`
- `Competitive / Peer Context` (optional)
- `What Would Change The View`
- `Evidence Chips`

In concise mode, shorten the artifact, not the analysis. The reader still needs a concrete badge, signal direction, thesis impact, main risk, and the missing or moving evidence.

## Competitive / Peer Context

Include this section only when it materially sharpens the investment judgment. It is optional, not default.

When you include it:

- Pick `2-5` relevant peers, substitutes, adjacent platforms, or structural threats.
- Explain briefly why that peer set matters for this thesis. If the peer set is weak or imperfect, say so instead of forcing fake comparables.
- Compare only the dimensions that actually matter to the call now: pricing power, margin durability, software moat, customer concentration, capital intensity, autonomy credibility, regulatory pressure, or sector-relative valuation.
- Keep the section concise. Usually four short bullets are enough:
  - `Relevant peers/substitutes`
  - `Where the target looks stronger`
  - `Where peers create pressure`
  - `Investment implication`
- If a table is clearer, keep it small and thesis-relevant rather than exhaustive.
- Current or factual peer claims must be sourced. Do not rely only on the target company's materials to make claims about competitors.
- Do not invent peer sets, market-share numbers, or current metrics when the comparison is fuzzy.
- Do not let this become a full sector report unless the user explicitly asks for one.

Illustrative peer-selection patterns:

- `NVDA`: AMD, Broadcom, hyperscaler custom ASIC efforts, cloud providers' internal silicon programs.
- `TSLA`: BYD, legacy OEM EV programs, and autonomy competitors when autonomy is part of the thesis.
- `AAPL`: Samsung, Google, Microsoft, or Meta when ecosystem durability, services, AI platform risk, or regulation is the relevant lens.
- `SHOP`: Amazon, Adobe Commerce, BigCommerce, Wix/Squarespace, or merchant-stack/payment substitutes when platform economics are the question.

## Anti-waffle rules

- Do not default to `Wait` or `Hold` merely because risk exists.
- Do not confuse "good company" with "strong current signal."
- Do not write generic sentences like "It depends," "be careful," "do not rush," or "wait and see."
- Do not hide behind valuation language unless you explain which expectation is already priced in and why that limits the signal.
- Do not give portfolio sizing, tax, cost-basis, or risk-tolerance advice.
- Do not turn the answer into a disclaimer block.
- Do not write empty competition filler such as "faces competition from many players" without naming the specific pressure and why it matters now.

## Compact examples

**1. Strong positive catalyst**

- Badge: `Review Now`
- Signal Direction: `Positive`
- Thesis Impact: `Positive`
- Why Now: `The guidance revision lifts the revenue assumption that drives the bull case, so the thesis inputs changed immediately.`
- Main Risk To The Signal: `Valuation already discounts aggressive growth, so the market may have pre-priced part of the upside.`
- What Would Change The View: `Demand deceleration, margin compression, or hyperscaler capex cuts.`

**2. Huge price increase but weak evidence**

- Badge: `Watch Closely`
- Signal Direction: `Positive`
- Thesis Impact: `Neutral`
- Signal Quality: `Thin`
- Why Now: `The price move is large, but the available evidence does not yet prove a durable thesis change.`
- Main Risk To The Signal: `Momentum may be detached from fundamentals.`

**3. No material news**

- Badge: `No Material Change`
- Signal Direction: `Neutral`
- Thesis Impact: `Neutral`
- Why Now: `No new filing, guide change, or disclosed demand break has altered the thesis since the last decision point.`

**4. Negative thesis break**

- Badge: `Review Now`
- Signal Direction: `Negative`
- Thesis Impact: `Negative`
- Why Now: `The new financing event materially changes dilution risk and reduces confidence in the prior profitability path.`
- Main Risk To The Signal: `Management may stabilize liquidity faster than the market expects.`

**5. Insufficient evidence**

- Badge: `Insufficient Evidence`
- Signal Direction: `Neutral`
- Thesis Impact: `Neutral`
- Why Now: `There is no verified issuer disclosure yet, so the rumor cannot be treated as a thesis change.`
- What Would Change The View: `A filing, guidance update, or reported operating data that confirms the effect on revenue or margin assumptions.`

**6. Good company but no near-term thesis delta**

- Badge: `No Material Change`
- Signal Direction: `Neutral`
- Thesis Impact: `Neutral`
- Why Now: `The company still looks high quality, but there is no new evidence strong enough to revise the thesis today.`
- Main Risk To The Signal: `The market may keep rerating the stock before fundamentals move again.`

## Anti-waffle checklist

Before finalizing, check this in one pass while you draft:

- Did I choose exactly one top-level badge?
- Did I state `Signal Direction`?
- Did I state `Thesis Impact`?
- Did I explain `Why Now` specifically?
- Did I identify the `Main Risk To The Signal`?
- Did I state what would change the view?
- Did I avoid generic `wait/hold` language?
- Did I avoid personalized buy/sell instructions?
- If I used `Competitive / Peer Context`, did it stay concise, use real peers/substitutes, and change the judgment rather than pad the answer?

## What this framework rejects

- Generic hedges such as "wait and see," "do not rush," "be careful," or "it depends."
- Company-quality praise that never resolves into a signal judgment.
- Uncertainty language that is not attached to a concrete missing fact, threshold, or catalyst.
- Big disclaimer blocks or repeated "not financial advice" language.
- Unsupported "monitor closely" prose that never names the variable being monitored.
- Peer sections that read like a generic industry roundup rather than a thesis-calibration tool.

## Special cases

The default workflow assumes a profitable single-name large-cap with at least five quarters of history. ETFs, recent IPOs, loss-making names, and spin-offs need different metric choices and emphasis. See [`references/special-cases.md`](references/special-cases.md) before applying the template to those names.

## Quick path (SkillsBench `equity-investment-memo` task)

When the fixture at `/root/data/` is present, the workflow still collapses to four steps:

1. Read `/root/data/question.txt` so the monitor answers what was actually asked.
2. Run `python3 scripts/compute_metrics.py --data-dir /root/data` and keep the output for the Derived Metrics block.
3. Copy `assets/memo-template.md`, fill every placeholder, cite source URLs verbatim from `news.json`, and keep the signal-monitor fields intact even though the output path is still `/root/memo.md`.
4. Run the author checklist at the bottom of the template, then write the result to `/root/memo.md`.

The example at [`assets/example-memo-nvda.md`](assets/example-memo-nvda.md) shows the target tone, density, and anti-waffle behavior.

## Bundled resources

- [`references/sections.md`](references/sections.md) — required section order and field definitions.
- [`references/formulas.md`](references/formulas.md) — TTM and YoY formulas for standard derived metrics.
- [`references/evidence.md`](references/evidence.md) — source-quality framework and citation conventions.
- [`references/special-cases.md`](references/special-cases.md) — metric and framing adjustments for ETFs, loss-makers, IPOs, and spin-offs.
- [`references/data-adapters.md`](references/data-adapters.md) — canonical input schema for the metrics script.
- [`references/task-adapter-skillsbench.md`](references/task-adapter-skillsbench.md) — fixed input/output paths for the legacy SkillsBench task.
- [`assets/memo-template.md`](assets/memo-template.md) — fillable signal-monitor skeleton.
- [`assets/example-memo-nvda.md`](assets/example-memo-nvda.md) — full illustrative example.
- [`scripts/compute_metrics.py`](scripts/compute_metrics.py) — reference adapter for derived metrics.
