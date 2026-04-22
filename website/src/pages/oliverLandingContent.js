export const OLIVER_ENTRY_SURFACE = 'oliver_tpm';
export const OLIVER_LANDING_VARIANT = 'oliver_tpm_hook_v1';
export const OLIVER_AUTH_OVERVIEW_HREF = '/auth/index.html?loggedIn=true&entry=oliver_tpm#section-overview';
export const OLIVER_AUTH_WORK_HREF = '/auth/index.html?loggedIn=true&entry=oliver_tpm#section-work';

export const oliverLandingContent = {
  metadata: {
    title: 'Oliver by DoWhiz | AI TPM for product and engineering teams',
    description:
      'Oliver is the AI TPM for product and engineering teams. Turn scattered execution signals into status updates, risk flags, decision follow-through, and clear next moves.',
    canonicalUrl: 'https://dowhiz.com/oliver',
    robots: 'noindex, nofollow',
    htmlLang: 'en',
    ogLocale: 'en_US',
    themeColor: '#eef1f5',
    ogImage: 'https://dowhiz.com/assets/DoWhiz.svg',
    ogImageAlt: 'Oliver TPM hook landing page preview'
  },
  nav: {
    links: [
      { href: '#proof', label: 'Outputs' },
      { href: '#ownership', label: 'What Oliver Owns' },
      { href: '#workflow', label: 'How It Works' },
      { href: '#controls', label: 'Controls' }
    ],
    primaryCta: 'Open Oliver',
    secondaryCta: 'See the work view'
  },
  hero: {
    eyebrow: 'AI TPM for product and engineering teams',
    title: 'Keep launches, updates, and follow-through moving.',
    subtitle:
      'Oliver turns scattered Slack threads, GitHub issues, meeting notes, and docs into owner-based next steps, status drafts, risk flags, and decision follow-through.',
    note:
      'This is a TPM-shaped entrypoint on top of the current DoWhiz auth and dashboard flow. The story is narrower now, not the underlying system.',
    proofPills: [
      'Weekly status drafts',
      'Risk register',
      'Decision follow-through',
      'Owner nudges across tools'
    ],
    primaryCta: 'Open Oliver',
    secondaryCta: 'See the outputs',
    artifact: {
      label: 'Cross-functional launch review',
      title: 'Mobile release',
      healthLabel: 'At risk',
      healthDetail: '2 blockers, 1 pending decision',
      signalSources: ['Slack blocker thread', 'GitHub regression', 'Meeting recap', 'Release notes draft'],
      summary: {
        label: 'Draft update',
        text:
          'Checkout is feature-complete, but QA sign-off is blocked by the payment callback regression. Android fallback is defined; iOS fallback still needs approval.'
      },
      actions: [
        { owner: 'Engineering', task: 'Confirm callback fix ETA', due: 'Today, 3:00 PM' },
        { owner: 'Product', task: 'Approve iOS fallback copy', due: 'Today, 4:30 PM' },
        { owner: 'Ops', task: 'Update launch note once decision lands', due: 'Before send' }
      ],
      risks: [
        { level: 'High', label: 'QA sign-off depends on one unresolved callback bug' },
        { level: 'Medium', label: 'Fallback plan is partially drafted but not approved' }
      ]
    }
  },
  proof: {
    eyebrow: 'Outputs',
    title: 'Oliver proves the role through TPM artifacts, not chat.',
    intro:
      'The point is not another assistant panel. The point is producing the exact artifacts a TPM has to keep current and reviewable.',
    cards: [
      {
        eyebrow: 'Weekly status update',
        title: 'A draft you can send, not a transcript you still need to interpret',
        summary:
          'Oliver groups signal into what moved, what is blocked, and what needs leadership attention next.',
        bullets: [
          'Launch health, milestone movement, and current owner status',
          'A short escalation block for risks that need help',
          'A next-48-hours section that turns noise into concrete follow-through'
        ]
      },
      {
        eyebrow: 'Risk register',
        title: 'A living view of blockers before they become surprises',
        summary:
          'Each risk tracks severity, owner, mitigation, and the next review point instead of vanishing inside threads.',
        bullets: [
          'Severity and impact stay explicit',
          'Mitigation notes stay attached to the source signal',
          'Owners and review dates stay visible until resolved'
        ]
      },
      {
        eyebrow: 'Decision log',
        title: 'Decisions stay linked to owners, rationale, and the next move',
        summary:
          'Oliver flags unresolved calls, keeps the latest direction attached to the program, and tracks what each decision changed.',
        bullets: [
          'What was decided and why it matters',
          'Who still needs to act on the decision',
          'What Oliver should draft or follow up next'
        ]
      }
    ]
  },
  ownership: {
    eyebrow: 'What Oliver Owns',
    title: 'The story only works if the work objects are clear.',
    intro:
      'Oliver should feel like an AI TPM, not a shape-shifting helper. These are the four outcomes this hook is optimizing for.',
    cards: [
      {
        title: 'Turns messy threads into owner-based action plans',
        description:
          'He pulls the next move out of meetings, chat, issues, and scattered follow-up without asking you to rewrite everything yourself.'
      },
      {
        title: 'Drafts stakeholder-ready status updates',
        description:
          'He turns execution signal into concise, reviewable updates instead of leaving you to translate raw progress on Friday afternoon.'
      },
      {
        title: 'Surfaces risks before they become blockers',
        description:
          'He makes dependencies, slippage, and missing approvals visible early enough for you to intervene.'
      },
      {
        title: 'Keeps decisions and follow-through from getting lost',
        description:
          'He remembers what changed, who owns the next step, and what still has to be chased down.'
      }
    ]
  },
  workflow: {
    eyebrow: 'How Oliver Works',
    title: 'A TPM loop, not a generic prompt box.',
    steps: [
      {
        label: '01',
        title: 'Collect signals',
        description:
          'Slack messages, GitHub issues, meeting notes, docs, and email become source material instead of isolated fragments.'
      },
      {
        label: '02',
        title: 'Build the operating picture',
        description:
          'Oliver groups what changed, what is blocked, who owns the next move, and which decisions are still open.'
      },
      {
        label: '03',
        title: 'Drive the next move',
        description:
          'He drafts updates, prepares follow-ups, and keeps the coordination loop alive until you review or send.'
      }
    ]
  },
  tools: {
    eyebrow: 'Works Across Your Tools',
    title: 'The tools are signal sources. They are not the product story.',
    intro:
      'Slack, GitHub, email, docs, and meetings matter because they hold execution signal. Oliver is the TPM layer that turns that signal into forward motion.',
    items: ['Slack', 'GitHub', 'Email', 'Docs', 'Meeting notes', 'Shared trackers']
  },
  controls: {
    eyebrow: 'Controls and Review',
    title: 'You stay in control of the moves that matter.',
    intro:
      'This hook should not imply blind automation. The point is tighter follow-through with clear boundaries.',
    cards: [
      {
        title: 'Review before send',
        description: 'Draft updates and follow-ups can wait for your approval when the stakes are high.'
      },
      {
        title: 'Bounded permissions',
        description: 'Oliver should act inside the tools and scopes you explicitly connect, not through vague blanket access.'
      },
      {
        title: 'Saved context',
        description: 'Team norms, recurring deliverables, and known stakeholders should compound instead of being re-explained every time.'
      },
      {
        title: 'Configurable follow-up rules',
        description: 'You decide what deserves a nudge, what waits, and what should always stay human-reviewed.'
      }
    ]
  },
  faq: {
    eyebrow: 'FAQ',
    title: 'Narrow answers for a narrow story.',
    items: [
      {
        question: 'Is Oliver replacing a TPM?',
        answer:
          'No. This hook is for teams that want tighter execution follow-through. You still review the output, decide what matters, and own the actual calls.'
      },
      {
        question: 'Do I need to move my team into a new tool?',
        answer:
          'No. `/oliver` is just a TPM-shaped entrypoint. Sign-in and setup still hand off to the current DoWhiz auth and dashboard surface.'
      },
      {
        question: 'Which teams is this for right now?',
        answer:
          'Product and engineering teams running launches, releases, migrations, or cross-functional work where follow-through and status clarity matter more than feature planning.'
      }
    ]
  },
  finalCta: {
    eyebrow: 'Start Small',
    title: 'Start with the launch or program that already feels too noisy.',
    description:
      'Bring one weekly review, one release train, or one cross-functional stream. If the story is right, the rest of the product can catch up behind it.',
    primaryCta: 'Open Oliver',
    secondaryCta: 'See the work view',
    disclaimer:
      'This is still the current DoWhiz account flow underneath. The experiment here is the TPM story, not a separate product backend.'
  }
};
