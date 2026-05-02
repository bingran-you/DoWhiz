# Monitor vs Deep Research

This skill supports two artifact shapes:

- a short monitor update
- a full decision memo

Choose the smaller one that still answers the user's decision.

## Use the short monitor update when

- the correct monitor status is `No Material Change`
- the latest evidence does not materially change the thesis or action
- the user mainly needs to know whether anything important changed
- stretching to a full memo would mostly create more `Hold` or `Wait` prose

Even if the user says "deep research," do not pad a no-edge answer into a long memo.
If the user says "only tell me if I should act," the `No Material Change` path should be extremely short.
Once that short update is written, stop. Do not spend the last stretch re-checking length by dumping the full artifact back to the terminal.

## Use the full memo when

- the monitor status is `Watch Closely` or `Review Now`
- the user is making a fresh position decision
- the case needs dual-horizon framing, scenarios, or a factual correction

## Time-budget rule

- Start updating the final artifact early for long research tasks.
- For real tickers, begin with a bounded first pass: latest company release or filing, current price/reference check, and one independent cross-check. Do not default to page-by-page annual-report extraction.
- Create the first `reply_email_draft.html` skeleton before you start a second layer of research.
- Reserve the last stretch of the run for drafting and tightening the visible artifact, not for one more marginal search.
- If evidence is still incomplete near the end, send the best calibrated artifact you can support and state the missing evidence instead of timing out with no reply.
- For incomplete real-ticker work, prefer `Watch Closely` with explicit missing evidence and concrete confirmation triggers over no artifact.
