import test from 'node:test';
import assert from 'node:assert/strict';

import {
  buildDashboardHomepageModel,
  buildDashboardSetupModel
} from '../public/auth/homepage-state.js';

test('connected apps without tasks no longer send the user to Work as the next step', () => {
  const model = buildDashboardHomepageModel({
    connectedTypes: ['slack', 'github'],
    taskCount: 0,
    successfulTaskCount: 0,
    reviewableTaskCount: 0,
    hasSavedMemory: false
  });

  assert.equal(model.stateKey, 'connected_no_tasks');
  assert.equal(model.primaryAction.section, 'section-channels');
  assert.equal(model.primaryAction.label, 'See Where to Start');
  assert.match(model.summary, /work stays empty until that request is sent/i);
});

test('review-ready work resolves to a truthful open-work next step', () => {
  const model = buildDashboardHomepageModel({
    connectedTypes: ['slack'],
    taskCount: 2,
    successfulTaskCount: 1,
    reviewableTaskCount: 1,
    hasSavedMemory: false
  });

  assert.equal(model.stateKey, 'review_ready');
  assert.equal(model.primaryAction.section, 'section-work');
  assert.equal(model.primaryAction.label, 'Open Work');
});

test('mature usage keeps setup demoted and marks the core setup loop complete', () => {
  const homeModel = buildDashboardHomepageModel({
    connectedTypes: ['slack', 'github'],
    taskCount: 4,
    successfulTaskCount: 3,
    reviewableTaskCount: 2,
    hasSavedMemory: true
  });
  const setupModel = buildDashboardSetupModel({
    connectedTypes: ['slack', 'github'],
    taskCount: 4,
    reviewableTaskCount: 2,
    hasSavedMemory: true
  });

  assert.equal(homeModel.stateKey, 'ongoing_usage');
  assert.equal(setupModel.progressLabel, '3 of 3 core steps done');
  assert.equal(setupModel.isComplete, true);
  assert.equal(setupModel.note?.action?.section, 'section-work');
});
