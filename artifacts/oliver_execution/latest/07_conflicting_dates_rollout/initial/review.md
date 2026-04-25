# Review Sheet: Rollout with conflicting date signals that should stay uncertain (initial)

- Fixture ID: `07_conflicting_dates_rollout`
- Provenance: `synthetic-realistic`
- Categories: conflicting-status, launch-window, adversarial
- Signal profile: `mixed`
- Auto result: `PASS`
- Auto overall score: `0.913`

## Expected Behavior

- Goal: Roll out the Chrome extension after beta close once the permissions issue and Chrome review are resolved.
- Expected readiness: `yellow`
- Expected owners: Tessa, Ron, Mia
- Expected blockers: permissions regression in Chrome 137
- Expected decisions: What the real GA date should be
- Expected follow-up targets: Ron, beta feedback summary

## Actual Snapshot

- Readiness: `yellow`
- Readiness reason: Launch has meaningful open follow-ups, at-risk dependencies, or timeline gaps.
- Owners: Tessa, Ron, Mia
- Timeline markers: May 20, week of June 3
- Blockers: permissions regression in Chrome 137
- Open decisions: What the real GA date should be
- Follow-up targets: Ron, decision: What the real GA date should be

## Auto Failure Modes

- Follow-up targets were weak or aimed at the wrong thing.

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
