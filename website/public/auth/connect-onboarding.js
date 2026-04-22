export const CONNECT_ONBOARDING_STORAGE_KEY = 'dw_connect_onboarding_pending';
export const CONNECT_ONBOARDING_MAX_AGE_MS = 30 * 60 * 1000;

const PROVIDER_LABELS = {
  discord: 'Discord',
  email: 'Email',
  github: 'GitHub',
  lark: 'Lark',
  notion: 'Notion',
  phone: 'Phone',
  slack: 'Slack',
  telegram: 'Telegram'
};

const BOT_INSTALL_TASK_IDS = {
  discord: 'add-oliver-discord',
  slack: 'add-oliver-slack'
};

function normalizeString(value) {
  return String(value || '').trim();
}

export function normalizeConnectOnboardingProvider(provider) {
  const normalized = normalizeString(provider).toLowerCase();
  return normalized || null;
}

export function connectOnboardingProviderLabel(provider) {
  const normalized = normalizeConnectOnboardingProvider(provider);
  if (!normalized) {
    return 'Connected app';
  }
  return PROVIDER_LABELS[normalized]
    || normalized
      .split(/[_\-\s]+/)
      .filter(Boolean)
      .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
      .join(' ');
}

export function connectOnboardingBotInstallTaskId(provider) {
  const normalized = normalizeConnectOnboardingProvider(provider);
  return normalized ? BOT_INSTALL_TASK_IDS[normalized] || null : null;
}

export function clearConnectOnboardingStateForProvider({
  provider,
  completedAdminTasks = [],
  pendingPayload = null,
  now = Date.now()
} = {}) {
  const normalizedProvider = normalizeConnectOnboardingProvider(provider);
  const nextCompletedTasks = normalizeCompletedAdminTasks(completedAdminTasks);
  const parsedPending = parseStoredConnectOnboarding(pendingPayload, now);

  if (!normalizedProvider) {
    return {
      completedAdminTasks: Array.from(nextCompletedTasks),
      pendingPayload: parsedPending
    };
  }

  const botInstallTaskId = connectOnboardingBotInstallTaskId(normalizedProvider);
  if (botInstallTaskId) {
    nextCompletedTasks.delete(botInstallTaskId);
  }

  return {
    completedAdminTasks: Array.from(nextCompletedTasks),
    pendingPayload: parsedPending?.provider === normalizedProvider ? null : parsedPending
  };
}

export function createConnectOnboardingPayload(provider, options = {}) {
  const normalizedProvider = normalizeConnectOnboardingProvider(provider);
  if (!normalizedProvider) {
    return null;
  }

  const eventType = options.eventType === 'install' ? 'install' : 'connect';
  const source = normalizeString(options.source) || 'oauth';
  const createdAt = Number.isFinite(options.createdAt) ? options.createdAt : Date.now();

  return {
    provider: normalizedProvider,
    eventType,
    source,
    createdAt
  };
}

export function parseStoredConnectOnboarding(rawValue, now = Date.now()) {
  if (!rawValue) {
    return null;
  }

  let parsed = rawValue;
  if (typeof rawValue === 'string') {
    try {
      parsed = JSON.parse(rawValue);
    } catch {
      return null;
    }
  }

  const payload = createConnectOnboardingPayload(parsed?.provider, {
    eventType: parsed?.eventType,
    source: parsed?.source,
    createdAt: Number(parsed?.createdAt)
  });
  if (!payload || !Number.isFinite(payload.createdAt) || payload.createdAt <= 0) {
    return null;
  }

  if (now - payload.createdAt > CONNECT_ONBOARDING_MAX_AGE_MS) {
    return null;
  }

  return payload;
}

function normalizeCompletedAdminTasks(completedAdminTasks = []) {
  return new Set(
    (Array.isArray(completedAdminTasks) ? completedAdminTasks : [])
      .map((taskId) => normalizeString(taskId))
      .filter(Boolean)
  );
}

function hasCompletedBotInstall(provider, completedAdminTasks = []) {
  const taskId = connectOnboardingBotInstallTaskId(provider);
  if (!taskId) {
    return false;
  }
  return normalizeCompletedAdminTasks(completedAdminTasks).has(taskId);
}

function anchorCta(label, href) {
  return {
    kind: 'anchor',
    label,
    href
  };
}

function botInstallCta(provider) {
  const label = provider === 'discord' ? 'Add Oliver to Discord' : 'Add Oliver to Slack';
  return {
    kind: 'bot_install',
    label,
    provider
  };
}

function genericConnectModel(provider) {
  const label = connectOnboardingProviderLabel(provider);
  const lowerLabel = label.toLowerCase();

  const providerExamples = {
    github: [
      'Summarize a repo before you delegate work.',
      'Review a pull request and call out the risky parts.',
      'Turn a set of issues into a short execution plan.'
    ],
    notion: [
      'Summarize a long page into a quick brief.',
      'Turn notes into an action plan with owners and next steps.',
      'Pull decisions from a doc into a clean task list.'
    ],
    lark: [
      'Draft a reply to a teammate in the right tone.',
      'Summarize a discussion and list the next actions.',
      'Turn a request into a task you can review in DoWhiz.'
    ],
    email: [
      'Draft a concise reply before you send it.',
      'Summarize a long thread into the key decisions.',
      'Turn an email request into a checklist with next steps.'
    ],
    phone: [
      'Keep one contact route ready for quick confirmations.',
      'Capture a short request and turn it into a task.',
      'Use the next task review to confirm the result is correct.'
    ],
    telegram: [
      'Ask for a quick summary when something changes fast.',
      'Draft a short response without losing context.',
      'Turn a chat request into a task you can review later.'
    ]
  };

  return {
    provider,
    eyebrow: 'Connection ready',
    title: `${label} connected`,
    description: `${label} is linked to your account. Keep the first ask small and concrete so you can review the result quickly.`,
    examples: providerExamples[provider] || [
      `Start with one small request in ${lowerLabel}.`,
      'Review the result in Work before you expand the workflow.',
      'Save any tone or approval rules in Memory once the first task lands.'
    ],
    cta: anchorCta('Open Work', '#section-work'),
    footnote: 'Aim for a first win that takes less than five minutes to review.'
  };
}

