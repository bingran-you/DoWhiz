import { Link } from 'react-router-dom';

import { demoWorkspace } from '../data/demoWorkspace';
import { getProvisioningLabel } from '../domain/resourceModel';
import { createWorkspaceHomeModel } from '../domain/workspaceHomeModel';

const model = createWorkspaceHomeModel(demoWorkspace.blueprint, { demoMode: true });

function toStatusClassName(status) {
  const normalized = String(status || '').toLowerCase();

  if (normalized.includes('connected')) {
    return 'workspace-status-pill is-connected';
  }
  if (normalized.includes('available')) {
    return 'workspace-status-pill is-available';
  }
  if (normalized.includes('planned')) {
    return 'workspace-status-pill is-manual';
  }
  if (normalized.includes('pending')) {
    return 'workspace-status-pill is-pending-review';
  }
  if (normalized.includes('blocked')) {
    return 'workspace-status-pill is-blocked';
  }
  if (normalized.includes('draft')) {
    return 'workspace-status-pill is-draft';
  }
  return 'workspace-status-pill';
}

function WorkspaceDemoPage() {
  return (
    <main className="route-shell route-shell-workspace">
      <div className="route-card route-card-workspace">
        <header className="workspace-demo-hero">
          <div className="workspace-demo-hero-copy">
            <p className="route-kicker">Open-source demo</p>
            <h1>{model.title}</h1>
            <p className="workspace-demo-summary">{demoWorkspace.summary}</p>
            <p className="workspace-inline-note">
              This page is the fastest supported product walkthrough for external contributors. It
              runs locally without private cloud credentials and uses the same workspace domain
              model that powers the broader startup workflow.
            </p>
          </div>

          <div className="workspace-demo-hero-aside">
            <div className="workspace-health-item">
              <span>Stage</span>
              <strong>{model.stage}</strong>
            </div>
            <div className="workspace-health-item">
              <span>Planning horizon</span>
              <strong>{model.planHorizonDays} days</strong>
            </div>
            <div className="workspace-health-item">
              <span>Readiness</span>
              <strong>{model.workspaceHealth.readinessLabel}</strong>
            </div>
          </div>
        </header>

        <div className="route-actions">
          <Link className="btn btn-primary" to="/?view=landing">
            View landing page
          </Link>
          <Link className="btn btn-secondary" to="/start">
            Open intake flow
          </Link>
        </div>

        <section className="workspace-health-row">
          <div className="workspace-health-item">
            <span>Connected resources</span>
            <strong>{model.workspaceHealth.connected}</strong>
          </div>
          <div className="workspace-health-item">
            <span>Needs setup</span>
            <strong>{model.workspaceHealth.nonConnected}</strong>
          </div>
          <div className="workspace-health-item">
            <span>Founder</span>
            <strong>{model.founderName}</strong>
          </div>
        </section>

        <section className="workspace-quick-grid">
          <article className="workspace-quick-card">
            <h2>Goals</h2>
            <ul className="workspace-list">
              {model.goals.map((goal) => (
                <li key={goal}>{goal}</li>
              ))}
            </ul>
          </article>

          <article className="workspace-quick-card">
            <h2>Current assets</h2>
            <ul className="workspace-list">
              {model.currentAssets.map((asset) => (
                <li key={asset}>{asset}</li>
              ))}
            </ul>
          </article>

          <article className="workspace-quick-card">
            <h2>Preferred channels</h2>
            <ul className="workspace-list">
              {model.preferredChannels.map((channel) => (
                <li key={channel}>{channel}</li>
              ))}
            </ul>
          </article>
        </section>

        <section className="workspace-grid">
          <article className="workspace-card">
            <div className="workspace-card-header">
              <h2>Agent roster</h2>
              <p>Founding-team ownership mapped into active and planned operator roles.</p>
            </div>
            <div className="workspace-card-body">
              <ul className="workspace-list">
                {model.agentRoster.map((agent) => (
                  <li key={`${agent.role}-${agent.name}`}>
                    <div className="workspace-list-row">
                      <div>
                        <strong>{agent.name}</strong>
                        <p>{agent.focus}</p>
                      </div>
                      <div className="workspace-row-right">
                        <span>{agent.role}</span>
                        <span className={toStatusClassName(agent.status)}>{agent.status}</span>
                      </div>
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          </article>

          <article className="workspace-card">
            <div className="workspace-card-header">
              <h2>Next actions</h2>
              <p>Human-visible actions surfaced before broad automation is enabled.</p>
            </div>
            <div className="workspace-card-body">
              <ol className="workspace-list workspace-list-ordered">
                {model.nextActions.map((action) => (
                  <li key={action}>{action}</li>
                ))}
              </ol>
            </div>
          </article>

          <article className="workspace-card">
            <div className="workspace-card-header">
              <h2>Starter tasks</h2>
              <p>Initial execution graph generated from the founder brief.</p>
            </div>
            <div className="workspace-card-body">
              <ul className="workspace-list">
                {model.starterTasks.map((task) => (
                  <li key={task.id}>
                    <div className="workspace-list-row">
                      <div>
                        <strong>{task.title}</strong>
                        <p>{task.rationale}</p>
                      </div>
                      <div className="workspace-row-right">
                        <span>{task.ownerRole}</span>
                        <span className={toStatusClassName(task.status)}>{task.status}</span>
                      </div>
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          </article>

          <article className="workspace-card">
            <div className="workspace-card-header">
              <h2>Approval queue</h2>
              <p>Human review remains explicit for sensitive or external work.</p>
            </div>
            <div className="workspace-card-body">
              <ul className="workspace-list">
                {model.approvalQueue.map((item) => (
                  <li key={item.id}>
                    <div className="workspace-list-row">
                      <div>
                        <strong>{item.title}</strong>
                        <p>{item.reason}</p>
                      </div>
                      <div className="workspace-row-right">
                        <span>{item.owner}</span>
                        <span className={toStatusClassName(item.status)}>{item.status}</span>
                      </div>
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          </article>

          <article className="workspace-card">
            <div className="workspace-card-header">
              <h2>Recent artifacts</h2>
              <p>Generated outputs stay visible and reviewable instead of disappearing into chats.</p>
            </div>
            <div className="workspace-card-body">
              <ul className="workspace-list">
                {model.recentArtifacts.map((artifact) => (
                  <li key={artifact.id}>
                    <div className="workspace-list-row">
                      <div>
                        <strong>{artifact.title}</strong>
                        <p>{artifact.surface}</p>
                      </div>
                      <div className="workspace-row-right">
                        <span className={toStatusClassName(artifact.status)}>{artifact.status}</span>
                        <span>{artifact.updatedAtLabel}</span>
                      </div>
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          </article>

          <article className="workspace-card">
            <div className="workspace-card-header">
              <h2>Connected surfaces</h2>
              <p>Resource planning blends ready-now tools with manual setup steps.</p>
            </div>
            <div className="workspace-card-body">
              <ul className="workspace-list">
                {model.resources.map((resource) => (
                  <li key={`${resource.category}-${resource.provider.key}`}>
                    <div className="workspace-list-row">
                      <div>
                        <strong>{resource.object_name}</strong>
                        <p>
                          {resource.provider.display_name}
                          {resource.manual_next_step ? ` - ${resource.manual_next_step}` : ''}
                        </p>
                      </div>
                      <div className="workspace-row-right">
                        <span className={toStatusClassName(resource.state)}>
                          {getProvisioningLabel(resource.state)}
                        </span>
                      </div>
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          </article>
        </section>
      </div>
    </main>
  );
}

export default WorkspaceDemoPage;
