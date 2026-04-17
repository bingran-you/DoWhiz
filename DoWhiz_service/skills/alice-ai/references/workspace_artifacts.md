# Alice Workspace Artifacts

Alice uses structured workspace artifacts under an `alice/` directory inside a DoWhiz thread workspace.

This document defines the expected artifact names and their intended roles.

Some artifacts are fully implemented through Step 10. Others remain reserved so later Alice steps can expand coverage without changing the workspace shape.

## Layout

```text
alice/
  session_state.json
  request_normalized.json
  subject_resolution.json
  subject_resolution_summary.json
  jurisdiction_context.json
  coverage_assessment.json
  source_plan.json
  source_plan_summary.json
  raw_evidence/
    <source_id>/
      *.html
      *.json
      *.xml
      *.txt
      *.headers.json
  source_fetch_log.json
  extracted_evidence.json
  parcel_candidates.json
  parcel_memo.json
  research_assembly_summary.json
  report.md
  report_summary.json
  report_slack.txt
  report_email.txt
  recommendation/
    candidate_universe.json
    shortlist.json
    shortlist_report.md
    shortlist_slack.txt
    shortlist_email.txt
```

## Artifact contract

| Artifact | When it exists | What it contains | Required now? | Future expectation |
| --- | --- | --- | --- | --- |
| `alice/session_state.json` | Any multi-turn Alice thread | Active subject assumptions, current thesis, last known report refs, unresolved follow-ups | Contract only | Recommended for follow-up flows |
| `alice/request_normalized.json` | After request normalization | Canonical request envelope validating against `schemas/alice_request.schema.json` | Yes, contractually | Required for both recommendation and deep research |
| `alice/subject_resolution.json` | After subject-resolution phase | Subject-resolution artifact validating against `schemas/subject_resolution.schema.json`, including normalized inputs, active subjects, parcel candidates, ambiguities, and next actions | Yes, contractually | Required before full parcel-level conclusions |
| `alice/subject_resolution_summary.json` | After subject-resolution phase | Compact summary of the active subject set, status, primary label, and blockers for channel-friendly checkpoints | Contract only | Recommended for future Slack/email checkpoint responses |
| `alice/jurisdiction_context.json` | After jurisdiction assembly | Jurisdiction-context artifact validating against `schemas/jurisdiction_context.schema.json`, including explicit vs inferred vs inherited geography, registry attachment, and unresolved questions | Yes, contractually | Required before county-aware coverage assessment or source planning |
| `alice/coverage_assessment.json` | After coverage evaluation | Coverage-assessment artifact validating against `schemas/coverage_assessment.schema.json`, including effective county tier, capability footing, missing surfaces, and deep-research readiness | Yes, contractually | Required before later synthesis can claim local diligence footing |
| `alice/source_plan.json` | After source planning | Source-plan artifact validating against `schemas/source_plan.schema.json`, including capability groups, source priorities, deferred steps, blocked steps, and fallback logic | Yes, contractually | Required once source retrieval is implemented |
| `alice/source_plan_summary.json` | After source planning | Compact summary of plan status, planned group counts, blockers, and missing capabilities for channel-friendly status messages | Contract only | Recommended for Slack/email checkpoints before retrieval |
| `alice/raw_evidence/` | After any successful live or fixture-backed retrieval | Stored raw payloads, minimal text snapshots, and response header artifacts keyed by source and fetch entry | Yes in Step 6 when retrieval succeeds | Required for reproducibility and parser debugging |
| `alice/source_fetch_log.json` | After any source retrieval | Schema-backed provenance log of source attempts, request descriptors, artifact refs, and blocked/unsupported/failure states | Yes, contractually in Step 6 | Required for reproducibility and audits |
| `alice/extracted_evidence.json` | After extraction from raw evidence | Schema-backed extracted facts keyed back to fetch-entry IDs, citation IDs, and section hints | Yes, contractually in Step 6 | Required before research assembly should claim extracted facts |
| `alice/parcel_candidates.json` | After parcel-candidate synthesis | Schema-backed intermediate candidate set linking APN/address/acreage clues to one strong candidate, multiple competing candidates, or no viable candidate, including explicit `confirmation_basis` for the leading parcel clue path | Yes, contractually in Step 7 | Required before parcel-level findings should be marked as parcel-confirmed or parcel-candidate-linked |
| `alice/parcel_memo.json` | After structured synthesis | Canonical land research object validating against `schemas/alice_land_research.schema.json`, including universal findings, use-case modules, directional economics, and parcel-identity `confirmation_basis` | Yes, contractually | Core system-of-record artifact |
| `alice/research_assembly_summary.json` | After structured synthesis | Compact summary of report type, confirmation level, confidence/completeness, and blocker counts for channel-friendly checkpoints | Contract only in earlier steps; written in Step 7 | Recommended for future Slack/email checkpoint responses after live retrieval |
| `alice/report.md` | After report rendering | Canonical markdown memo derived from `parcel_memo.json`, `parcel_candidates.json`, and nearby Step 5-7 context artifacts | Yes, contractually in Step 8 | Canonical human-readable deep research artifact |
| `alice/report_summary.json` | After rendering a user-facing summary | Schema-backed normalized summary layer validating against `schemas/report_summary.schema.json`, including title, conclusion, evidence panel, findings, risks, unknowns, next actions, and grouped sources | Yes, contractually in Step 8 | Shared delivery contract for markdown, Slack, and email views |
| `alice/report_slack.txt` | After Slack-style rendering | Concise decision-oriented Slack summary derived from `report_summary.json` | Yes, derived in Step 8 | Recommended for channel delivery or notifications |
| `alice/report_email.txt` | After email-style rendering | Email-friendly summary derived from `report_summary.json` | Yes, derived in Step 8 | Recommended for outbound summary delivery |
| `alice/recommendation/candidate_universe.json` | Recommendation flow only | Schema-backed candidate-universe artifact validating against `schemas/recommendation_candidates.schema.json`, including acquisition path, dedupe, and observed-universe limitations | Yes, contractually in Step 10 | Required before shortlist ranking or recommendation rendering |
| `alice/recommendation/shortlist.json` | Recommendation flow only | Schema-backed shortlist artifact validating against `schemas/shortlist.schema.json`, including ranked items, filtered candidates, ranking factors, blockers, unknowns, and universe-limit notes | Yes, contractually in Step 10 | Required for recommendation output |
| `alice/recommendation/shortlist_report.md` | Recommendation flow only | Canonical markdown shortlist memo derived from `candidate_universe.json` and `shortlist.json` | Yes, derived in Step 10 | Canonical human-readable recommendation artifact |
| `alice/recommendation/shortlist_slack.txt` | Recommendation flow only | Slack-style recommendation summary derived from the shortlist artifacts | Yes, derived in Step 10 | Recommended for channel delivery |
| `alice/recommendation/shortlist_email.txt` | Recommendation flow only | Email-friendly recommendation summary derived from the shortlist artifacts | Yes, derived in Step 10 | Recommended for outbound recommendation delivery |

