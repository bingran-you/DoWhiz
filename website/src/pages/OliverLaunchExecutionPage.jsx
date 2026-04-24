import { useState } from 'react';
import { Link } from 'react-router-dom';

import { analyzeLaunchExecution, formatLaunchDate, formatReadinessLabel, getReadinessTone } from '../domain/launchExecutionApi';
import { launchExecutionDemo } from '../data/launchExecutionDemo';

function LaunchReadinessPill({ status }) {
  const tone = getReadinessTone(status);
  return <span className={`launch-readiness-pill is-${tone}`}>{formatReadinessLabel(status)}</span>;
}

function LaunchList({ items, emptyLabel, renderItem }) {
  if (!items?.length) {
    return <p className="launch-empty-state">{emptyLabel}</p>;
  }

  return (
    <div className="launch-list">
      {items.map((item, index) => (
        <article
          key={`${item.title || item.name || item.target || item.label || 'item'}-${index}`}
          className="launch-list-item"
        >
          {renderItem(item)}
        </article>
      ))}
    </div>
  );
}

function LaunchExecutionPage() {
  const [sourceLabel, setSourceLabel] = useState('');
  const [contextText, setContextText] = useState('');
  const [updateText, setUpdateText] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [response, setResponse] = useState(null);

  const plan = response?.launch_execution_plan || null;
  const trackerRows = response?.tracker_rows || [];
  const readinessBrief = response?.readiness_brief || null;

  async function handleAnalyze(event) {
    event.preventDefault();
    setLoading(true);
    setError('');

    try {
      const result = await analyzeLaunchExecution({
        sourceLabel,
        contextText,
        updateText,
        priorPlan: plan
      });
      setResponse(result);
    } catch (requestError) {
      setError(requestError.message);
    } finally {
      setLoading(false);
    }
  }

  function loadDemoThread() {
    setSourceLabel(launchExecutionDemo.sourceLabel);
    setContextText(launchExecutionDemo.contextText);
    setUpdateText('');
    setError('');
  }

  return (
    <main className="route-shell launch-execution-shell">
      <div className="route-card launch-execution-card">
        <header className="launch-hero">
          <div className="launch-hero-copy">
            <p className="route-kicker">Oliver v1</p>
            <h1>Turn one messy launch thread into accountable execution.</h1>
            <p className="launch-hero-summary">
              Paste one launch-related thread or planning bundle. Oliver extracts owners, dates,
              dependencies, blockers, open decisions, and a live readiness brief backed by
              evidence.
            </p>
            <p className="workspace-inline-note">
              Narrow v1 scope: pasted thread or message bundle only. No autonomous outreach, no
              generic workflow platform, no broad “AI employee” positioning.
            </p>
          </div>

          <div className="launch-hero-aside">
            <div className="launch-hero-note">
              <span>Start</span>
              <strong>Thread-native</strong>
            </div>
            <div className="launch-hero-note">
              <span>Output</span>
              <strong>Execution layer + readiness brief</strong>
            </div>
            <div className="launch-hero-note">
              <span>Loop</span>
              <strong>Paste updates, refresh the brief</strong>
            </div>
          </div>
        </header>

        <div className="route-actions">
          <Link className="btn btn-secondary" to="/oliver">
            View Oliver positioning
          </Link>
          <button type="button" className="btn btn-primary" onClick={loadDemoThread}>
            Load demo launch thread
          </button>
        </div>

        <section className="launch-input-layout">
          <form className="launch-input-card" onSubmit={handleAnalyze}>
            <div className="launch-field">
              <label htmlFor="launch-source-label">Launch label</label>
              <input
                id="launch-source-label"
                value={sourceLabel}
                onChange={(event) => setSourceLabel(event.target.value)}
                placeholder="Q2 launch thread, June webinar launch, release war room..."
              />
            </div>

            <div className="launch-field">
              <label htmlFor="launch-context">Pasted launch thread or planning bundle</label>
              <textarea
                id="launch-context"
                value={contextText}
                onChange={(event) => setContextText(event.target.value)}
                placeholder="Paste the thread, planning note, or message bundle here."
                required
              />
            </div>

            <div className="launch-field">
              <label htmlFor="launch-update">Latest update for refresh loop (optional)</label>
              <textarea
                id="launch-update"
                value={updateText}
                onChange={(event) => setUpdateText(event.target.value)}
                placeholder="Paste the newest owner reply or status update when you want Oliver to refresh the plan."
              />
            </div>

            {error ? <p className="launch-error-banner">{error}</p> : null}

            <div className="launch-submit-row">
              <button type="submit" className="btn btn-primary" disabled={loading}>
                {loading
                  ? 'Analyzing launch...'
                  : plan
                    ? 'Refresh readiness brief'
                    : 'Build launch execution plan'}
              </button>
              <p className="launch-submit-note">
                {plan
                  ? 'The existing plan is sent back with the latest update so Oliver can show what changed.'
                  : 'First run extracts the plan, tracker, follow-ups, and readiness brief.'}
              </p>
            </div>
          </form>

          <aside className="launch-scope-card">
            <h2>Readiness model</h2>
            <div className="launch-model-list">
              <div>
                <strong>Green</strong>
                <p>Owners and dates are present, no critical blockers remain, and the evidence is credible.</p>
              </div>
              <div>
                <strong>Yellow</strong>
                <p>There are missing updates, at-risk dependencies, unresolved follow-ups, or timeline gaps.</p>
              </div>
              <div>
                <strong>Red</strong>
                <p>There is no credible evidence, a critical blocker is open, a launch-blocking decision is unresolved, or the critical path lacks ownership.</p>
              </div>
            </div>

            <div className="launch-scope-divider"></div>

            <h2>What this v1 does</h2>
            <div className="launch-model-list">
              <div>
                <strong>Extract</strong>
                <p>Launch goal, target date or window, milestones, owners, risks, dependencies, and decisions.</p>
              </div>
              <div>
                <strong>Track</strong>
                <p>Critical path candidates, missing updates, missing dates, unresolved blockers, and follow-up prompts.</p>
              </div>
              <div>
                <strong>Refresh</strong>
                <p>Paste the newest update and Oliver will refresh the plan and call out what changed.</p>
              </div>
            </div>
          </aside>
        </section>

        {plan ? (
          <>
            <section className="launch-summary-card">
              <div className="launch-summary-head">
                <div>
                  <p className="route-kicker">Launch execution plan</p>
                  <h2>{plan.title}</h2>
                  <p className="launch-summary-objective">
                    {plan.objective || 'Objective not explicitly stated in the source context.'}
                  </p>
                </div>
                <div className="launch-summary-status">
                  <LaunchReadinessPill status={plan.readiness_status} />
                  <span>{formatLaunchDate(plan.target_date, plan.launch_window)}</span>
                  <span>{plan.last_updated_at}</span>
                </div>
              </div>

              <div className="launch-summary-grid">
                <div className="launch-summary-cell">
                  <span>Why this status</span>
                  <strong>{plan.readiness_reason}</strong>
                </div>
                <div className="launch-summary-cell">
                  <span>Source</span>
                  <strong>{plan.source_context?.source_label || 'Pasted thread'}</strong>
                </div>
                <div className="launch-summary-cell">
                  <span>Evidence</span>
                  <strong>{plan.evidence?.length || 0} supporting snippets</strong>
                </div>
                <div className="launch-summary-cell">
                  <span>Follow-ups</span>
                  <strong>{plan.follow_up_items?.length || 0} next asks ready</strong>
                </div>
              </div>
            </section>

            <section className="launch-results-grid">
              <article className="launch-section-card">
                <div className="launch-section-head">
                  <h2>Milestones</h2>
                  <p>Execution layer extracted from the thread.</p>
                </div>
                <LaunchList
                  items={plan.milestones}
                  emptyLabel="No milestones were clearly grounded in the source context."
                  renderItem={(item) => (
                    <>
                      <div className="launch-item-row">
                        <strong>{item.title}</strong>
                        <span className={`launch-mini-pill is-${item.status}`}>{item.status.replace('_', ' ')}</span>
                      </div>
                      <p>
                        Owner: {item.owner || 'Unassigned'}
                        {' · '}
                        Date: {item.target_date || 'Needs date'}
                        {item.critical_path ? ' · Critical path' : ''}
                      </p>
                      {item.notes ? <p>{item.notes}</p> : null}
                    </>
                  )}
                />
              </article>

              <article className="launch-section-card">
                <div className="launch-section-head">
                  <h2>Critical path</h2>
                  <p>The few items most likely to move the launch date.</p>
                </div>
                <LaunchList
                  items={plan.critical_path}
                  emptyLabel="No critical path candidate was explicitly extracted yet."
                  renderItem={(item) => (
                    <>
                      <div className="launch-item-row">
                        <strong>{item.title}</strong>
                        <span className="launch-mini-pill is-critical">{item.item_type}</span>
                      </div>
                      <p>
                        Owner: {item.owner || 'Unassigned'}
                        {' · '}
                        Date: {item.target_date || 'Needs date'}
                        {' · '}
                        Status: {item.status}
                      </p>
                      {item.reason ? <p>{item.reason}</p> : null}
                    </>
                  )}
                />
              </article>

              <article className="launch-section-card launch-tracker-card">
                <div className="launch-section-head">
                  <h2>Owner / date / risk tracker</h2>
                  <p>One compact view of what needs attention.</p>
                </div>
                {trackerRows.length ? (
                  <div className="launch-tracker-table" role="table" aria-label="Launch tracker">
                    <div className="launch-tracker-row launch-tracker-head" role="row">
                      <span>Type</span>
                      <span>Item</span>
                      <span>Owner</span>
                      <span>Date</span>
                      <span>Status</span>
                      <span>Risk</span>
                    </div>
                    {trackerRows.map((row, index) => (
                      <div key={`${row.category}-${row.title}-${index}`} className="launch-tracker-row" role="row">
                        <span>{row.category}</span>
                        <strong>{row.title}</strong>
                        <span>{row.owner || 'Unassigned'}</span>
                        <span>{row.target_date || 'Needs date'}</span>
                        <span>{row.status}</span>
                        <span>{row.risk_level || 'n/a'}</span>
                      </div>
                    ))}
                  </div>
                ) : (
                  <p className="launch-empty-state">Tracker rows will appear once execution items are extracted.</p>
                )}
              </article>

              <article className="launch-section-card">
                <div className="launch-section-head">
                  <h2>Dependencies</h2>
                  <p>Cross-functional or external items that can gate launch.</p>
                </div>
                <LaunchList
                  items={plan.dependencies}
                  emptyLabel="No dependencies were explicitly grounded in the source context."
                  renderItem={(item) => (
                    <>
                      <div className="launch-item-row">
                        <strong>{item.title}</strong>
                        <span className={`launch-mini-pill is-${item.status}`}>{item.status.replace('_', ' ')}</span>
                      </div>
                      <p>
                        Owner: {item.owner || 'Unassigned'}
                        {' · '}
                        Date: {item.target_date || 'Needs date'}
                        {item.critical_path ? ' · Critical path' : ''}
                      </p>
                      {item.notes ? <p>{item.notes}</p> : null}
                    </>
                  )}
                />
              </article>

              <article className="launch-section-card">
                <div className="launch-section-head">
                  <h2>Risks and blockers</h2>
                  <p>Explicit launch risks, not generic concerns.</p>
                </div>
                <LaunchList
                  items={plan.risks}
                  emptyLabel="No explicit risks or blockers were extracted."
                  renderItem={(item) => (
                    <>
                      <div className="launch-item-row">
                        <strong>{item.title}</strong>
                        <span className={`launch-mini-pill is-${item.severity}`}>{item.severity}</span>
                      </div>
                      <p>
                        Owner: {item.owner || 'Unassigned'}
                        {' · '}
                        Status: {item.status}
                        {item.blocker ? ' · Blocker' : ''}
                      </p>
                      {item.notes ? <p>{item.notes}</p> : null}
                    </>
                  )}
                />
              </article>

              <article className="launch-section-card">
                <div className="launch-section-head">
                  <h2>Open decisions</h2>
                  <p>Unanswered choices or approvals still needed for launch.</p>
                </div>
                <LaunchList
                  items={plan.decisions}
                  emptyLabel="No unresolved decisions were extracted."
                  renderItem={(item) => (
                    <>
                      <div className="launch-item-row">
                        <strong>{item.title}</strong>
                        <span className={`launch-mini-pill is-${item.status}`}>{item.status}</span>
                      </div>
                      <p>
                        Owner: {item.owner || 'Unassigned'}
                        {' · '}
                        Due: {item.due_date || 'Needs date'}
                        {item.launch_blocking ? ' · Launch blocking' : ''}
                      </p>
                      {item.notes ? <p>{item.notes}</p> : null}
                    </>
                  )}
                />
              </article>

              <article className="launch-section-card">
                <div className="launch-section-head">
                  <h2>Recommended follow-ups</h2>
                  <p>Generated from missing owners, missing dates, stale updates, blockers, and decisions.</p>
                </div>
                <LaunchList
                  items={plan.follow_up_items}
                  emptyLabel="No immediate follow-up prompt was generated."
                  renderItem={(item) => (
                    <>
                      <div className="launch-item-row">
                        <strong>{item.target}</strong>
                        <span className={`launch-mini-pill is-${item.priority}`}>{item.priority}</span>
                      </div>
                      <p>{item.reason}</p>
                      <div className="launch-message-box">{item.suggested_message}</div>
                    </>
                  )}
                />
              </article>

              <article className="launch-section-card">
                <div className="launch-section-head">
                  <h2>Evidence</h2>
                  <p>Visible grounding for the readiness state.</p>
                </div>
                <LaunchList
                  items={plan.evidence}
                  emptyLabel="No evidence snippets were returned."
                  renderItem={(item) => (
                    <>
                      <div className="launch-item-row">
                        <strong>{item.label}</strong>
                        <span className="launch-mini-pill is-evidence">{item.source_ref || 'context'}</span>
                      </div>
                      <div className="launch-evidence-quote">“{item.snippet}”</div>
                    </>
                  )}
                />
              </article>

              <article className="launch-section-card">
                <div className="launch-section-head">
                  <h2>Readiness brief</h2>
                  <p>Concise status readout with visible rationale.</p>
                </div>
                {readinessBrief ? (
                  <div className="launch-brief-grid">
                    <div className="launch-brief-block">
                      <span>Overall readiness</span>
                      <strong>{readinessBrief.overall_readiness}</strong>
                    </div>
                    <div className="launch-brief-block">
                      <span>Critical blockers</span>
                      <LaunchList
                        items={readinessBrief.critical_blockers.map((item) => ({ title: item }))}
                        emptyLabel="No critical blockers listed."
                        renderItem={(item) => <p>{item.title}</p>}
                      />
                    </div>
                    <div className="launch-brief-block">
                      <span>At-risk dependencies</span>
                      <LaunchList
                        items={readinessBrief.at_risk_dependencies.map((item) => ({ title: item }))}
                        emptyLabel="No at-risk dependencies listed."
                        renderItem={(item) => <p>{item.title}</p>}
                      />
                    </div>
                    <div className="launch-brief-block">
                      <span>Open decisions</span>
                      <LaunchList
                        items={readinessBrief.open_decisions.map((item) => ({ title: item }))}
                        emptyLabel="No open decisions listed."
                        renderItem={(item) => <p>{item.title}</p>}
                      />
                    </div>
                    <div className="launch-brief-block">
                      <span>Missing owner updates</span>
                      <LaunchList
                        items={readinessBrief.missing_owner_updates.map((item) => ({ title: item }))}
                        emptyLabel="No missing owner updates listed."
                        renderItem={(item) => <p>{item.title}</p>}
                      />
                    </div>
                    <div className="launch-brief-block">
                      <span>What changed since last update</span>
                      <LaunchList
                        items={readinessBrief.what_changed_since_last_update.map((item) => ({ title: item }))}
                        emptyLabel="No change summary available yet."
                        renderItem={(item) => <p>{item.title}</p>}
                      />
                    </div>
                    <div className="launch-brief-block">
                      <span>Evidence summary</span>
                      <LaunchList
                        items={readinessBrief.evidence_summary.map((item) => ({ title: item }))}
                        emptyLabel="No evidence summary available."
                        renderItem={(item) => <p>{item.title}</p>}
                      />
                    </div>
                    <div className="launch-brief-block">
                      <span>Next follow-up focus</span>
                      <strong>{readinessBrief.next_follow_up_focus || 'No follow-up target selected.'}</strong>
                    </div>
                  </div>
                ) : (
                  <p className="launch-empty-state">Readiness brief will appear after analysis.</p>
                )}
              </article>
            </section>
          </>
        ) : null}
      </div>
    </main>
  );
}

export default LaunchExecutionPage;
