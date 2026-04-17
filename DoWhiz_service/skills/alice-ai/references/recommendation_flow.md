# Alice Recommendation Flow

Step 10 adds Alice's first usable recommendation layer.

The goal is not full market coverage.

The goal is to produce an honest shortlist from the candidate universe Alice actually observed and could process.

## Recommendation inputs

Recommendation flow starts from:

1. `alice/request_normalized.json`
2. `alice/subject_resolution.json`
3. existing Step 6-9 candidate artifacts when available, especially:
   `parcel_memo.json`, `coverage_assessment.json`, `parcel_candidates.json`, and `report.md`

## Recommendation modes in v0

Step 10 supports three practical execution modes:

1. `user_supplied_ranking`
   rank listing URLs the user supplied directly
2. `open_discovery`
   filter Alice's current observed listing catalog by thesis and request constraints
3. `prior_set_refinement`
   reserved for recommendation follow-ups that narrow or refine an existing set

The first two are the main supported paths in v0.

## Candidate acquisition in v0

Step 10 acquires candidates from:

1. user-supplied listing URLs
2. Alice's observed listing catalog built from previously processed Step 6-9 candidate workspaces

This is intentionally narrow.

Alice does **not** yet claim:

1. live exhaustive listing search across supported platforms
2. off-market discovery
3. background crawling coverage

## Candidate universe artifact

`alice/recommendation/candidate_universe.json` is the system of record for recommendation acquisition.

It preserves:

1. how each candidate was observed
2. what listing hints were available
3. which candidates were deduped
4. which candidates were already hard mismatches
5. what the observed-universe limitations were

## Reusing the research pipeline

Step 10 does not invent a separate shallow land scorer.

Instead it reuses existing Alice outputs:

1. universal parcel memo fields
2. parcel-identity strength from `parcel_candidates.json`
3. coverage and local-footing signals from `coverage_assessment.json`
4. Step 9 use-case modules and directional economics, recalculated against the active recommendation thesis

This keeps ranking consistent with the rest of the Alice pipeline.

## Shortlist artifact

`alice/recommendation/shortlist.json` records:

1. ranking basis
2. ranked candidates
3. filtered candidates
4. ranking factors
5. blockers and unknowns
6. candidate-universe limitation notes

`alice/recommendation/shortlist_report.md` is the canonical human-readable recommendation memo.

Slack and email recommendation summaries are derived from the same shortlist artifacts.

## Truthfulness rule

Recommendation output should say:

1. "from the candidates Alice was able to observe"
2. "from the listings reviewed"
3. "this shortlist is limited by observed candidate coverage"

Recommendation output should **not** imply:

1. full market coverage
2. best parcel in the whole market
3. ranking confidence independent of evidence quality

## What remains future work

Later steps can expand:

1. live search breadth
2. stronger prior-shortlist refinement
3. broader parcel coverage
4. richer portfolio comparison
5. more detailed economics and underwriting