## Schema-backed artifacts in the current implementation

The current implementation includes schemas for:

1. `alice/request_normalized.json`
2. `alice/subject_resolution.json`
3. `alice/session_state.json`
4. `alice/jurisdiction_context.json`
5. `alice/coverage_assessment.json`
6. `alice/source_plan.json`
7. `alice/source_fetch_log.json`
8. `alice/extracted_evidence.json`
9. `alice/parcel_candidates.json`
10. `alice/parcel_memo.json`
11. `alice/report_summary.json`
12. `alice/recommendation/candidate_universe.json`
13. `alice/recommendation/shortlist.json`

It also formalizes registry row schemas that future source-planning code should use:

1. county coverage registry entries
2. source descriptor entries

This step does **not** yet create dedicated schemas for:

1. `subject_resolution_summary.json`
2. `source_plan_summary.json`
3. `research_assembly_summary.json`
4. recommendation render text artifacts

Those artifact names are contractually reserved here so future implementation can fill them in without changing the workspace shape later.

## Step 5 sequencing rule

Step 5 adds a required middle layer between subject normalization and live retrieval:

1. `alice/subject_resolution.json` stabilizes what the user is talking about.
2. `alice/jurisdiction_context.json` determines what jurisdiction footing is explicit, inferred, inherited, or still unresolved.
3. `alice/coverage_assessment.json` explains what local/state/federal diligence footing Alice actually has for that subject and county path.
4. `alice/source_plan.json` decides what sources should be attempted next and what remains blocked or deferred.

Future retrieval and memo-generation steps should consume these artifacts in order rather than recomputing them ad hoc.

## Step 6 sequencing rule

Step 6 turns Step 5 planning into live evidence and a universal research object:

1. `alice/source_plan.json` decides what Alice should attempt next.
2. `alice/source_fetch_log.json` records what Alice actually attempted and what happened.
3. `alice/raw_evidence/` stores the supporting payloads for successful attempts.
4. `alice/extracted_evidence.json` records conservative structured facts extracted from those payloads.
5. `alice/parcel_memo.json` assembles the first universal land-research object from prior artifacts plus extracted evidence.

Future steps should preserve these boundaries:

1. planning is not retrieval
2. retrieval is not extraction
3. extraction is not final research synthesis

## Step 7 sequencing rule

Step 7 adds a required parcel-identity strengthening layer between extraction and final research assembly:

