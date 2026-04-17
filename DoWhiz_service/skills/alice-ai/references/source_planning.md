# Alice Source Planning

Source planning turns Step 3 registry data plus Step 4/5 artifacts into a retrieval plan.

It is still planning, not fetching.

## Inputs

Source planning consumes:

1. normalized request intent
2. subject resolution
3. jurisdiction context
4. coverage assessment
5. county/source registry data

## Outputs

The output is a structured `source_plan.json` that says:

1. which capabilities matter next
2. which source descriptors should be attempted
3. what order makes sense
4. what is blocked or deferred

Planning should now also respect descriptor `access_mode`:

1. `machine_endpoint` sources are the cleanest candidates for direct execution
2. `landing_page` sources are usually page-access or download-entry steps until a deeper adapter exists
3. `viewer` sources should be treated as surface visibility, not as proof of machine query support
4. `mixed` sources may still be worth planning early, but their machine-executable path should stay explicit

## Priority Philosophy

Default priority:

1. official county or city-local sources when county attachment is stable
2. official state sources when they improve parcel geometry or infrastructure footing
3. federal baseline sources for overlays and screening
4. listing-platform sources for listing context and market framing

Listing platforms should never outrank authoritative parcel, tax, zoning, or planning records for parcel-level claims.

## Capability Grouping

Step 5 groups sources by capability rather than by raw source list.

Examples:

1. `parcel_identity`
2. `parcel_geometry`
3. `zoning`
4. `planning_docs`
5. `environmental_baseline`
6. `water_signals`
7. `utilities`
8. `transmission`
9. `market_context`

This keeps future retrieval orchestration modular and easier to test.

## Deferred vs Blocked

### Deferred

Use when the source exists, but Alice should wait until an earlier dependency is satisfied.

Examples:

1. recommendation mode before a parcel shortlist exists
2. city-scoped source where city applicability is still unknown

### Blocked

Use when Alice should not attempt the step yet because jurisdiction or subject footing is insufficient.

Examples:

1. no county for local tax lookup
2. no stable parcel candidate for zoning verification

## Why This Matters

Planning before retrieval forces Alice to explain:

1. why a source is in scope
2. why it is not in scope yet
3. what unknowns still prevent stronger conclusions
