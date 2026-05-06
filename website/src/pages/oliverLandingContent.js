export const OLIVER_ENTRY_SURFACE = 'oliver_launch_execution';
export const OLIVER_LANDING_VARIANT = 'oliver_launch_execution_hook_v1';
export const OLIVER_LAUNCH_HREF = '/oliver/launch';
export const OLIVER_AUTH_SIGN_IN_HREF = '/auth/index.html?entry=oliver_launch_execution';
export const OLIVER_HOME_HREF = '/';

export const oliverLandingContent = {
  metadata: {
    title: 'Oliver by DoWhiz | Launch Execution Copilot',
    description:
      'Turn one messy launch thread into an accountable execution plan and a live readiness brief backed by evidence.',
    canonicalUrl: 'https://dowhiz.com/oliver',
    robots: 'noindex, nofollow',
    htmlLang: 'en',
    ogLocale: 'en_US',
    themeColor: '#050816',
    ogImage: 'https://dowhiz.com/assets/DoWhiz.svg',
    ogImageAlt: 'Oliver landing page for launch execution'
  },
  nav: {
    homeLabel: 'Home',
    signInLabel: 'Sign in',
    primaryCta: 'Try Launch Flow'
  },
  hero: {
    badge: 'Launch Execution Copilot',
    eyebrow: 'For one launch thread at a time',
    title: 'Turn one launch thread into accountable execution',
    subtitle:
      'Paste the thread. Oliver extracts owners, dates, dependencies, blockers, open decisions, and a readiness brief backed by evidence.',
    activity: 'Readiness brief refreshed from the latest owner update.',
    primaryCta: 'Try Launch Flow',
    secondaryCta: 'See output samples',
    proofPills: ['Owners + dates', 'Blockers + decisions', 'Readiness brief'],
    orbitSignals: ['Thread', 'Milestones', 'Risks', 'Owners']
  },
  workflow: {
    eyebrow: 'How to start',
    title: 'Paste one launch thread.',
    subtitle: 'Oliver turns the thread into a narrow execution layer instead of a generic project dashboard.',
    signalsLabel: 'Signals in',
    signals: [
      { title: 'Launch thread bundle', meta: 'Slack or email paste', tone: 'teal' },
      { title: 'Planning note', meta: 'milestones + dependencies', tone: 'gold' },
      { title: 'Latest owner reply', meta: 'refresh the brief', tone: 'sky' }
    ],
    coreLabel: 'Oliver',
    coreTitle: 'Builds the execution layer',
    coreSubtitle: 'Extracts ownership, dates, risks, decisions, and follow-ups.',
    outputsLabel: 'What you get',
    outputs: [
      { title: 'Launch plan', meta: 'Milestones + owners', tone: 'emerald' },
      { title: 'Readiness brief', meta: 'Evidence-backed status', tone: 'amber' },
      { title: 'Follow-ups', meta: 'Missing owners and blockers', tone: 'sky' }
    ]
  },
  proof: {
    eyebrow: 'Output samples',
    title: 'Scan the outputs.',
    cards: [
      {
        key: 'update',
        eyebrow: 'Readiness brief',
        title: 'One accountable launch view',
        description: 'Overall readiness, blockers, and what changed since the last update.',
        visual: {
          type: 'update',
          windowTitle: 'Mobile checkout launch readiness',
          status: 'Yellow',
          metrics: ['1 blocker', '4 owners', 'June 11 review'],
          lines: [
            { label: 'Build', value: 82 },
            { label: 'QA', value: 45 },
            { label: 'Comms', value: 68 }
          ],
          bullets: [
            'Payments callback still blocks QA signoff',
            'Fallback copy approval is unresolved',
            'Lifecycle needs final date and screenshots'
          ]
        }
      },
      {
        key: 'risks',
        eyebrow: 'Owner / date / risk tracker',
        title: 'Tracker, not task sprawl',
        description: 'The smallest useful layer for launch accountability.',
        visual: {
          type: 'risks',
          rows: [
            { tone: 'high', label: 'Callback retries stable in staging', owner: 'Jon', review: 'Needs date' },
            { tone: 'medium', label: 'Fallback copy legal approval', owner: 'Product', review: 'Open' },
            { tone: 'low', label: 'Lifecycle comms scheduled', owner: 'Lena', review: 'After screenshots' }
          ]
        }
      },
      {
        key: 'follow-up',
        eyebrow: 'Next follow-ups',
        title: 'Keep the loop moving',
        description: 'The next asks are generated from missing owners, dates, blockers, and stale updates.',
        visual: {
          type: 'follow-up',
          rows: [
            { tool: 'Thread', owner: 'Infra', action: 'Confirm retry queue ETA', state: 'Queued' },
            { tool: 'Thread', owner: 'Product', action: 'Name owner for copy approval', state: 'Waiting' },
            { tool: 'Thread', owner: 'Lifecycle', action: 'Request final launch date', state: 'Ready' }
          ]
        }
      }
    ]
  },
  trust: {
    eyebrow: 'Stay In Control',
    title: 'Keep the launch loop tight.',
    items: [
      'Evidence stays visible',
      'Follow-up drafts first',
      'One launch at a time'
    ],
    primaryCta: 'Try Launch Flow',
    secondaryCta: 'Sign in'
  }
};
