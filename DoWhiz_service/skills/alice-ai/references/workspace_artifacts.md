# Alice Workspace Artifacts

Alice uses structured workspace artifacts under an `alice/` directory inside a DoWhiz thread workspace.

This document defines the expected artifact names and their intended roles. In this implementation step, only the contract is being formalized. Future pipeline work will decide exactly when each artifact is produced.

## Layout

```text
alice/
  session_state.json
  request_normalized.json
  subject_resolution.json
  jurisdiction_context.json
  source_plan.json
  source_fetch_log.json
  coverage_assessment.json
  parcel_memo.json
  report.md
  report_summary.json
  recommendation/
    candidate_universe.json
    shortlist.json
```

## Artifact contract

| Artifact | When it exists | What it contains | Required now? | Future expectation |
| --- | --- | --- | --- | --- |
| `alice/session_state.json` | Any multi-turn Alice thread | Active subject assumptions, current thesis, last known report refs, unresolved follow-ups | Contract only | Recommended for follow-up flows |
| `alice/request_normalized.json` | After request normalization | Canonical request envelope validating against `schemas/alice_request.schema.json` | Yes, contractually | Required for both recommendation and deep research |
| `alice/subject_resolution.json` | After subject-resolution phase | Candidate parcel matches, listing-to-parcel mapping, resolution status, identity conflicts | Contract only | Required before full parcel-level conclusions |
| `alice/jurisdiction_context.json` | After jurisdiction assembly | County, city, state, federal, and special-district context for the active subject | Contract only | Recommended for all parcel research flows |
| `alice/source_plan.json` | After source planning | Ordered source plan by capability, priority, and jurisdiction relevance | Contract only | Required once source retrieval is implemented |
| `alice/source_fetch_log.json` | After any source retrieval | Provenance log of source requests, retrieval times, parse notes, and failures | Contract only | Required for reproducibility and audits |
| `alice/coverage_assessment.json` | After coverage evaluation | Coverage tier and section-level availability of local/state/federal diligence surfaces | Contract only | Required once county/source discovery exists |
| `alice/parcel_memo.json` | After structured synthesis | Canonical land research object validating against `schemas/alice_land_research.schema.json` | Yes, contractually | Core system-of-record artifact |
| `alice/report.md` | After report rendering | Human-readable markdown view over `parcel_memo.json` | Contract only | Recommended for deep research and batch comparison |
| `alice/report_summary.json` | After rendering a user-facing summary | Channel-ready summary metadata, headline findings, blockers, and score snippets | Contract only | Recommended for Slack/email delivery layers |
| `alice/recommendation/candidate_universe.json` | Recommendation flow only | Normalized candidate set before ranking and pruning | Contract only | Required once recommendation acquisition exists |
| `alice/recommendation/shortlist.json` | Recommendation flow only | Ranked shortlist and rationale for surfaced candidates | Contract only | Required for recommendation output |

## Schema-backed artifacts in this step

This step formalizes schemas for:

1. `alice/request_normalized.json`
2. `alice/parcel_memo.json`

It also formalizes registry row schemas that future source-planning code should use:

1. county coverage registry entries
2. source descriptor entries

This step does **not** yet create dedicated schemas for:

1. `subject_resolution.json`
2. `jurisdiction_context.json`
3. `source_plan.json`
4. `source_fetch_log.json`
5. `coverage_assessment.json`
6. `report_summary.json`
7. recommendation artifacts

Those artifact names are contractually reserved here so future implementation can fill them in without changing the workspace shape later.

## Naming rule

Future Alice implementations should reuse these filenames exactly unless there is a strong platform-wide reason to migrate them. Stable artifact naming matters for:

1. validation
2. test fixtures
3. report rendering
4. evaluation harnesses
5. follow-up conversational continuity
