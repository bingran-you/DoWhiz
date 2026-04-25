# Review Sheet: Migration refresh that reveals a new blocker and should stay red (refresh)

- Fixture ID: `10_email_domain_migration_refresh_still_red`
- Provenance: `synthetic-realistic`
- Categories: migration, refresh, external-dependency, conflicting-plan
- Signal profile: `mixed`
- Auto result: `PASS`
- Auto overall score: `0.899`

## Expected Behavior

- Goal: Complete the customer email domain migration before the Nov 20 advisory board.
- Expected readiness: `red`
- Expected owners: Paul, Gina, Mark
- Expected blockers: EU vendor cannot finish validation before Nov 18
- Expected decisions: Whether phased cutover is acceptable
- Expected follow-up targets: Paul, decision: whether phased cutover is acceptable

## Actual Snapshot

- Readiness: `red`
- Readiness reason: Critical blocker still open: EU vendor cannot finish validation before Nov 18.
- Owners: Paul, Gina, Mark, Sara
- Timeline markers: Nov 20, Nov 18, Nov 14
- Blockers: EU vendor cannot finish validation before Nov 18
- Open decisions: Whether phased cutover is acceptable
- Follow-up targets: Paul

## Auto Failure Modes

- Follow-up targets were weak or aimed at the wrong thing.
- Evidence grounding was thin or did not point to the right facts.

## Human Review Questions

- [ ] Did Oliver identify the real execution structure of the thread?
- [ ] Did Oliver hallucinate owners, dates, blockers, or decisions?
- [ ] Is the readiness judgment believable based on the thread evidence?
- [ ] Are the follow-up drafts useful enough to approve with light edits?
- [ ] Did Oliver surface something operationally important that a summary-only tool might miss?
- [ ] Did Oliver preserve uncertainty where the thread stayed incomplete or conflicted?

## Reviewer Notes

- Trust this brief?
- Approve/send any draft follow-up?
- Would you give Oliver a second thread after seeing this output?
- What felt wrong, noisy, or overconfident?
