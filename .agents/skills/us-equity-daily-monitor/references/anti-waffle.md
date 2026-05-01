# Anti-Waffle Rules

This skill fails when it sounds prudent but does not help the user decide.

## Generic language that needs translation

These ideas are not usable on their own:

- "good company, but do not chase"
- "great business, but wait"
- "hold for now"
- "buy in tranches"
- "wait for clarity"
- "not a broken asset"

Translate them into decision logic with thresholds or remove them.

## Neutral-output rule

If the visible recommendation lands on `Wait`, `Hold`, or `Hold/Do not add`, the final artifact must include all three:

1. `Upgrade / Review Now`
2. `Downgrade / De-risk`
3. `Invalidation`

Each trigger must be concrete. Use numbers, dates, or explicit measurable events. "If results improve" and "if the stock pulls back" are not enough.

## No Material Change rule

When the right answer is `No Material Change`, do not inflate the artifact into a long memo. The user should see:

- what changed
- why it did not change the call
- what would change the call

## Final self-check

- Would this still sound specific if the company name were removed?
- Did I explain what moves the call up, down, or breaks it?
- If I recommended `Wait` or `Hold`, did I show a mechanical path to stop waiting or holding?
