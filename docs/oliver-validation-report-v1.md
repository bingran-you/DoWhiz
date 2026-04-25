# Oliver Validation Report V1

This report summarizes the offline validation pass for the current Oliver v1 launch-execution workflow.

Synthetic and repo-derived fixtures are useful for surfacing extraction and grounding failures, but they are not a substitute for live human trials.

## Dataset Overview

- Initial fixtures: 10
- Total evaluated stages: 12
- Provenance mix: repo-derived-demo=1, repo-derived-sanitized=1, synthetic-realistic=8
- Signal mix: mixed=3, strong=5, weak=2
- Top categories: launch=2, repo-derived=2, refresh=2, migration=2, conflicting-status=2, adversarial=2, external-dependency=2, weak-signal=2

## Aggregate Scores

- Pass rate: 8/12
- Blocked stages: 0
- Average overall score: 0.885
- Readiness accuracy: 1.000
- Owner extraction average: 0.946
- Blocker extraction average: 0.971
- Follow-up target average: 0.736
- Evidence grounding average: 0.667

## Per-Case Summary

- `01_repo_mobile_checkout_blocked:initial` -> PASS, overall=0.807, readiness=1.000
- `02_pricing_rollout_refresh_to_green:initial` -> PASS, overall=0.949, readiness=1.000
- `02_pricing_rollout_refresh_to_green:refresh` -> GRADED, overall=0.932, readiness=1.000
- `03_identity_migration_conflict:initial` -> GRADED, overall=0.889, readiness=1.000
- `04_partner_api_launch_external_dependency:initial` -> PASS, overall=0.893, readiness=1.000
- `05_release_thread_incomplete_signal:initial` -> PASS, overall=0.860, readiness=1.000
- `06_brainstorm_not_execution:initial` -> GRADED, overall=0.868, readiness=1.000
- `07_conflicting_dates_rollout:initial` -> PASS, overall=0.913, readiness=1.000
- `08_security_review_missing_owner:initial` -> PASS, overall=0.918, readiness=1.000
- `09_onboarding_rollout_dogfood:initial` -> GRADED, overall=0.811, readiness=1.000
- `10_email_domain_migration_refresh_still_red:initial` -> PASS, overall=0.876, readiness=1.000
- `10_email_domain_migration_refresh_still_red:refresh` -> PASS, overall=0.899, readiness=1.000

## Top Recurring Failure Modes

- Evidence grounding was thin or did not point to the right facts. (7 stage(s))
- Follow-up targets were weak or aimed at the wrong thing. (5 stage(s))
- Unresolved decisions were missed or misclassified. (4 stage(s))
- Dates or launch windows were missing, wrong, or overly certain. (3 stage(s))
- Dependencies were incomplete or conflated with blockers. (2 stage(s))

## Good Output Examples

- `02_pricing_rollout_refresh_to_green:initial` scored 0.949. Readiness `yellow` with blockers `none` and follow-up targets `Mina, Ava`.
- `02_pricing_rollout_refresh_to_green:refresh` scored 0.932. Readiness `green` with blockers `none` and follow-up targets `none`.

## Bad Output Examples

- `01_repo_mobile_checkout_blocked:initial` scored 0.807. Main issues: Dates or launch windows were missing, wrong, or overly certain., Dependencies were incomplete or conflated with blockers., Unresolved decisions were missed or misclassified..
- `09_onboarding_rollout_dogfood:initial` scored 0.811. Main issues: Dates or launch windows were missing, wrong, or overly certain., Unresolved decisions were missed or misclassified., Evidence grounding was thin or did not point to the right facts..

## Recommendation

- Offline recommendation: **Ready for limited concierge testing**
- Approval/send workflows should stay off until live reviewers confirm the readiness brief is trustworthy and the drafted follow-ups are approval-worthy.

## What To Fix Before Approval/Send Workflows

- Evidence grounding was thin or did not point to the right facts.
- Follow-up targets were weak or aimed at the wrong thing.
- Unresolved decisions were missed or misclassified.
- Dates or launch windows were missing, wrong, or overly certain.
- Dependencies were incomplete or conflated with blockers.

## Offline vs Live Validation Limits

- Validated offline here: repeated extraction quality, readiness alignment, evidence grounding, weak-signal handling, and follow-up target quality proxies.
- Not validated offline here: whether PMs trust the brief in a live workflow, whether they would approve/send the drafts, and whether the product creates enough confidence to hand it a second thread.
