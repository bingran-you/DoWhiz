# Alice Phase 1 Output

Alice phase 1 is now a measurable, end-to-end land research pipeline rather than a loose collection of schemas and examples.

It accepts normalized land-investor requests, resolves the subject conservatively, plans source usage, executes a limited public-data retrieval layer, assembles a structured land research object, renders human-readable reports, runs screening-grade thesis modules, and produces shortlist recommendations from the candidate universe Alice actually observed.

## What phase 1 does now

1. Resolves land subjects from listing URLs, APNs, addresses, coordinates, follow-up context, and batch-compare inputs.
2. Builds jurisdiction context, county coverage assessment, and deterministic source plans before retrieval.
3. Executes limited official-source-first retrieval across the current seeded source surface.
4. Preserves raw evidence, extracted evidence, source fetch logs, citations, and field-level evidence scope.
5. Produces parcel candidates with explicit confirmation level and `confirmation_basis`.
6. Assembles a canonical `alice_land_research` object in `alice/parcel_memo.json`.
7. Renders a markdown memo plus Slack and email summary views from the same structured research object.
8. Runs screening-grade use-case modules for energy, agriculture, residential/light development, industrial/storage, and recreational/rural hold.
9. Produces directional economics scaffolding that stays explicitly directional rather than pretending to be underwriting.
10. Ranks user-supplied or observed listing candidates into a shortlist, while preserving blockers, unknowns, evidence quality, and candidate-universe limitations.

## Supported input forms

1. Listing URL requests
2. APN requests
3. Address requests
4. Coordinate requests
5. Follow-up requests with inherited session-state context
6. Batch compare requests
7. Recommendation requests for user-supplied candidate lists or thin open discovery across the observed listing universe

## Canonical output chain

Phase 1 now treats structured JSON as the system of record and rendering as a downstream layer.

Key structured artifacts:

1. `alice/request_normalized.json`
2. `alice/subject_resolution.json`
3. `alice/jurisdiction_context.json`
4. `alice/coverage_assessment.json`
5. `alice/source_plan.json`
6. `alice/source_fetch_log.json`
7. `alice/extracted_evidence.json`
8. `alice/parcel_candidates.json`
9. `alice/parcel_memo.json`
10. `alice/report_summary.json`
11. `alice/recommendation/candidate_universe.json`
12. `alice/recommendation/shortlist.json`

Key human-readable artifacts:

1. `alice/report.md`
2. `alice/report_slack.txt`
3. `alice/report_email.txt`
4. `alice/recommendation/shortlist_report.md`
5. `alice/recommendation/shortlist_slack.txt`
6. `alice/recommendation/shortlist_email.txt`

## Evaluation harness

Step 11 adds a unified runner at [validate_step11.py](/Users/yegaoyang/Desktop/workspace/DoWhiz/DoWhiz_service/skills/alice-ai/scripts/validate_step11.py). It orchestrates the committed Alice validators and packages the results into [evaluation_report.schema.json](/Users/yegaoyang/Desktop/workspace/DoWhiz/DoWhiz_service/skills/alice-ai/schemas/evaluation_report.schema.json).

Current benchmark buckets:

| Bucket | Primary validator | Current coverage |
| --- | --- | --- |
| Registry / coverage | `validate_registry.py` | 3,235 county rows, 2 curated overrides, 20 source descriptors |
| Subject resolution | `validate_subject_resolution.py` | 12 request examples, 12 subject-resolution examples, follow-up session-state coverage |
| Planning / source plan | `validate_step5.py` | 7 planning scenarios |
| Retrieval / extraction / assembly | `validate_step6.py` | 4 fixture-backed workspaces |
| Parcel candidates | `validate_step7.py` | 5 scenario workspaces, including the wind exercise case |
| Report rendering | `validate_step8.py` | 5 markdown/Slack/email rendering scenarios |
| Use-case modules | `validate_step9.py` | 5 thesis-screening scenarios |
| Recommendation / shortlist | `validate_step10.py` | 4 recommendation scenarios |
| Stabilization semantics | `validate_step11.py` internal checks | access_mode, confirmation_basis, coverage-tier rubric, wind scenario, truthfulness guardrails |

## Truthfulness checks now made explicit

The Step 11 runner turns important Alice guardrails into reportable checks rather than relying only on pass/fail script output.

Highlighted checks:

1. No exhaustive-market language in recommendation renders
2. Unknown visibility in weak-data outputs
3. Conflict visibility in competing-candidate outputs
4. Scope-aware report phrasing instead of unsupported certainty language
5. `access_mode` presence and interface compatibility across source descriptors
6. `confirmation_basis` presence in parcel candidates, parcel memo output, and rendered summaries
7. No accidental `geometry_confirmed` in committed phase-1 scenarios
8. Coverage-tier rubric consistency across curated overrides and generated fallback counties
9. Weak-data economics remaining directional and non-underwriting
10. Wind module execution, rendering, and economics scaffolding coverage

## Release-readiness view

| Area | Phase 1 status | Notes |
| --- | --- | --- |
| Subject resolution | Ready for internal use | Conservative normalization and follow-up inheritance are covered by examples and validators |
| Planning and source plans | Ready for internal use | Structured county-coverage and source-planning chain is in place |
| Retrieval / extraction | Limited but real | Works on the current seeded source surface, mostly via deterministic fixtures |
| Parcel identity | Conservative beta | Stronger than Step 6, but still mostly text/local-record corroboration |
| Report rendering | Ready for internal use | Markdown is the canonical human-readable output; Slack/email are derived views |
| Use-case screening | Screening-grade beta | Useful for thesis framing, not entitlement or interconnection certainty |
| Directional economics | Screening-grade beta | Explicitly non-underwriting |
| Recommendation / shortlist | Limited beta | Honest observed-universe ranking, not full-market discovery |
| Truthfulness guardrails | Ready for internal evaluation | Step 11 now makes the main guardrails mechanically checkable |

## Current operating posture

Phase 1 is appropriate for:

1. Internal evaluation
2. Controlled team demos
3. Limited pilot use where users understand the observed-universe and pilot-county limits

Phase 1 is not yet positioned as:

1. exhaustive market coverage
2. parcel-fabric-grade parcel confirmation nationwide
3. final underwriting or feasibility
4. polished end-user product delivery

## Primary limitations

1. Retrieval breadth is intentionally narrow and strongest in Hudspeth County, TX and Kern County, CA.
2. The evaluation suite is strong for deterministic regression, but not yet broad enough to claim full real-world calibration.
3. Open discovery remains a thin observed-universe feature rather than a market crawler.
4. Directional economics remain a structured honesty layer, not a final investment model.
5. Recommendation quality is still tightly bounded by candidate-universe quality and evidence depth.
