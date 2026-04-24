import { getDoWhizApiBaseUrl } from '../analytics';

export const LAUNCH_EXECUTION_API_PATH = '/api/launch-execution/analyze';

export async function analyzeLaunchExecution({
  contextText,
  updateText,
  sourceLabel,
  priorPlan
}) {
  const response = await fetch(`${getDoWhizApiBaseUrl()}${LAUNCH_EXECUTION_API_PATH}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      source_type: 'pasted_thread',
      source_label: sourceLabel,
      context_text: contextText,
      update_text: updateText,
      prior_plan: priorPlan || null
    })
  });

  const body = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(body?.error || `Launch execution request failed (${response.status})`);
  }

  return body;
}

export function getReadinessTone(status) {
  const normalized = String(status || '').trim().toLowerCase();
  if (normalized === 'green') {
    return 'green';
  }
  if (normalized === 'red') {
    return 'red';
  }
  return 'yellow';
}

export function formatReadinessLabel(status) {
  const tone = getReadinessTone(status);
  return tone.charAt(0).toUpperCase() + tone.slice(1);
}

export function formatLaunchDate(targetDate, launchWindow) {
  return targetDate || launchWindow || 'Needs date';
}