1. `alice/source_fetch_log.json` remains the record of what was actually fetched.
2. `alice/extracted_evidence.json` remains the normalized evidence ledger.
3. `alice/parcel_candidates.json` reconciles listing clues, subject-resolution carry-forward clues, and local public-record clues into candidate-level parcel identities.
4. `alice/parcel_candidates.json` now also says whether the best current parcel footing is text-corroborated, local-record-corroborated, geometry-confirmed, or still unknown.
5. `alice/parcel_memo.json` can then mark facts as `parcel_confirmed`, `parcel_candidate`, `listing_derived`, `county_level`, `geography_only`, `inferred`, or `unresolved`.
6. `alice/research_assembly_summary.json` gives downstream channel layers a compact read on confirmation level, coverage, and blocker counts.

Future steps should consume `parcel_candidates.json` instead of inferring parcel identity ad hoc inside report rendering or recommendation code.

## Step 8 sequencing rule

Step 8 adds a dedicated delivery layer on top of structured research assembly:

1. `alice/parcel_memo.json` remains the canonical structured research object.
2. `alice/report_summary.json` extracts a compact, schema-backed summary for downstream channels.
3. `alice/report.md` renders the canonical human-readable memo from the structured memo plus the summary layer.
4. `alice/report_slack.txt` and `alice/report_email.txt` are derived from the same summary layer rather than independently re-deciding facts.

Future delivery surfaces should attach here rather than reading raw fetch logs or raw evidence directly.

## Step 9 sequencing rule

Step 9 adds a thesis-aware interpretation layer inside the structured research object rather than creating a new top-level workspace artifact:

1. `alice/parcel_memo.json` remains the system of record.
2. `use_case_modules` inside that object records screening-grade thesis assessments, blockers, supporting signals, and unknowns.
3. `directional_economics` inside that object records scenario framing, assumptions, carry-cost gaps, capex proxies, and explicit limitations.
4. `alice/report.md` and the summary-channel artifacts render those module outputs downstream rather than inventing them ad hoc.

Step 10 now consumes these Step 9 fields instead of rebuilding thesis screening from raw evidence again.

## Step 10 sequencing rule

Step 10 adds a recommendation layer on top of the existing parcel-research pipeline:

1. `alice/request_normalized.json` and `alice/subject_resolution.json` still anchor the active thesis and any user-supplied candidate URLs.
2. `alice/recommendation/candidate_universe.json` records the candidate universe Alice actually observed, including acquisition path, dedupe decisions, and limitation notes.
3. Existing Step 6-9 artifacts such as `parcel_memo.json`, `coverage_assessment.json`, and `parcel_candidates.json` are reused per candidate rather than replaced by a shallow parallel scorer.
4. `alice/recommendation/shortlist.json` records the comparative ranking, surfaced candidates, filtered candidates, blockers, unknowns, and candidate-universe limitations.
5. `alice/recommendation/shortlist_report.md`, `shortlist_slack.txt`, and `shortlist_email.txt` are derived recommendation views over the shortlist artifacts.

Future recommendation expansion should attach here rather than claiming full-market coverage or bypassing the structured candidate universe.

## Step 11 repository-level evaluation outputs

Step 11 does not add new per-workspace Alice artifacts.

Instead, it adds repository-level evaluation and packaging outputs that help the team review Alice phase 1 as a coherent deliverable:

1. `examples/evaluation/phase1_evaluation_report.json`
   - normalized committed example of the consolidated phase-1 evaluation report
   - validates against `schemas/evaluation_report.schema.json`
   - useful for regression review and PR packaging
2. `docs/alice_phase1_output.md`
   - human-readable summary of what phase 1 currently does
3. `docs/alice_phase2_backlog.md`
   - explicit list of deferred phase-2 work
4. `docs/alice_phase1_pr_summary.md`
   - PR-ready summary material and reviewer checklist

These files sit beside, rather than inside, the `alice/` workspace artifact chain because they summarize the whole Alice package rather than a single request workspace.

## Follow-up anchor fields

`alice/session_state.json` is the minimum contract for follow-up attachment.

Use these fields consistently:

1. `active_subjects[].subject_id` is the stable subject handle that `follow_up_context.carry_forward_subject_ids` and later `subject_ref_key` values should point at.
2. `active_resolution_ref` points to the latest authoritative `alice/subject_resolution.json` that downstream follow-up logic should reuse before rebuilding subject state.
3. `artifact_refs.parcel_memo_ref` and `artifact_refs.report_ref` are the carry-forward links for follow-up prompts like "redo this assuming solar" where the parcel stays the same but the thesis changes.
4. `unresolved_questions` should be reused rather than discarded when a follow-up does not actually clarify them.

## Naming rule

Future Alice implementations should reuse these filenames exactly unless there is a strong platform-wide reason to migrate them. Stable artifact naming matters for:

1. validation
2. test fixtures
3. report rendering
4. evaluation harnesses
5. follow-up conversational continuity
