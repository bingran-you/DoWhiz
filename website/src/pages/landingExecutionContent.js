const EN_HOMEPAGE_OVERRIDES = {
  metadata: {
    title: 'DoWhiz | Keep launches from drifting',
    description:
      'Turn messy planning threads into owners, dates, blockers, decisions, and evidence-backed readiness across Slack, email, GitHub, docs, and your systems of record.'
  },
  nav: {
    links: [
      { href: '#workflow', label: 'How it works' },
      { href: '#outputs', label: 'What you get' }
    ],
    signIn: 'Open workspace',
    dashboard: 'Workspace',
    contactAriaLabel: 'Email DoWhiz'
  },
  hero: {
    eyebrow: 'For PMs and product leads running launches',
    title: 'DoWhiz keeps launches from drifting',
    subtitle:
      'Start with the thread. Get owners, blockers, proof, and updates across Slack, Discord, GitHub, docs, and Notion.',
    primaryCta: 'Start with a thread',
    secondaryCta: 'See the workflow',
    secondaryHref: '#workflow',
    manageAnonymous: 'Open workspace',
    manageAuthenticated: 'Open your workspace',
    contactSubject: 'Help me turn this launch thread into a plan',
    contactBody:
      'Hi DoWhiz,\n\nPlease turn this launch thread into an execution plan.\n\n- Launch or rollout:\n- Current thread or notes:\n- Owners already in motion:\n- What is blocked right now:\n- What done looks like:\n\nThanks!',
    toolsEyebrow: 'Where the work starts',
    toolsFootnote:
      'DoWhiz starts in the thread, turns it into a live plan, and keeps follow-through tied to your system of record.',
    previewLabel: 'Thread-native preview',
    primaryChannelsLabel: 'Starts in the thread',
    secondaryChannelsLabel: 'Follows through into systems of record',
    showcaseSummary:
      'The thread is where the work starts. The output is a plan, tracked follow-through, and linked evidence.',
    proofPills: ['Thread in', 'Plan out', 'Updates synced'],
    supportNote:
      'For PMs and product leads running launches.',
    operatorName: 'Oliver',
    operatorRole: 'Representative coordination agent',
    tools: {
      email: {
        anonymousStatus: 'Direct thread start',
        authenticatedStatus: 'Direct thread start',
        anonymousActionLabel: 'Forward thread',
        authenticatedActionLabel: 'Forward thread',
        description: 'Forward the thread or launch recap you already have',
        samplePrompt: 'Turn this launch thread into an execution plan with owners, blockers, and next steps',
        sampleReply: 'I can extract owners, dates, blockers, open decisions, and missing readiness evidence.',
        stage: {
          appMeta: 'Launch follow-through',
          threads: [
            {
              from: 'Launch core thread',
              subject: 'Friday release review',
              preview: 'Docs owner is still open and QA sign-off order is unclear.',
              time: '2m',
              active: true
            },
            {
              from: 'Support lead',
              subject: 'Macro and escalation check',
              preview: 'Need final owner plus evidence before launch-day handoff.',
              time: '48m'
            },
            {
              from: 'Eng manager',
              subject: 'Rollback note gap',
              preview: 'We still need the linked staging proof before go-live.',
              time: '2h'
            }
          ],
          composeTitle: 'Forward thread',
          draftBadge: 'Forward',
          ccLabel: 'Thread',
          ccValue: 'launch-core@acme.com',
          subjectValue: 'Turn Friday release thread into an execution plan',
          tags: ['Launch', 'Needs owners'],
          bodyLines: [
            'Hi DoWhiz,',
            'Please turn this thread into a live launch plan.',
            '- docs owner is still unclear',
            '- QA sign-off order is not locked',
            '- support macro and rollback proof are still missing'
          ],
          footerNote: 'DoWhiz extracts owners, blockers, and missing evidence before the next meeting',
          footerValue: 'Forward thread',
          resultLabel: 'DoWhiz output',
          resultTitle: 'Execution plan ready',
          resultItems: [
            'Owner, date, and blocker table',
            'Unresolved decision log',
            'Readiness gaps to chase before launch'
          ]
        }
      },
      slack: {
        description: 'Read the thread where launch coordination is already happening',
        samplePrompt: 'Read this launch thread and pull owners, dates, blockers, and missing evidence',
        sampleReply: 'I can turn the thread into a reviewable plan and post the follow-through back to the channel.',
        stage: {
          workspace: 'launch-core',
          workspaceMeta: '11 teammates online',
          channels: ['#launch-ops', '#release-room', '#support-handoff'],
          roomMeta: '41 messages today',
          threadPills: ['launch', 'needs owner', 'readiness'],
          threadActivity: '7 replies in thread',
          messages: [
            {
              author: 'Mina',
              meta: 'PM',
              time: '10:14 AM',
              text: 'We still need one owner for docs, support macros, and the launch email.',
              reactions: ['eyes 4', 'check 1']
            },
            {
              author: 'Theo',
              meta: 'Eng',
              time: '10:19 AM',
              text: 'QA can validate today, but only if rollback notes and staging proof are linked first.',
              reactions: ['warning 2']
            },
            {
              author: 'You',
              meta: 'Ask DoWhiz',
              time: '10:23 AM',
              text: 'Turn this thread into owners, blockers, dates, and what is still missing for readiness.',
              tone: 'user'
            }
          ],
          threadLabel: 'Launch thread summary',
          threadTitle: 'Release blockers',
          threadText:
            'Docs owner is still missing. QA order is not final. Launch email, support macro, and rollback evidence still need owners and proof.',
          cardLabel: 'Execution plan',
          cardTitle: 'Reviewable next steps',
          cardSummary: 'One clean update, owners, blockers, and next actions',
          cardItems: [
            'Assign docs owner before 3 PM',
            'Lock QA sign-off order with linked staging proof',
            'Attach support macro and rollback evidence to the launch brief'
          ],
          cardFooter: 'Ready to post back into the thread',
          cardActions: ['Post update', 'Open brief']
        }
      },
      discord: {
        description: 'Turn a customer blocker thread into tracked follow-through',
        samplePrompt: 'Read this blocker report and turn it into owner, severity, and follow-through',
        sampleReply: 'I can pull the repro gaps, open the task, and keep the original thread updated.',
        stage: {
          server: 'beta-customers',
          onlineLabel: '52 online',
          channels: ['announcements', 'launch-blockers', 'release-notes'],
          room: '#launch-blockers',
          roomTopic: 'Customer blocker follow-through',
          roomMeta: '3 active threads',
          messages: [
            {
              author: 'Avery',
              meta: 'Customer',
              time: 'Today at 9:12 AM',
              accent: '#f2b4ff',
              text: 'Upgrade still blocks checkout for EU testers after the last release.'
            },
            {
              author: 'Mina',
              meta: 'Launch lead',
              time: 'Today at 9:14 AM',
              accent: '#8ec5ff',
              text: 'We need severity, owner, acceptance criteria, and staging validation before relaunch.'
            },
            {
              author: 'You',
              meta: 'Prompt DoWhiz',
              time: 'Today at 9:16 AM',
              accent: '#9aa4ff',
              text: 'Turn this blocker thread into tracked follow-through and prep the handoff.',
              tone: 'user'
            }
          ],
          planLabel: 'DoWhiz follow-through',
          planTitle: 'Customer blocker loop',
          planItems: [
            'Ask for missing repro details and affected segment',
            'Open the tracked task with owner, priority, and acceptance criteria',
            'Return staging evidence and status back to the original thread'
          ],
          planActions: ['Open task', 'Reply in thread'],
          botLabel: 'DoWhiz follow-through',
          botTitle: 'Hotfix handoff',
          botDescription: 'Here is the execution loop pulled from the blocker thread.',
          botFields: [
            { label: 'Owner', value: 'Payments engineer plus launch lead' },
            { label: 'Now', value: 'Confirm repro details and affected customers' },
            { label: 'Before relaunch', value: 'Link staging proof and validation back to the thread' }
          ],
          composerValue: '/dowhiz turn this blocker into tracked follow-through',
          membersTitle: 'Online now',
          members: [
            { name: 'DoWhiz', role: 'Bot', accent: true },
            { name: 'Avery', role: 'Customer' },
            { name: 'Mina', role: 'Launch lead' }
          ]
        }
      },
      github: {
        anonymousStatus: 'Follow through from the thread',
        authenticatedStatus: 'Connected repo context',
        anonymousActionLabel: 'Start in thread',
        authenticatedActionLabel: 'Connect',
        description: 'Carry launch blockers into tracked repo follow-through',
        samplePrompt: 'Prepare the GitHub issue from this blocker thread and show what still needs proof',
        sampleReply: 'I can open the issue context, flag review steps, and keep readiness evidence attached.',
        stage: {
          repo: 'acme/launch-app',
          repoMeta: 'release branch',
          filters: ['is:open', 'label:launch', 'sort:updated-desc'],
          overview: { open: '9 Open', closed: '214 Closed' },
          issues: [
            {
              id: '#482',
              title: 'Checkout banner blocks EU upgrade during launch window',
              meta: 'mina opened 24m ago',
              status: 'Open',
              labels: ['launch', 'checkout'],
              comments: '6',
              active: true
            },
            {
              id: '#479',
              title: 'Support macro approval still missing',
              meta: 'sam updated 1h ago',
              status: 'Open',
              labels: ['launch', 'support'],
              comments: '3'
            },
            {
              id: '#477',
              title: 'Rollback evidence not linked in readiness brief',
              meta: 'tina updated yesterday',
              status: 'Open',
              labels: ['launch', 'risk'],
              comments: '4'
            }
          ],
          detailLabel: 'Tracked issue',
          detailTitle: 'Checkout banner blocks EU upgrade during launch window',
          detailSummary:
            'Customer report is confirmed. Owner is assigned. Relaunch still depends on staging proof, support readiness, and go or no-go review.',
          detailMeta: ['launch', 'customer blocker', 'priority: high'],
          detailItems: [
            'Capture repro, owner, and acceptance criteria from the original thread',
            'Attach staging validation evidence before relaunch',
            'Post the status back to launch ops and support'
          ],
          detailChecklist: [
            { title: 'Owner and acceptance criteria captured', meta: 'In progress', state: 'progress' },
            { title: 'Staging proof linked', meta: 'Waiting on validation', state: 'todo' },
            { title: 'Support handoff updated', meta: 'Queued after review', state: 'todo' }
          ],
          detailCommentTitle: 'DoWhiz coordination note',
          detailComment:
            'Human approval and staging validation are still required before relaunch. Keep the original thread and readiness brief in sync.',
          detailActivity: [
            {
              actor: 'DoWhiz',
              text: 'Attached owner, blocker, and evidence checklist from the launch thread.',
              meta: 'commented 3m ago'
            },
            {
              actor: 'mina',
              text: 'Marked relaunch decision as blocked on staging proof and support confirmation.',
              meta: 'updated 21m ago'
            }
          ],
          detailFooter: 'Ready to review before posting back to GitHub'
        }
      },
      notion: {
        anonymousStatus: 'Draft from the thread',
        authenticatedStatus: 'Connected docs context',
        anonymousActionLabel: 'Start in thread',
        authenticatedActionLabel: 'Connect',
        description: 'Keep a readiness brief synced to the original launch thread',
        samplePrompt: 'Turn this planning thread into a launch-readiness brief',
        sampleReply: 'I can create the brief, list missing evidence, and keep it updated as status changes.',
        stage: {
          breadcrumb: 'Product / Launches / Friday release',
          collaborators: ['You', 'DoWhiz', 'Launch team'],
          pageTitle: 'Friday launch readiness',
          pageIntro: 'Owner, blocker, decision, and evidence brief',
          properties: [
            { label: 'Launch', value: 'EU upgrade release' },
            { label: 'Status', value: 'Needs proof' },
            { label: 'Owner', value: 'Launch lead + DoWhiz' }
          ],
          blocks: [
            { type: 'heading', text: 'Open blockers' },
            { type: 'bullet', text: 'Docs owner still missing' },
            { type: 'bullet', text: 'Support macro needs final approval' },
            { type: 'todo', text: 'Link staging proof for checkout hotfix' },
            { type: 'callout', text: 'DoWhiz keeps this brief tied to the original thread and linked evidence' }
          ],
          databaseLabel: 'Readiness tracker',
          databaseColumns: ['Item', 'Owner', 'Status'],
          rows: [
            { name: 'Docs owner', meta: 'PMM', status: 'Missing' },
            { name: 'Support macro', meta: 'Support lead', status: 'Awaiting approval' },
            { name: 'Staging proof', meta: 'QA', status: 'In progress' }
          ],
          sideLabel: 'Readiness brief',
          sideTitle: 'What still needs proof',
          sideItems: [
            'One accountable docs owner',
            'Support handoff approval',
            'Linked staging validation before go-live'
          ],
          sideFootnote: 'Ready to review in the go or no-go meeting'
        }
      },
      lark: {
        anonymousStatus: 'Continue from the thread',
        authenticatedStatus: 'Connected workspace',
        anonymousActionLabel: 'Start in thread',
        authenticatedActionLabel: 'Connect',
        description: 'Carry launch follow-through into the workspace your team already uses',
        samplePrompt: 'Turn this launch sync into owners, dates, and a sendable update',
        sampleReply: 'I can keep the sync actionable and return a clean update card with the latest blockers.',
        stage: {
          workspace: 'Launch sync',
          workspaceMeta: '7 participants',
          chatTitle: 'Launch sync',
          chatMeta: '7 participants',
          participants: ['Nina', 'Sam', 'DoWhiz'],
          recapLabel: 'Sync recap',
          recapTitle: 'Launch email, support handoff, and final readiness proof',
          recapText:
            'Need final owners for the launch email, support staffing confirmation, and one clean readiness update before Friday.',
          messages: [
            {
              author: 'Nina',
              meta: 'PM',
              time: '10:02',
              text: 'We have the checklist, but the final owners and send order are still fuzzy.'
            },
            {
              author: 'Sam',
              meta: 'Support',
              time: '10:07',
              badge: 'Needs update',
              text: 'I also need the final macro approval and the evidence link before handoff.'
            }
          ],
          trackerLabel: 'DoWhiz follow-through',
          trackerTitle: 'Owners and timing',
          ownerColumns: ['Owner', 'Task', 'Due'],
          owners: [
            { owner: 'Nina', task: 'Launch email timing', due: 'Thu 3 PM', status: 'In progress' },
            { owner: 'Sam', task: 'Support macro approval', due: 'Thu 5 PM', status: 'Queued' },
            { owner: 'DoWhiz', task: 'Readiness update card', due: 'Now', status: 'Ready' }
          ],
          updateLabel: 'Sendable update',
          updateText:
            'Launch email timing locks Thursday. Support handoff closes after approval and linked proof. Readiness brief updates automatically.',
          updateActions: ['Share update', 'Open brief']
        }
      }
    }
  },
  workflowVisual: {
    eyebrow: 'How it works',
    title: 'How DoWhiz drives one item',
    subtitle: 'Thread to task to deploy to update.',
    note: 'Human review stays visible.',
    stages: [
      {
        key: 'discord',
        app: 'Slack / Discord',
        title: 'Reply in thread',
        copy: 'Ask repro. Confirm blocker.'
      },
      {
        key: 'notion',
        app: 'Notion',
        title: 'Create task',
        copy: 'Set owner. Track priority.'
      },
      {
        key: 'github',
        app: 'GitHub',
        title: 'Open issue',
        copy: 'Link context. Route engineering.'
      },
      {
        key: 'devin',
        app: 'Devin',
        title: 'Route fix',
        copy: 'Assign Devin or teammate.'
      },
      {
        key: 'review',
        app: 'Review',
        title: 'Review fix',
        copy: 'Keep approval visible.'
      },
      {
        key: 'deploy',
        app: 'GitHub Action',
        title: 'Deploy build',
        copy: 'Run staging deploy.'
      },
      {
        key: 'staging',
        app: 'Staging',
        title: 'Link proof',
        copy: 'Verify fix in staging.'
      },
      {
        key: 'thread',
        app: 'Thread update',
        title: 'Post update',
        copy: 'Sync thread and record.'
      }
    ]
  },
  artifactRail: {
    eyebrow: 'What you get',
    title: 'What DoWhiz delivers',
    subtitle: 'Plan, issue, proof, and update.',
    items: [
      {
        title: 'Execution plan',
        description: 'Owners, dates, blockers, and decisions.'
      },
      {
        title: 'Task + issue',
        description: 'Tracked handoff with linked context.'
      },
      {
        title: 'Readiness brief',
        description: 'Go or no-go with linked proof.'
      },
      {
        title: 'Thread update',
        description: 'Clean status back to the source.'
      }
    ]
  },
  controlRail: {
    items: [
      {
        kicker: 'Not chat',
        title: 'Drives the work',
        description: 'Turns the thread into owned follow-through.'
      },
      {
        kicker: 'Not a dashboard',
        title: 'Starts in thread',
        description: 'Carries source context into the tools that matter.'
      },
      {
        kicker: 'Not a black box',
        title: 'Keeps review visible',
        description: 'Human review and evidence stay explicit.'
      }
    ]
  },
  problems: {
    eyebrow: 'Why launches drift',
    title: 'The thread gets longer. The plan gets fuzzier.',
    intro:
      'When launch coordination lives across chat, docs, tickets, and status meetings, the work stops being obvious long before the launch is actually ready.',
    items: [
      {
        title: 'Ownership stays fuzzy',
        description: 'Launch threads get long, but nobody is clearly accountable for the next move.'
      },
      {
        title: 'Status goes stale',
        description: 'By the next meeting, the update is already outdated or missing the latest blocker.'
      },
      {
        title: 'Blockers hide in chat',
        description: 'Critical dependencies and unresolved decisions disappear inside Slack, email, and side threads.'
      },
      {
        title: 'Readiness runs on vibes',
        description: 'Go or no-go calls happen without linked evidence, clear owners, or explicit open risks.'
      },
      {
        title: 'PMs become the glue',
        description: 'Product leads spend their time chasing updates across tools instead of driving the launch.'
      }
    ]
  },
  workflow: {
    eyebrow: 'From thread to plan',
    title: 'From messy thread to execution plan',
    subtitle:
      'DoWhiz reads the thread where the work already lives, pulls the execution objects out of it, and keeps follow-through moving across tools.',
    steps: [
      {
        title: 'Start in the thread',
        description: 'DoWhiz reads the launch thread, recap, or blocker conversation where the work already exists.'
      },
      {
        title: 'Extract the work',
        description: 'It identifies owners, dates, blockers, dependencies, unresolved decisions, and missing evidence.'
      },
      {
        title: 'Create the execution plan',
        description: 'DoWhiz turns the thread into a live plan with clear next steps and reviewable artifacts.'
      },
      {
        title: 'Follow through across tools',
        description: 'Context carries into docs, tickets, GitHub, Notion, and other systems of record.'
      },
      {
        title: 'Chase what is missing',
        description: 'DoWhiz asks for stale updates, unresolved decisions, and blocked dependencies before they drift.'
      },
      {
        title: 'Produce readiness evidence',
        description: 'You get a launch-readiness brief backed by linked evidence instead of status vibes.'
      }
    ]
  },
  exampleLoop: {
    eyebrow: 'Concrete example',
    title: 'From customer feedback to shipped follow-through',
    subtitle:
      'A blocker lands in Slack or Discord. DoWhiz keeps the loop moving without losing the original thread or overclaiming autonomy.',
    summaryTitle: 'Example thread',
    summaryItems: [
      'A customer reports a launch blocker in a public or internal thread.',
      'The team needs severity, owner, acceptance criteria, and the next validation step.',
      'The original thread still needs clean follow-up after implementation and review.'
    ],
    steps: [
      {
        title: 'Acknowledge the thread',
        description: 'DoWhiz asks for missing repro details, scope, and what still blocks the launch.'
      },
      {
        title: 'Track the work',
        description: 'It turns the blocker into a tracked task with priority, owner, and acceptance criteria.'
      },
      {
        title: 'Prepare the handoff',
        description: 'DoWhiz opens or drafts the GitHub issue with the relevant context from the original thread.'
      },
      {
        title: 'Keep specialists unblocked',
        description: 'A teammate or specialist agent can take implementation, review, or validation work from there.'
      },
      {
        title: 'Summarize evidence',
        description: 'DoWhiz keeps review status, risk, and validation evidence tied back to the execution thread.'
      },
      {
        title: 'Close the loop',
        description: 'After human approval and staging validation, DoWhiz updates the original thread and system of record.'
      }
    ],
    calloutTitle: 'Oliver can coordinate this loop',
    calloutCopy:
      'Oliver coordinates the launch thread. Specialist agents or teammates can take over implementation, review, or follow-through when needed.'
  },
  outputs: {
    eyebrow: 'Outputs',
    title: 'What DoWhiz produces',
    subtitle:
      'The homepage promise is not smart chat. It is concrete execution artifacts your team can review, update, and use.',
    items: [
      {
        title: 'Execution plan',
        description: 'A live plan with clear next steps instead of an unstructured thread.'
      },
      {
        title: 'Owner, date, blocker table',
        description: 'Who owns what, when it is due, and what is currently blocked.'
      },
      {
        title: 'Decision log',
        description: 'The unresolved calls that still need answers before launch.'
      },
      {
        title: 'Dependency map',
        description: 'The blockers, prerequisites, and handoffs that are easy to miss in chat.'
      },
      {
        title: 'Launch-readiness brief',
        description: 'A reviewable go or no-go summary backed by linked evidence.'
      },
      {
        title: 'Follow-up messages',
        description: 'Clean updates ready to post back into the original thread or channel.'
      },
      {
        title: 'System-of-record updates',
        description: 'Docs, tickets, and issue trackers updated without losing the original context.'
      },
      {
        title: 'Evidence-backed status summary',
        description: 'Status with proof attached, not memory, hearsay, or stale meeting notes.'
      }
    ]
  },
  useCases: {
    eyebrow: 'Use cases',
    title: 'Built for execution-critical threads',
    subtitle:
      'Launch is the hero use case, but the same thread-to-plan loop helps anywhere cross-team follow-through can drift.',
    items: [
      {
        tag: 'Hero use case',
        title: 'Product launch',
        description: 'Turn launch chatter into owners, blockers, readiness, and launch-day follow-through.'
      },
      {
        tag: 'Customer-facing',
        title: 'Customer rollout',
        description: 'Keep rollout blockers, approvals, and evidence tied to the customer thread.'
      },
      {
        tag: 'Infra',
        title: 'Infrastructure migration',
        description: 'Track dependencies, unresolved risks, and readiness across teams and systems.'
      },
      {
        tag: 'Partner',
        title: 'Partner integration',
        description: 'Turn fragmented follow-up across email, docs, and tickets into one execution loop.'
      },
      {
        tag: 'Incident',
        title: 'Incident follow-through',
        description: 'Carry decisions, owners, fixes, and post-incident actions across the tools already in use.'
      },
      {
        tag: 'Review',
        title: 'Release readiness review',
        description: 'Create evidence-backed readiness instead of relying on whichever update was heard last.'
      }
    ]
  },
  continuity: {
    eyebrow: 'Cross-tool continuity',
    title: 'Start in the thread. Follow through across tools.',
    subtitle:
      'DoWhiz carries context from the original thread into the tools your team already maintains, so nobody has to keep a second dashboard manually in sync.',
    pillars: [
      {
        title: 'Thread-native start',
        description: 'The work begins in Slack, email, Discord, or the thread where the discussion already exists.'
      },
      {
        title: 'Reviewable artifacts',
        description: 'Docs, issues, briefs, and follow-up messages stay tied to the source thread and linked evidence.'
      },
      {
        title: 'System-of-record updates',
        description: 'DoWhiz keeps GitHub, docs, Notion, tickets, and status updates aligned as the work moves.'
      }
    ],
    threadStarts: ['Slack', 'Email', 'Discord'],
    systems: ['GitHub', 'Docs', 'Notion', 'Tickets', 'Launch updates']
  },
  trust: {
    eyebrow: 'Safety and control',
    title: 'Reviewable by default',
    subtitle:
      'Execution is only credible if sensitive actions stay visible, scoped, and attributable. The homepage should say that plainly.',
    items: [
      {
        title: 'Human review before sensitive changes',
        description: 'Sensitive code, release, and production decisions stay reviewable instead of happening in the background.'
      },
      {
        title: 'Scoped tool permissions',
        description: 'Teams decide which tools DoWhiz can access and how deep those permissions should go.'
      },
      {
        title: 'Linked evidence',
        description: 'Readiness and status summaries can point back to the thread, issue, doc, or validation proof behind them.'
      },
      {
        title: 'Clear audit trail',
        description: 'Updates, artifacts, and follow-through stay attributable instead of disappearing inside hidden automation.'
      }
    ],
    note: 'No hidden autonomous PR approvals. No silent production merges. No invisible code changes.'
  },
  faqItems: [
    {
      question: 'Who is DoWhiz for on the homepage?',
      answer:
        'The main homepage is for PMs, product leads, founders, engineering leads, operations leads, and launch owners running execution-critical threads. It is not positioned as a generic chatbot or a TPM-only tool.'
    },
    {
      question: 'Where does the work start?',
      answer:
        'Usually in the messy thread you already have: Slack, email, Discord, or the planning recap nobody wants to manually turn into a plan.'
    },
    {
      question: 'What does DoWhiz actually produce?',
      answer:
        'Execution plans, owner and blocker tables, decision logs, readiness briefs, follow-up messages, and linked system-of-record updates that stay reviewable.'
    },
    {
      question: 'Is this only for launches?',
      answer:
        'Launch is the hero use case, but the same workflow helps with rollouts, migrations, partner integrations, incident follow-through, and other execution-critical threads.'
    },
    {
      question: 'Does DoWhiz make hidden code or production changes?',
      answer:
        'No. The page now states the opposite. Sensitive implementation, review, and production actions still require human review and visible approval.'
    }
  ],
  labels: {
    faqEyebrow: 'Questions',
    faqTitle: 'Before you start',
    faqIntro: 'Short answers before you drop a real thread into the workflow.',
    footerTagline: 'DoWhiz keeps launch work from drifting.',
    footerPill: 'Starts in the thread. Stays reviewable.',
    footerBottomSecondary: 'Thread in. Plan out. Tools stay aligned.',
    footerContactLabel: 'Contact DoWhiz'
  },
  finalCta: {
    title: 'Start with the thread',
    description: 'DoWhiz turns the thread into a live execution plan and keeps follow-through moving across tools.',
    support: 'Best first input: a launch thread, blocker thread, rollout recap, or release-readiness thread.'
  }
};

