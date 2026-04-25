# Oliver Execution Validation Pack

This pack validates the current Oliver v1 launch-execution workflow against a small but adversarial offline dataset.

What this pack is for:

- measure whether Oliver extracts execution structure from messy threads
- inspect whether readiness is grounded in evidence instead of optimism
- inspect whether follow-up drafts look targeted enough to review
- surface failure modes before approval/send workflows expand

What this pack is not:

- real user validation
- a benchmark of production thread distribution
- proof that the product is ready for autonomous follow-up or write-back

Fixture provenance labels:

- `repo-derived-demo`: adapted from an example already in the repo
- `repo-derived-sanitized`: derived from repo docs or product material, rewritten into a realistic thread
- `synthetic-realistic`: authored for evaluation only

Recommended workflow:

```bash
python3 evals/oliver_execution/run_validation.py
python3 evals/oliver_execution/grade_validation.py
```

Default outputs:

- run artifacts: `artifacts/oliver_execution/latest/`
- generated report: `docs/oliver-validation-report-v1.md`

Each case stores:

- raw request
- raw model output
- parsed model output
- final analyzer response
- readiness brief
- follow-up drafts
- grade JSON
- human review sheet
