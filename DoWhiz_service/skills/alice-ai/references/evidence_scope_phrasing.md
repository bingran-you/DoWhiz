# Alice Evidence-Scope Phrasing

Step 8 requires Alice to make evidence scope visible in human-readable language, not just in JSON.

## Principle

The same underlying fact can require different prose depending on scope.

Examples:

1. `parcel_confirmed` is strong enough for direct parcel language
2. `parcel_candidate` is not strong enough for direct parcel language
3. `listing_derived` is seller-facing and should stay explicitly attributed to the listing
4. `county_level` is jurisdiction or local-surface context, not site confirmation
5. `geography_only` is point- or area-linked rather than parcel-boundary confirmed
6. `inferred` is carried or synthesized context
7. `unresolved` means Alice should say that the fact is not yet defensibly known

## Canonical phrasing style

Renderers should use explicit scope-led phrasing such as:

1. `Parcel-confirmed: County public records corroborate APN ...`
2. `Parcel-candidate: Kern County mapping points to APN ..., but parcel identity is still provisional.`
3. `Listing-derived: The current listing asks $640,000 for 160 acres.`
4. `County-level: Local planning pages are reachable, but parcel-specific zoning was not verified.`
5. `Geography-only: Point-based flood and wetlands screens did not hit the sampled location.`
6. `Inferred: Follow-up context reused the prior subject and coordinates before new retrieval ran.`
7. `Unresolved: No defensible zoning designation was recovered yet.`

These prefixes are intentionally visible.

They help reviewers distinguish evidence quality quickly, and they make evaluation easier later.

## Conflict phrasing

When sources disagree, renderers should not flatten the disagreement.

Preferred style:

1. `Parcel-candidate: County mapping strengthens APN X, but the listing still advertises APN Y.`
2. `Conflict: Listing acreage and county acreage do not match.`
3. `Next action: Confirm the authoritative APN before treating local planning findings as parcel-specific.`

## Module phrasing

Step 9 adds thesis-aware phrasing on top of the same scope rules.

Preferred style:

1. `Parcel-confirmed: Recreational / Rural Hold Screening looks directionally favorable at screening grade.`
2. `Parcel-candidate: Battery Energy Storage Screening stays weak because competing parcel candidates and zoning ambiguity remain active.`
3. `Geography-only: Agriculture Screening relies mainly on point-based soils and flood clues, not parcel-boundary confirmation.`
4. `Listing-derived: Solar Energy Screening still depends heavily on seller-facing acreage and access claims.`

Module prose should still say:

1. what supports the thesis
2. what blocks the thesis
3. what remains unknown

It should not hide scope distinctions just because the content is now more interpretive.

## Unknown phrasing

Unknowns are not the same as risks.

Preferred style:

1. risks explain what could go wrong
2. unknowns explain what Alice still does not know
3. next actions explain how to reduce those unknowns

If the evidence is too thin, Alice should say so directly rather than softening it into vague prose.

## Future renderer rule

Any future Alice channel renderer should either:

1. reuse these exact scope labels, or
2. preserve an equally explicit equivalent

Later steps may improve style, but they should not hide the scope distinction that Step 7 made machine-readable.
