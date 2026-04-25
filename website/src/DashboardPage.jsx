import { useDeferredValue, useEffect, useMemo, useState } from 'react';
import { getDoWhizApiBaseUrl } from './analytics';
import { supabase } from './app/supabaseClient';
import './dashboard.css';

const RANGE_OPTIONS = [
  { label: 'Last 7 days', value: '7d' },
  { label: 'Last 30 days', value: '30d' },
  { label: 'Last 90 days', value: '90d' },
  { label: 'Last 180 days', value: '180d' }
];

const clampPercent = (value) => {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(1, value));
};

const formatNumber = (value) => {
  if (!Number.isFinite(value)) return '0';
  return new Intl.NumberFormat('en-US').format(value);
};

const formatPercent = (value) => `${(clampPercent(value) * 100).toFixed(1)}%`;

const formatOptionalPercent = (value) => {
  if (value === null || value === undefined) return 'N/A';
  return formatPercent(value);
};

const formatDeferredAwareCount = (value, dashboard, eventNames) => {
  const zeroish = !Number.isFinite(value) || value === 0;
  const deferredEvents = new Set(dashboard?.deferred_events || []);
  const implementedEvents = new Set(dashboard?.implemented_events || []);
  const allDeferred = eventNames.every(
    (eventName) => deferredEvents.has(eventName) && !implementedEvents.has(eventName)
  );

  if (zeroish && allDeferred) {
    return 'N/A';
  }

  return formatNumber(value);
};

const formatHours = (value) => {
  if (!Number.isFinite(value)) return 'N/A';
  return `${value.toFixed(1)}h`;
};

const formatCurrency = (value) => {
  if (!Number.isFinite(value)) return '$0';
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    maximumFractionDigits: 0
  }).format(value);
};

const METRIC_HELP = {
  unique_visitors: 'Distinct identities that triggered landing_page_view in the selected window.',
  signup_conversion: 'signup_completed identities / unique visitor identities.',
  signup_to_activation: 'Identities with first successful task / identities with signup_completed.',
  activation_rate: 'Identities with first successful task / identities with signup_completed.',
  activation_to_paid: 'Paid identities (payment_succeeded or subscription_activated) / activated identities.',
  visitor_to_paid: 'Paid identities (payment_succeeded or subscription_activated) / unique visitor identities.',
  time_to_first_value: 'Median hours from signup_completed to first_task_succeeded.',
  d7_retention: 'Eligible signup_completed identities with a usage event 7-8 days later.',
  active_workspaces: 'Distinct workspace/account ids associated with tracked identities in the selected window.',
  paid_accounts_30d: 'Distinct accounts with at least one payment in the 30 days ending at window end.',
  revenue_range: 'Sum of successful payment amounts (USD) recorded in the selected window.',
  funnel_step_conversion: 'Current step identities / previous funnel step identities.',
  funnel_overall_conversion: 'Current step identities / first funnel step identities.',
  pricing_page_views: 'Count of pricing_page_view events.',
  upgrade_clicks: 'Count of upgrade_clicked events.',
  paywall_views: 'Count of upgrade_viewed_or_paywall_seen and paywall_seen events.',
  checkout_starts: 'Count of checkout_started events (includes fallback interpretation in funnel sequencing).',
  checkout_abandon_rate:
    'checkout_abandoned / checkout_started when available; otherwise max(checkout_started - payment_succeeded, 0) / checkout_started.',
  payment_succeeded: 'Count of payment_succeeded events.',
  subscription_activated: 'Count of subscription_activated events.',
  subscription_renewals: 'Count of subscription_renewed events.',
  subscription_canceled: 'Count of subscription_canceled events.',
  agent_or_workflow_creation_rate:
    'Signup_completed identities with first_agent_or_workflow_created / signup_completed identities.',
  multi_channel_connection_rate:
    'Signup_completed identities connected to 2+ channel/tool types / signup_completed identities.',
  d1_retention: 'Eligible signup_completed identities with a usage event 1-2 days later.',
  d30_retention: 'Eligible signup_completed identities with a usage event 30-31 days later.',
  repeat_value_rate:
    'Identities with second_successful_task within 7 days of first_task_succeeded / identities with first_task_succeeded.',
  repeat_successful_task_rate:
    'Identities with second_successful_task within 7 days of first_task_succeeded / identities with first_task_succeeded.',
  dau_wau: 'Distinct active users today / distinct active users in trailing 7 days.',
  dau_mau: 'Distinct active users today / distinct active users in trailing 30 days.',
  active_users_trend: 'Daily count of active identities based on usage events.',
  active_workspaces_trend: 'Daily count of active workspaces based on usage events.',
  task_success_rate: 'task_succeeded / (task_succeeded + task_failed).',
  api_error_rate: 'api_error / api_request, when api_request telemetry is present.',
  integration_failure_rate:
    '(channel_connect_failed + tool_connect_failed + integration_error) / connection attempts, when attempts telemetry is present.',
  checkout_failure_rate: 'checkout_error / checkout_started, when checkout_started > 0.',
  trial_to_paid_rate: 'payment_succeeded / trial_started, when trial_started telemetry is present.',
  avg_latency_ms: 'Average latency in milliseconds across latency_metric_logged events for each endpoint/workflow.',
  p95_latency_ms: '95th percentile latency in milliseconds across latency_metric_logged events for each endpoint/workflow.',
  cohort_users: 'Number of signup_completed identities in the cohort week.',
  retention_rate_col: 'Retention rate for that cohort at the given day marker.',
  breakdown_count: 'Identity count for this segment in the selected window.',
  breakdown_rate: 'Segment count / segment denominator for this table.'
};

const FUNNEL_STEP_HELP = {
  landing_page_view: 'Visitor identity generated landing_page_view.',
  primary_cta_click:
    'Visitor clicked the landing-page primary CTA. This can be lower than later signup steps when users enter directly on auth pages.',
  signup_started:
    'Identities with signup_started. To prevent instrumentation undercount, signup_completed is also treated as a fallback signal for this step.',
  signup_completed: 'Account signup completed successfully (created or backfilled account creation within the window).',
  first_authenticated_session: 'First authenticated app session for the identity.',
  workspace_created: 'Initial account workspace provisioned.',
  first_channel_or_tool_connected: 'First successful external channel/tool connection.',
  first_agent_or_workflow_created: 'First agent or workflow creation event.',
  first_task_started:
    'First task start event. Success events are accepted as fallback to avoid impossible success-without-start ordering.',
  first_task_succeeded:
    'First successful task event. second_successful_task is accepted as fallback for ordering consistency.',
  second_successful_task: 'Second successful task event for the identity.',
  upgrade_viewed_or_paywall_seen:
    'Upgrade/paywall intent event. In-app upgrade surfaces can vary by route and entrypoint.',
  checkout_started:
    'Checkout initiated. payment_succeeded/subscription_activated are accepted as fallback signals for ordering consistency.',
  payment_succeeded: 'Successful payment event.',
  subscription_activated: 'Subscription or paid credit state activated.'
};

