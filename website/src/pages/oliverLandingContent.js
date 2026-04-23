export const OLIVER_ENTRY_SURFACE = 'oliver_tpm';
export const OLIVER_LANDING_VARIANT = 'oliver_tpm_hook_v2_clarity';
export const OLIVER_AUTH_OVERVIEW_HREF = '/auth/index.html?loggedIn=true&entry=oliver_tpm#section-overview';

export const oliverLandingContent = {
  metadata: {
    title: 'Oliver by DoWhiz | AI TPM for product and engineering teams',
    description:
      'Oliver is the AI TPM for product and engineering teams. He turns scattered execution signals into a weekly update draft, current risks, and clear next owners.',
    canonicalUrl: 'https://dowhiz.com/oliver',
    robots: 'noindex, nofollow',
    htmlLang: 'en',
    ogLocale: 'en_US',
    themeColor: '#f5f7fb',
    ogImage: 'https://dowhiz.com/assets/DoWhiz.svg',
    ogImageAlt: 'Oliver landing page for product and engineering teams'
  },
  nav: {
    proofLink: 'See sample update',
    primaryCta: 'Try Oliver on one program'
  },
  hero: {
    eyebrow: 'AI TPM for product and engineering teams',
    title: 'Oliver turns execution noise into updates, risks, and next owners.',
    subtitle:
      'Point him at one launch, release, or cross-functional stream. He watches the Slack threads, GitHub issues, meeting notes, and docs around it, then drafts the weekly update and tells you what needs attention next.',
    primaryCta: 'Try Oliver on one program',
    secondaryCta: 'See a sample weekly update',
    highlights: ['Weekly update draft', 'Current risks', 'Next owners'],
    artifact: {
      label: 'Sample weekly update',
      program: 'Mobile release',
      healthLabel: 'At risk',
      healthDetail: '1 blocker needs resolution before QA sign-off',
      summaryLabel: 'This week',
      summary:
        'Checkout is code-complete, but QA is blocked by the payment callback regression. The fallback copy is drafted and still needs one product approval.',
      risksLabel: 'Current risks',
      risks: [
        'Payment callback bug is still blocking QA sign-off.',
        'Fallback copy is drafted but not approved yet.',
        'Release note timing depends on the final QA decision.'
      ],
      ownersLabel: 'Next owners',
      owners: [
        { owner: 'Engineering', action: 'Confirm callback fix ETA by 3 PM' },
        { owner: 'Product', action: 'Approve fallback copy before today ends' },
        { owner: 'Ops', action: 'Send the release note once QA goes green' }
      ],
      sourceLabel: 'Signals watched',
      sources: 'Slack, GitHub, meeting notes, and release docs'
    }
  },
  outputs: {
    eyebrow: 'What You Get This Week',
    title: 'Three outputs a TPM actually needs.',
    cards: [
      {
        title: 'Weekly update draft',
        description: 'A concise update you can review and send, not a transcript you still have to interpret.',
        bullets: [
          'What moved this week',
          'What is blocked right now',
          'What leadership or partners need to know next'
        ]
      },
      {
        title: 'Current risks',
        description: 'A short list of blockers with clear severity, owners, and the next review point.',
        bullets: [
          'Risks stay visible until they move',
          'Mitigations stay attached to the issue',
          'Escalations stay obvious instead of hiding in threads'
        ]
      },
      {
        title: 'Next owners',
        description: 'A follow-through list that makes the next move explicit instead of leaving it inside chat.',
        bullets: [
          'Who owns the next action',
          'What they need to do',
          'What Oliver should draft or chase down next'
        ]
      }
    ]
  },
  setup: {
    eyebrow: 'How To Start',
    title: 'One short loop from signal to follow-through.',
    steps: [
      {
        label: '01',
        title: 'Connect one program',
        description: 'Start with the Slack threads, GitHub issues, docs, and notes around one launch or release.'
      },
      {
        label: '02',
        title: 'Review the draft',
        description: 'Oliver pulls the signal into a weekly update, current risks, and clear next owners.'
      },
      {
        label: '03',
        title: 'Send or follow up',
        description: 'Approve the update, send it, or let Oliver follow up on the moves that need to happen next.'
      }
    ]
  },
  trust: {
    eyebrow: 'Trust And Control',
    title: 'You review the moves that matter.',
    intro:
      'Oliver should tighten follow-through, not create mystery automation. The operating rule is simple: clear boundaries, reviewable output, and one shared DoWhiz account flow underneath.',
    items: [
      {
        title: 'Review before send',
        description: 'Status updates and follow-ups can wait for your approval when the stakes are high.'
      },
      {
        title: 'Bounded permissions',
        description: 'Oliver only works inside the tools and scopes you explicitly connect.'
      },
      {
        title: 'Current account flow',
        description: 'This page hands off to the existing DoWhiz auth and dashboard experience for now.'
      }
    ],
    primaryCta: 'Start with one weekly review'
  }
};
