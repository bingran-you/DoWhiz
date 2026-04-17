# Alice Evidence Linkage

Step 7 formalizes field-level evidence scope in `alice_land_research`.

Every material wrapped field should be explicit about both:

1. what evidence supports it
2. what geographic or identity scope that evidence actually reaches

For parcel identity specifically, Step 10 also carries `confirmation_basis` so evaluators can tell whether a parcel-confirmed statement is text-corroborated, local-record-corroborated, geometry-confirmed, or still unknown.

## Scope vocabulary

Use these values consistently:

1. `parcel_confirmed`: supported by evidence anchored to the confirmed parcel
2. `parcel_candidate`: supported by evidence tied to one candidate parcel, but not final parcel confirmation
3. `listing_derived`: taken from a listing or seller-facing page
4. `county_level`: tied to county or local public context, but not clearly to one parcel
5. `geography_only`: tied only to a point, county, city, or other non-parcel geography anchor
6. `inferred`: carried forward from prior Alice state or structured inference rather than a fetched record
7. `unresolved`: no defensible scope anchor exists yet

## Required linkage fields

For wrapped report fields, Step 7 uses:

1. `evidence_scope`
2. `supporting_item_ids`
3. `linked_candidate_ids`
4. `citation_ids`

This lets downstream systems answer:

1. which extracted items support a claim
2. which parcel candidate a claim belongs to
3. whether the claim is parcel-safe, candidate-level, or only geography-level

## Practical examples

1. Listing acreage should normally be `listing_derived`.
2. County APN plus site address on a parcel page may support `parcel_confirmed` or `parcel_candidate`, depending on competing evidence.
3. If parcel identity is `parcel_confirmed`, the candidate or parcel record should also say *how* it was confirmed via `confirmation_basis`.
4. Zoning text on a local planning page should stay `parcel_candidate` unless parcel identity is stabilized strongly enough.
5. FEMA, wetlands, soils, and elevation point queries should stay `geography_only` unless a later parcel-geometry step upgrades them.
6. Session-state carry-forward APNs should usually stay `inferred` until fresh public evidence corroborates them.

## Reporting rule

Alice should prefer a weaker truthful scope over a stronger but unsupported one.

If a field could be rendered as either:

1. a parcel fact with hidden uncertainty, or
2. a candidate-level or geography-level fact with explicit scope,

choose the second option.