export function buildConnectOnboardingModel({
  provider,
  eventType = 'connect',
  completedAdminTasks = []
} = {}) {
  const normalizedProvider = normalizeConnectOnboardingProvider(provider);
  if (!normalizedProvider) {
    return null;
  }

  const installComplete = hasCompletedBotInstall(normalizedProvider, completedAdminTasks);

  if (normalizedProvider === 'slack') {
    if (eventType === 'install' || installComplete) {
      return {
        provider: normalizedProvider,
        eyebrow: 'Oliver is live',
        title: 'Oliver is now available in Slack',
        description: 'The workspace install is complete. Start with one small ask so the team can see how Oliver works without adding noise.',
        examples: [
          '@Oliver summarize this thread and list the next steps.',
          '@Oliver draft a crisp reply I can send from here.',
          '@Oliver turn this request into a task I can review in DoWhiz.'
        ],
        cta: anchorCta('Open Work', '#section-work'),
        footnote: 'If you want a tighter tone or approval rules, save them in Memory before the next request.'
      };
    }

    return {
      provider: normalizedProvider,
      eyebrow: 'Connection ready',
      title: 'Slack connected',
      description: 'Your Slack identity is linked. Add Oliver to the workspace so teammates can talk with the bot in one channel and keep follow-through in Slack.',
      examples: [
        'Introduce Oliver in one safe channel after install.',
        'Answer an @Oliver question in-channel with a clear next step.',
        'Turn a Slack request into a task you can review in DoWhiz.'
      ],
      cta: botInstallCta('slack'),
      footnote: 'Connecting your account does not install the Slack bot by itself.'
    };
  }

  if (normalizedProvider === 'discord') {
    if (eventType === 'install' || installComplete) {
      return {
        provider: normalizedProvider,
        eyebrow: 'Oliver is live',
        title: 'Oliver is now available in Discord',
        description: 'The server install is complete. Start with one small ask so the first interaction feels useful, not loud.',
        examples: [
          '@Oliver summarize this discussion and suggest the next actions.',
          '@Oliver draft a reply I can post back in this channel.',
          '@Oliver turn this request into a task I can review in DoWhiz.'
        ],
        cta: anchorCta('Open Work', '#section-work'),
        footnote: 'If you want Oliver to follow team tone or approval rules, save them in Memory now.'
      };
    }

    return {
      provider: normalizedProvider,
      eyebrow: 'Connection ready',
      title: 'Discord connected',
      description: 'Your Discord identity is linked. Add Oliver to the server so people can talk with the bot in one safe text channel.',
      examples: [
        'Introduce Oliver in one public text channel after install.',
        'Answer an @Oliver question without leaving the thread of work.',
        'Turn a Discord request into a task you can review in DoWhiz.'
      ],
      cta: botInstallCta('discord'),
      footnote: 'Connecting your account does not install the Discord bot by itself.'
    };
  }

  return genericConnectModel(normalizedProvider);
}

export function chooseConnectOnboardingPayload({
  pendingPayload = null,
  connectedTypes = [],
  completedAdminTasks = []
} = {}) {
  const normalizedConnectedTypes = new Set(
    (Array.isArray(connectedTypes) ? connectedTypes : [])
      .map((provider) => normalizeConnectOnboardingProvider(provider))
      .filter(Boolean)
  );

  const parsedPending = parseStoredConnectOnboarding(pendingPayload);
  if (
    parsedPending
    && (
      parsedPending.eventType === 'install'
      || normalizedConnectedTypes.has(parsedPending.provider)
    )
  ) {
    return parsedPending;
  }

  const completedTasks = normalizeCompletedAdminTasks(completedAdminTasks);
  for (const provider of ['slack', 'discord']) {
    const taskId = BOT_INSTALL_TASK_IDS[provider];
    if (normalizedConnectedTypes.has(provider) && taskId && !completedTasks.has(taskId)) {
      return createConnectOnboardingPayload(provider, {
        eventType: 'connect',
        source: 'pending_bot_install'
      });
    }
  }

  return null;
}

export function filterOptionalExtensionsForNextSteps(adminTasks = [], connectedTypes = []) {
  const normalizedConnectedTypes = new Set(
    (Array.isArray(connectedTypes) ? connectedTypes : [])
      .map((provider) => normalizeConnectOnboardingProvider(provider))
      .filter(Boolean)
  );

  return (Array.isArray(adminTasks) ? adminTasks : []).filter((task) => {
    const channelKey = normalizeConnectOnboardingProvider(task?.channelKey);
    if (!channelKey || !normalizedConnectedTypes.has(channelKey)) {
      return false;
    }
    return task?.nextStepsEligible !== false;
  });
}
