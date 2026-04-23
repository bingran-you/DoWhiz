export const OLIVER_ENTRY_SURFACE = 'oliver_tpm';
export const OLIVER_LANDING_VARIANT = 'oliver_tpm_hook_v5_cta_avatar';
export const OLIVER_AUTH_OVERVIEW_HREF = '/auth/index.html?loggedIn=true&entry=oliver_tpm#section-overview';
export const OLIVER_AUTH_SIGN_IN_HREF = '/auth/index.html?entry=oliver_tpm';
export const OLIVER_HOME_HREF = '/';

export const oliverLandingContent = {
  metadata: {
    title: 'Oliver by DoWhiz | AI TPM for product and engineering teams',
    description:
      'Oliver is the AI TPM for product and engineering teams. Sign in, point him at one program, and get the weekly update, current risks, and next owners.',
    canonicalUrl: 'https://dowhiz.com/oliver',
    robots: 'noindex, nofollow',
    htmlLang: 'en',
    ogLocale: 'en_US',
    themeColor: '#050816',
    ogImage: 'https://dowhiz.com/assets/DoWhiz.svg',
    ogImageAlt: 'Oliver landing page for product and engineering teams'
  },
  nav: {
    homeLabel: 'Home',
    signInLabel: 'Sign in',
    primaryCta: 'Open Oliver'
  },
  hero: {
    badge: 'AI TPM',
    eyebrow: 'For launches, releases, and cross-functional work',
    title: 'Oliver is your AI TPM.',
    subtitle:
      'Sign in, point him at one program, and get the update, current risks, and next owners.',
    activity: 'Weekly review ready to send.',
    primaryCta: 'Open Oliver',
    secondaryCta: 'See output samples',
    proofPills: ['Weekly update', 'Current risks', 'Next owners'],
    orbitSignals: ['Slack', 'GitHub', 'Docs', 'Meetings']
  },
  workflow: {
    eyebrow: 'How to start',
    title: 'Connect one program.',
    subtitle: 'Oliver watches the signals and drafts the output.',
    signalsLabel: 'Signals in',
    signals: [
      { title: 'Slack blocker thread', meta: '#launch-ops', tone: 'teal' },
      { title: 'GitHub regression', meta: 'payments/callback', tone: 'violet' },
      { title: 'Launch notes', meta: 'decision recap', tone: 'gold' }
    ],
    coreLabel: 'Oliver',
    coreTitle: 'Reviews the week',
    coreSubtitle: 'Builds one operating picture.',
    outputsLabel: 'What you get',
    outputs: [
      { title: 'Weekly update', meta: 'Ready to review', tone: 'emerald' },
      { title: 'Current risks', meta: '2 need decisions', tone: 'amber' },
      { title: 'Next owners', meta: '3 follow-ups ready', tone: 'sky' }
    ]
  },
  proof: {
    eyebrow: 'Output samples',
    title: 'Scan the outputs.',
    cards: [
      {
        key: 'update',
        eyebrow: 'Weekly update',
        title: 'Send-ready draft',
        description: 'Health, blockers, and next asks.',
        visual: {
          type: 'update',
          windowTitle: 'Mobile release weekly update',
          status: 'At risk',
          metrics: ['2 blockers', '3 owners', 'Fri review'],
          lines: [
            { label: 'Build', value: 86 },
            { label: 'QA', value: 52 },
            { label: 'Comms', value: 74 }
          ],
          bullets: ['Checkout is code-complete', 'QA waits on callback fix', 'Fallback copy needs approval']
        }
      },
      {
        key: 'risks',
        eyebrow: 'Risk register',
        title: 'Live risk list',
        description: 'Severity, owner, and next review.',
        visual: {
          type: 'risks',
          rows: [
            { tone: 'high', label: 'Callback regression', owner: 'Eng', review: 'Today' },
            { tone: 'medium', label: 'Fallback copy approval', owner: 'Product', review: 'Today' },
            { tone: 'low', label: 'Release note timing', owner: 'Ops', review: 'Fri' }
          ]
        }
      },
      {
        key: 'follow-up',
        eyebrow: 'Owner follow-through',
        title: 'Next owners',
        description: 'Who moves next, in one place.',
        visual: {
          type: 'follow-up',
          rows: [
            { tool: 'Slack', owner: 'Engineering', action: 'Confirm ETA', state: 'Queued' },
            { tool: 'GitHub', owner: 'Product', action: 'Approve fallback copy', state: 'Waiting' },
            { tool: 'Docs', owner: 'Ops', action: 'Send release note', state: 'Ready' }
          ]
        }
      }
    ]
  },
  trust: {
    eyebrow: 'Stay In Control',
    title: 'Review before send.',
    items: [
      'Review before send',
      'Only connected scopes',
      'Shared DoWhiz setup'
    ],
    primaryCta: 'Open Oliver',
    secondaryCta: 'Sign in'
  }
};
