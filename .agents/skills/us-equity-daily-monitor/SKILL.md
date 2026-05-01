---
name: us-equity-daily-monitor
description: Produce a calibrated U.S. equity decision memo or monitor update for one stock or ETF. Use whenever the user wants a decision-useful view on a single U.S. stock or ETF, including prompts like "should I buy NVDA", "is XOM still a hold", "what changed on PLUG", "thoughts on AAPL into earnings", "do I add or trim", or "is there any edge here". This skill separates monitor status from new-money and existing-holder actions, shortens no-edge situations into concise `No Material Change` updates, and forces concrete upgrade, downgrade, and invalidation triggers for neutral stances. Do not use for portfolio construction, crypto, FX, fixed income, multi-asset allocation, or broad macro notes.
---

# U.S. Equity Daily Monitor

Use this skill to produce calibrated decision support, not safe commentary. Decide the monitor mode first, then choose separate actions for new money and existing holders.

## Workflow

1. Restate the investor's actual question in one line.
2. Choose the monitor mode from [`references/action-taxonomy.md`](references/action-taxonomy.md):
   - `No Material Change`
   - `Watch Closely`
   - `Review Now`
3. Pick the output contract from [`references/output-contract.md`](references/output-contract.md):
   - `No Material Change` uses the short update contract.
   - `Watch Closely` and `Review Now` use the full memo contract.
4. Decide the fields in the Decision Card:
   - `Monitor Status`
   - `New Money Action`
   - `Existing Holder Action`
   - `Thesis Impact`
   - `Signal Quality`
   - `Confidence`
5. Build the factual spine and derived metrics. Follow [`references/source-policy.md`](references/source-policy.md), [`references/data-adapters.md`](references/data-adapters.md), and [`references/formulas.md`](references/formulas.md). Use `scripts/compute_metrics.py` when the inputs fit the canonical schema.
6. Run the anti-waffle pass from [`references/anti-waffle.md`](references/anti-waffle.md) before finalizing. If the visible recommendation lands on `Wait`, `Hold`, or `Hold/Do not add`, the artifact must include explicit `Upgrade / Review Now`, `Downgrade / De-risk`, and `Invalidation` triggers with concrete thresholds.
7. Check [`references/monitor-vs-deep-research.md`](references/monitor-vs-deep-research.md) for when to stay short versus when to produce the full memo.
8. Check [`references/special-cases.md`](references/special-cases.md) when the name is an ETF, loss-maker, recent IPO, or spin-off.

## Core rules

- `No Material Change` means the output gets shorter, not safer.
- `Review Now` means the evidence justifies a decision or re-underwrite now. It does not automatically mean `Buy` or `Sell`.
- `Watch Closely` is for mixed but relevant change with concrete confirmation triggers, not filler prose.
- Separate monitor status from the audience-specific actions.
- Every material fact is sourced, every derived metric shows a formula, and every neutral stance has measurable movement criteria.
- Do not pad low-edge cases into essays. If there is no new edge, say so briefly and specify what would change the call.

## Bundled resources

- [`references/output-contract.md`](references/output-contract.md) — exact full-memo and short-update contracts.
- [`references/action-taxonomy.md`](references/action-taxonomy.md) — monitor-mode definitions and allowed action combinations.
- [`references/anti-waffle.md`](references/anti-waffle.md) — generic-language traps and final-artifact self-checks.
- [`references/monitor-vs-deep-research.md`](references/monitor-vs-deep-research.md) — when to stay brief versus when to go full memo.
- [`references/source-policy.md`](references/source-policy.md) — source-tier rules and citation minimums.
- [`references/formulas.md`](references/formulas.md) — standard metric formulas.
- [`references/data-adapters.md`](references/data-adapters.md) — how to coerce upstream data into the metrics script.
- [`references/special-cases.md`](references/special-cases.md) — changes for ETFs, loss-makers, IPOs, and spin-offs.
- [`references/task-adapter-skillsbench.md`](references/task-adapter-skillsbench.md) — fixed I/O paths for the SkillsBench fixture only.
- [`assets/memo-template.md`](assets/memo-template.md) — fillable full-contract skeleton.
- [`assets/example-memo-nvda.md`](assets/example-memo-nvda.md) — calibrated example that shows `Watch Closely` without generic hold/wait waffle.
- [`scripts/compute_metrics.py`](scripts/compute_metrics.py) — deterministic derived-metrics helper.
