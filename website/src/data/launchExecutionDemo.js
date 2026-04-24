export const launchExecutionDemo = {
  sourceLabel: 'Q2 mobile checkout launch thread',
  contextText: `Slack thread: #launch-war-room

Maya (PM) - Apr 16
We still want to launch the mobile checkout refresh before the partner webinar on June 14. Goal is to reduce drop-off and have the webinar point to the new flow.

Jon (Eng) - Apr 16
Implementation is mostly done. Payments callback retries are still failing in staging after about 20 minutes, and that blocks QA from signing off.

Priya (Design) - Apr 17
Final screenshots are ready once Product approves the fallback copy for the payment error state.

Lena (Lifecycle) - Apr 17
Email + in-app launch comms draft is 80% there. I need the final launch date and approved screenshots.

Maya (PM) - Apr 18
We need a go/no-go review by June 11. Owners should come with status, blockers, and what they need from others.

Jon (Eng) - Apr 19
I can own the callback fix, but I need infra to confirm whether the retry queue config can change before code freeze.

Sam (Infra) - Apr 19
I haven't checked the retry queue setting yet. I should know by Tuesday.

QA note - Apr 20
Regression pass is blocked until callback retries are stable in staging.

Product comment - Apr 20
Fallback copy still needs approval from legal because of the refund language.

Launch checklist excerpt
- Engineering fix callback retries in staging
- QA regression pass
- Final screenshots approved
- Lifecycle email + in-app message scheduled
- Webinar deck updated
- Go/no-go review on June 11`
};