function MetricHeading({ label, help }) {
  if (!help) {
    return <>{label}</>;
  }
  return (
    <span className="dash-metric-heading">
      <span>{label}</span>
      <span className="dash-help-wrap">
        <button type="button" className="dash-help-btn" aria-label={`${label}. ${help}`}>
          ?
        </button>
        <span className="dash-help-tooltip" role="tooltip">
          {help}
        </span>
      </span>
    </span>
  );
}

function Section({ title, subtitle, children }) {
  return (
    <section className="dash-section">
      <div className="dash-section-head">
        <h2>{title}</h2>
        {subtitle ? <p>{subtitle}</p> : null}
      </div>
      {children}
    </section>
  );
}

function EmptyState({ label }) {
  return <div className="dash-empty">{label}</div>;
}

function BreakdownTable({
  rows,
  firstCol,
  firstColHelp,
  secondCol = 'Count',
  secondColHelp = METRIC_HELP.breakdown_count,
  rateCol = 'Rate',
  rateColHelp = METRIC_HELP.breakdown_rate
}) {
  if (!rows?.length) {
    return <EmptyState label="No data in selected range." />;
  }

  return (
    <div className="dash-table-wrap">
      <table className="dash-table">
        <thead>
          <tr>
            <th>
              <MetricHeading label={firstCol} help={firstColHelp} />
            </th>
            <th>
              <MetricHeading label={secondCol} help={secondColHelp} />
            </th>
            <th>
              <MetricHeading label={rateCol} help={rateColHelp} />
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={`${firstCol}-${row.key}`}>
              <td>{row.key}</td>
              <td>{formatNumber(row.count ?? row.visitors ?? 0)}</td>
              <td>{formatPercent(row.rate ?? row.signup_conversion_rate ?? 0)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function AcquisitionTable({ rows, title }) {
  return (
    <div className="dash-card">
      <h3>
        <MetricHeading
          label={title}
          help="Segmented acquisition funnel: visitors, signups, activated, and paid identities grouped by this dimension."
        />
      </h3>
      {!rows?.length ? (
        <EmptyState label="No data in selected range." />
      ) : (
        <div className="dash-table-wrap">
          <table className="dash-table">
            <thead>
              <tr>
                <th>Segment</th>
                <th>
                  <MetricHeading label="Visitors" help="Distinct identities with landing_page_view in this segment." />
                </th>
                <th>
                  <MetricHeading label="Signups" help="Distinct identities with signup_completed in this segment." />
                </th>
                <th>
                  <MetricHeading
                    label="Activated"
                    help="Distinct identities with first_task_succeeded or task_succeeded in this segment."
                  />
                </th>
                <th>
                  <MetricHeading
                    label="Paid"
                    help="Distinct identities with payment_succeeded or subscription_activated in this segment."
                  />
                </th>
                <th>
                  <MetricHeading label="Signup CVR" help="Signups / Visitors for this segment." />
                </th>
                <th>
                  <MetricHeading label="Activation" help="Activated / Signups for this segment." />
                </th>
                <th>
                  <MetricHeading label="Paid CVR" help="Paid / Signups for this segment." />
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={`${title}-${row.key}`}>
                  <td>{row.key}</td>
                  <td>{formatNumber(row.visitors)}</td>
                  <td>{formatNumber(row.signups)}</td>
                  <td>{formatNumber(row.activated)}</td>
                  <td>{formatNumber(row.paid)}</td>
                  <td>{formatPercent(row.signup_conversion_rate)}</td>
                  <td>{formatPercent(row.activation_rate)}</td>
                  <td>{formatPercent(row.paid_conversion_rate)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function TrendBars({ points, title, titleHelp }) {
  if (!points?.length) {
    return (
      <div className="dash-card">
        <h3>
          <MetricHeading label={title} help={titleHelp} />
        </h3>
        <EmptyState label="No trend data in selected range." />
      </div>
    );
  }

  const trimmed = points.slice(-21);
  const max = Math.max(...trimmed.map((point) => point.count), 1);

  return (
    <div className="dash-card">
      <h3>
        <MetricHeading label={title} help={titleHelp} />
      </h3>
      <div className="dash-bars" role="img" aria-label={title}>
        {trimmed.map((point) => {
          const height = Math.max((point.count / max) * 100, point.count > 0 ? 6 : 2);
          return (
            <div key={`${title}-${point.day}`} className="dash-bar-group" title={`${point.day}: ${point.count}`}>
              <div className="dash-bar" style={{ height: `${height}%` }} />
              <span>{point.day.slice(5)}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

const DASHBOARD_VIEWS = [
  { label: 'Business', value: 'business' },
  { label: 'Task Ops', value: 'task_ops' }
];
const TASK_OPS_DEFAULT_PAGE_SIZE = 50;

const TASK_OPS_STATUS_OPTIONS = [
  { label: 'All statuses', value: '' },
  { label: 'Running', value: 'running' },
  { label: 'Success', value: 'success' },
  { label: 'Failed', value: 'failed' },
  { label: 'Superseded', value: 'superseded' },
  { label: 'Cancelled', value: 'cancelled' }
];

const TASK_OPS_CHANNEL_LABELS = {
  email: 'Email',
  slack: 'Slack',
  discord: 'Discord',
  sms: 'SMS',
  telegram: 'Telegram',
  whatsapp: 'WhatsApp',
  google_docs: 'Google Docs',
  google_sheets: 'Google Sheets',
  google_slides: 'Google Slides',
  bluebubbles: 'iMessage',
  wechat: 'WeCom',
  wechat_mp: 'WeChat MP',
  lark: 'Lark',
  notion: 'Notion',
  zoom: 'Zoom'
};

const TASK_OPS_STAGE_LABELS = {
  initializing: 'Initializing',
  preparing_azure_aci: 'Preparing ACI',
  creating_ephemeral_share: 'Creating share',
  uploading_workspace: 'Uploading workspace',
  starting_aci_container: 'Starting container',
  executing_codex: 'Executing Codex',
  executing_codex_local: 'Running Codex (local)',
  executing_codex_docker: 'Running Codex (docker)',
  executing_claude_local: 'Running Claude',
  validating_reply_artifact: 'Validating output',
  downloading_results: 'Downloading results',
  preparing_warm_pool: 'Preparing warm pool',
  completed: 'Completed',
  failed: 'Failed'
};

const TASK_OPS_CHANNEL_OPTIONS = [
  { label: 'All channels', value: '' },
  ...Object.entries(TASK_OPS_CHANNEL_LABELS).map(([value, label]) => ({ value, label }))
];

const normalizeTaskOpsStatus = (status) => String(status || '').trim().toLowerCase();

const humanizeTaskOpsToken = (value) =>
  String(value || '')
    .split(/[_\-\s]+/)
    .filter(Boolean)
    .map((part) => (part.length <= 3 ? part.toUpperCase() : part.charAt(0).toUpperCase() + part.slice(1)))
    .join(' ');

const formatTaskOpsChannel = (channel) =>
  TASK_OPS_CHANNEL_LABELS[String(channel || '').trim().toLowerCase()] || humanizeTaskOpsToken(channel) || 'Unknown';

const formatTaskOpsStage = (stage) => {
  const normalized = String(stage || '').trim().toLowerCase();
  return TASK_OPS_STAGE_LABELS[normalized] || humanizeTaskOpsToken(normalized) || 'N/A';
};

const formatTaskOpsDuration = (seconds) => {
  const total = Number(seconds);
  if (!Number.isFinite(total)) return 'N/A';
  if (total < 60) return `${Math.max(0, Math.round(total))}s`;
  const minutes = Math.floor(total / 60);
  const remainingSeconds = Math.round(total % 60);
  if (minutes < 60) return remainingSeconds ? `${minutes}m ${remainingSeconds}s` : `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  return remainingMinutes ? `${hours}h ${remainingMinutes}m` : `${hours}h`;
};

const formatTaskOpsDateTime = (value) => {
  if (!value) return 'N/A';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return 'N/A';
  return date.toLocaleString();
};

const formatTaskOpsTiming = (value) => {
  const timing = Number(value);
  if (!Number.isFinite(timing)) return 'N/A';
  if (timing >= 1000) return `${(timing / 1000).toFixed(1)}s`;
  return `${Math.round(timing)}ms`;
};

const summarizeTaskOpsError = (message) => {
  const raw = String(message || '').trim();
  if (!raw) return '';
  const firstLine = raw.split('\n').map((line) => line.trim()).find(Boolean) || raw;
  return firstLine.length > 160 ? `${firstLine.slice(0, 157)}...` : firstLine;
};

function TaskOpsStatusBadge({ status, isRunningLong }) {
  const normalized = normalizeTaskOpsStatus(status);
  let tone = 'muted';
  let label = humanizeTaskOpsToken(normalized) || 'Unknown';

  if (normalized === 'success') {
    tone = 'success';
    label = 'Completed';
  } else if (normalized === 'failed') {
    tone = 'danger';
    label = 'Failed';
  } else if (normalized === 'running') {
    tone = isRunningLong ? 'warning' : 'info';
    label = isRunningLong ? 'Running > 1h' : 'Running';
  } else if (normalized === 'superseded') {
    tone = 'neutral';
    label = 'Superseded';
  } else if (normalized === 'cancelled') {
    tone = 'neutral';
    label = 'Cancelled';
  }

  return <span className={`dash-pill dash-pill-${tone}`}>{label}</span>;
}

function TaskOpsDetailsModal({ row, onClose }) {
  if (!row) return null;

  const timingEntries = Object.entries({
    Queue: row.timing_ms?.queue_latency_ms,
    Setup: row.timing_ms?.setup_latency_ms,
    'Share create': row.timing_ms?.ephemeral_share_create_ms,
    'Share upload': row.timing_ms?.ephemeral_share_upload_ms,
    'ACI cold start': row.timing_ms?.aci_cold_start_ms,
    'Codex / Claude': row.timing_ms?.codex_execution_ms,
    Download: row.timing_ms?.result_download_ms,
    Total: row.timing_ms?.total_ms
  }).filter(([, value]) => Number.isFinite(value));

  const metadataRows = [
    ['Task ID', row.task_id],
    ['Execution ID', row.execution_id],
    ['Title', row.title],
    ['Channel', formatTaskOpsChannel(row.channel)],
    ['Sender', row.sender_name || row.sender || 'N/A'],
    ['Status', formatTaskOpsStage(row.status)],
    ['Current stage', row.current_stage ? formatTaskOpsStage(row.current_stage) : 'N/A'],
    ['Duration', formatTaskOpsDuration(row.duration_seconds)],
    ['Started at', formatTaskOpsDateTime(row.started_at)],
    ['Finished at', formatTaskOpsDateTime(row.finished_at)],
    ['Created at', formatTaskOpsDateTime(row.created_at)],
    ['Runner', row.runner || 'N/A'],
    ['Model', row.model_name || 'N/A'],
    ['Backend', row.backend || 'N/A'],
    ['Retry count', row.retry_count],
    ['Schedule', humanizeTaskOpsToken(row.schedule_type || 'one_shot')],
    ['Next run', formatTaskOpsDateTime(row.next_run)],
    ['Run at', formatTaskOpsDateTime(row.run_at)]
  ];

  return (
    <div className="dash-modal-shell" onClick={onClose} role="presentation">
      <div className="dash-modal" onClick={(event) => event.stopPropagation()} role="dialog" aria-modal="true">
        <div className="dash-modal-head">
          <div>
            <h3>{row.title}</h3>
            <p>
              {formatTaskOpsChannel(row.channel)} · {row.sender_name || row.sender || 'Unknown sender'}
            </p>
          </div>
          <button type="button" className="dash-modal-close" onClick={onClose} aria-label="Close details">
            ×
          </button>
        </div>

        <div className="dash-modal-section">
          <div className="dash-modal-grid">
            {metadataRows.map(([label, value]) => (
              <div className="dash-modal-card" key={`${row.task_id}-${row.execution_id}-${label}`}>
                <span>{label}</span>
                <strong>{String(value)}</strong>
              </div>
            ))}
          </div>
        </div>

        {row.error_message ? (
          <div className="dash-modal-section">
            <h4>Error</h4>
            <div className="dash-error dash-error-inline">{row.error_message}</div>
          </div>
        ) : null}

        <div className="dash-modal-section">
          <h4>Stage Timing</h4>
          {!timingEntries.length ? (
            <EmptyState label="No stage timing captured for this run." />
          ) : (
            <div className="dash-modal-grid dash-modal-grid-tight">
              {timingEntries.map(([label, value]) => (
                <div className="dash-modal-card" key={`${row.task_id}-${label}`}>
                  <span>{label}</span>
                  <strong>{formatTaskOpsTiming(value)}</strong>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function TaskOpsView({
  taskOps,
  loading,
  error,
  statusFilter,
  channelFilter,
  searchDraft,
  onStatusChange,
  onChannelChange,
  onSearchChange,
  onPrevPage,
  onNextPage,
  onSelectRow
}) {
  const totalPages = Math.max(
    1,
    Math.ceil((taskOps?.total_rows || 0) / (taskOps?.page_size || TASK_OPS_DEFAULT_PAGE_SIZE))
  );
  const canPrev = (taskOps?.page || 1) > 1;
  const canNext = (taskOps?.page || 1) < totalPages;

  return (
    <>
      <Section
        title="Task Ops Overview"
        subtitle="A global run ledger for the latest user-visible work across channels, with live stage status for the newest execution of each task."
      >
        {loading ? <EmptyState label="Loading task operations..." /> : null}
        {error ? <div className="dash-error">{error}</div> : null}

        {!loading && !error && taskOps ? (
          <>
            <div className="dash-kpi-grid">
              <article className="dash-kpi-card">
                <h3>Total runs</h3>
                <p>{formatNumber(taskOps.summary.total_runs)}</p>
              </article>
              <article className="dash-kpi-card">
                <h3>Running now</h3>
                <p>{formatNumber(taskOps.summary.running_now)}</p>
              </article>
              <article className="dash-kpi-card">
                <h3>Long-running</h3>
                <p>{formatNumber(taskOps.summary.long_running)}</p>
              </article>
              <article className="dash-kpi-card">
                <h3>Failed</h3>
                <p>{formatNumber(taskOps.summary.failed_runs)}</p>
              </article>
              <article className="dash-kpi-card">
                <h3>Success rate</h3>
                <p>{formatOptionalPercent(taskOps.summary.success_rate)}</p>
              </article>
              <article className="dash-kpi-card">
                <h3>Median duration</h3>
                <p>{formatTaskOpsDuration(taskOps.summary.median_duration_seconds)}</p>
              </article>
              <article className="dash-kpi-card">
                <h3>P95 duration</h3>
                <p>{formatTaskOpsDuration(taskOps.summary.p95_duration_seconds)}</p>
              </article>
            </div>

            <div className="dash-toolbar">
              <div className="dash-toolbar-group">
                <label htmlFor="task-ops-status">Status</label>
                <select id="task-ops-status" value={statusFilter} onChange={(event) => onStatusChange(event.target.value)}>
                  {TASK_OPS_STATUS_OPTIONS.map((option) => (
                    <option key={option.value || 'all'} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </div>
              <div className="dash-toolbar-group">
                <label htmlFor="task-ops-channel">Channel</label>
                <select id="task-ops-channel" value={channelFilter} onChange={(event) => onChannelChange(event.target.value)}>
                  {TASK_OPS_CHANNEL_OPTIONS.map((option) => (
                    <option key={option.value || 'all'} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </div>
              <div className="dash-toolbar-group dash-toolbar-search">
                <label htmlFor="task-ops-search">Search</label>
                <input
                  id="task-ops-search"
                  type="search"
                  value={searchDraft}
                  onChange={(event) => onSearchChange(event.target.value)}
                  placeholder="Task, sender, error, task id"
                />
              </div>
            </div>

            {!taskOps.rows?.length ? (
              <EmptyState label="No task runs match the current filters." />
            ) : (
              <div className="dash-table-wrap">
                <table className="dash-table dash-table-task-ops">
                  <thead>
                    <tr>
                      <th>Task</th>
                      <th>Sender</th>
                      <th>Channel</th>
                      <th>Stage</th>
                      <th>Status</th>
                      <th>Duration</th>
                      <th>Started</th>
                      <th>Details</th>
                    </tr>
                  </thead>
                  <tbody>
                    {taskOps.rows.map((row) => (
                      <tr key={`${row.task_id}-${row.execution_id}`}>
                        <td>
                          <div className="dash-row-stack">
                            <strong>{row.title}</strong>
                            <span>
                              {row.task_id.slice(0, 8)} · exec {row.execution_id}
                            </span>
                            {row.error_message ? <span>{summarizeTaskOpsError(row.error_message)}</span> : null}
                          </div>
                        </td>
                        <td>
                          <div className="dash-row-stack">
                            <strong>{row.sender_name || row.sender || 'Unknown'}</strong>
                            <span>{row.sender && row.sender_name && row.sender !== row.sender_name ? row.sender : row.runner || 'N/A'}</span>
                          </div>
                        </td>
                        <td>{formatTaskOpsChannel(row.channel)}</td>
                        <td>
                          <div className="dash-row-stack">
                            <strong>{row.current_stage ? formatTaskOpsStage(row.current_stage) : 'N/A'}</strong>
                            <span>{row.backend || row.model_name || 'No trace yet'}</span>
                          </div>
                        </td>
                        <td>
                          <TaskOpsStatusBadge status={row.status} isRunningLong={row.is_running_long} />
                        </td>
                        <td>{formatTaskOpsDuration(row.duration_seconds)}</td>
                        <td>{formatTaskOpsDateTime(row.started_at)}</td>
                        <td>
                          <button type="button" className="dash-btn dash-btn-secondary" onClick={() => onSelectRow(row)}>
                            Open
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}

            <div className="dash-pagination">
              <span>
                Page {taskOps.page} of {totalPages} · {formatNumber(taskOps.total_rows)} runs
              </span>
              <div className="dash-pagination-actions">
                <button type="button" className="dash-btn dash-btn-secondary" disabled={!canPrev} onClick={onPrevPage}>
                  Previous
                </button>
                <button type="button" className="dash-btn dash-btn-secondary" disabled={!canNext} onClick={onNextPage}>
                  Next
                </button>
              </div>
            </div>
          </>
        ) : null}
      </Section>
    </>
  );
}

function DashboardPage() {
  const [activeView, setActiveView] = useState('business');
  const [range, setRange] = useState('30d');
  const [refreshTick, setRefreshTick] = useState(0);
  const [session, setSession] = useState(null);
  const [dashboardLoading, setDashboardLoading] = useState(true);
  const [taskOpsLoading, setTaskOpsLoading] = useState(false);
  const [dashboardError, setDashboardError] = useState('');
  const [taskOpsError, setTaskOpsError] = useState('');
  const [dashboard, setDashboard] = useState(null);
  const [taskOps, setTaskOps] = useState(null);
  const [taskOpsStatusFilter, setTaskOpsStatusFilter] = useState('');
  const [taskOpsChannelFilter, setTaskOpsChannelFilter] = useState('');
  const [taskOpsSearchDraft, setTaskOpsSearchDraft] = useState('');
  const [taskOpsPage, setTaskOpsPage] = useState(1);
  const [selectedTaskRun, setSelectedTaskRun] = useState(null);
  const deferredTaskOpsSearch = useDeferredValue(taskOpsSearchDraft.trim());

  useEffect(() => {
    document.title = 'DoWhiz Internal Dashboard';

    let robots = document.querySelector('meta[name="robots"]');
    if (!robots) {
      robots = document.createElement('meta');
      robots.setAttribute('name', 'robots');
      document.head.appendChild(robots);
    }
    const previousRobots = robots.getAttribute('content');
    robots.setAttribute('content', 'noindex, nofollow');

    return () => {
      if (previousRobots) {
        robots.setAttribute('content', previousRobots);
      } else {
        robots.removeAttribute('content');
      }
    };
  }, []);

  useEffect(() => {
    if (activeView !== 'business') {
      return undefined;
    }

    let cancelled = false;

    const loadDashboard = async () => {
      setDashboardLoading(true);
      setDashboardError('');

      const {
        data: { session: currentSession }
      } = await supabase.auth.getSession();

      if (cancelled) return;

      setSession(currentSession ?? null);
      if (!currentSession) {
        setDashboard(null);
        setDashboardLoading(false);
        return;
      }

      try {
        const res = await fetch(`${getDoWhizApiBaseUrl()}/analytics/dashboard?range=${encodeURIComponent(range)}`, {
          headers: {
            Authorization: `Bearer ${currentSession.access_token}`
          }
        });

        if (!res.ok) {
          const body = await res.json().catch(() => ({}));
          const message =
            body.error ||
            (res.status === 403
              ? 'This dashboard is admin-only. Your authenticated user is not allowlisted.'
              : 'Failed to load dashboard data.');
          throw new Error(message);
        }

        const payload = await res.json();
        if (!cancelled) {
          setDashboard(payload);
        }
      } catch (fetchError) {
        if (!cancelled) {
          setDashboard(null);
          setDashboardError(fetchError instanceof Error ? fetchError.message : 'Failed to load dashboard data.');
        }
      } finally {
        if (!cancelled) {
          setDashboardLoading(false);
        }
      }
    };

    loadDashboard();

    return () => {
      cancelled = true;
    };
  }, [activeView, range, refreshTick]);

  useEffect(() => {
    if (activeView !== 'task_ops') {
      return undefined;
    }

    let cancelled = false;

    const loadTaskOps = async () => {
      setTaskOpsLoading(true);
      setTaskOpsError('');

      const {
        data: { session: currentSession }
      } = await supabase.auth.getSession();

      if (cancelled) return;

      setSession(currentSession ?? null);
      if (!currentSession) {
        setTaskOps(null);
        setTaskOpsLoading(false);
        return;
      }

      try {
        const params = new URLSearchParams({ range, page: String(taskOpsPage) });
        if (taskOpsStatusFilter) params.set('status', taskOpsStatusFilter);
        if (taskOpsChannelFilter) params.set('channel', taskOpsChannelFilter);
        if (deferredTaskOpsSearch) params.set('q', deferredTaskOpsSearch);

        const res = await fetch(`${getDoWhizApiBaseUrl()}/analytics/task-ops?${params.toString()}`, {
          headers: {
            Authorization: `Bearer ${currentSession.access_token}`
          }
        });

        if (!res.ok) {
          const body = await res.json().catch(() => ({}));
          const message =
            body.error ||
            (res.status === 403
              ? 'This dashboard is admin-only. Your authenticated user is not allowlisted.'
              : 'Failed to load task operations.');
          throw new Error(message);
        }

        const payload = await res.json();
        if (!cancelled) {
          setTaskOps(payload);
        }
      } catch (fetchError) {
        if (!cancelled) {
          setTaskOps(null);
          setTaskOpsError(fetchError instanceof Error ? fetchError.message : 'Failed to load task operations.');
        }
      } finally {
        if (!cancelled) {
          setTaskOpsLoading(false);
        }
      }
    };

    loadTaskOps();

    return () => {
      cancelled = true;
    };
  }, [
    activeView,
    deferredTaskOpsSearch,
    range,
    refreshTick,
    taskOpsChannelFilter,
    taskOpsPage,
    taskOpsStatusFilter
  ]);

  const generatedAtLabel = useMemo(() => {
    const activePayload = activeView === 'task_ops' ? taskOps : dashboard;
    if (!activePayload?.generated_at) {
      return null;
    }
    return new Date(activePayload.generated_at).toLocaleString();
  }, [activeView, dashboard, taskOps]);

  const activeRangeSummary = activeView === 'task_ops' ? taskOps?.range : dashboard?.range;
  const activeLoading = activeView === 'task_ops' ? taskOpsLoading : dashboardLoading;

  useEffect(() => {
    setSelectedTaskRun(null);
  }, [activeView, taskOpsPage, taskOpsStatusFilter, taskOpsChannelFilter, deferredTaskOpsSearch]);

  useEffect(() => {
    setTaskOpsPage(1);
  }, [range]);

  if (!activeLoading && !session) {
    return (
      <div className="dash-shell">
        <div className="dash-panel dash-auth-required">
          <h1>Internal Analytics Dashboard</h1>
          <p>You need a signed-in DoWhiz account to access this page.</p>
          <a className="dash-btn" href="/auth/index.html?loggedIn=true">
            Sign in to continue
          </a>
        </div>
      </div>
    );
  }

  return (
    <div className="dash-shell">
      <div className="dash-panel">
        <header className="dash-header">
          <div>
            <h1>DoWhiz Internal Dashboard</h1>
            <p>
              {activeView === 'task_ops'
                ? 'A global operator view of user-visible task runs, with sender context, live stage status, duration, and failure detail.'
                : 'End-to-end funnel visibility from first touch to paid conversion. Revenue here reflects purchased credits in the selected date range.'}
            </p>
          </div>
          <div className="dash-header-controls">
            <label htmlFor="range-select">Date range</label>
            <select id="range-select" value={range} onChange={(event) => setRange(event.target.value)}>
              {RANGE_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
            <button type="button" className="dash-btn" onClick={() => setRefreshTick((tick) => tick + 1)}>
              Refresh
            </button>
          </div>
        </header>

        <div className="dash-meta-row">
          <span>
            Signed in as <strong>{session?.user?.email || 'unknown'}</strong>
          </span>
          {activeRangeSummary ? (
            <span>
              Window: {new Date(activeRangeSummary.start).toLocaleDateString()} to{' '}
              {new Date(activeRangeSummary.end).toLocaleDateString()} ({activeRangeSummary.days}d)
            </span>
          ) : null}
          {generatedAtLabel ? <span>Generated: {generatedAtLabel}</span> : null}
        </div>

        <div className="dash-view-tabs" role="tablist" aria-label="Dashboard views">
          {DASHBOARD_VIEWS.map((view) => (
            <button
              key={view.value}
              type="button"
              className={`dash-view-tab ${activeView === view.value ? 'is-active' : ''}`}
              onClick={() => setActiveView(view.value)}
              role="tab"
              aria-selected={activeView === view.value}
            >
              {view.label}
            </button>
          ))}
        </div>

        {activeView === 'business' && dashboardLoading ? <EmptyState label="Loading analytics data..." /> : null}
        {activeView === 'business' && dashboardError ? <div className="dash-error">{dashboardError}</div> : null}

        {activeView === 'business' && !dashboardLoading && !dashboardError && dashboard ? (
          <>
            <Section title="Executive KPI Row" subtitle="Top-line conversion, activation, paid, and retention indicators.">
              <div className="dash-kpi-grid">
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Unique visitors" help={METRIC_HELP.unique_visitors} />
                  </h3>
                  <p>{formatNumber(dashboard.kpis.unique_visitors)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Signup conversion" help={METRIC_HELP.signup_conversion} />
                  </h3>
                  <p>{formatPercent(dashboard.kpis.signup_conversion_rate)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Signup -> activation" help={METRIC_HELP.signup_to_activation} />
                  </h3>
                  <p>{formatOptionalPercent(dashboard.kpis.signup_to_activation_rate ?? dashboard.kpis.activation_rate)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Activation -> paid conversion" help={METRIC_HELP.activation_to_paid} />
                  </h3>
                  <p>{formatPercent(dashboard.kpis.activation_to_paid_rate)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Visitor -> paid conversion" help={METRIC_HELP.visitor_to_paid} />
                  </h3>
                  <p>{formatOptionalPercent(dashboard.kpis.visitor_to_paid_rate)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Time to first value" help={METRIC_HELP.time_to_first_value} />
                  </h3>
                  <p>{formatHours(dashboard.kpis.median_time_to_first_value_hours)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="D7 retention" help={METRIC_HELP.d7_retention} />
                  </h3>
                  <p>{formatPercent(dashboard.kpis.d7_retention_rate)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Repeat-value rate" help={METRIC_HELP.repeat_value_rate} />
                  </h3>
                  <p>{formatOptionalPercent(dashboard.kpis.repeat_value_rate)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Active workspaces" help={METRIC_HELP.active_workspaces} />
                  </h3>
                  <p>{formatNumber(dashboard.kpis.active_workspaces)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Paid accounts (30d)" help={METRIC_HELP.paid_accounts_30d} />
                  </h3>
                  <p>{formatNumber(dashboard.kpis.active_paid_accounts_30d)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Revenue (range)" help={METRIC_HELP.revenue_range} />
                  </h3>
                  <p>{formatCurrency(dashboard.kpis.revenue_usd)}</p>
                </article>
              </div>
            </Section>

            <Section title="Main Funnel" subtitle="Ordered funnel from first visit to active paid state.">
              {!dashboard.funnel?.steps?.length ? (
                <EmptyState label="No funnel events in selected range." />
              ) : (
                <div className="dash-funnel">
                  {dashboard.funnel.steps.map((step, stepIndex) => (
                    <article className="dash-funnel-step" key={step.event_name}>
                      <div className="dash-funnel-title-row">
                        <h3>
                          <MetricHeading
                            label={step.label}
                            help={FUNNEL_STEP_HELP[step.event_name] || 'Funnel step count for this event.'}
                          />
                        </h3>
                        <span>{formatNumber(step.identities)}</span>
                      </div>
                      <div className="dash-funnel-bar" aria-hidden="true">
                        <div
                          className="dash-funnel-bar-fill"
                          style={{ width: `${(clampPercent(step.overall_conversion_rate) * 100).toFixed(1)}%` }}
                        />
                      </div>
                      <div className="dash-funnel-metrics">
                        <span>
                          <MetricHeading
                            label={`Step conversion: ${formatPercent(step.step_conversion_rate)}`}
                            help={
                              stepIndex === 0
                                ? 'For the first funnel step this is fixed at 100%.'
                                : METRIC_HELP.funnel_step_conversion
                            }
                          />
                        </span>
                        <span>
                          <MetricHeading
                            label={`Overall conversion: ${formatPercent(step.overall_conversion_rate)}`}
                            help={METRIC_HELP.funnel_overall_conversion}
                          />
                        </span>
                      </div>
                    </article>
                  ))}
                </div>
              )}
            </Section>

            <Section
              title="Acquisition Breakdown"
              subtitle="Compare source/campaign, referrer, and device segments through signup, activation, and paid conversion."
            >
              <div className="dash-grid-2">
                <AcquisitionTable rows={dashboard.acquisition?.by_source_campaign} title="UTM source / medium / campaign" />
                <AcquisitionTable rows={dashboard.acquisition?.by_referrer} title="Referrer" />
                <AcquisitionTable rows={dashboard.acquisition?.by_device_type} title="Device type" />
                <AcquisitionTable rows={dashboard.acquisition?.by_landing_variant} title="Landing page variant" />
              </div>
            </Section>

            <Section
              title="Activation Breakdown"
              subtitle="Onboarding behavior that correlates with first task success and deeper usage."
            >
              <div className="dash-grid-2">
                <div className="dash-card">
                  <h3>
                    <MetricHeading
                      label="Signup auth method"
                      help="Breakdown of signup_completed identities by auth_method captured at signup."
                    />
                  </h3>
                  <BreakdownTable
                    rows={dashboard.activation?.by_auth_method}
                    firstCol="Auth method"
                    firstColHelp="Authentication method recorded on signup_completed."
                    rateColHelp="Auth method identities / all signup_completed identities."
                  />
                </div>
                <div className="dash-card">
                  <h3>
                    <MetricHeading
                      label="Workspace type"
                      help="Breakdown of signup_completed identities by workspace_type from workspace_created."
                    />
                  </h3>
                  <BreakdownTable
                    rows={dashboard.activation?.by_workspace_type}
                    firstCol="Workspace"
                    firstColHelp="Workspace type captured on workspace_created."
                    rateColHelp="Workspace type identities / all signup_completed identities."
                  />
                </div>
                <div className="dash-card">
                  <h3>
                    <MetricHeading
                      label="Connected channel / tool type"
                      help="Breakdown of first connected channel/tool type among signup_completed identities."
                    />
                  </h3>
                  <BreakdownTable
                    rows={dashboard.activation?.by_connected_channel_type}
                    firstCol="Channel/Tool"
                    firstColHelp="Channel/tool type captured from connection events."
                    rateColHelp="Channel/tool identities / all signup_completed identities."
                  />
                </div>
                <div className="dash-card">
                  <h3>
                    <MetricHeading
                      label="First task type"
                      help="Breakdown of first task type observed among signup_completed identities."
                    />
                  </h3>
                  <BreakdownTable
                    rows={dashboard.activation?.by_first_task_type}
                    firstCol="Task type"
                    firstColHelp="Task type captured on first_task_started or task_started."
                    rateColHelp="Task-type identities / all signup_completed identities."
                  />
                </div>
              </div>
              <div className="dash-rate-row">
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading
                      label="Agent/workflow creation rate"
                      help={METRIC_HELP.agent_or_workflow_creation_rate}
                    />
                  </h3>
                  <p>{formatPercent(dashboard.activation?.agent_or_workflow_creation_rate || 0)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading
                      label="Multi-channel connection rate"
                      help={METRIC_HELP.multi_channel_connection_rate}
                    />
                  </h3>
                  <p>{formatPercent(dashboard.activation?.multi_channel_connection_rate || 0)}</p>
                </article>
              </div>
            </Section>

            <Section title="Monetization" subtitle="Upgrade intent, checkout flow, successful payment, and paid state activation.">
              <div className="dash-kpi-grid">
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Pricing page views" help={METRIC_HELP.pricing_page_views} />
                  </h3>
                  <p>{formatDeferredAwareCount(dashboard.monetization?.pricing_page_views, dashboard, ['pricing_page_view'])}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Upgrade clicks" help={METRIC_HELP.upgrade_clicks} />
                  </h3>
                  <p>{formatNumber(dashboard.monetization?.upgrade_clicks || 0)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Paywall views" help={METRIC_HELP.paywall_views} />
                  </h3>
                  <p>{formatNumber(dashboard.monetization?.paywall_views || 0)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Checkout starts" help={METRIC_HELP.checkout_starts} />
                  </h3>
                  <p>{formatNumber(dashboard.monetization?.checkout_starts || 0)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Checkout abandon rate" help={METRIC_HELP.checkout_abandon_rate} />
                  </h3>
                  <p>{formatPercent(dashboard.monetization?.checkout_abandon_rate || 0)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Payment succeeded" help={METRIC_HELP.payment_succeeded} />
                  </h3>
                  <p>{formatNumber(dashboard.monetization?.payment_succeeded || 0)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Subscription activated" help={METRIC_HELP.subscription_activated} />
                  </h3>
                  <p>{formatNumber(dashboard.monetization?.subscription_activated || 0)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Subscription renewals" help={METRIC_HELP.subscription_renewals} />
                  </h3>
                  <p>
                    {formatDeferredAwareCount(dashboard.monetization?.subscription_renewed, dashboard, [
                      'subscription_renewed'
                    ])}
                  </p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Subscription canceled" help={METRIC_HELP.subscription_canceled} />
                  </h3>
                  <p>
                    {formatDeferredAwareCount(dashboard.monetization?.subscription_canceled, dashboard, [
                      'subscription_canceled'
                    ])}
                  </p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Trial -> paid rate" help={METRIC_HELP.trial_to_paid_rate} />
                  </h3>
                  <p>{formatOptionalPercent(dashboard.monetization?.trial_to_paid_rate)}</p>
                </article>
              </div>

              <div className="dash-card">
                <h3>
                  <MetricHeading label="Plan mix" help="Distribution of payment/subscription events by plan type." />
                </h3>
                <BreakdownTable
                  rows={dashboard.monetization?.plan_mix}
                  firstCol="Plan"
                  firstColHelp="Plan type from payment/subscription events."
                  rateColHelp="Plan event count / max(payment_succeeded, subscription_activated) events."
                />
              </div>
            </Section>

            <Section title="Retention and Cohorts" subtitle="D1/D7/D30 retention, repeat-value behavior, stickiness, and weekly cohorts.">
              <div className="dash-kpi-grid">
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="D1 retention" help={METRIC_HELP.d1_retention} />
                  </h3>
                  <p>{formatPercent(dashboard.retention?.d1_retention_rate || 0)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="D7 retention" help={METRIC_HELP.d7_retention} />
                  </h3>
                  <p>{formatPercent(dashboard.retention?.d7_retention_rate || 0)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="D30 retention" help={METRIC_HELP.d30_retention} />
                  </h3>
                  <p>{formatPercent(dashboard.retention?.d30_retention_rate || 0)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Repeat successful task rate" help={METRIC_HELP.repeat_successful_task_rate} />
                  </h3>
                  <p>{formatPercent(dashboard.retention?.repeat_successful_task_rate || 0)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="DAU / WAU" help={METRIC_HELP.dau_wau} />
                  </h3>
                  <p>{formatPercent(dashboard.retention?.stickiness?.dau_wau_ratio || 0)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="DAU / MAU" help={METRIC_HELP.dau_mau} />
                  </h3>
                  <p>{formatPercent(dashboard.retention?.stickiness?.dau_mau_ratio || 0)}</p>
                </article>
              </div>

              <div className="dash-grid-2">
                <TrendBars
                  points={dashboard.retention?.active_users_trend}
                  title="Active users trend"
                  titleHelp={METRIC_HELP.active_users_trend}
                />
                <TrendBars
                  points={dashboard.retention?.active_workspaces_trend}
                  title="Active workspaces trend"
                  titleHelp={METRIC_HELP.active_workspaces_trend}
                />
              </div>

              <div className="dash-card">
                <h3>
                  <MetricHeading
                    label="Weekly cohorts"
                    help="Each row groups users by signup week and shows retention rates at D1, D7, and D30."
                  />
                </h3>
                {!dashboard.retention?.cohorts?.length ? (
                  <EmptyState label="No cohorts in selected range." />
                ) : (
                  <div className="dash-table-wrap">
                    <table className="dash-table">
                      <thead>
                        <tr>
                          <th>Cohort week</th>
                          <th>
                            <MetricHeading label="Users" help={METRIC_HELP.cohort_users} />
                          </th>
                          <th>
                            <MetricHeading label="D1" help={METRIC_HELP.retention_rate_col} />
                          </th>
                          <th>
                            <MetricHeading label="D7" help={METRIC_HELP.retention_rate_col} />
                          </th>
                          <th>
                            <MetricHeading label="D30" help={METRIC_HELP.retention_rate_col} />
                          </th>
                        </tr>
                      </thead>
                      <tbody>
                        {dashboard.retention.cohorts.map((cohort) => (
                          <tr key={cohort.cohort_week}>
                            <td>{cohort.cohort_week}</td>
                            <td>{formatNumber(cohort.users)}</td>
                            <td>{formatPercent(cohort.d1_retention_rate)}</td>
                            <td>{formatPercent(cohort.d7_retention_rate)}</td>
                            <td>{formatPercent(cohort.d30_retention_rate)}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </div>
            </Section>

            <Section title="Reliability" subtitle="Task delivery quality, error rates, latency hotspots, and top failure reasons.">
              <div className="dash-kpi-grid">
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Task success rate" help={METRIC_HELP.task_success_rate} />
                  </h3>
                  <p>{formatPercent(dashboard.reliability?.task_success_rate || 0)}</p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="API error rate" help={METRIC_HELP.api_error_rate} />
                  </h3>
                  <p>
                    {dashboard.reliability?.api_error_rate === null || dashboard.reliability?.api_error_rate === undefined
                      ? 'N/A'
                      : formatPercent(dashboard.reliability.api_error_rate)}
                  </p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Integration failure rate" help={METRIC_HELP.integration_failure_rate} />
                  </h3>
                  <p>
                    {dashboard.reliability?.integration_failure_rate === null ||
                    dashboard.reliability?.integration_failure_rate === undefined
                      ? 'N/A'
                      : formatPercent(dashboard.reliability.integration_failure_rate)}
                  </p>
                </article>
                <article className="dash-kpi-card">
                  <h3>
                    <MetricHeading label="Checkout failure rate" help={METRIC_HELP.checkout_failure_rate} />
                  </h3>
                  <p>
                    {dashboard.reliability?.checkout_failure_rate === null ||
                    dashboard.reliability?.checkout_failure_rate === undefined
                      ? 'N/A'
                      : formatPercent(dashboard.reliability.checkout_failure_rate)}
                  </p>
                </article>
              </div>

              <div className="dash-grid-2">
                <div className="dash-card">
                  <h3>
                    <MetricHeading
                      label="Slowest endpoints / workflows"
                      help="Latency summary for endpoints/workflows ranked by highest average latency."
                    />
                  </h3>
                  {!dashboard.reliability?.slowest_endpoints_or_workflows?.length ? (
                    <EmptyState label="No latency metrics recorded yet." />
                  ) : (
                    <div className="dash-table-wrap">
                      <table className="dash-table">
                        <thead>
                          <tr>
                            <th>Endpoint / workflow</th>
                            <th>
                              <MetricHeading label="Avg latency (ms)" help={METRIC_HELP.avg_latency_ms} />
                            </th>
                            <th>
                              <MetricHeading label="P95 latency (ms)" help={METRIC_HELP.p95_latency_ms} />
                            </th>
                          </tr>
                        </thead>
                        <tbody>
                          {dashboard.reliability.slowest_endpoints_or_workflows.map((item) => (
                            <tr key={item.key}>
                              <td>{item.key}</td>
                              <td>{Math.round(item.avg_latency_ms)}</td>
                              <td>{Math.round(item.p95_latency_ms)}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                </div>

                <div className="dash-card">
                  <h3>
                    <MetricHeading
                      label="Top failure reasons"
                      help="Most frequent error reasons aggregated across failure event types in the selected window."
                    />
                  </h3>
                  {!dashboard.reliability?.top_failure_reasons?.length ? (
                    <EmptyState label="No failure reasons in selected range." />
                  ) : (
                    <div className="dash-table-wrap">
                      <table className="dash-table">
                        <thead>
                          <tr>
                            <th>Reason</th>
                            <th>
                              <MetricHeading label="Count" help="Number of events mapped to this failure reason." />
                            </th>
                          </tr>
                        </thead>
                        <tbody>
                          {dashboard.reliability.top_failure_reasons.map((item) => (
                            <tr key={item.reason}>
                              <td>{item.reason}</td>
                              <td>{formatNumber(item.count)}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                </div>
              </div>
            </Section>

            <Section title="Metric Definitions" subtitle="Formulas used by this dashboard for trust and consistency.">
              {!dashboard.metric_definitions?.length ? (
                <EmptyState label="Metric definitions unavailable." />
              ) : (
                <div className="dash-table-wrap">
                  <table className="dash-table">
                    <thead>
                      <tr>
                        <th>Metric</th>
                        <th>Formula</th>
                      </tr>
                    </thead>
                    <tbody>
                      {dashboard.metric_definitions.map((row) => (
                        <tr key={row.metric}>
                          <td>{row.metric}</td>
                          <td>{row.formula}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </Section>

            <Section title="Event Taxonomy" subtitle="Implemented and deferred events with trigger, properties, source, and status.">
              {!dashboard.taxonomy?.length ? (
                <EmptyState label="Taxonomy unavailable." />
              ) : (
                <div className="dash-table-wrap">
                  <table className="dash-table">
                    <thead>
                      <tr>
                        <th>Category</th>
                        <th>Event</th>
                        <th>Trigger</th>
                        <th>Required properties</th>
                        <th>Optional properties</th>
                        <th>Source</th>
                        <th>Status</th>
                      </tr>
                    </thead>
                    <tbody>
                      {dashboard.taxonomy.map((row) => (
                        <tr key={`${row.category}-${row.event_name}`}>
                          <td>{row.category}</td>
                          <td>{row.event_name}</td>
                          <td>{row.trigger}</td>
                          <td>{(row.required_properties || []).join(', ')}</td>
                          <td>{(row.optional_properties || []).join(', ')}</td>
                          <td>{row.emitted_from}</td>
                          <td>{row.status}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </Section>
          </>
        ) : null}

        {activeView === 'task_ops' ? (
          <TaskOpsView
            taskOps={taskOps}
            loading={taskOpsLoading}
            error={taskOpsError}
            statusFilter={taskOpsStatusFilter}
            channelFilter={taskOpsChannelFilter}
            searchDraft={taskOpsSearchDraft}
            onStatusChange={(value) => {
              setTaskOpsStatusFilter(value);
              setTaskOpsPage(1);
            }}
            onChannelChange={(value) => {
              setTaskOpsChannelFilter(value);
              setTaskOpsPage(1);
            }}
            onSearchChange={(value) => {
              setTaskOpsSearchDraft(value);
              setTaskOpsPage(1);
            }}
            onPrevPage={() => setTaskOpsPage((page) => Math.max(1, page - 1))}
            onNextPage={() => setTaskOpsPage((page) => page + 1)}
            onSelectRow={setSelectedTaskRun}
          />
        ) : null}
      </div>
      <TaskOpsDetailsModal row={selectedTaskRun} onClose={() => setSelectedTaskRun(null)} />
    </div>
  );
}

export default DashboardPage;
