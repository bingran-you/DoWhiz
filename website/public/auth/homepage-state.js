const DEFAULT_PROVIDER_ORDER = ['notion', 'slack', 'github', 'discord', 'lark', 'wechat'];

const PROVIDER_LABELS = {
  discord: 'Discord',
  github: 'GitHub',
  lark: 'Lark',
  notion: 'Notion',
  slack: 'Slack',
  wechat: 'WeCom'
};

function normalizeString(value) {
  return String(value || '').trim();
}

function normalizeProvider(provider) {
  return normalizeString(provider).toLowerCase();
}

function uniqueProviders(providers = []) {
  return Array.from(
    new Set(
      (Array.isArray(providers) ? providers : [])
        .map((provider) => normalizeProvider(provider))
        .filter(Boolean)
    )
  );
}

export function providerLabel(provider) {
  const normalized = normalizeProvider(provider);
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

export function summarizeProviders(providers = []) {
  const labels = uniqueProviders(providers).map((provider) => providerLabel(provider));
  if (labels.length === 0) {
    return 'no connected apps yet';
  }
  if (labels.length === 1) {
    return labels[0];
  }
  if (labels.length === 2) {
    return `${labels[0]} and ${labels[1]}`;
  }
  return `${labels.slice(0, -1).join(', ')}, and ${labels[labels.length - 1]}`;
}

export function buildPreferredHomepageProviderOrder(preferredChannels = [], baseOrder = DEFAULT_PROVIDER_ORDER) {
  const normalizedBaseOrder = uniqueProviders(baseOrder);
  const preferredOrder = [];

  (Array.isArray(preferredChannels) ? preferredChannels : []).forEach((channel) => {
    const normalizedChannel = normalizeProvider(channel);
    const matchedProvider = normalizedBaseOrder.find((provider) => normalizedChannel.includes(provider));
    if (matchedProvider && !preferredOrder.includes(matchedProvider)) {
      preferredOrder.push(matchedProvider);
    }
  });

  return preferredOrder.concat(
    normalizedBaseOrder.filter((provider) => !preferredOrder.includes(provider))
  );
}

function countInProgressTasks(taskCount, reviewableTaskCount) {
  return Math.max(Number(taskCount || 0) - Number(reviewableTaskCount || 0), 0);
}

function buildPrimaryAction(section, label) {
  return {
    kind: 'open_dashboard_section',
    section,
    label
  };
}

function stateLabel(stateKey) {
  switch (stateKey) {
    case 'no_connected_app':
      return 'Needs connection';
    case 'connected_no_tasks':
      return 'Needs first request';
    case 'tasks_in_progress':
      return 'Work in progress';
    case 'review_ready':
      return 'Review ready';
    case 'ongoing_usage':
      return 'Ongoing usage';
    default:
      return 'In progress';
  }
}

export function buildDashboardHomepageModel({
  connectedTypes = [],
  taskCount = 0,
  successfulTaskCount = 0,
  reviewableTaskCount = 0,
  hasSavedMemory = false,
  preferredChannels = []
} = {}) {
  const normalizedConnectedTypes = uniqueProviders(connectedTypes);
  const nextSuggestedProvider = buildPreferredHomepageProviderOrder(preferredChannels)
    .find((provider) => !normalizedConnectedTypes.includes(provider)) || null;
  const normalizedTaskCount = Number(taskCount || 0);
  const normalizedSuccessfulTaskCount = Number(successfulTaskCount || 0);
  const normalizedReviewableTaskCount = Number(reviewableTaskCount || 0);
  const normalizedHasSavedMemory = hasSavedMemory === true;
  const inProgressCount = countInProgressTasks(normalizedTaskCount, normalizedReviewableTaskCount);
  const connectedSummary = summarizeProviders(normalizedConnectedTypes);

  let stateKey = 'no_connected_app';
  let title = 'Connect the first place Oliver should work';
  let summary = 'Until one app is connected, Oliver has nowhere to receive requests. Start with the surface where work already lands.';
  let helper = 'Results will appear in Work after a request comes in from a connected app.';
  let primaryAction = buildPrimaryAction('section-channels', 'Open Connections');

  if (normalizedConnectedTypes.length === 0) {
    stateKey = 'no_connected_app';
  } else if (normalizedTaskCount === 0) {
    stateKey = 'connected_no_tasks';
    title = 'Send one small request from a connected app';
    summary = `You already connected ${connectedSummary}. The first request still has to start in that app. Work stays empty until that request is sent.`;
    helper = 'Use Connections to confirm the best place to start, then send one small request there and review the result in Work.';
    primaryAction = buildPrimaryAction('section-channels', 'See Where to Start');
  } else if (normalizedReviewableTaskCount === 0) {
    stateKey = 'tasks_in_progress';
    title = normalizedTaskCount === 1 ? 'Your first request is in motion' : `${normalizedTaskCount} requests are in motion`;
    summary = inProgressCount <= 1
      ? 'Work already has activity, but nothing is reviewable yet. The next step is to watch Work until the first result lands.'
      : `${inProgressCount} requests are still in progress. Review should happen in Work once the first result lands.`;
    helper = normalizedHasSavedMemory
      ? 'If the next result needs a tighter tone or approval rule, adjust Memory before the second request.'
      : 'Once the first result lands, review it in Work before you connect more surfaces or expand the setup.';
    primaryAction = buildPrimaryAction('section-work', 'Open Work');
  } else if (
    normalizedHasSavedMemory &&
    normalizedSuccessfulTaskCount > 0 &&
    normalizedReviewableTaskCount > 0 &&
    normalizedTaskCount >= 3
  ) {
    stateKey = 'ongoing_usage';
    title = 'Oliver is already active in your workflow';
    summary = `${normalizedReviewableTaskCount} result${normalizedReviewableTaskCount === 1 ? '' : 's'} are ready in Work, ${normalizedSuccessfulTaskCount} completed successfully, and Memory is already saved. The next gain is in reviewing fresh work, not adding more setup.`;
    helper = nextSuggestedProvider
      ? `Only connect ${providerLabel(nextSuggestedProvider)} next if it supports work you already trust Oliver to handle.`
      : 'Use Memory or Settings only when you need a sharper boundary, not as the default next step.';
    primaryAction = buildPrimaryAction('section-work', 'Open Work');
  } else {
    stateKey = 'review_ready';
    title = normalizedReviewableTaskCount === 1
      ? 'Review the latest result in Work'
      : `Review ${normalizedReviewableTaskCount} results in Work`;
    summary = normalizedReviewableTaskCount === 1
      ? 'A result is ready. Review it before you connect more surfaces or add more workflow complexity.'
      : `${normalizedReviewableTaskCount} results are ready. Review them in Work before you expand the setup.`;
    helper = normalizedHasSavedMemory
      ? 'Once the review loop feels right, keep the workflow tight instead of adding more setup.'
      : 'After the first review, save tone or approval rules in Memory so the next result needs less cleanup.';
    primaryAction = buildPrimaryAction('section-work', 'Open Work');
  }

  return {
    stateKey,
    title,
    summary,
    helper,
    primaryAction,
    connectedSummary,
    nextSuggestedProvider,
    chips: [
      {
        label: 'State',
        value: stateLabel(stateKey)
      },
      {
        label: 'Connections',
        value: normalizedConnectedTypes.length > 0
          ? `${normalizedConnectedTypes.length} connected`
          : 'None connected'
      },
      {
        label: 'Work',
        value: normalizedReviewableTaskCount > 0
          ? `${normalizedReviewableTaskCount} ready to review`
          : normalizedTaskCount > 0
            ? `${Math.max(inProgressCount, 1)} in progress`
            : 'No work yet'
      },
      {
        label: 'Memory',
        value: normalizedHasSavedMemory ? 'Saved' : 'Needs setup'
      }
    ]
  };
}

function buildSetupStep({ key, title, description, status, action }) {
  return {
    key,
    title,
    description,
    status,
    action
  };
}

export function buildDashboardSetupModel({
  connectedTypes = [],
  taskCount = 0,
  reviewableTaskCount = 0,
  hasSavedMemory = false
} = {}) {
  const normalizedConnectedTypes = uniqueProviders(connectedTypes);
  const normalizedTaskCount = Number(taskCount || 0);
  const normalizedReviewableTaskCount = Number(reviewableTaskCount || 0);
  const normalizedHasSavedMemory = hasSavedMemory === true;
  const hasConnectedApp = normalizedConnectedTypes.length > 0;
  const hasStartedWork = normalizedTaskCount > 0;
  const hasReviewReadyWork = normalizedReviewableTaskCount > 0;
  const coreStepsComplete = [hasConnectedApp, hasStartedWork, hasReviewReadyWork].filter(Boolean).length;

  const steps = [
    buildSetupStep({
      key: 'connect',
      title: 'Connect one app',
      description: 'Start with the first place where requests already land.',
      status: hasConnectedApp ? 'complete' : 'current',
      action: buildPrimaryAction('section-channels', 'Open Connections')
    }),
    buildSetupStep({
      key: 'send',
      title: 'Send one small request',
      description: hasConnectedApp
        ? `Send it in ${summarizeProviders(normalizedConnectedTypes)}. It cannot start from inside Work.`
        : 'Connect an app first, then send one small request there.',
      status: hasStartedWork ? 'complete' : hasConnectedApp ? 'current' : 'pending',
      action: buildPrimaryAction('section-channels', hasConnectedApp ? 'See Connected Apps' : 'Open Connections')
    }),
    buildSetupStep({
      key: 'review',
      title: 'Review the result in Work',
      description: hasStartedWork
        ? 'Once the result lands, review it in Work before you expand anything else.'
        : 'Work becomes useful after the first request lands.',
      status: hasReviewReadyWork ? 'complete' : hasStartedWork ? 'current' : 'pending',
      action: buildPrimaryAction('section-work', 'Open Work')
    })
  ];

  let note = null;
  if (!normalizedHasSavedMemory) {
    note = {
      title: 'Memory is still optional, but it should come after the first review loop.',
      description: 'Save tone, recurring context, or approval rules only after the first result tells you what is actually missing.',
      action: buildPrimaryAction('section-memo', 'Open Memory')
    };
  } else if (coreStepsComplete === 3) {
    note = {
      title: 'Setup is no longer the bottleneck.',
      description: 'Only come back here when you add another app or need to re-check the first-loop basics.',
      action: buildPrimaryAction('section-work', 'Open Work')
    };
  }

  return {
    title: coreStepsComplete === 3 && normalizedHasSavedMemory
      ? 'Setup is no longer the main job'
      : 'Keep setup lean',
    intro: coreStepsComplete === 3 && normalizedHasSavedMemory
      ? 'The basics are in place. Review Work first, then expand only when the current loop already feels solid.'
      : 'Connect one app, send one request from that app, then review the result in Work.',
    progressLabel: `${coreStepsComplete} of 3 core steps done`,
    steps,
    note,
    isComplete: coreStepsComplete === 3 && normalizedHasSavedMemory
  };
}
