# Monitor Sections

The monitor is a single document with the sections below, in order. The structure is intentionally scan-first: the badge, thesis impact, and `Why Now` appear before the supporting evidence so the reader gets the signal quickly without losing the evidence trail.

| # | Section | Why it exists |
|---|---|---|
| 1 | `## Decision Card` | Front-loads the badge plus the supporting signal fields. This is the headline judgment. |
| 2 | `## Why Now` | Explains why the signal matters now, or why there is no material signal now. |
| 3 | `## Dual-Horizon Framing` | Full monitors only. Separates near-term timing from long-term ownership. |
| 4 | `## Verified Facts` | Full monitors only. Bullet list of sourced factual claims. |
| 5 | `## Derived Metrics` | Full monitors only. Computed values with formulas and inputs. |
| 6 | `## Scenarios` | Full monitors only. Bull/base/bear framing with explicit conditions. |
| 7 | `## What Would Change The View` | The explicit thresholds or events that move the signal. |
| 8 | `## Judgment` | Full monitors only. One short paragraph of labelled inference. |
| 9 | `## Evidence Chips` | Concise monitors only. Short sourced fact list in place of the full evidence stack. |

## Required Decision Card fields

The `Decision Card` is a field/value table. Always include:

- `Monitor Status`
- `Signal Direction`
- `Thesis Impact`
- `Signal Quality`
- `Urgency`
- `Confidence`
- `Main Risk To The Signal`
- `Not Personalized Advice`

When the channel or product contract already expects them, also include:

- `New Money Action`
- `Existing Holder Action`

Those action rows are secondary translations. They do not replace the monitor badge.

## Recommended top-to-bottom layout

```markdown
# {TICKER} — Equity Signal Monitor
**As of:** {YYYY-MM-DD} · **Price:** ${PRICE}

**Investor question:** {one-line restatement}

## Decision Card                    (table: Field | Value)
**One-line rationale:** ...
**Main Risk To The Signal:** ...
**Not Personalized Advice:** This is a public-market signal assessment, not personalized financial advice.

## Why Now

## Dual-Horizon Framing             (full monitor only)
### Near-Term Timing View (next 1-2 quarters)
### Long-Term Ownership View (multi-year)

## Verified Facts                   (full monitor only)
- ... ([Source name](URL))

## Derived Metrics                  (full monitor only)
| Metric | Value | Formula / Inputs |

## Scenarios                        (full monitor only)
### Bull Case
### Base Case
### Bear Case

## What Would Change The View
- **Upgrade / Review Now:** ...
- **Downgrade / De-risk:** ...
- **Invalidation:** ...

## Judgment                         (full monitor only)

## Evidence Chips                   (concise monitor only)
- ... ([Source name](URL))
```

## Header conventions

- Use ATX headers (`#`, `##`, `###`).
- Keep `Why Now`, `What Would Change The View`, and `Evidence Chips` as exact strings in concise monitors.
- Keep `Bull Case`, `Base Case`, and `Bear Case` as exact strings in full monitors.
- Prefer short bold labels inside paragraphs over long prose preambles.

## Length and density

- Full monitors should stay roughly one page. If the answer is drifting into a long essay, cut repetition before cutting the signal fields.
- Concise monitors should still feel decision-useful. Short means compressed, not vague.
- Bullets and tables beat generic paragraphs for evidence, scenarios, and thresholds.
