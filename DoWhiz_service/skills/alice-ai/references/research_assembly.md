# Alice Research Assembly

Step 9 assembles a stronger `alice_land_research` object from:

1. request normalization
2. subject resolution
3. jurisdiction context
4. coverage assessment
5. source planning
6. live retrieval outputs
7. extracted evidence
8. parcel candidates
9. use-case screening logic

The assembly target is still intentionally universal and conservative, but Step 7 adds a formal parcel-candidate layer and field-level evidence scope.

## Assembly philosophy

Alice should only populate facts that are supported by:

1. a fetch-log entry
2. a raw evidence artifact or extracted evidence item
3. a citation carried into the final research object

If one of those links is missing, the field should stay `missing`, `estimated`, or `conflicting`.

Step 7 added one more requirement, and Step 9 keeps it:

4. parcel-specific claims should flow through `parcel_candidates.json` instead of being inferred ad hoc

## Universal sections in Step 9

Step 9 still keeps the universal baseline sections of `alice_land_research` intact:

1. request context
2. subject
3. listing
4. source URLs
5. jurisdiction
6. parcel identity
7. planning and land use
8. environmental constraints
9. water and agriculture
10. infrastructure and utilities
11. market signals
12. risks
13. unknowns
14. next actions
15. citations
16. scores

Step 9 now also populates:

17. use-case modules
18. directional economics

These are still screening-grade interpretation layers on top of the universal object.

## Parcel-candidate-first assembly

Step 7 should assemble parcel-sensitive sections with this precedence:

1. build parcel candidates from subject carry-forward plus fetched clues
2. determine whether one candidate is strong, weak, competing, grouped, or still non-viable
3. determine confirmation basis for the leading candidate without overstating geometry
4. only then assign parcel-identity fields and parcel-sensitive evidence scopes

This keeps listing clues, county clues, and geography-only signals from being silently mixed together.

## Evidence mapping rules

### Listing-derived evidence

Listing sources can populate:

1. platform
2. canonical URL
3. asking price
4. listed acreage
5. listing text summary
6. address-like clues
7. APN clues
8. coordinates if clearly present

Listing evidence should stay in listing or market-context roles unless later official sources confirm the same fact.

When listing clues are reused in parcel identity, their field wrapper should still say `listing_derived` unless later evidence upgrades the scope.

### Official-source evidence

Official sources should dominate factual claims when available:

1. county/state/federal sources outrank listing pages for parcel or regulatory facts
2. federal point queries can populate directional environmental and terrain signals
3. official county/local page-access evidence can justify “surface exists / page reachable / next local step available” style claims, but not parcel-specific conclusions unless parcel-specific data was actually retrieved

## Evidence scope rules

Step 7 made scope explicit on material wrapped fields, and Step 9 must preserve that scope inside module reasoning:

1. `parcel_confirmed`
2. `parcel_candidate`
3. `listing_derived`
4. `county_level`
5. `geography_only`
6. `inferred`
7. `unresolved`

Use the weakest truthful scope that the evidence supports.

## Step 9 interpretation layer

Step 9 should interpret the assembled object rather than rebuilding it.

That means:

1. modules consume assembled evidence
2. modules do not fetch new live data
3. modules do not silently upgrade evidence scope
4. directional economics stays downstream from module output

## Conflict handling

Step 7 and Step 9 must preserve conflicts instead of smoothing them away.

Examples:

1. listing acreage differs from official acreage
2. listing address text differs from county site address
3. one source implies a flood overlay while another point query does not

Parcel identity should also preserve confirmation nuance:

1. `parcel_confirmed + local_record_corroborated` is materially different from `parcel_confirmed + geometry_confirmed`
2. Step 10 should surface that distinction in `parcel_identity` and per-parcel records

Use `identity_conflicts`, `risks`, `unknowns`, and `score_notes` to keep these disagreements explicit.

## Unknowns and next actions

Unknowns should be emitted when a later decision would materially change the conclusion.

Common Step 7 unknowns:

1. confirmed parcel/APN still missing
2. zoning still not directly verified
3. no parcel-boundary-based flood or wetlands confirmation
4. utility territory/interconnection still unresolved
5. water-rights status still unresolved

Next actions should point to the next best concrete source or diligence step, not vague suggestions.

## Confidence vs completeness

Assembly must keep confidence and completeness separate:

1. confidence asks whether the populated claims look well-supported
2. completeness asks how much of the diligence surface Alice actually covered

Examples:

1. a fallback-county memo can have moderate confidence on a few supported facts and low completeness overall
2. a richer follow-up case with parcel-corroborated local clues can improve identity confidence while still keeping environmental sections geography-only

## Step 9 use-case modules

Step 9 adds a screening layer for:

1. solar
2. wind
3. battery storage
4. agriculture
5. residential / light development
6. industrial / storage
7. recreational / rural hold

Modules should output:

1. fit assessment
2. module status
3. supporting signals
4. blocking flags
5. key unknowns
6. economics inputs still required
7. evidence scope summary

These are still screening-grade calls.

## Step 9 directional economics

Step 9 directional economics should:

1. frame scenario cases honestly
2. keep assumptions explicit
3. show missing cost buckets
4. avoid pseudo-precision

If price basis or local footing is thin, the economics section should stay `limited` or `not_enough_data`.

## What Step 9 intentionally leaves thin

Step 9 does not yet attempt:

1. final underwriting
2. robust comparable-sales analysis
3. definitive zoning interpretation
4. guaranteed development feasibility
5. advanced underwriting
6. recommendation ranking
7. broad new retrieval breadth

Those remain future layers on top of the universal research object.

## Step 8+ handoff

Step 8 rendering still consumes the assembled research object rather than replacing it, and Step 9 enriches what that renderer can say.

The rendering layer should:

1. treat `alice/parcel_memo.json` as the structured system of record
2. derive `alice/report_summary.json` as a compact delivery-layer summary
3. derive `alice/report.md` as the canonical human-readable memo
4. derive Slack and email text from the same summary layer
5. keep conflicts, unknowns, and evidence scope visible instead of smoothing them away in prose

Rendering is downstream from assembly.

If a fact is not explicit enough in `alice_land_research`, the fix belongs in the assembly layer rather than in memo prose.
