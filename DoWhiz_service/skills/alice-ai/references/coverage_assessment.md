# Alice Coverage Assessment

Coverage assessment is Alice's statement of research footing before retrieval starts.

It combines:

1. jurisdiction context
2. the effective county registry entry when county attachment exists
3. source-registry presence by capability

## What It Means

Coverage assessment is not a parcel memo and not a confidence score.

It answers:

1. what local footing Alice currently has
2. which capabilities are usable, thin, or blocked
3. whether parcel-specific deep research can start safely

## Coverage Tier Is Not Confidence

County `coverage_tier` is registry metadata about source environment.

Its meaning should follow the mechanical rubric documented in `references/coverage_tiers.md`, not ad hoc intuition.

It is **not**:

1. final parcel confidence
2. report completeness
3. certainty that a specific use case will work

Example:

1. a county can have `coverage_tier = full` while a specific parcel is still unresolved
2. a county can have `coverage_tier = minimal` while Alice still gives an honest baseline report using federal sources

## Capability Status

Each capability is assessed independently.

Supported status values:

1. `available`
2. `partial`
3. `minimal`
4. `unavailable`
5. `blocked_by_unresolved_jurisdiction`

`blocked_by_unresolved_jurisdiction` means the issue is not source absence alone. It means Alice cannot safely attach the local source surface yet because county/state footing is still unstable.

## Deep Research Readiness

Use these labels conservatively:

### `deep_research_ready`

County attachment is stable and the critical parcel/local surfaces are present strongly enough to begin real parcel diligence.

### `partially_ready`

Alice can begin some retrieval and baseline diligence, but one or more critical surfaces remain thin.

Typical cases:

1. county is known but zoning is weak
2. county is fallback-only
3. parcel identity is workable but planning depth is limited

### `not_ready`

Alice should not claim parcel-level deep research footing yet.

Typical cases:

1. county is unresolved
2. only a vague subject exists
3. critical surfaces are blocked rather than merely thin

## Only Federal Baseline

`only_federal_baseline_available = true` is an important honesty flag.

It means Alice can still plan flood, wetlands, soils, elevation, broadband, and environmental screening, but local parcel/zoning/tax footing is not yet present.
