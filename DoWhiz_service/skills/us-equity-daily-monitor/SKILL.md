---
name: us-equity-daily-monitor
description: Produce a simple, bounded U.S. equity monitor reply for one stock or ETF. Use whenever the user wants a concise decision-useful update on a single U.S. stock or ETF, such as "should I buy NVDA", "what changed on PLUG", "do I add or trim", or "is there any edge here". Do not use for portfolio construction, crypto, FX, fixed income, multi-asset allocation, or broad macro notes.
---

# U.S. Equity Daily Monitor

This skill is a simple bounded monitor, not a deep-research pipeline.

## Output modes

Use exactly one of these modes:

1. `Actionable Update`
   - `Status`
   - `New Money Action`: `Buy` | `Wait` | `Avoid`
   - `Existing Holder Action`: `Hold` | `Add` | `Trim` | `Exit`
   - `Why now`
   - `What changed`
   - `What would change the view`
   - `Confidence`

2. `No Material Update`
   - `Status`
   - `Action`: `No new action`
   - `Why`
   - `What would matter next`

3. `Unable to Verify`
   - `Status`
   - `Action`: `No recommendation`
   - `Why`
   - `Next step`

## Rules

- Keep the reply short and scan-first.
- Do not write a long memo.
- Do not produce bull/base/bear framing, large tables, or charts.
- Do not mine annual reports page by page in the email path.
- Use `Actionable Update` only when there is clear verified information.
- Use `No Material Update` when no verified new catalyst is found.
- Use `Unable to Verify` when tools, sources, or time budget are insufficient.
- If a deeper report would help, say that you can run it separately, but do not start it automatically.
