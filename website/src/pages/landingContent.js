const EN_LANDING_CONTENT = {
  metadata: {
    title: 'DoWhiz | AI digital employees in email, Slack, and GitHub',
    description:
      'Start with Oliver in email, Slack, or Discord. DoWhiz turns real requests into finished work across GitHub, docs, and shared tools.',
    canonicalUrl: 'https://dowhiz.com/',
    ogLocale: 'en_US',
    themeColor: '#2C2C2E',
    htmlLang: 'en',
    robots: 'index, follow'
  },
  nav: {
    homePath: '/',
    links: [
      { href: '#channels', label: 'Start now' },
      { href: '#examples', label: 'What Oliver can do' },
      { href: '#faq', label: 'FAQ' }
    ],
    signIn: 'Manage setup',
    dashboard: 'Your Oliver',
    signOut: 'Sign out',
    githubAriaLabel: 'GitHub',
    discordAriaLabel: 'Discord',
    contactAriaLabel: 'Email Oliver'
  },
  hero: {
    eyebrow: 'Start with a real request',
    title: 'Use Oliver where you already work',
    subtitle:
      'Start with one real request in email, Slack, or Discord. Add GitHub, Notion, or Lark only when deeper context will genuinely help.',
    primaryCta: 'Start with one request',
    secondaryCta: 'Watch demo',
    manageAnonymous: 'Manage setup',
    manageAuthenticated: 'Open your setup',
    contactSubject: 'A first task for Oliver',
    contactBody:
      'Hi Oliver,\n\nHere is the first task I want help with:\n\n- Context:\n- What done looks like:\n- Any deadline:\n\nThanks!',
    toolsEyebrow: 'Choose a channel',
    toolsHintAnonymous:
      'Hover to pause. Click a channel to start. GitHub, Notion, and Lark can begin from email first.',
    toolsHintAuthenticated: 'Hover to pause. Signed-in channels can open direct connect flows when needed.',
    toolsFootnote:
      'Start in one place now. Save memory and deeper app access only when the first loop feels useful.',
    autoplayLabel: 'Rotating preview',
    pausedLabel: 'Preview paused',
    previewLabel: 'Live preview',
    primaryChannelsLabel: 'Start directly',
    secondaryChannelsLabel: 'Connect deeper later',
    showcaseSummary:
      'Email, Slack, and Discord are the fastest ways to begin. GitHub, Notion, and Lark stay available when deeper context will improve the result.',
    proofPills: ['One clear request', 'Reviewable output', 'Expand setup only when it helps'],
    entryEyebrow: 'Fastest start',
    previewEyebrow: 'What it looks like',
    youLabel: 'You',
    actionLabels: {
      connect: 'Connect',
      loading: 'Opening...'
    },
    tools: [
      {
        key: 'email',
        label: 'Email',
        pillLabel: 'Compose',
        anonymousStatus: 'Direct compose',
        authenticatedStatus: 'Direct compose',
        anonymousActionLabel: 'Send request',
        authenticatedActionLabel: 'Send request',
        description: 'Send the request you already have in mind',
        samplePrompt: 'Turn these notes into a concise follow-up email',
        sampleReply: 'I can draft the reply, tighten the tone, and point out what is still missing',
        accent: '#ff8a3d',
        stage: {
          appName: 'Inbox',
          appMeta: 'Draft with Oliver',
          folders: [
            { label: 'Inbox', count: '12' },
            { label: 'Drafts', count: '3', active: true },
            { label: 'Sent', count: '' },
            { label: 'Archive', count: '' }
          ],
          threads: [
            {
              from: 'Sage Capital',
              subject: 'Friday follow-up',
              preview: 'Loved the conversation today. Could you send the recap by Friday?',
              time: '2m',
              active: true
            },
            {
              from: 'Course admin',
              subject: 'Midterm office hours',
              preview: 'New time slots are open for next Tuesday.',
              time: '1h'
            },
            {
              from: 'Mina',
              subject: 'Launch recap',
              preview: 'Shared the latest checklist in the ops folder.',
              time: '3h'
            }
          ],
          composeTitle: 'Draft request',
          draftBadge: 'Reply',
          toLabel: 'To',
          toValue: 'oliver@dowhiz.com',
          ccLabel: 'Target',
          ccValue: 'sponsor@sagecap.com',
          subjectLabel: 'Request',
          subjectValue: 'Draft a warm follow-up after the sponsor call',
          tags: ['Warm tone', 'Friday recap'],
          bodyLines: [
            'Hi Oliver,',
            'Draft a concise reply for the sponsor.',
            '- thank them for hosting',
            '- mention the Friday recap',
            '- keep the ask light'
          ],
          footerNote: 'Oliver can tighten tone before you send',
          footerValue: 'Send request',
          resultLabel: 'Oliver notes',
          resultTitle: 'Reply ready',
          resultItems: [
            'A tighter subject line',
            'A clean three-paragraph draft',
            'A note on what detail is still missing'
          ]
        }
      },
      {
        key: 'slack',
        label: 'Slack',
        pillLabel: 'Thread',
        anonymousStatus: 'Public install',
        authenticatedStatus: 'Connect workspace',
        anonymousActionLabel: 'Add bot',
        authenticatedActionLabel: 'Connect',
        description: 'Install Oliver and ask in Slack',
        samplePrompt: 'Summarize this channel and list the next three actions',
        sampleReply: 'I can pull the thread context and return a clean action list',
        accent: '#36c58b',
        stage: {
          workspace: 'product-ops',
          workspaceMeta: '8 teammates online',
          channels: ['#launch-ops', '#customer-handoffs', '#weekly-plan'],
          sections: [
            {
              title: 'Channels',
              items: [
                { label: '#launch-ops', active: true },
                { label: '#customer-handoffs' },
                { label: '#weekly-plan' }
              ]
            },
            {
              title: 'Direct messages',
              items: [
                { label: 'Oliver', accent: 'bot' },
                { label: 'Mina' }
              ]
            }
          ],
          room: '#launch-ops',
          roomMeta: '23 messages today',
          roomMembers: ['Mina', 'Theo', 'Oliver'],
          threadPills: ['launch', 'handoff', 'needs owner'],
          threadActivity: '3 replies in thread',
          messages: [
            {
              author: 'Mina',
              meta: 'Ops',
              time: '10:14 AM',
              text: 'We still need one clear owner for docs, QA, and launch email.',
              reactions: ['eyes 2', 'check 1']
            },
            {
              author: 'Theo',
              meta: 'Design',
              time: '10:18 AM',
              text: 'Assets are ready, but the rollout order is not written down.',
              reactions: ['memo 1']
            },
            {
              author: 'You',
              meta: 'Ask Oliver',
              time: '10:22 AM',
              text: 'Summarize this thread and give me the next three actions.',
              tone: 'user'
            }
          ],
          threadLabel: 'Thread summary',
          threadTitle: 'Launch blockers',
          threadText: 'Docs owner is still missing. QA order is not final. The launch email needs one clean checklist.',
          cardLabel: 'Oliver block kit',
          cardTitle: 'Ready for the channel',
          cardSummary: 'One clean update and next actions',
          cardItems: [
            'Assign docs owner before 3 PM',
            'Lock QA sign-off order',
            'Post one rollout checklist back to the channel'
          ],
          cardFooter: 'One clean update and next actions',
          cardActions: ['Post summary', 'Open thread'],
          composerPlaceholder: 'Reply in #launch-ops',
          composerHint: 'Type a message'
        }
      },
      {
        key: 'discord',
        label: 'Discord',
        pillLabel: 'Server',
        anonymousStatus: 'Public invite',
        authenticatedStatus: 'Connect server',
        anonymousActionLabel: 'Add bot',
        authenticatedActionLabel: 'Connect',
        description: 'Invite Oliver and start in your server',
        samplePrompt: 'Read this discussion and turn it into a plan for the week',
        sampleReply: 'I can organize the thread into owners, deadlines, and a reply-ready summary',
        accent: '#6c78ff',
        stage: {
          server: 'study-lab',
          onlineLabel: '18 online',
          channels: ['announcements', 'planning-room', 'resources'],
          sections: [
            {
              title: 'TEXT CHANNELS',
              items: [
                { label: 'announcements' },
                { label: 'planning-room', active: true },
                { label: 'resources' }
              ]
            }
          ],
          room: '#planning-room',
          roomTopic: 'Weekly reading coordination',
          roomMeta: '2 active threads',
          messages: [
            {
              author: 'Lena',
              meta: 'Moderator',
              time: 'Today at 6:12 PM',
              accent: '#f2b4ff',
              text: 'We should split the readings and make the check-in deadline obvious.'
            },
            {
              author: 'Marco',
              meta: 'Member',
              time: 'Today at 6:15 PM',
              accent: '#8ec5ff',
              text: 'Let us pin one summary so new people stop asking the same thing.'
            },
            {
              author: 'You',
              meta: 'Prompt Oliver',
              time: 'Today at 6:18 PM',
              accent: '#9aa4ff',
              text: 'Turn this into a plan for the week.',
              tone: 'user'
            }
          ],
          planLabel: 'Oliver bot',
          planTitle: 'Weekly plan',
          planItems: [
            'Mon: Post the reading queue and due dates',
            'Wed: Collect open questions in one thread',
            'Fri: Pin the recap and next steps'
          ],
          planActions: ['Pin plan', 'Open room'],
          botLabel: 'Oliver bot',
          botTitle: 'Weekly plan',
          botDescription: 'Here is a clean plan based on the thread.',
          botFields: [
            { label: 'Mon', value: 'Post the reading queue and due dates' },
            { label: 'Wed', value: 'Collect open questions in one thread' },
            { label: 'Fri', value: 'Pin the recap and next steps' }
          ],
          composerValue: '/oliver turn this into a plan',
          composerHint: 'Press enter to send',
          membersTitle: 'Online now',
          members: [
            { name: 'Oliver', role: 'Bot', accent: true },
            { name: 'Lena', role: 'Moderator' },
            { name: 'Marco', role: 'Member' }
          ]
        }
      },
      {
        key: 'github',
        label: 'GitHub',
        pillLabel: 'Issues',
        anonymousStatus: 'Start with email',
        authenticatedStatus: 'Connect repo access',
        anonymousActionLabel: 'Send request',
        authenticatedActionLabel: 'Connect',
        description: 'Start with repo context, connect later if needed',
        samplePrompt: 'Review these issues and tell me what should be fixed first',
        sampleReply: 'I can rank the work, flag blockers, and draft the next follow-up',
        accent: '#2c2c2e',
        stage: {
          repo: 'dowhiz/website',
          repoMeta: 'main branch',
          tabs: ['Code', 'Issues', 'Pull requests', 'Actions'],
          filters: ['is:open', 'label:landing', 'sort:updated-desc'],
          overview: { open: '18 Open', closed: '128 Closed' },
          issues: [
            {
              id: '#184',
              title: 'Hero states still look too similar',
              meta: 'shayne opened 2h ago',
              status: 'Open',
              labels: ['landing', 'design'],
              comments: '12',
              active: true
            },
            {
              id: '#181',
              title: 'Keep no-login CTA behavior intact',
              meta: 'james updated 4h ago',
              status: 'Open',
              labels: ['growth', 'routing'],
              comments: '5'
            },
            {
              id: '#177',
              title: 'Tighten mobile hero spacing',
              meta: 'yegaoyang updated yesterday',
              status: 'Open',
              labels: ['ui'],
              comments: '3'
            }
          ],
          detailLabel: 'Selected issue',
          detailTitle: 'Hero states still look too similar',
          detailSummary: 'Users still read the hero as one repeated panel with accent swaps instead of six distinct product surfaces.',
          detailMeta: ['landing', 'design system', 'priority: high'],
          detailItems: [
            'Ship distinct per-channel layouts',
            'Preserve public Slack and Discord entry flows',
            'Treat mobile readability as a release blocker'
          ],
          detailChecklist: [
            { title: 'Ship distinct per-channel layouts', meta: 'In progress', state: 'progress' },
            { title: 'Keep Slack and Discord public entry flows', meta: 'Blocked by regressions', state: 'todo' },
            { title: 'Treat mobile as release-critical', meta: 'Needs visual QA', state: 'todo' }
          ],
          detailCommentTitle: 'Oliver triage note',
          detailComment:
            'Ship channel-native structures first. Keep click behavior truthful. Treat smaller screens as part of the release, not a follow-up.',
          detailActivity: [
            {
              actor: 'Oliver',
              text: 'Split the issue into layout, CTA truthfulness, and mobile follow-up.',
              meta: 'commented 4m ago'
            },
            {
              actor: 'shayne',
              text: 'Marked the hero as release-blocking until the channel states stop looking reused.',
              meta: 'updated 58m ago'
            }
          ],
          detailFooter: 'Ready to post as an issue comment'
        }
      },
      {
        key: 'notion',
        label: 'Notion',
        pillLabel: 'Page',
        anonymousStatus: 'Start with email',
        authenticatedStatus: 'Connect docs access',
        anonymousActionLabel: 'Send request',
        authenticatedActionLabel: 'Connect',
        description: 'Start with docs or notes work, connect later if helpful',
        samplePrompt: 'Turn these notes into a clean study outline',
        sampleReply: 'I can organize the draft, tighten the structure, and flag what still needs research',
        accent: '#6f6b63',
        stage: {
          breadcrumb: 'Personal / Study / Draft',
          collaborators: ['You', 'Oliver', 'TA'],
          pageIcon: 'N',
          pageTitle: 'Oliver study outline',
          pageIntro: 'Modern China midterm review',
          properties: [
            { label: 'Course', value: 'HIST 242' },
            { label: 'Status', value: 'Draft' },
            { label: 'Owner', value: 'You + Oliver' }
          ],
          blocks: [
            { type: 'heading', text: 'Core questions' },
            { type: 'bullet', text: 'What changed after the reform era' },
            { type: 'bullet', text: 'How the three assigned readings compare' },
            { type: 'todo', text: 'Add one source for rural policy' },
            { type: 'callout', text: 'Oliver reordered the notes into a better study path' }
          ],
          databaseLabel: 'Next up',
          databaseTabs: ['Table', 'Board'],
          databaseColumns: ['Task', 'When', 'Status'],
          rows: [
            { name: 'Lecture notes cleanup', meta: 'Tonight', status: 'In progress' },
            { name: 'Open questions', meta: '3 gaps', status: 'Queued' },
            { name: 'Revision pass', meta: 'Tomorrow', status: 'Next' }
          ],
          sideLabel: 'Oliver organized',
          sideTitle: 'From notes to outline',
          sideItems: ['Cleaner sections', 'A clearer reading order', 'What still needs research'],
          sideFootnote: 'Ready to paste into your page'
        }
      },
      {
        key: 'lark',
        label: 'Lark',
        pillLabel: 'Follow-up',
        anonymousStatus: 'Start with email',
        authenticatedStatus: 'Connect workspace',
        anonymousActionLabel: 'Send request',
        authenticatedActionLabel: 'Connect',
        description: 'Start ops follow-up now, connect later if needed',
        samplePrompt: 'Draft a follow-up plan from this meeting summary',
        sampleReply: 'I can turn it into owners, deadlines, and an update you can send',
        accent: '#3f88ff',
        stage: {
          workspace: 'Growth sync',
          workspaceMeta: '6 participants',
          chatTitle: 'Growth sync',
          chatMeta: '6 participants',
          tabs: ['Chat', 'Docs', 'Meetings'],
          participants: ['Nina', 'Sam', 'Oliver'],
          recapLabel: 'Meeting recap',
          recapTitle: 'Vendor shortlist + leadership update',
          recapText: 'Need a vendor shortlist, a sendable leadership update, and one clear follow-up owner list.',
          messages: [
            {
              author: 'Nina',
              meta: 'PM',
              time: '10:02',
              text: 'We have the meeting notes, but not the final owners yet.'
            },
            {
              author: 'Sam',
              meta: 'Ops',
              time: '10:07',
              badge: 'Needs send',
              text: 'We also need a clean update card before tomorrow morning.'
            }
          ],
          trackerLabel: 'Oliver follow-up',
          trackerTitle: 'Owners and timing',
          ownerColumns: ['Owner', 'Task', 'Due'],
          owners: [
            { owner: 'Nina', task: 'Vendor shortlist', due: 'Thu', status: 'In progress' },
            { owner: 'Sam', task: 'Leadership update', due: 'Fri 9 AM', status: 'Queued' },
            { owner: 'Oliver', task: 'Draft sendable recap', due: 'Now', status: 'Ready' }
          ],
          updateLabel: 'Sendable update',
          updateText:
            'Vendor shortlist ships Thursday. Leadership update goes out Friday morning with owners attached.',
          updateActions: ['Share to chat', 'Open checklist']
        }
      }
    ]
  },
  demo: {
    eyebrow: 'Examples',
    title: 'See Oliver in action',
    intro: 'One full demo and three quick examples.',
    desktopTitle: 'Desktop walkthrough',
    desktopDescription: 'A full request-to-result walkthrough.',
    desktopVideoId: 'IsSOTSYIIIY',
    desktopVideoHref: 'https://youtu.be/IsSOTSYIIIY',
    desktopCta: 'Open on YouTube',
    shortsTitle: 'Mobile quick looks',
    shortsDescription: 'Quick mobile demos.',
    shorts: [
      {
        title: 'Mobile demo 01',
        videoId: 'SI9mxW_Top0',
        href: 'https://www.youtube.com/shorts/SI9mxW_Top0'
      },
      {
        title: 'Mobile demo 02',
        videoId: 'PSsJ7WBk71w',
        href: 'https://www.youtube.com/shorts/PSsJ7WBk71w'
      },
      {
        title: 'Mobile demo 03',
        videoId: '5H9g3LOGkMc',
        href: 'https://www.youtube.com/shorts/5H9g3LOGkMc'
      }
    ]
  },
  examples: {
    eyebrow: 'What you can ask',
    title: 'Start with real work',
    intro: 'Real, bounded requests work best.',
    cards: [
      {
        tag: 'Research',
        title: 'Deep research briefs',
        description: 'Compare options and return a concise decision memo.',
        href: '/agents/oliver/',
        ctaLabel: 'Meet Oliver'
      },
      {
        tag: 'Inbox',
        title: 'Email triage and drafts',
        description: 'Draft replies and keep follow-ups moving.',
        href: '/solutions/email-task-automation/',
        ctaLabel: 'Explore email workflows'
      },
      {
        tag: 'Writing',
        title: 'Study and drafting support',
        description: 'Outline, revise, and tighten documents.',
        href: '/solutions/google-docs-automation/',
        ctaLabel: 'Open docs workflow'
      },
      {
        tag: 'Tax prep',
        title: 'Document organization',
        description: 'Sort filing materials and flag what is missing.',
        href: '/help-center/',
        ctaLabel: 'Read the FAQ'
      },
      {
        tag: 'Content',
        title: 'Posts with approval',
        description: 'Draft and schedule posts for review.',
        href: '/agents/rachel/',
        ctaLabel: 'Meet Rachel'
      },
      {
        tag: 'GitHub',
        title: 'Repo follow-up',
        description: 'Summarize issues and keep code-adjacent work moving.',
        href: '/solutions/github-issue-automation/',
        ctaLabel: 'See GitHub workflow'
      }
    ]
  },
  faqItems: [
    {
      question: 'What is the fastest way to start?',
      answer:
        'Email is still the fastest no-login start. Slack and Discord also have direct add flows, and GitHub, Notion, or Lark work can start with a first request before you connect anything.'
    },
    {
      question: 'Do I need an account before the first request?',
      answer:
        'No. You can start with email, Slack, or Discord without creating an account first. GitHub, Notion, and Lark work can start with a request first and be linked later when deeper access helps.'
    },
    {
      question: 'Can I use Slack or Discord immediately?',
      answer:
        'Yes. Slack and Discord open public install or invite flows today. GitHub, Notion, and Lark still start with a request first, then connect later when you want saved app access.'
    },
    {
      question: 'What kinds of first requests work best?',
      answer:
        'Research, drafting, inbox follow-up, document organization, repo-side coordination, and posts that still wait for your approval are all good first uses.'
    },
    {
      question: 'Do I stay in control?',
      answer:
        'Yes. You decide which apps Oliver can use, when setup is worth saving, and which sensitive actions should stay reviewable.'
    }
  ],
  labels: {
    faqEyebrow: 'Questions',
    faqTitle: 'Before you start',
    faqIntro: 'Short answers before you begin.',
    faqLinkLabel: 'Open the Help Center',
    footerTitle: 'Resources',
    footerTagline: 'Oliver helps turn requests into finished work.',
    footerPill: 'Start with the request. Expand later.',
    footerBottomSecondary: 'Use first. Connect more only when it helps.'
  },
  footerLinks: [
    { href: '/privacy/', label: 'Privacy' },
    { href: '/terms/', label: 'Terms of Service' },
    { href: '/trust-safety/', label: 'Trust & Safety' },
    { href: '/integrations/', label: 'Integrations' },
    { href: '/blog/', label: 'Blog' },
    { href: 'https://www.dowhiz.com/help-center/', label: 'Help Center' },
    { href: '/user-guide/', label: 'User Guide' }
  ]
};

