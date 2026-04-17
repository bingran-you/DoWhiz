# Alice Report Rendering

Step 8 turns Alice's structured research artifacts into a usable human deliverable without changing the underlying system of record.

## Rendering inputs

The renderer consumes:

1. `alice/parcel_memo.json` as the canonical structured research object
2. `alice/parcel_candidates.json` for candidate-level parcel context
3. `alice/coverage_assessment.json` for evidence-footing and readiness language
4. `alice/jurisdiction_context.json` for jurisdiction phrasing and inherited-context warnings
5. `alice/research_assembly_summary.json` when a compact count summary is useful

The renderer should not read raw evidence, fetch logs, or registry files directly when the assembled research object already exposes the needed fact.

## Output artifacts

Step 8 formalizes four deep-research report-layer artifacts:

1. `alice/report_summary.json`
2. `alice/report.md`
3. `alice/report_slack.txt`
4. `alice/report_email.txt`

`alice/report_summary.json` is the normalized summary layer.

`alice/report.md` is the canonical human-readable memo.

Slack and email outputs are derived views that should stay aligned with the same normalized summary rather than inventing their own independent conclusions.

Step 10 adds a separate recommendation-rendering set:

1. `alice/recommendation/shortlist_report.md`
2. `alice/recommendation/shortlist_slack.txt`
3. `alice/recommendation/shortlist_email.txt`

## Rendering order

Step 8 deep-research rendering should execute in this order:

1. extract a normalized summary from `parcel_memo.json` plus nearby context artifacts
2. render markdown from the summary plus the full structured memo
3. render Slack and email views from the same summary layer

This keeps Alice's narrative outputs consistent across channels.

The stabilization patch adds one more rendering expectation:

1. when parcel identity is strong enough to name a parcel confirmation level, the memo and summary views should also expose the current `confirmation_basis`
2. `local_record_corroborated` should not be phrased like `geometry_confirmed`

Step 10 recommendation rendering follows a parallel sequence:

1. build `alice/recommendation/candidate_universe.json`
2. build `alice/recommendation/shortlist.json`
3. render recommendation markdown, Slack, and email outputs from the shortlist artifacts

Recommendation rendering should not bypass shortlist artifacts and read raw candidate workspaces ad hoc.

## Markdown memo structure

The markdown memo should stay close to this shape:

1. Title and scope
2. Advisory boundary
3. Executive summary
4. Subject snapshot
5. Evidence quality / coverage panel
6. Parcel identity and jurisdiction
7. Public-data findings
8. Use-case screening
9. Directional economics
10. Parcel candidate analysis
11. Risks
12. Unknowns
13. Recommended next actions
14. Sources and citations
15. Notes

Thin sections should stay thin. A short explicit "Unresolved" bullet is better than padded prose.

## Summary-layer expectations

`report_summary.json` should be compact but still traceable.

It should carry:

1. a title and one-line conclusion
2. an executive summary bullet set
3. subject snapshot fields
4. evidence-quality signals
5. top findings, risks, unknowns, and next actions
6. grouped sources
7. rendering hints for Slack and email

This summary layer is not a replacement for `parcel_memo.json`.

It is a derived delivery contract that makes downstream formatting predictable.

## Truthfulness rules

Renderers must preserve Alice's evidence semantics:

1. parcel-confirmed findings should read differently from parcel-candidate findings
2. listing-derived claims should stay visibly seller-facing
3. county-level and geography-only claims should not be phrased as parcel confirmation
4. unknowns should remain first-class content
5. conflicts should remain visible in executive summary, candidate analysis, risks, or next actions when material

Step 9 adds one more rendering requirement:

6. use-case modules should be rendered as screening outputs, not as guaranteed feasibility statements
7. directional economics should be rendered as scenario framing, not as underwriting

Step 10 adds one more recommendation-rendering requirement:

8. recommendation outputs must say they come from the observed candidate universe rather than from exhaustive market coverage

## Step 9 summary behavior

The summary layer does not need a separate module artifact.

Instead, Step 9 should:

1. let top findings and one-line conclusion reflect the most relevant module outputs
2. keep the markdown memo responsible for the fuller module-by-module explanation
3. keep Slack and email concise by highlighting the most material thesis screens

## Recommendation rendering

Step 10 recommendation rendering should keep these distinctions visible:

1. the candidate universe Alice actually observed
2. which candidates were hard-filtered vs shortlisted
3. why a candidate ranks higher or lower
4. how parcel identity strength and evidence quality affect rank
5. what remains a blocker or unknown even for top-ranked candidates

Recommendation markdown should be the canonical human-readable shortlist artifact.

Slack and email recommendation views should stay shorter, but they should still carry:

1. the observed-universe limitation note
2. the top-ranked candidates
3. the main blocker or theme across the set
4. a pointer back to `alice/recommendation/shortlist_report.md`

## What this layer still does not solve

Even after Step 10, Alice does not yet attempt:

1. polished UI presentation
2. PDF or DOCX export
3. exhaustive market crawling
4. advanced underwriting prose
5. polished underwriting narrative depth
