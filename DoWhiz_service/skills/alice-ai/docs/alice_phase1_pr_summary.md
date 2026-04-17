# Alice Phase 1 PR Summary

## Suggested title

`Alice phase 1: end-to-end research, rendering, screening, recommendation, and evaluation harness`

## Summary

This PR completes Alice phase 1 by packaging the existing Step 3-10 pipeline into a measurable, regression-safe deliverable.

Alice phase 1 now includes:

1. county and source registry foundations
2. subject resolution and follow-up/session-state handling
3. jurisdiction context, coverage assessment, and deterministic source planning
4. live retrieval foundation with source fetch logs, raw evidence, and extracted evidence
5. parcel candidates plus field-level evidence scope
6. markdown, Slack, and email report rendering
7. use-case screening modules and directional economics scaffolding
8. recommendation candidate-universe and shortlist flow
9. pre-Step11 stabilization semantics
10. a unified Step 11 evaluation harness and phase-1 packaging docs

## Evaluation coverage

Primary command:

```bash
python DoWhiz_service/skills/alice-ai/scripts/validate_step11.py
```

The Step 11 runner consolidates the existing Alice validation chain and reports:

1. registry / coverage checks
2. subject-resolution checks
3. Step 5 planning checks
4. Step 6 retrieval / extraction / assembly checks
5. Step 7 parcel-candidate checks
6. Step 8 rendering checks
7. Step 9 use-case module and directional economics checks
8. Step 10 recommendation checks
9. stabilization and truthfulness checks for `access_mode`, `confirmation_basis`, coverage-tier rubric consistency, weak-data honesty, conflict visibility, recommendation wording, and wind coverage

Committed phase-1 evaluation artifact:

1. [phase1_evaluation_report.json](/Users/yegaoyang/Desktop/workspace/DoWhiz/DoWhiz_service/skills/alice-ai/examples/evaluation/phase1_evaluation_report.json)

## Major known limitations

1. Retrieval breadth remains intentionally narrow and strongest in the current pilot counties.
2. Recommendation outputs rank only within the observed candidate universe, not the full market.
3. Parcel confirmation is still mostly text- or local-record-corroborated rather than geometry-backed.
4. Directional economics remain screening-grade and non-underwriting.
5. Evaluation is strong for deterministic regression, but still narrower than a full calibration program.

## Reviewer focus areas

1. Does the Step 11 evaluation report accurately describe what the current Alice pipeline does and does not do?
2. Do the truthfulness checks catch the most important overclaiming risks for phase 1?
3. Are the phase-1 output and phase-2 backlog docs clear enough to support a clean PR and limited-release discussion?
4. Does the unified runner stay additive, or does it accidentally blur into new feature work?

## Reviewer checklist

1. [ ] Run `python DoWhiz_service/skills/alice-ai/scripts/validate_step11.py`
2. [ ] Confirm [alice_phase1_output.md](/Users/yegaoyang/Desktop/workspace/DoWhiz/DoWhiz_service/skills/alice-ai/docs/alice_phase1_output.md) matches the actual product envelope
3. [ ] Confirm [alice_phase2_backlog.md](/Users/yegaoyang/Desktop/workspace/DoWhiz/DoWhiz_service/skills/alice-ai/docs/alice_phase2_backlog.md) cleanly captures deferred work
4. [ ] Spot-check the evaluation artifact and a few scenario workspaces for consistency
5. [ ] Verify that recommendation renders remain explicit about observed-universe limits