const ZH_LANDING_CONTENT = {
  metadata: {
    title: 'DoWhiz 中文 | Email、Slack、GitHub 里的 AI 数字员工',
    description:
      '先从 Email、Slack 或 Discord 里的 Oliver 开始。DoWhiz 会把真实请求推进到 GitHub、文档和更多协作工具里，直接返回完成结果。',
    canonicalUrl: 'https://dowhiz.com/',
    ogLocale: 'zh_CN',
    themeColor: '#2C2C2E',
    htmlLang: 'zh-CN',
    robots: 'noindex, follow'
  },
  nav: {
    homePath: '/cn',
    links: [
      { href: '#channels', label: '现在开始' },
      { href: '#examples', label: 'Oliver 能做什么' },
      { href: '#faq', label: '常见问题' }
    ],
    signIn: '管理 setup',
    dashboard: '你的 Oliver',
    signOut: '退出登录',
    githubAriaLabel: 'GitHub',
    discordAriaLabel: 'Discord',
    contactAriaLabel: '给 Oliver 发邮件'
  },
  hero: {
    eyebrow: '先从一个真实请求开始',
    title: '在你常用的渠道里用 Oliver',
    subtitle:
      '先在 Email、Slack 或 Discord 里发出一个真实请求。只有当更深的上下文真的能提升结果时，再连接 GitHub、Notion 或 Lark。',
    primaryCta: '先发一个请求',
    secondaryCta: '看演示',
    manageAnonymous: '管理 setup',
    manageAuthenticated: '打开你的 setup',
    contactSubject: '给 Oliver 的第一个任务',
    contactBody:
      '你好 Oliver，\n\n这是我想先让你帮忙处理的第一个任务：\n\n- 背景：\n- 什么算完成：\n- 截止时间：\n\n谢谢！',
    toolsEyebrow: '选择一个渠道',
    toolsHintAnonymous:
      '悬停会暂停轮播，点击就直接开始。GitHub、Notion 和 Lark 可以先从邮件请求开始。',
    toolsHintAuthenticated: '悬停会暂停轮播。登录后，这些入口也可以在需要时直接启动连接流程。',
    toolsFootnote:
      '先在一个渠道里跑通第一轮。只有当它已经有用时，再保存 memory 或接入更深权限。',
    autoplayLabel: '轮播预览',
    pausedLabel: '预览已暂停',
    previewLabel: '实时预览',
    primaryChannelsLabel: '直接开始',
    secondaryChannelsLabel: '需要时再深度连接',
    showcaseSummary:
      'Email、Slack 和 Discord 是最快开始的方式。GitHub、Notion 和 Lark 会在更深上下文真的能提升结果时再进入。',
    proofPills: ['一个清楚的请求', '结果可 review', '只有有帮助时才扩展 setup'],
    entryEyebrow: '最快开始方式',
    previewEyebrow: '交互会是什么样',
    youLabel: '你',
    actionLabels: {
      connect: '连接',
      loading: '打开中...'
    },
    tools: [
      {
        key: 'email',
        label: 'Email',
        pillLabel: '写邮件',
        anonymousStatus: '直接写邮件',
        authenticatedStatus: '直接写邮件',
        anonymousActionLabel: '发送请求',
        authenticatedActionLabel: '发送请求',
        description: '把你已经想好的请求直接发出去',
        samplePrompt: '把这些要点整理成一封简洁的跟进邮件',
        sampleReply: '我可以先起草回复、收紧语气，并指出还缺什么信息',
        accent: '#ff8a3d',
        stage: {
          appName: '收件箱',
          appMeta: '和 Oliver 一起起草',
          folders: [
            { label: '收件箱', count: '12' },
            { label: '草稿箱', count: '3', active: true },
            { label: '已发送', count: '' },
            { label: '归档', count: '' }
          ],
          threads: [
            {
              from: 'Sage Capital',
              subject: '周五跟进',
              preview: '今天沟通很顺利，能否在周五前发一份 recap？',
              time: '2分',
              active: true
            },
            {
              from: '课程助教',
              subject: '期中 office hours',
              preview: '下周二新开放了一批时间段。',
              time: '1小时'
            },
            {
              from: 'Mina',
              subject: 'Launch recap',
              preview: '我把最新 checklist 放到 ops folder 里了。',
              time: '3小时'
            }
          ],
          composeTitle: '起草请求',
          draftBadge: '回复',
          toLabel: '收件人',
          toValue: 'oliver@dowhiz.com',
          ccLabel: '目标对象',
          ccValue: 'sponsor@sagecap.com',
          subjectLabel: '请求内容',
          subjectValue: '帮我起草一封更自然的赞助方跟进邮件',
          tags: ['语气温和', '提到周五 recap'],
          bodyLines: [
            '你好 Oliver，',
            '请帮我起草一封更简洁的赞助方回复。',
            '- 感谢对方今天接待',
            '- 提到周五会发 recap',
            '- 语气保持轻一点'
          ],
          footerNote: '发送前 Oliver 还可以帮你继续收紧语气',
          footerValue: '发送请求',
          resultLabel: 'Oliver 备注',
          resultTitle: '回复草稿已准备好',
          resultItems: ['更清楚的标题', '一版可直接发送的三段式草稿', '还缺哪条信息的提醒']
        }
      },
      {
        key: 'slack',
        label: 'Slack',
        pillLabel: '线程',
        anonymousStatus: '公开安装',
        authenticatedStatus: '连接工作区',
        anonymousActionLabel: '添加机器人',
        authenticatedActionLabel: '连接',
        description: '装好 Oliver 后直接在 Slack 里开始',
        samplePrompt: '帮我总结这个频道，并列出接下来三件事',
        sampleReply: '我可以把线程上下文整理成清晰的行动清单',
        accent: '#36c58b',
        stage: {
          workspace: 'product-ops',
          workspaceMeta: '8 位同事在线',
          channels: ['#launch-ops', '#customer-handoffs', '#weekly-plan'],
          sections: [
            {
              title: '频道',
              items: [
                { label: '#launch-ops', active: true },
                { label: '#customer-handoffs' },
                { label: '#weekly-plan' }
              ]
            },
            {
              title: '私信',
              items: [
                { label: 'Oliver', accent: 'bot' },
                { label: 'Mina' }
              ]
            }
          ],
          room: '#launch-ops',
          roomMeta: '今天 23 条消息',
          roomMembers: ['Mina', 'Theo', 'Oliver'],
          threadPills: ['launch', 'handoff', '缺 owner'],
          threadActivity: '线程里已有 3 条回复',
          messages: [
            {
              author: 'Mina',
              meta: '运营',
              time: '10:14',
              text: '文档、QA 和上线邮件还缺一个明确 owner。',
              reactions: ['围观 2', '确认 1']
            },
            {
              author: 'Theo',
              meta: '设计',
              time: '10:18',
              text: '素材已经好了，但 rollout 顺序还没有写清楚。',
              reactions: ['记录 1']
            },
            {
              author: '你',
              meta: '问 Oliver',
              time: '10:22',
              text: '帮我总结这个线程，并列出接下来三件事。',
              tone: 'user'
            }
          ],
          threadLabel: '线程总结',
          threadTitle: '上线 blocker',
          threadText: '文档 owner 还没定，QA 顺序没有锁住，上线邮件也还缺一个干净的 checklist。',
          cardLabel: 'Oliver Block Kit',
          cardTitle: '可以直接发回频道',
          cardSummary: '一段清楚的更新 + 三个动作',
          cardItems: ['下午 3 点前确认文档 owner', '锁定 QA sign-off 顺序', '把 checklist 发回频道'],
          cardFooter: '一段清楚的更新 + 三个动作',
          cardActions: ['发 summary', '打开 thread'],
          composerPlaceholder: '回复到 #launch-ops',
          composerHint: '输入消息'
        }
      },
      {
        key: 'discord',
        label: 'Discord',
        pillLabel: '服务器',
        anonymousStatus: '公开邀请',
        authenticatedStatus: '连接服务器',
        anonymousActionLabel: '添加机器人',
        authenticatedActionLabel: '连接',
        description: '邀请 Oliver 后直接在 server 里开始',
        samplePrompt: '读一下这段讨论，并把它整理成本周执行计划',
        sampleReply: '我可以把讨论整理成负责人、截止时间和可直接发送的总结',
        accent: '#6c78ff',
        stage: {
          server: 'study-lab',
          onlineLabel: '18 人在线',
          channels: ['announcements', 'planning-room', 'resources'],
          sections: [
            {
              title: '文字频道',
              items: [
                { label: 'announcements' },
                { label: 'planning-room', active: true },
                { label: 'resources' }
              ]
            }
          ],
          room: '#planning-room',
          roomTopic: '本周阅读安排',
          roomMeta: '2 个活跃 thread',
          messages: [
            {
              author: 'Lena',
              meta: '管理员',
              time: '今天 18:12',
              accent: '#f2b4ff',
              text: '我们应该把阅读任务拆开，也把 check-in 的时间说得更明确。'
            },
            {
              author: 'Marco',
              meta: '成员',
              time: '今天 18:15',
              accent: '#8ec5ff',
              text: '还需要一条置顶总结，不然新人会一直重复提问。'
            },
            {
              author: '你',
              meta: '问 Oliver',
              time: '今天 18:18',
              accent: '#9aa4ff',
              text: '把这段讨论整理成本周计划。',
              tone: 'user'
            }
          ],
          planLabel: 'Oliver Bot',
          planTitle: '本周计划',
          planItems: [
            '周一：发阅读清单和截止时间',
            '周三：把开放问题收集到同一个 thread',
            '周五：置顶 recap 和下一步'
          ],
          planActions: ['置顶计划', '打开频道'],
          botLabel: 'Oliver Bot',
          botTitle: '本周计划',
          botDescription: '我根据这段讨论整理了一份更清楚的节奏。',
          botFields: [
            { label: '周一', value: '发阅读清单和截止时间' },
            { label: '周三', value: '把开放问题收集到同一个 thread' },
            { label: '周五', value: '置顶 recap 和下一步' }
          ],
          composerValue: '/oliver 把这段整理成本周计划',
          composerHint: '回车发送',
          membersTitle: '在线成员',
          members: [
            { name: 'Oliver', role: 'Bot', accent: true },
            { name: 'Lena', role: '管理员' },
            { name: 'Marco', role: '成员' }
          ]
        }
      },
      {
        key: 'github',
        label: 'GitHub',
        pillLabel: 'Issue',
        anonymousStatus: '先发邮件开始',
        authenticatedStatus: '连接仓库权限',
        anonymousActionLabel: '发送请求',
        authenticatedActionLabel: '连接',
        description: '先带着 repo 上下文开始，需要时再连接',
        samplePrompt: '看看这些 issues，告诉我应该先修哪几个',
        sampleReply: '我可以帮你排优先级、指出 blocker，并起草下一步跟进',
        accent: '#2c2c2e',
        stage: {
          repo: 'dowhiz/website',
          repoMeta: 'main 分支',
          tabs: ['Code', 'Issues', 'Pull requests', 'Actions'],
          filters: ['is:open', 'label:landing', 'sort:updated-desc'],
          overview: { open: '18 个 Open', closed: '128 个 Closed' },
          issues: [
            {
              id: '#184',
              title: 'Hero 状态看起来还是太像了',
              meta: 'shayne 2 小时前创建',
              status: 'Open',
              labels: ['landing', 'design'],
              comments: '12',
              active: true
            },
            {
              id: '#181',
              title: '保留无登录 CTA 行为',
              meta: 'james 4 小时前更新',
              status: 'Open',
              labels: ['growth', 'routing'],
              comments: '5'
            },
            {
              id: '#177',
              title: '继续收紧移动端 hero 间距',
              meta: 'yegaoyang 昨天更新',
              status: 'Open',
              labels: ['ui'],
              comments: '3'
            }
          ],
          detailLabel: '当前 issue',
          detailTitle: 'Hero 状态看起来还是太像了',
          detailSummary: '用户仍然会把 hero 读成“同一个面板只换颜色”，而不是六种不同的软件表面。',
          detailMeta: ['landing', 'design system', 'priority: high'],
          detailItems: ['先做每个 channel 独立布局', '保留 Slack 和 Discord 的直接入口', '把移动端可读性当成上线门槛'],
          detailChecklist: [
            { title: '先做每个 channel 独立布局', meta: '进行中', state: 'progress' },
            { title: '保留 Slack 和 Discord 公开入口', meta: '避免回退', state: 'todo' },
            { title: '把移动端当作上线门槛', meta: '还需要视觉检查', state: 'todo' }
          ],
          detailCommentTitle: 'Oliver triage note',
          detailComment:
            '先做 channel-native 的结构，再继续打磨局部细节。点击路径必须保持真实，移动端不能当作后续优化。',
          detailActivity: [
            {
              actor: 'Oliver',
              text: '已拆成布局、入口真实性、移动端三个跟进方向。',
              meta: '4 分钟前评论'
            },
            {
              actor: 'shayne',
              text: '在 channel state 停止复用同一骨架前，hero 仍视作 release blocker。',
              meta: '58 分钟前更新'
            }
          ],
          detailFooter: '可以直接作为 issue comment'
        }
      },
      {
        key: 'notion',
        label: 'Notion',
        pillLabel: '页面',
        anonymousStatus: '先发邮件开始',
        authenticatedStatus: '连接文档权限',
        anonymousActionLabel: '发送请求',
        authenticatedActionLabel: '连接',
        description: '先从文档或笔记任务开始，有需要再连接',
        samplePrompt: '把这些笔记整理成一份清楚的学习提纲',
        sampleReply: '我可以先整理结构、收紧逻辑，并指出还要补哪些研究',
        accent: '#6f6b63',
        stage: {
          breadcrumb: 'Personal / Study / Draft',
          collaborators: ['你', 'Oliver', '助教'],
          pageIcon: 'N',
          pageTitle: 'Oliver 学习提纲',
          pageIntro: '中国近现代史期中复习',
          properties: [
            { label: '课程', value: 'HIST 242' },
            { label: '状态', value: 'Draft' },
            { label: '协作', value: '你 + Oliver' }
          ],
          blocks: [
            { type: 'heading', text: '核心问题' },
            { type: 'bullet', text: '改革开放后最关键的变化是什么' },
            { type: 'bullet', text: '三篇阅读材料该怎么对照' },
            { type: 'todo', text: '补一条关于农村政策的来源' },
            { type: 'callout', text: 'Oliver 已经把原始笔记重新整理成更适合复习的顺序' }
          ],
          databaseLabel: '下一步',
          databaseTabs: ['Table', 'Board'],
          databaseColumns: ['任务', '时间', '状态'],
          rows: [
            { name: '整理 lecture notes', meta: '今晚', status: '进行中' },
            { name: '补开放问题', meta: '3 个缺口', status: '待开始' },
            { name: '最后 revision', meta: '明天', status: '下一步' }
          ],
          sideLabel: 'Oliver 已整理',
          sideTitle: '从笔记到提纲',
          sideItems: ['章节更清楚', '阅读顺序更明确', '还缺什么研究一眼可见'],
          sideFootnote: '可以直接贴回你的页面'
        }
      },
      {
        key: 'lark',
        label: 'Lark',
        pillLabel: '跟进卡',
        anonymousStatus: '先发邮件开始',
        authenticatedStatus: '连接工作区',
        anonymousActionLabel: '发送请求',
        authenticatedActionLabel: '连接',
        description: '先从协作跟进任务开始，需要时再连接',
        samplePrompt: '根据这次会议总结，起草一个后续推进计划',
        sampleReply: '我可以把它整理成负责人、时间点和一段可直接发送的更新',
        accent: '#3f88ff',
        stage: {
          workspace: 'Growth sync',
          workspaceMeta: '6 位参与者',
          chatTitle: 'Growth sync',
          chatMeta: '6 位参与者',
          tabs: ['Chat', 'Docs', 'Meetings'],
          participants: ['Nina', 'Sam', 'Oliver'],
          recapLabel: '会议 recap',
          recapTitle: 'Vendor shortlist + 领导更新',
          recapText: '需要一份 vendor shortlist、一条可直接发送的领导更新，以及明确的后续 owner 列表。',
          messages: [
            {
              author: 'Nina',
              meta: 'PM',
              time: '10:02',
              text: '会议纪要有了，但最终 owner 还没定下来。'
            },
            {
              author: 'Sam',
              meta: '运营',
              time: '10:07',
              badge: '待发送',
              text: '明天上午前还要有一张干净的更新卡片。'
            }
          ],
          trackerLabel: 'Oliver 跟进',
          trackerTitle: '负责人和时间点',
          ownerColumns: ['负责人', '事项', '时间'],
          owners: [
            { owner: 'Nina', task: 'Vendor shortlist', due: '周四', status: '进行中' },
            { owner: 'Sam', task: '领导更新', due: '周五 9:00', status: '待开始' },
            { owner: 'Oliver', task: '起草可发送 recap', due: '现在', status: '已准备' }
          ],
          updateLabel: '可发送更新',
          updateText: 'Vendor shortlist 周四完成，周五上午发出领导更新，并附上 owner 分工。',
          updateActions: ['分享到群聊', '打开 checklist']
        }
      }
    ]
  },
  demo: {
    eyebrow: '示例',
    title: '看看 Oliver 怎么工作',
    intro: '一个完整演示，外加三个快速示例。',
    desktopTitle: '桌面完整演示',
    desktopDescription: '完整看一遍从请求到结果。',
    desktopVideoId: 'IsSOTSYIIIY',
    desktopVideoHref: 'https://youtu.be/IsSOTSYIIIY',
    desktopCta: '去 YouTube 看',
    shortsTitle: '移动端快速演示',
    shortsDescription: '几个更短的移动端示例。',
    shorts: [
      {
        title: '移动端演示 01',
        videoId: 'SI9mxW_Top0',
        href: 'https://www.youtube.com/shorts/SI9mxW_Top0'
      },
      {
        title: '移动端演示 02',
        videoId: 'PSsJ7WBk71w',
        href: 'https://www.youtube.com/shorts/PSsJ7WBk71w'
      },
      {
        title: '移动端演示 03',
        videoId: '5H9g3LOGkMc',
        href: 'https://www.youtube.com/shorts/5H9g3LOGkMc'
      }
    ]
  },
  examples: {
    eyebrow: '你可以怎么问',
    title: '先从真实任务开始',
    intro: '真实、具体的请求最适合第一次开始。',
    cards: [
      {
        tag: 'Research',
        title: 'Deep research 简报',
        description: '比较方案，最后给你一份能直接判断的结果。',
        href: '/agents/oliver/',
        ctaLabel: '认识 Oliver'
      },
      {
        tag: 'Inbox',
        title: '邮件整理和回复起草',
        description: '起草回复，同时把后续动作继续往前推。',
        href: '/solutions/email-task-automation/',
        ctaLabel: '查看邮件工作流'
      },
      {
        tag: 'Writing',
        title: '学习支持和文稿起草',
        description: '做提纲、改写、收紧结构。',
        href: '/solutions/google-docs-automation/',
        ctaLabel: '打开文档工作流'
      },
      {
        tag: 'Tax prep',
        title: '材料整理',
        description: '整理报税前的文件并指出还缺什么。',
        href: '/help-center/',
        ctaLabel: '查看常见问题'
      },
      {
        tag: 'Content',
        title: '发帖前的草稿与排期',
        description: '先起草和排期，最终仍由你确认。',
        href: '/agents/rachel/',
        ctaLabel: '认识 Rachel'
      },
      {
        tag: 'GitHub',
        title: 'Repo 跟进',
        description: '整理 issue 上下文，推进和代码相关的后续工作。',
        href: '/solutions/github-issue-automation/',
        ctaLabel: '查看 GitHub 工作流'
      }
    ]
  },
  faqItems: [
    {
      question: '最快怎么开始？',
      answer:
        'Email 仍然是最快的无登录入口。Slack 和 Discord 也已经有直接可用的公开安装入口，而 GitHub、Notion、Lark 相关任务也可以先发出第一个请求，再决定要不要连接。'
    },
    {
      question: '第一条消息之前必须先有账号吗？',
      answer:
        '不需要。你可以先通过 Email、Slack 或 Discord 开始，不用先注册账号。GitHub、Notion 和 Lark 相关任务也可以先发请求，之后再在更深权限真的有帮助时完成连接。'
    },
    {
      question: 'Slack 或 Discord 现在能直接开始吗？',
      answer:
        '可以。Slack 和 Discord 现在就会打开公开安装或邀请入口。GitHub、Notion 和 Lark 目前仍然更适合先发出请求，再在你希望保存 app 访问权限时完成连接。'
    },
    {
      question: '第一条消息最适合发什么？',
      answer:
        '研究、起草、收件箱跟进、材料整理、repo 协调，以及需要你最终确认的内容发布，都是很好的开始。'
    },
    {
      question: '我还掌控流程吗？',
      answer:
        '掌控权仍然在你手里。你决定连哪些工具、哪些动作要先审阅，以及 Oliver 什么时候值得保存更多上下文。'
    }
  ],
  labels: {
    faqEyebrow: '常见问题',
    faqTitle: '开始之前',
    faqIntro: '开始前先看这几条短答案。',
    faqLinkLabel: '打开帮助中心',
    footerTitle: '资源',
    footerTagline: 'Oliver 帮你把请求变成结果。',
    footerPill: '先从请求开始，再决定是否扩展。',
    footerBottomSecondary: '先用起来，再在真正有帮助的时候连接更多。'
  },
  footerLinks: [
    { href: '/privacy/', label: '隐私政策' },
    { href: '/terms/', label: '服务条款' },
    { href: '/trust-safety/', label: 'Trust & Safety' },
    { href: '/integrations/', label: '集成' },
    { href: '/blog/', label: '博客' },
    { href: 'https://www.dowhiz.com/help-center/', label: '帮助中心' },
    { href: '/user-guide/', label: '使用指南' }
  ]
};

export function getLandingContent(locale = 'en-US') {
  return locale === 'zh-CN' ? ZH_LANDING_CONTENT : EN_LANDING_CONTENT;
}
