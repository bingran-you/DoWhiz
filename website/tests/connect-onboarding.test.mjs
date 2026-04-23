import test from 'node:test';
import assert from 'node:assert/strict';

import {
  CONNECT_ONBOARDING_MAX_AGE_MS,
  buildConnectOnboardingModel,
  clearConnectOnboardingStateForProvider,
  chooseConnectOnboardingPayload,
  createConnectOnboardingPayload,
  filterOptionalExtensionsForNextSteps,
  parseStoredConnectOnboarding
} from '../public/auth/connect-onboarding.js';

function flattenedCopy(model) {
  return [
    model?.title || '',
    model?.description || '',
    ...(Array.isArray(model?.examples) ? model.examples : []),
    model?.footnote || ''
  ].join(' ').toLowerCase();
}

function assertContentGuardrails(model) {
  const copy = flattenedCopy(model);
  assert.ok(!copy.includes('read the whole workspace'));
  assert.ok(!copy.includes('read the whole server'));
  assert.ok(!copy.includes('i know your team'));
  assert.ok(!copy.includes('everyone in your workspace'));
}

test('Slack connect onboarding asks the user to add Oliver to Slack before bot install', () => {
  const model = buildConnectOnboardingModel({
    provider: 'slack',
    eventType: 'connect',
    completedAdminTasks: []
  });

  assert.equal(model.title, 'Slack connected');
  assert.equal(model.cta.kind, 'bot_install');
  assert.equal(model.cta.provider, 'slack');
  assert.match(model.footnote, /does not install the slack bot/i);
  assertContentGuardrails(model);
});

test('Slack install onboarding switches to a calm live-state CTA', () => {
  const model = buildConnectOnboardingModel({
    provider: 'slack',
    eventType: 'install',
    completedAdminTasks: ['add-oliver-slack']
  });

  assert.equal(model.title, 'Oliver is now available in Slack');
  assert.equal(model.cta.kind, 'anchor');
  assert.equal(model.cta.href, '#section-work');
  assert.equal(model.examples.length, 3);
  assertContentGuardrails(model);
});

test('Discord connect onboarding uses the same guardrailed install handoff', () => {
  const model = buildConnectOnboardingModel({
    provider: 'discord',
    eventType: 'connect',
    completedAdminTasks: []
  });

  assert.equal(model.cta.kind, 'bot_install');
  assert.equal(model.cta.provider, 'discord');
  assert.match(model.footnote, /does not install the discord bot/i);
  assertContentGuardrails(model);
});

test('Generic connected apps still produce a deterministic onboarding model', () => {
  const model = buildConnectOnboardingModel({
    provider: 'github',
    eventType: 'connect',
    completedAdminTasks: []
  });

  assert.equal(model.title, 'GitHub connected');
  assert.equal(model.cta.kind, 'anchor');
  assert.equal(model.cta.href, '#section-work');
  assert.equal(model.examples.length, 3);
  assertContentGuardrails(model);
});

test('Optional extensions no longer duplicate Slack or Discord install tasks in Next Steps', () => {
  const adminTasks = [
    {
      id: 'add-oliver-slack',
      channelKey: 'slack',
      nextStepsEligible: false
    },
    {
      id: 'add-oliver-discord',
      channelKey: 'discord',
      nextStepsEligible: false
    },
    {
      id: 'some-future-admin-task',
      channelKey: 'github',
      nextStepsEligible: true
    }
  ];

  const optionalExtensions = filterOptionalExtensionsForNextSteps(adminTasks, [
    'slack',
    'discord',
    'github'
  ]);

  assert.deepEqual(optionalExtensions.map((task) => task.id), ['some-future-admin-task']);
});

test('Connected Slack falls back to a persistent add-bot onboarding state when needed', () => {
  const payload = chooseConnectOnboardingPayload({
    pendingPayload: null,
    connectedTypes: ['slack'],
    completedAdminTasks: []
  });

  assert.equal(payload.provider, 'slack');
  assert.equal(payload.eventType, 'connect');
  assert.equal(payload.source, 'pending_bot_install');
});

test('A valid recent pending onboarding event wins over fallback state', () => {
  const pendingPayload = createConnectOnboardingPayload('github', {
    source: 'oauth_callback',
    createdAt: Date.now()
  });

  const payload = chooseConnectOnboardingPayload({
    pendingPayload,
    connectedTypes: ['github', 'slack'],
    completedAdminTasks: []
  });

  assert.equal(payload.provider, 'github');
  assert.equal(payload.source, 'oauth_callback');
});

test('Stale pending onboarding payloads are discarded safely', () => {
  const stalePayload = JSON.stringify({
    provider: 'slack',
    eventType: 'connect',
    source: 'oauth_callback',
    createdAt: Date.now() - CONNECT_ONBOARDING_MAX_AGE_MS - 1000
  });

  assert.equal(parseStoredConnectOnboarding(stalePayload), null);
});

test('Clearing Slack onboarding state removes the local bot-install completion marker and matching pending payload', () => {
  const result = clearConnectOnboardingStateForProvider({
    provider: 'slack',
    completedAdminTasks: ['add-oliver-slack', 'add-oliver-discord'],
    pendingPayload: createConnectOnboardingPayload('slack', {
      eventType: 'install',
      source: 'bot_install_callback',
      createdAt: Date.now()
    })
  });

  assert.deepEqual(result.completedAdminTasks, ['add-oliver-discord']);
  assert.equal(result.pendingPayload, null);
});

test('Clearing one provider leaves unrelated onboarding state intact', () => {
  const pendingPayload = createConnectOnboardingPayload('discord', {
    eventType: 'connect',
    source: 'oauth_callback',
    createdAt: Date.now()
  });

  const result = clearConnectOnboardingStateForProvider({
    provider: 'slack',
    completedAdminTasks: ['add-oliver-slack', 'add-oliver-discord'],
    pendingPayload
  });

  assert.deepEqual(result.completedAdminTasks, ['add-oliver-discord']);
  assert.deepEqual(result.pendingPayload, pendingPayload);
});
