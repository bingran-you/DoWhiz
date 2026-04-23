export const OLIVER_ENTRY_SURFACE = 'oliver_tpm';
export const OLIVER_LANDING_VARIANT = 'oliver_tpm_hook_v4_visual_scan';
export const OLIVER_AUTH_OVERVIEW_HREF = '/auth/index.html?loggedIn=true&entry=oliver_tpm#section-overview';

export const oliverLandingContent = {
  metadata: {
    title: 'Oliver by DoWhiz | AI TPM for product and engineering teams',
    description:
      'Oliver is the AI TPM for product and engineering teams. He watches one program, drafts the weekly update, tracks current risks, and keeps the next owners moving.',
    canonicalUrl: 'https://dowhiz.com/oliver',
    robots: 'noindex, nofollow',
    htmlLang: 'en',
    ogLocale: 'en_US',
    themeColor: '#050816',
    ogImage: 'https://dowhiz.com/assets/DoWhiz.svg',
    ogImageAlt: 'Oliver landing page for product and engineering teams'
  },
  hero: {
    badge: 'TPM loop',
    eyebrow: 'AI TPM for product and engineering teams',
    title: 'Meet Oliver, your AI TPM.',
    accent: 'He turns one messy week into a clear next move.',
    subtitle:
      'Point him at one launch, release, or cross-functional stream. He watches the Slack threads, GitHub issues, docs, and meeting notes around it, then gives you the update, the risks, and the next owners.',
    activity: 'Weekly launch review drafted in 8 min. Ready for review.',
    primaryCta: 'Try Oliver on one program',
    secondaryCta: 'See output samples',
    proofPills: ['Weekly update draft', 'Current risks', 'Owner follow-through'],
    orbitSignals: ['Slack', 'GitHub', 'Docs', 'Notes']
  },
  workflow: {
    eyebrow: 'How Oliver Works',
    title: 'Show him one program.',
    subtitle: 'You connect the signals. Oliver returns the TPM artifacts.',
    signalsLabel: 'Signals watched',
    signals: [
      { title: 'Slack blocker thread', meta: '#launch-ops', tone: 'teal' },
      { title: 'GitHub regression', meta: 'payments/callback', tone: 'violet' },
      { title: 'Launch notes', meta: 'decision recap', tone: 'gold' }
    ],
    coreLabel: 'Oliver',
    coreTitle: 'Reviews the week',
    coreSubtitle: 'Builds the TPM view before it gets lost in threads.',
    outputsLabel: 'What comes back',
    outputs: [
      { title: 'Weekly update', meta: 'Ready to review', tone: 'emerald' },
      { title: 'Current risks', meta: '2 need decisions', tone: 'amber' },
      { title: 'Next owners', meta: '3 follow-ups ready', tone: 'sky' }
    ]
  },
  proof: {
    eyebrow: 'What You Can Scan',
    title: 'Three screens explain the product faster than paragraphs.',
    cards: [
      {
        key: 'update',
        eyebrow: 'Weekly update',
        title: 'A status note you can actually send.',
        description: 'Health, movement, blockers, and next asks in one reviewable draft.',
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
        title: 'A live blocker list, not a dead recap.',
        description: 'Severity, owner, and next review stay attached to the problem.',
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
        title: 'One view of what should move next.',
        description: 'Oliver keeps the next owners, tools, and statuses in the same loop.',
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
    title: 'Review before send. Keep scope tight. Start on the existing DoWhiz account flow.',
    items: [
      'Review before send',
      'Only connected scopes',
      'Shared DoWhiz auth and dashboard'
    ],
    primaryCta: 'Try Oliver on one program'
  }
};
