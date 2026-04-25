# Review Sheet: Migration refresh that reveals a new blocker and should stay red (initial)

- Fixture ID: `10_email_domain_migration_refresh_still_red`
- Provenance: `synthetic-realistic`
- Categories: migration, refresh, external-dependency, conflicting-plan
- Signal profile: `mixed`
- Auto result: `PASS`
- Auto overall score: `0.876`

## Expected Behavior

- Goal: Complete the customer email domain migration before the Nov 20 advisory board.
- Expected readiness: `yellow`
- Expected owners: Paul, Gina, Mark, Sara
- Expected blockers: none
- Expected decisions: Whether phased cutover would be acceptable if the EU domain is late
- Expected follow-up targets: vendor ticket for the EU domain, Sara

## Actual Snapshot

- Readiness: `yellow`
- Readiness reason: Launch has meaningful open follow-ups, at-risk dependencies, or timeline gaps.
- Owners: Paul, Gina, Mark, Sara
- Timeline markers: Nov 20, Nov 14
- Blockers: none
- Open decisions: Whether phased cutover is acceptable if the EU domain is late
- Follow-up targets: Gina, vendor ticket owner for EU domain, Paul

## Auto Failure Modes

- Dependencies were incomplete or conflated with blockers.
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