function mergeHeroTools(baseTools = [], overrides = {}) {
  return baseTools.map((tool) => {
    const override = overrides[tool.key];
    if (!override) {
      return tool;
    }

    return {
      ...tool,
      ...override,
      stage: override.stage ? { ...tool.stage, ...override.stage } : tool.stage
    };
  });
}

export function buildEnglishHomepageContent(base) {
  const heroOverrides = EN_HOMEPAGE_OVERRIDES.hero || {};

  return {
    ...base,
    metadata: { ...base.metadata, ...EN_HOMEPAGE_OVERRIDES.metadata },
    nav: { ...base.nav, ...EN_HOMEPAGE_OVERRIDES.nav },
    hero: {
      ...base.hero,
      ...heroOverrides,
      actionLabels: { ...base.hero.actionLabels, ...(heroOverrides.actionLabels || {}) },
      tools: mergeHeroTools(base.hero.tools, heroOverrides.tools)
    },
    workflowVisual: EN_HOMEPAGE_OVERRIDES.workflowVisual,
    artifactRail: EN_HOMEPAGE_OVERRIDES.artifactRail,
    controlRail: EN_HOMEPAGE_OVERRIDES.controlRail,
    problems: EN_HOMEPAGE_OVERRIDES.problems,
    workflow: EN_HOMEPAGE_OVERRIDES.workflow,
    exampleLoop: EN_HOMEPAGE_OVERRIDES.exampleLoop,
    outputs: EN_HOMEPAGE_OVERRIDES.outputs,
    useCases: EN_HOMEPAGE_OVERRIDES.useCases,
    continuity: EN_HOMEPAGE_OVERRIDES.continuity,
    trust: EN_HOMEPAGE_OVERRIDES.trust,
    finalCta: EN_HOMEPAGE_OVERRIDES.finalCta,
    faqItems: EN_HOMEPAGE_OVERRIDES.faqItems,
    labels: { ...base.labels, ...EN_HOMEPAGE_OVERRIDES.labels }
  };
}
