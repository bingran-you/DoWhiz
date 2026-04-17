# Alice Parcel Candidates

`parcel_candidates.json` is Alice's Step 7 bridge between raw evidence extraction and final land-research assembly.

It answers a narrower question than the final report:

1. What parcel or parcel-candidate set do the current clues point to?
2. How strong is each candidate?
3. Which clues support or contradict each candidate?
4. Is Alice safe to treat any finding as parcel-confirmed yet?

## Why this artifact exists

Listing URLs, APNs, county pages, and planning pages rarely line up perfectly on the first pass.

Without a dedicated parcel-candidate layer, Alice would be forced to do one of two bad things:

1. overclaim parcel identity from weak evidence
2. throw away useful but incomplete parcel clues

The candidate layer preserves the middle ground.

## Candidate set statuses

`candidate_set_status` should be interpreted conservatively:

1. `single_strong_candidate`: one candidate clearly dominates and is supported by corroborating clues
2. `single_weak_candidate`: one candidate exists, but the evidence is still thin
3. `multiple_competing_candidates`: materially different parcel candidates remain active
4. `parcel_group_case`: evidence suggests a multi-parcel offering rather than one parcel
5. `no_viable_candidate`: current evidence does not support a usable parcel candidate yet

## Confirmation levels

Per-candidate `confirmation_level` is intentionally stricter than "best guess":

1. `parcel_confirmed`: strong corroboration from local public-record clues exists and no active competing candidate is stronger
2. `candidate_corroborated`: evidence is strong enough to treat the candidate as the leading parcel target, but not as final parcel confirmation
3. `candidate_unconfirmed`: there is a usable candidate, but it still needs more corroboration
4. `listing_hint_only`: the candidate is mostly anchored by listing clues
5. `geography_only`: the candidate is really a point or place anchor, not a parcel anchor
6. `unresolved`: current clues do not justify candidate-level reliance

## Confirmation basis

The stabilization patch adds `confirmation_basis` so `parcel_confirmed` is not treated as one undifferentiated bucket.

Supported values:

1. `text_corroborated`
   - multiple text-level clues line up, but Alice still lacks geometry confirmation
2. `local_record_corroborated`
   - county or city-local public-record clues line up strongly enough to anchor the candidate conservatively
3. `geometry_confirmed`
   - boundary geometry was actually confirmed
4. `unknown`
   - current evidence does not justify a stronger basis label yet

Important:

1. `parcel_confirmed` plus `local_record_corroborated` is still not the same thing as `geometry_confirmed`
2. current pilot cases should mostly stay `local_record_corroborated` or `unknown`
3. Alice should not emit `geometry_confirmed` unless a real boundary-confirmation step exists

## Merge rules

Step 7 should merge clues conservatively:

1. Same-source APN and address clues may merge into one candidate when they clearly belong to the same record context.
2. Matching APNs across listing and county/local sources should strengthen one candidate.
3. Matching address plus acreage plus county/local APN should strengthen one candidate materially.
4. Conflicting APNs should create competing candidates rather than silent merging.
5. Weak subject-seed candidates should be pruned when stronger APN-backed candidates exist.

## What parcel candidates are not

`parcel_candidates.json` is not:

1. a legal parcel determination
2. a geometry-confirmed parcel-boundary result
3. a title or deed chain result
4. a final report

Later steps can use this artifact to decide:

1. whether parcel-specific local claims are safe
2. whether findings should stay candidate-level
3. what next resolution actions should be recommended
