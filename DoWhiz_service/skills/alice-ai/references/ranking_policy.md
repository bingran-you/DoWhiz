# Alice Ranking Policy

Step 10 uses ordered, interpretable ranking rules rather than opaque score soup.

## Core ordering

Alice ranks candidates in this order:

1. hard request mismatches first
2. thesis-fit modules next
3. blocker burden
4. unknown burden
5. evidence quality and completeness
6. parcel identity strength
7. coverage and local-footing strength
8. directional economics availability
9. remaining tie-breaks such as price or label order

This keeps ranking explainable.

## Hard filters

Step 10 can filter or heavily penalize candidates for:

1. geography mismatch
2. budget mismatch
3. acreage mismatch
4. price-per-acre mismatch
5. unsupported candidate footing
6. explicit request exclusions when the evidence is strong enough

Hard-filtered candidates stay visible in the artifacts instead of disappearing silently.

## Module fit

Step 9 module outputs are the leading thesis-fit input.

Alice looks at:

1. `fit_assessment`
2. `module_status`
3. module blockers
4. module unknowns

These modules matter, but they do not override evidence quality or parcel-identity weakness automatically.

## Evidence and identity

Two candidates with similar thesis fit should not rank the same if one has:

1. parcel-confirmed identity
2. stronger local coverage
3. better overall confidence and completeness

Step 10 therefore rewards:

1. `parcel_confirmed` over `candidate_corroborated`
2. `candidate_corroborated` over weak or unresolved parcel states
3. stronger local footing over federal-baseline-only footing

## Dedupe

Step 10 dedupes conservatively.

Confirmed duplicate collapse is strongest when:

1. canonical listing URL matches exactly
2. later candidate workspaces clearly point to the same observed listing

Potential duplicates that are not strong enough for collapse should stay separate with notes.

## Request preferences

Step 10 can also reflect explicit preferences such as:

1. transmission proximity
2. avoiding major floodplain burden
3. requiring some access footing
4. budget or acreage targets

These preferences influence ranking only when current evidence is strong enough to justify them.

## Why the shortlist may look conservative

A candidate can rank below another even when the listing sounds exciting if:

1. parcel identity is weaker
2. local coverage is thinner
3. blockers are already visible
4. too many critical unknowns remain

That conservatism is intentional.

## Output expectations

Every ranked item should preserve:

1. why it ranked there
2. what the biggest blockers are
3. what remains unknown
4. how evidence quality limits the recommendation

This is a shortlist from the observed universe, not a hidden scoring black box.
