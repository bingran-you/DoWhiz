# Output Contract

Validate the final user-visible artifact, not just your internal notes.

## Full memo contract

Use this for `Watch Closely` and `Review Now`.

Required order:

1. Header with `As of:`, `Price:`, and `Investor question:`
2. `## Decision Card`
3. `## Dual-Horizon Framing`
4. `## Verified Facts`
5. `## Derived Metrics`
6. `## Scenarios`
7. `## Triggers — Verdict Movement`
8. `## Judgment`

Decision Card fields:

- `Monitor Status`
- `New Money Action`
- `Existing Holder Action`
- `Thesis Impact`
- `Signal Quality`
- `Confidence`
- `One-line rationale:`

Use these exact sub-sections:

- `### Near-Term Timing View`
- `### Long-Term Ownership View`
- `### Bull Case`
- `### Base Case`
- `### Bear Case`

Use these exact trigger labels:

- `Upgrade / Review Now`
- `Downgrade / De-risk`
- `Invalidation`

## Short update contract

Use this for `No Material Change`.

Required order:

1. Header with `As of:`, `Price:`, and `Investor question:`
2. `## Decision Card`
3. `## Why Now`
4. `## What Would Change The View`
5. `## Evidence Chips`

The short update should stay extremely concise. Target roughly 250 to 1200 characters of visible text.
If the artifact already fits this shape by inspection, finalize it instead of running extra dump-the-whole-artifact checks.

Inside `## What Would Change The View`, keep these exact trigger labels:

- `Upgrade / Review Now`
- `Downgrade / De-risk`
- `Invalidation`

## Shared minimums

- keep clickable source links close to factual claims
- include at least 3 clickable links for the full memo and at least 2 for the short update
- keep the artifact scan-first
- if the user says "only tell me if I should act" and the correct mode is `No Material Change`, do not write a mini-memo
- if the recommendation is `Wait`, `Hold`, or `Hold/Do not add`, the trigger block must make the next decision mechanical
- if the scenario is synthetic or assumption-based, say `assumption-based` explicitly in the visible artifact
- synthetic assumption-only outputs do not need issuer evidence links, but they also must not use generic market homepages as fake evidence
- synthetic assumption-only outputs should cap `Confidence` at `Medium` unless real issuer-specific evidence is actually verified and linked
