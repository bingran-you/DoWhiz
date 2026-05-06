import { useEffect, useRef, useState } from 'react';
import {
  getDoWhizApiBaseUrl,
  getOrCreateSessionId,
  persistAttributionFromLocation,
  trackAnalyticsEvent
} from '../analytics';
import { supabase } from '../app/supabaseClient';
import oliverImg from '../assets/Oliver.jpg';
import MouseField from '../components/landing/MouseField';
import {
  getThemeForLocalTime,
  shouldEnableMouseField
} from '../components/landing/mouseFieldUtils';
import { LOCAL_THEME_CHANGE_EVENT, THEME_META_COLORS } from '../theme/localTheme';
import { getLandingContent } from './landingContent';

const SITE_URL = 'https://dowhiz.com';
const LOGO_URL = `${SITE_URL}/assets/DoWhiz.svg`;
const SUPPORT_EMAIL = 'admin@dowhiz.com';
const ORG_NAME = 'DoWhiz';
const CN_PATH_PREFIX = '/cn';
const LANDING_PAGE_OVERRIDE_PARAM = 'view';
const LANDING_PAGE_OVERRIDE_VALUE = 'landing';
const LANDING_DASHBOARD_SUFFIX = '?loggedIn=true#section-overview';
const LANDING_SETTINGS_SUFFIX = '#section-settings';
const AUTHENTICATED_SETTINGS_SUFFIX = '?loggedIn=true#section-settings';
const LANDING_PAGE_VARIANT = 'launch_execution_workspace_v1';
const HERO_SHOWCASE_INTERVAL_MS = 4200;
const PUBLIC_CHANNEL_URLS = {
  slack:
    'https://slack.com/oauth/v2/authorize?client_id=5584751405762.10556678065461&scope=app_mentions:read,channels:history,channels:read,chat:write,groups:history,groups:read,im:history,im:read,mpim:history,mpim:read,users:read&user_scope=',
  discord:
    'https://discord.com/oauth2/authorize?client_id=1475574666830680175&permissions=68608&integration_type=0&scope=bot'
};
const OAUTH_ENDPOINTS = {
  discord: '/auth/discord',
  slack: '/auth/slack',
  github: '/auth/github',
  notion: '/auth/notion',
  lark: '/auth/lark'
};
const PRIMARY_HERO_TOOL_KEYS = new Set(['email', 'slack', 'discord']);

function SlackIcon({ className }) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" className={className} aria-hidden="true">
      <path d="M5.042 15.165a2.528 2.528 0 0 1-2.52 2.523A2.528 2.528 0 0 1 0 15.165a2.527 2.527 0 0 1 2.522-2.52h2.52v2.52zM6.313 15.165a2.527 2.527 0 0 1 2.521-2.52 2.527 2.527 0 0 1 2.521 2.52v6.313A2.528 2.528 0 0 1 8.834 24a2.528 2.528 0 0 1-2.521-2.522v-6.313zM8.834 5.042a2.528 2.528 0 0 1-2.521-2.52A2.528 2.528 0 0 1 8.834 0a2.528 2.528 0 0 1 2.521 2.522v2.52H8.834zM8.834 6.313a2.528 2.528 0 0 1 2.521 2.521 2.528 2.528 0 0 1-2.521 2.521H2.522A2.528 2.528 0 0 1 0 8.834a2.528 2.528 0 0 1 2.522-2.521h6.312zM18.956 8.834a2.528 2.528 0 0 1 2.522-2.521A2.528 2.528 0 0 1 24 8.834a2.528 2.528 0 0 1-2.522 2.521h-2.522V8.834zM17.688 8.834a2.528 2.528 0 0 1-2.523 2.521 2.527 2.527 0 0 1-2.52-2.521V2.522A2.527 2.527 0 0 1 15.165 0a2.528 2.528 0 0 1 2.523 2.522v6.312zM15.165 18.956a2.528 2.528 0 0 1 2.523 2.522A2.528 2.528 0 0 1 15.165 24a2.527 2.527 0 0 1-2.52-2.522v-2.522h2.52zM15.165 17.688a2.527 2.527 0 0 1-2.52-2.523 2.526 2.526 0 0 1 2.52-2.52h6.313A2.527 2.527 0 0 1 24 15.165a2.528 2.528 0 0 1-2.522 2.523h-6.313z" />
    </svg>
  );
}

function DiscordIcon({ className }) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" className={className} aria-hidden="true">
      <path d="M20.317 4.37a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037A19.736 19.736 0 0 0 3.677 4.37a.07.07 0 0 0-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 0 0 .031.057 19.9 19.9 0 0 0 5.993 3.03.078.078 0 0 0 .084-.028 14.09 14.09 0 0 0 1.226-1.994.076.076 0 0 0-.041-.106 13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128 10.2 10.2 0 0 0 .372-.292.074.074 0 0 1 .077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 0 1 .078.01c.12.098.246.198.373.292a.077.077 0 0 1-.006.127 12.299 12.299 0 0 1-1.873.892.077.077 0 0 0-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 0 0 .084.028 19.839 19.839 0 0 0 6.002-3.03.077.077 0 0 0 .032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.03zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z" />
    </svg>
  );
}

function GitHubIcon({ className }) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" className={className} aria-hidden="true">
      <path d="M12 0C5.37 0 0 5.37 0 12a12 12 0 0 0 8.2 11.4c.6.1.8-.2.8-.6v-2.2c-3.3.7-4-1.4-4-1.4-.6-1.4-1.3-1.8-1.3-1.8-1.1-.8.1-.8.1-.8 1.2.1 1.9 1.3 1.9 1.3 1.1 1.9 2.9 1.3 3.6 1 .1-.8.4-1.3.8-1.7-2.7-.3-5.5-1.3-5.5-6a4.7 4.7 0 0 1 1.3-3.3c-.1-.3-.6-1.5.1-3.2 0 0 1-.3 3.4 1.3a11.7 11.7 0 0 1 6.2 0c2.4-1.6 3.4-1.3 3.4-1.3.7 1.7.2 2.9.1 3.2a4.7 4.7 0 0 1 1.3 3.3c0 4.7-2.8 5.7-5.5 6 .4.4.8 1 .8 2.1v3.1c0 .4.2.7.8.6A12 12 0 0 0 24 12C24 5.37 18.63 0 12 0z" />
    </svg>
  );
}

function NotionIcon({ className }) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" className={className} aria-hidden="true">
      <path d="M4.459 4.208c.746.606 1.026.56 2.428.466l13.215-.793c.28 0 .047-.28-.046-.326L18.4 2.297c-.466-.373-.98-.56-2.055-.466L3.525 3.033c-.466.047-.56.28-.373.466l1.307.709zM5.252 7.27v14.083c0 .793.466 1.073 1.446.933l14.008-.793c.98-.14 1.073-.606 1.073-1.306V6.29c0-.7-.28-1.026-.933-.933l-14.615.84c-.653.094-.98.467-.98 1.073zm13.868.84c.14.653 0 1.306-.653 1.353l-.7.14v10.41c-.606.327-1.166.514-1.632.514-.746 0-.933-.234-1.493-.933l-4.666-7.334v7.1l1.446.327s0 1.306-1.819 1.306l-5.006.28c-.14-.28 0-.98.466-1.12l1.213-.327V9.476L4.679 9.29c-.14-.653.234-1.586 1.306-1.68l5.359-.327 4.852 7.428V8.083l-1.213-.14c-.14-.793.42-1.353 1.12-1.4l5.016-.326z" />
    </svg>
  );
}

function EmailIcon({ className }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" className={className} aria-hidden="true">
      <rect x="3" y="5" width="18" height="14" rx="3" stroke="currentColor" strokeWidth="1.8" />
      <path d="M4.5 7l7.5 6 7.5-6" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function ToolFallbackIcon({ className }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" className={className} aria-hidden="true">
      <rect x="4.5" y="4.5" width="15" height="15" rx="4" stroke="currentColor" strokeWidth="1.8" />
      <circle cx="12" cy="12" r="2.6" fill="currentColor" />
    </svg>
  );
}

function HeroToolIcon({ toolKey }) {
  const iconClassName = 'hero-tool-icon';

  switch (toolKey) {
    case 'email':
      return <EmailIcon className={iconClassName} />;
    case 'slack':
      return <SlackIcon className={iconClassName} />;
    case 'discord':
      return <DiscordIcon className={iconClassName} />;
    case 'github':
      return <GitHubIcon className={iconClassName} />;
    case 'notion':
      return <NotionIcon className={iconClassName} />;
    case 'lark':
      return <img src="/svgs/lark.svg" alt="" className={`${iconClassName} hero-tool-icon-image`} aria-hidden="true" />;
    default:
      return <ToolFallbackIcon className={iconClassName} />;
  }
}

const isCnPath = (pathname = '/') =>
  pathname === CN_PATH_PREFIX || pathname === `${CN_PATH_PREFIX}/` || pathname.startsWith(`${CN_PATH_PREFIX}/`);

const getLocalizedAuthPath = (
  suffix = '',
  pathname = typeof window !== 'undefined' ? window.location.pathname : '/'
) => `${isCnPath(pathname) ? CN_PATH_PREFIX : ''}/auth/index.html${suffix}`;

const getLocalizedDashboardPath = (
  pathname = typeof window !== 'undefined' ? window.location.pathname : '/'
) => getLocalizedAuthPath(LANDING_DASHBOARD_SUFFIX, pathname);

const getLocalizedLandingPagePath = (
  pathname = typeof window !== 'undefined' ? window.location.pathname : '/'
) => {
  const basePath = isCnPath(pathname) ? CN_PATH_PREFIX : '/';
  return `${basePath}?${LANDING_PAGE_OVERRIDE_PARAM}=${LANDING_PAGE_OVERRIDE_VALUE}`;
};

const hasSameOriginReferrer = () => {
  if (typeof window === 'undefined' || typeof document === 'undefined' || !document.referrer) {
    return false;
  }

  try {
    return new URL(document.referrer, window.location.origin).origin === window.location.origin;
  } catch {
    return false;
  }
};

const shouldStayOnLandingPage = () => {
  if (typeof window === 'undefined') {
    return false;
  }

  const { hash, search } = window.location;
  const searchParams = new URLSearchParams(search);

  if (searchParams.get(LANDING_PAGE_OVERRIDE_PARAM) === LANDING_PAGE_OVERRIDE_VALUE) {
    return true;
  }

  if (hash) {
    return true;
  }

  return hasSameOriginReferrer();
};

const updateMetaContent = (selector, content) => {
  if (typeof document === 'undefined' || !content) {
    return;
  }

  const node = document.querySelector(selector);
  if (node) {
    node.setAttribute('content', content);
  }
};

const updateLinkHref = (selector, href) => {
  if (typeof document === 'undefined' || !href) {
    return;
  }

  const node = document.querySelector(selector);
  if (node) {
    node.setAttribute('href', href);
  }
};

const getStageAvatarLabel = (author = '') => {
  const trimmedAuthor = author.trim();
  if (!trimmedAuthor) {
    return '?';
  }

  if (trimmedAuthor.length <= 2) {
    return trimmedAuthor;
  }

  return trimmedAuthor
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0])
    .join('')
    .toUpperCase();
};

function HeroStageAvatarCluster({ items = [], dark = false }) {
  if (!items.length) {
    return null;
  }

  const clusterClasses = ['hero-stage-avatar-cluster'];
  if (dark) {
    clusterClasses.push('is-dark');
  }

  return (
    <div className={clusterClasses.join(' ')} aria-hidden="true">
      {items.map((item, index) => {
        const label = typeof item === 'string' ? item : item?.name || '';
        return (
          <span key={`${label}-${index}`} className="hero-stage-avatar-cluster-item">
            {getStageAvatarLabel(label)}
          </span>
        );
      })}
    </div>
  );
}

function SlackStageMessage({ author, meta, time, text, tone = 'default', reactions = [] }) {
  return (
    <div className={`hero-stage-slack-message${tone === 'user' ? ' is-user' : ''}`}>
      <span className="hero-stage-slack-avatar" aria-hidden="true">
        {getStageAvatarLabel(author)}
      </span>
      <div className="hero-stage-slack-message-main">
        <div className="hero-stage-slack-message-head">
          <strong>{author}</strong>
          {meta ? <span className="hero-stage-slack-message-role">{meta}</span> : null}
          {time ? <small>{time}</small> : null}
        </div>
        <p>{text}</p>
        {reactions.length ? (
          <div className="hero-stage-slack-reactions">
            {reactions.map((reaction) => (
              <span key={reaction} className="hero-stage-slack-reaction">
                {reaction}
              </span>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}

function DiscordStageMessage({ author, meta, time, text, tone = 'default', accent }) {
  return (
    <div className={`hero-stage-discord-message${tone === 'user' ? ' is-user' : ''}`}>
      <span
        className="hero-stage-discord-avatar"
        aria-hidden="true"
        style={accent ? { '--discord-accent': accent } : undefined}
      >
        {getStageAvatarLabel(author)}
      </span>
      <div className="hero-stage-discord-message-main">
        <div className="hero-stage-discord-message-head">
          <strong style={accent ? { color: accent } : undefined}>{author}</strong>
          {meta ? <span className="hero-stage-discord-role">{meta}</span> : null}
          {time ? <small>{time}</small> : null}
        </div>
        <p>{text}</p>
      </div>
    </div>
  );
}

function GitHubChecklistItem({ item }) {
  return (
    <div className={`hero-stage-github-checklist-item hero-stage-github-checklist-item-${item.state}`}>
      <span className="hero-stage-github-check" aria-hidden="true">
        {item.state === 'done' ? '✓' : item.state === 'progress' ? '•' : ''}
      </span>
      <div className="hero-stage-github-checklist-copy">
        <strong>{item.title}</strong>
        <span>{item.meta}</span>
      </div>
    </div>
  );
}

function GitHubActivityRow({ item }) {
  return (
    <div className="hero-stage-github-activity">
      <span className="hero-stage-github-activity-dot" aria-hidden="true"></span>
      <div className="hero-stage-github-activity-copy">
        <strong>{item.actor}</strong>
        <p>{item.text}</p>
      </div>
      <small>{item.meta}</small>
    </div>
  );
}

function NotionCollaboratorBar({ items = [] }) {
  if (!items.length) {
    return null;
  }

  return (
    <div className="hero-stage-notion-collaborators" aria-hidden="true">
      {items.map((item, index) => (
        <span key={`${item}-${index}`} className="hero-stage-notion-collaborator">
          {getStageAvatarLabel(item)}
        </span>
      ))}
    </div>
  );
}

function LarkStageMessage({ author, meta, time, text, badge }) {
  return (
    <div className="hero-stage-lark-message">
      <span className="hero-stage-lark-avatar" aria-hidden="true">
        {getStageAvatarLabel(author)}
      </span>
      <div className="hero-stage-lark-message-main">
        <div className="hero-stage-lark-message-head">
          <strong>{author}</strong>
          {meta ? <span>{meta}</span> : null}
          {time ? <small>{time}</small> : null}
          {badge ? <em>{badge}</em> : null}
        </div>
        <p>{text}</p>
      </div>
    </div>
  );
}

function getStageStatusTone(status = '') {
  const normalized = status.toLowerCase();

  if (normalized.includes('ready') || status.includes('已准备')) {
    return 'success';
  }

  if (
    normalized.includes('progress') ||
    normalized.includes('active') ||
    normalized.includes('open') ||
    status.includes('进行中')
  ) {
    return 'progress';
  }

  if (
    normalized.includes('queue') ||
    normalized.includes('next') ||
    normalized.includes('draft') ||
    status.includes('待开始') ||
    status.includes('下一步')
  ) {
    return 'queued';
  }

  return 'neutral';
}

function HeroStageStatusPill({ label }) {
  return (
    <span className={['hero-stage-status-pill', `is-${getStageStatusTone(label)}`].join(' ')}>
      {label}
    </span>
  );
}

function getNotionBlockMarker(type) {
  switch (type) {
    case 'heading':
      return 'H1';
    case 'todo':
      return '[]';
    case 'callout':
      return '!';
    default:
      return '•';
  }
}

function EmailHeroStage({ tool }) {
  const stage = tool.stage;

  return (
    <div className="hero-stage-body hero-stage-email-layout">
      <aside className="hero-stage-email-nav">
        <div className="hero-stage-email-nav-head">
          <strong>{stage.appName}</strong>
          <span>{stage.appMeta}</span>
        </div>

        <div className="hero-stage-email-folder-list">
          {stage.folders.map((folder) => (
            <div
              key={folder.label}
              className={`hero-stage-email-folder${folder.active ? ' is-active' : ''}`}
            >
              <span>{folder.label}</span>
              {folder.count ? <small>{folder.count}</small> : null}
            </div>
          ))}
        </div>

        <div className="hero-stage-email-thread-list">
          {stage.threads.map((thread) => (
            <article
              key={`${thread.from}-${thread.subject}`}
              className={`hero-stage-email-thread${thread.active ? ' is-active' : ''}`}
            >
              <div className="hero-stage-email-thread-head">
                <strong>{thread.from}</strong>
                <span>{thread.time}</span>
              </div>
              <p>{thread.subject}</p>
              <small>{thread.preview}</small>
            </article>
          ))}
        </div>
      </aside>

      <section className="hero-stage-email-compose">
        <div className="hero-stage-window-bar">
          <div className="hero-stage-window-controls" aria-hidden="true">
            <span></span>
            <span></span>
            <span></span>
          </div>
          <strong>{stage.composeTitle}</strong>
        </div>

        <div className="hero-stage-email-badges">
          <span className="hero-stage-email-draft-badge">{stage.draftBadge}</span>
          {stage.tags.map((tag) => (
            <span key={tag} className="hero-stage-email-tag">
              {tag}
            </span>
          ))}
        </div>

        <div className="hero-stage-email-fields">
          <div className="hero-stage-email-field">
            <span>{stage.toLabel}</span>
            <strong>{stage.toValue}</strong>
          </div>
          <div className="hero-stage-email-field">
            <span>{stage.ccLabel}</span>
            <strong>{stage.ccValue}</strong>
          </div>
          <div className="hero-stage-email-field">
            <span>{stage.subjectLabel}</span>
            <strong>{stage.subjectValue}</strong>
          </div>
        </div>

        <div className="hero-stage-email-body">
          {stage.bodyLines.map((line) => (
            <p key={line}>{line}</p>
          ))}
        </div>

        <div className="hero-stage-email-footer">
          <span>{stage.footerNote}</span>
          <span className="hero-stage-mock-button hero-stage-mock-button-dark">{stage.footerValue}</span>
        </div>
      </section>

      <aside className="hero-stage-email-result">
        <span className="hero-stage-micro-label">{stage.resultLabel}</span>
        <strong>{stage.resultTitle}</strong>
        <ul className="hero-stage-list">
          {stage.resultItems.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      </aside>
    </div>
  );
}

function SlackHeroStage({ tool }) {
  const stage = tool.stage;

  return (
    <div className="hero-stage-body hero-stage-slack-layout">
      <aside className="hero-stage-slack-sidebar">
        <div className="hero-stage-slack-workspace">
          <span className="hero-stage-slack-workspace-mark" aria-hidden="true"></span>
          <div className="hero-stage-slack-workspace-copy">
            <strong>{stage.workspace}</strong>
            <span>{stage.workspaceMeta}</span>
          </div>
        </div>

        {stage.sections.map((section) => (
          <div key={section.title} className="hero-stage-slack-section">
            <span className="hero-stage-slack-section-title">{section.title}</span>
            <div className="hero-stage-slack-section-list">
              {section.items.map((item) => {
                const isDirectSection =
                  section.title.toLowerCase().includes('direct') || section.title.includes('私信');
                const prefix = isDirectSection ? (item.accent === 'bot' ? '@' : '•') : '#';
                const label = prefix === '#' ? item.label.replace(/^#\s*/, '') : item.label;

                return (
                  <div
                    key={item.label}
                    className={`hero-stage-slack-item${item.active ? ' is-active' : ''}${
                      item.accent === 'bot' ? ' is-bot' : ''
                    }`}
                  >
                    <span className="hero-stage-slack-item-marker" aria-hidden="true">
                      {prefix}
                    </span>
                    <span>{label}</span>
                  </div>
                );
              })}
            </div>
          </div>
        ))}
      </aside>

      <section className="hero-stage-slack-thread">
        <div className="hero-stage-slack-thread-head">
          <div className="hero-stage-slack-room-copy">
            <strong>{stage.room}</strong>
            <span>{stage.roomMeta}</span>
          </div>
          <div className="hero-stage-slack-room-members">
            <HeroStageAvatarCluster items={stage.roomMembers} />
            <span>{stage.roomMembers.join(' / ')}</span>
          </div>
        </div>

        <div className="hero-stage-slack-thread-toolbar">
          <div className="hero-stage-slack-thread-pills">
            {(stage.threadPills || []).map((pill) => (
              <span key={pill} className="hero-stage-slack-thread-pill">
                {pill}
              </span>
            ))}
          </div>
          {stage.threadActivity ? (
            <span className="hero-stage-slack-thread-activity">{stage.threadActivity}</span>
          ) : null}
        </div>

        <div className="hero-stage-slack-message-feed">
          {stage.messages.map((message) => (
            <SlackStageMessage key={`${message.author}-${message.text}`} {...message} />
          ))}
        </div>

        <div className="hero-stage-slack-summary">
          <span className="hero-stage-micro-label">{stage.threadLabel}</span>
          <strong>{stage.threadTitle}</strong>
          <p>{stage.threadText}</p>
        </div>

        <div className="hero-stage-slack-composer">
          <span className="hero-stage-slack-composer-pill">@DoWhiz</span>
          <div className="hero-stage-slack-composer-copy">
            <strong>{stage.composerPlaceholder}</strong>
            <span>{stage.composerHint}</span>
          </div>
        </div>
      </section>

      <aside className="hero-stage-slack-card">
        <span className="hero-stage-micro-label">{stage.cardLabel}</span>
        <strong>{stage.cardTitle}</strong>
        <p className="hero-stage-slack-card-summary">{stage.cardSummary}</p>
        <ul className="hero-stage-list">
          {stage.cardItems.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
        <div className="hero-stage-chip-row">
          {stage.cardActions.map((action) => (
            <span key={action} className="hero-stage-chip">
              {action}
            </span>
          ))}
        </div>
      </aside>
    </div>
  );
}

function DiscordHeroStage({ tool }) {
  const stage = tool.stage;

  return (
    <div className="hero-stage-body hero-stage-discord-layout">
      <aside className="hero-stage-discord-servers" aria-hidden="true">
        <span className="hero-stage-discord-server-dot is-active"></span>
        <span className="hero-stage-discord-server-dot"></span>
        <span className="hero-stage-discord-server-dot"></span>
        <span className="hero-stage-discord-server-dot"></span>
      </aside>

      <aside className="hero-stage-discord-channels">
        <div className="hero-stage-discord-server-head">
          <strong>{stage.server}</strong>
          <span>{stage.onlineLabel}</span>
        </div>

        {stage.sections.map((section) => (
          <div key={section.title} className="hero-stage-discord-section">
            <span className="hero-stage-discord-section-title">{section.title}</span>
            <div className="hero-stage-discord-channel-list">
              {section.items.map((item) => {
                const label = item.label.replace(/^#\s*/, '');
                return (
                  <span key={item.label} className={item.active ? 'is-active' : ''}>
                    # {label}
                  </span>
                );
              })}
            </div>
          </div>
        ))}

        <div className="hero-stage-discord-members-mini">
          <span>{stage.membersTitle}</span>
          <HeroStageAvatarCluster items={stage.members} dark />
        </div>
      </aside>

      <section className="hero-stage-discord-chat">
        <div className="hero-stage-discord-room-head">
          <div className="hero-stage-discord-room-copy">
            <strong>{stage.room}</strong>
            <span>{stage.roomTopic}</span>
          </div>
        </div>

        <div className="hero-stage-discord-chat-toolbar">
          <span className="hero-stage-discord-channel-badge">{stage.room}</span>
          {stage.roomMeta ? <span className="hero-stage-discord-room-meta">{stage.roomMeta}</span> : null}
        </div>

        <div className="hero-stage-discord-message-feed">
          {stage.messages.map((message) => (
            <DiscordStageMessage key={`${message.author}-${message.text}`} {...message} />
          ))}
        </div>

        <div className="hero-stage-discord-composer">
          <strong>{stage.composerValue}</strong>
          <span>{stage.composerHint}</span>
        </div>
      </section>

      <aside className="hero-stage-discord-plan">
        <span className="hero-stage-micro-label">{stage.botLabel}</span>
        <strong>{stage.botTitle}</strong>
        <p className="hero-stage-discord-bot-description">{stage.botDescription}</p>

        <div className="hero-stage-discord-embed">
          <div className="hero-stage-discord-embed-accent" aria-hidden="true"></div>
          <div className="hero-stage-discord-field-list">
            {stage.botFields.map((field) => (
              <div key={field.label} className="hero-stage-discord-field">
                <span>{field.label}</span>
                <strong>{field.value}</strong>
              </div>
            ))}
          </div>
        </div>

        <div className="hero-stage-discord-member-list">
          <span className="hero-stage-discord-member-title">{stage.membersTitle}</span>
          {stage.members.map((member) => (
            <div
              key={member.name}
              className={`hero-stage-discord-member${member.accent ? ' is-accent' : ''}`}
            >
              <strong>{member.name}</strong>
              <span>{member.role}</span>
            </div>
          ))}
        </div>
      </aside>
    </div>
  );
}

function GitHubHeroStage({ tool }) {
  const stage = tool.stage;

  return (
    <div className="hero-stage-body hero-stage-github-layout">
      <div className="hero-stage-github-shell">
        <div className="hero-stage-github-topbar">
          <div className="hero-stage-github-repo">
            <strong>{stage.repo}</strong>
            <span>{stage.repoMeta}</span>
          </div>
          <div className="hero-stage-github-tabs">
            {stage.tabs.map((tab) => (
              <span key={tab}>{tab}</span>
            ))}
          </div>
        </div>

        <div className="hero-stage-github-filter-row">
          {stage.filters.map((filter) => (
            <span key={filter} className="hero-stage-github-filter">
              {filter}
            </span>
          ))}
        </div>

        <div className="hero-stage-github-list-head">
          <span className="hero-stage-github-list-count is-open">{stage.overview.open}</span>
          <span className="hero-stage-github-list-count is-closed">{stage.overview.closed}</span>
        </div>

        <div className="hero-stage-github-grid">
          <section className="hero-stage-github-issues">
            {stage.issues.map((issue) => (
              <article
                key={issue.id}
                className={`hero-stage-github-issue${issue.active ? ' is-active' : ''}`}
              >
                <div className="hero-stage-github-issue-title-row">
                  <div className="hero-stage-github-state">
                    <span className="hero-stage-github-state-dot" aria-hidden="true"></span>
                    <strong>{issue.title}</strong>
                  </div>
                  <HeroStageStatusPill label={issue.status} />
                </div>

                <div className="hero-stage-github-issue-meta-row">
                  <span>{issue.id}</span>
                  <span>{issue.meta}</span>
                </div>

                <div className="hero-stage-github-issue-badges">
                  <div className="hero-stage-github-issue-labels">
                    {issue.labels.map((label) => (
                      <span key={label} className="hero-stage-github-issue-label">
                        {label}
                      </span>
                    ))}
                  </div>
                  <span className="hero-stage-github-issue-comments">{issue.comments}</span>
                </div>
              </article>
            ))}
          </section>

          <aside className="hero-stage-github-detail">
            <span className="hero-stage-micro-label">{stage.detailLabel}</span>
            <strong>{stage.detailTitle}</strong>
            <p>{stage.detailSummary}</p>

            <div className="hero-stage-github-detail-meta">
              {stage.detailMeta.map((item) => (
                <span key={item} className="hero-stage-github-detail-chip">
                  {item}
                </span>
              ))}
            </div>

            <div className="hero-stage-github-checklist">
              {stage.detailChecklist.map((item) => (
                <GitHubChecklistItem key={item.title} item={item} />
              ))}
            </div>

            <div className="hero-stage-github-detail-comment">
              <span>{stage.detailCommentTitle}</span>
              <p>{stage.detailComment}</p>
            </div>

            <div className="hero-stage-github-activity-list">
              {stage.detailActivity.map((item) => (
                <GitHubActivityRow key={`${item.actor}-${item.text}`} item={item} />
              ))}
            </div>

            <div className="hero-stage-inline-note">{stage.detailFooter}</div>
          </aside>
        </div>
      </div>
    </div>
  );
}

function NotionHeroStage({ tool }) {
  const stage = tool.stage;

  return (
    <div className="hero-stage-body hero-stage-notion-layout">
      <section className="hero-stage-notion-page">
        <div className="hero-stage-notion-meta-row">
          <div className="hero-stage-notion-breadcrumb">{stage.breadcrumb}</div>
          <NotionCollaboratorBar items={stage.collaborators} />
        </div>

        <div className="hero-stage-notion-page-head">
          <span className="hero-stage-notion-page-icon">{stage.pageIcon}</span>
          <div className="hero-stage-notion-title-copy">
            <h3>{stage.pageTitle}</h3>
            <p>{stage.pageIntro}</p>
          </div>
        </div>

        <div className="hero-stage-notion-properties">
          {stage.properties.map((property) => (
            <div key={property.label} className="hero-stage-notion-property">
              <span>{property.label}</span>
              <strong>{property.value}</strong>
            </div>
          ))}
        </div>

        <div className="hero-stage-notion-blocks">
          {stage.blocks.map((block) => (
            <div
              key={`${block.type}-${block.text}`}
              className={`hero-stage-notion-block hero-stage-notion-block-${block.type}`}
            >
              <span aria-hidden="true">{getNotionBlockMarker(block.type)}</span>
              <strong>{block.text}</strong>
            </div>
          ))}
        </div>

        <div className="hero-stage-notion-table">
          <div className="hero-stage-notion-table-head">
            <span className="hero-stage-micro-label">{stage.databaseLabel}</span>
            <div className="hero-stage-notion-tabs">
              {stage.databaseTabs.map((tab, index) => (
                <span key={tab} className={index === 0 ? 'is-active' : ''}>
                  {tab}
                </span>
              ))}
            </div>
          </div>

          {stage.databaseColumns?.length ? (
            <div className="hero-stage-notion-table-columns">
              {stage.databaseColumns.map((column) => (
                <span key={column}>{column}</span>
              ))}
            </div>
          ) : null}

          {stage.rows.map((row) => (
            <div key={row.name} className="hero-stage-notion-row">
              <div className="hero-stage-notion-row-main">
                <strong>{row.name}</strong>
                <span>{row.meta}</span>
              </div>
              <HeroStageStatusPill label={row.status} />
            </div>
          ))}
        </div>
      </section>

      <aside className="hero-stage-notion-sidecard">
        <span className="hero-stage-micro-label">{stage.sideLabel}</span>
        <strong>{stage.sideTitle}</strong>
        <ul className="hero-stage-list">
          {stage.sideItems.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
        <div className="hero-stage-inline-note">{stage.sideFootnote}</div>
      </aside>
    </div>
  );
}

function LarkHeroStage({ tool }) {
  const stage = tool.stage;

  return (
    <div className="hero-stage-body hero-stage-lark-layout">
      <section className="hero-stage-lark-feed">
        <div className="hero-stage-lark-toolbar">
          <div className="hero-stage-lark-head">
            <strong>{stage.workspace}</strong>
            <span>{stage.workspaceMeta}</span>
          </div>
          <div className="hero-stage-lark-tabs">
            {stage.tabs.map((tab, index) => (
              <span key={tab} className={index === 0 ? 'is-active' : ''}>
                {tab}
              </span>
            ))}
          </div>
        </div>

        {stage.participants?.length ? (
          <div className="hero-stage-lark-participants">
            <HeroStageAvatarCluster items={stage.participants} />
            <span>{stage.participants.join(' / ')}</span>
          </div>
        ) : null}

        <div className="hero-stage-lark-recap">
          <span className="hero-stage-micro-label">{stage.recapLabel}</span>
          <strong>{stage.recapTitle}</strong>
          <p>{stage.recapText}</p>
        </div>

        <div className="hero-stage-lark-message-feed">
          {stage.messages.map((message) => (
            <LarkStageMessage key={`${message.author}-${message.text}`} {...message} />
          ))}
        </div>
      </section>

      <aside className="hero-stage-lark-card">
        <span className="hero-stage-micro-label">{stage.trackerLabel}</span>
        <strong className="hero-stage-lark-title">{stage.trackerTitle}</strong>

        {stage.ownerColumns?.length ? (
          <div className="hero-stage-lark-owner-head">
            {stage.ownerColumns.map((column) => (
              <span key={column}>{column}</span>
            ))}
          </div>
        ) : null}

        <div className="hero-stage-lark-owners">
          {stage.owners.map((owner) => (
            <div key={`${owner.owner}-${owner.task}`} className="hero-stage-lark-owner-row">
              <div className="hero-stage-lark-owner-copy">
                <strong>{owner.owner}</strong>
                <span>{owner.task}</span>
              </div>
              <div className="hero-stage-lark-owner-meta">
                <small>{owner.due}</small>
                <HeroStageStatusPill label={owner.status} />
              </div>
            </div>
          ))}
        </div>

        <div className="hero-stage-lark-update">
          <span>{stage.updateLabel}</span>
          <p>{stage.updateText}</p>
        </div>

        <div className="hero-stage-chip-row">
          {stage.updateActions.map((action) => (
            <span key={action} className="hero-stage-chip">
              {action}
            </span>
          ))}
        </div>
      </aside>
    </div>
  );
}

function renderHeroStageBody(tool) {
  switch (tool.key) {
    case 'email':
      return <EmailHeroStage tool={tool} />;
    case 'slack':
      return <SlackHeroStage tool={tool} />;
    case 'discord':
      return <DiscordHeroStage tool={tool} />;
    case 'github':
      return <GitHubHeroStage tool={tool} />;
    case 'notion':
      return <NotionHeroStage tool={tool} />;
    case 'lark':
      return <LarkHeroStage tool={tool} />;
    default:
      return null;
  }
}

function LandingPage({ locale }) {
  const pathname = typeof window !== 'undefined' ? window.location.pathname : '/';
  const pageLocale = locale || (isCnPath(pathname) ? 'zh-CN' : 'en-US');
  const isChinesePage = pageLocale === 'zh-CN';
  const content = getLandingContent(pageLocale);
  const [theme, setTheme] = useState(() => getThemeForLocalTime());
  const [enableMouseField, setEnableMouseField] = useState(false);
  const [user, setUser] = useState(null);
  const [authStatus, setAuthStatus] = useState('checking');
  const [loadingToolKey, setLoadingToolKey] = useState(null);
  const [activeShowcaseIndex, setActiveShowcaseIndex] = useState(0);
  const [showcasePaused, setShowcasePaused] = useState(false);
  const [prefersReducedMotion, setPrefersReducedMotion] = useState(false);
  const [showUserMenu, setShowUserMenu] = useState(false);
  const [navHidden, setNavHidden] = useState(false);
  const userMenuRef = useRef(null);
  const lastScrollY = useRef(0);
  const authRedirectStartedRef = useRef(false);
  const isAuthenticated = authStatus === 'authenticated' && Boolean(user);
  const localizedHomePath = isAuthenticated ? getLocalizedLandingPagePath(pathname) : content.nav.homePath;
  const heroTools = content.hero.tools;
  const heroToolEntries = heroTools.map((tool, index) => ({ ...tool, index }));
  const primaryHeroTools = heroToolEntries.filter(({ key }) => PRIMARY_HERO_TOOL_KEYS.has(key));
  const secondaryHeroTools = heroToolEntries.filter(({ key }) => !PRIMARY_HERO_TOOL_KEYS.has(key));
  const activeHeroTool = heroTools[activeShowcaseIndex] || heroTools[0];

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }
    const { hash, pathname } = window.location;
    if (!hash || pathname.startsWith('/auth')) {
      return;
    }
    const params = new URLSearchParams(hash.substring(1));
    const hasTokens = params.get('access_token') && params.get('refresh_token');
    const hasError = params.get('error') || params.get('error_description');
    if (hasTokens || hasError) {
      window.location.replace(getLocalizedAuthPath(hash, pathname));
    }
  }, []);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }

    if (authStatus !== 'anonymous') {
      return;
    }

    persistAttributionFromLocation();
    const sessionId = getOrCreateSessionId();
    trackAnalyticsEvent(
      'landing_page_view',
      {
        landing_page_variant: LANDING_PAGE_VARIANT,
        landing_page_variant_legacy: 'oliver_consumer_v1',
        language: pageLocale
      },
      {
        eventKey: `landing_page_view:${sessionId}:${window.location.pathname}`
      }
    );
  }, [authStatus, pageLocale]);

  useEffect(() => {
    if (typeof document === 'undefined') {
      return;
    }

    document.documentElement.lang = content.metadata.htmlLang;
    document.title = content.metadata.title;

    updateMetaContent('meta[name="description"]', content.metadata.description);
    updateMetaContent('meta[property="og:title"]', content.metadata.title);
    updateMetaContent('meta[property="og:description"]', content.metadata.description);
    updateMetaContent('meta[property="og:url"]', content.metadata.canonicalUrl);
    updateMetaContent('meta[property="og:locale"]', content.metadata.ogLocale);
    updateMetaContent('meta[name="twitter:title"]', content.metadata.title);
    updateMetaContent('meta[name="twitter:description"]', content.metadata.description);
    updateMetaContent('meta[name="robots"]', content.metadata.robots);
    updateLinkHref('link[rel="canonical"]', content.metadata.canonicalUrl);
  }, [content.metadata]);

  useEffect(() => {
    updateMetaContent('meta[name="theme-color"]', THEME_META_COLORS[theme] || content.metadata.themeColor);
  }, [content.metadata.themeColor, theme]);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return undefined;
    }

    const scrollToHashTarget = () => {
      const hash = window.location.hash;
      if (!hash) {
        return;
      }

      const targetId = decodeURIComponent(hash.replace(/^#/, ''));
      const target = document.getElementById(targetId);
      if (target) {
        target.scrollIntoView({ behavior: 'auto', block: 'start' });
      }
    };

    const timeoutId = window.setTimeout(scrollToHashTarget, 120);
    window.addEventListener('hashchange', scrollToHashTarget);

    return () => {
      window.clearTimeout(timeoutId);
      window.removeEventListener('hashchange', scrollToHashTarget);
    };
  }, []);

  useEffect(() => {
    const handleClickOutside = (event) => {
      if (userMenuRef.current && !userMenuRef.current.contains(event.target)) {
        setShowUserMenu(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  useEffect(() => {
    let isActive = true;

    const syncSession = (session) => {
      if (!isActive) {
        return;
      }

      const currentUser = session?.user ?? null;
      setUser(currentUser);

      if (currentUser) {
        setAuthStatus('authenticated');
        if (
          !authRedirectStartedRef.current &&
          typeof window !== 'undefined' &&
          !shouldStayOnLandingPage()
        ) {
          authRedirectStartedRef.current = true;
          window.location.replace(getLocalizedDashboardPath(window.location.pathname));
        }
        return;
      }

      setAuthStatus('anonymous');
    };

    supabase.auth
      .getSession()
      .then(({ data: { session } }) => {
        syncSession(session);
      })
      .catch((error) => {
        console.error('App: Failed to load Supabase session', error);
        if (isActive) {
          setUser(null);
          setAuthStatus('anonymous');
        }
      });

    const {
      data: { subscription }
    } = supabase.auth.onAuthStateChange((_event, session) => {
      syncSession(session);
    });

    return () => {
      isActive = false;
      subscription.unsubscribe();
    };
  }, []);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return undefined;
    }

    const syncTheme = (event) => {
      setTheme(event?.detail?.theme || getThemeForLocalTime());
    };

    syncTheme();
    window.addEventListener(LOCAL_THEME_CHANGE_EVENT, syncTheme);

    return () => {
      window.removeEventListener(LOCAL_THEME_CHANGE_EVENT, syncTheme);
    };
  }, []);

  useEffect(() => {
    const handleScroll = () => {
      const currentScrollY = window.scrollY;
      const scrollThreshold = 100;

      if (currentScrollY > lastScrollY.current && currentScrollY > scrollThreshold) {
        setNavHidden(true);
      } else {
        setNavHidden(false);
      }
      lastScrollY.current = currentScrollY;
    };

    window.addEventListener('scroll', handleScroll, { passive: true });
    return () => window.removeEventListener('scroll', handleScroll);
  }, []);

  useEffect(() => {
    if (!shouldEnableMouseField()) {
      return undefined;
    }

    let idleId;
    let timeoutId;

    const revealMouseField = () => {
      setEnableMouseField(true);
    };

    if ('requestIdleCallback' in window) {
      idleId = window.requestIdleCallback(revealMouseField, { timeout: 1500 });
      return () => {
        if (typeof window.cancelIdleCallback === 'function') {
          window.cancelIdleCallback(idleId);
        }
      };
    }

    timeoutId = window.setTimeout(revealMouseField, 800);
    return () => {
      if (timeoutId) {
        window.clearTimeout(timeoutId);
      }
    };
  }, []);

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
      return undefined;
    }

    const mediaQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
    const syncMotionPreference = (event) => {
      setPrefersReducedMotion(event.matches);
    };

    setPrefersReducedMotion(mediaQuery.matches);

    if (typeof mediaQuery.addEventListener === 'function') {
      mediaQuery.addEventListener('change', syncMotionPreference);
      return () => mediaQuery.removeEventListener('change', syncMotionPreference);
    }

    mediaQuery.addListener(syncMotionPreference);
    return () => mediaQuery.removeListener(syncMotionPreference);
  }, []);

  useEffect(() => {
    setActiveShowcaseIndex(0);
  }, [pageLocale]);

  useEffect(() => {
    if (typeof window === 'undefined' || heroTools.length <= 1 || showcasePaused || prefersReducedMotion) {
      return undefined;
    }

    const intervalId = window.setInterval(() => {
      setActiveShowcaseIndex((currentIndex) => (currentIndex + 1) % heroTools.length);
    }, HERO_SHOWCASE_INTERVAL_MS);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [heroTools.length, prefersReducedMotion, showcasePaused]);

  const buildMailtoLink = (email, subject, body) => {
    const encodedSubject = encodeURIComponent(subject);
    const encodedBody = encodeURIComponent(body);
    return `mailto:${email}?subject=${encodedSubject}&body=${encodedBody}`;
  };

  const buildToolTrialMailto = (tool) => {
    const englishContext = {
      github: 'launch issue, blocker, or code-adjacent follow-through',
      notion: 'launch brief, decision log, or planning doc',
      lark: 'cross-team follow-through work'
    };
    const chineseContext = {
      github: 'repo、issue 或代码周边任务',
      notion: '文档、笔记或知识整理任务',
      lark: '协作运营或持续跟进任务'
    };
    const chineseTaskContext = chineseContext[tool.key] || `${tool.label} 相关任务`;
    const subject = isChinesePage ? `想通过 ${tool.label} 试用 DoWhiz` : `Trying DoWhiz with ${tool.label}`;
    const body = isChinesePage
      ? `你好 DoWhiz，\n\n我想先试一个和 ${tool.label} 有关的 ${chineseTaskContext}。\n\n- 我需要你处理什么：\n- 相关链接或上下文：\n- 希望最后拿到什么结果：\n- 如果后续更顺手，我是否愿意再连接 ${tool.label}：\n\n如果 setup 或绑定真的有帮助，也请你在回复里告诉我下一步。\n\n谢谢！`
      : `Hi DoWhiz,\n\nI want to try DoWhiz with a ${englishContext[tool.key] || tool.label} request.\n\n- What I need done:\n- Relevant links or context:\n- What a good result looks like:\n- If it helps later, should we connect ${tool.label} after this:\n\nIf setup or linking would actually help, please guide me in your reply.\n\nThanks!`;

    return buildMailtoLink('oliver@dowhiz.com', subject, body);
  };

  const openExternalHref = (href) => {
    const openedWindow = window.open(href, '_blank', 'noopener,noreferrer');
    if (!openedWindow) {
      window.location.href = href;
    }
  };

  const getAnonymousHeroToolAction = (tool) => {
    if (tool.key === 'slack' || tool.key === 'discord') {
      return {
        href: PUBLIC_CHANNEL_URLS[tool.key],
        type: 'public_install',
        openInNewTab: true
      };
    }

    return {
      href: buildToolTrialMailto(tool),
      type: 'tool_trial_email',
      openInNewTab: false
    };
  };

  const structuredData = {
    '@context': 'https://schema.org',
    '@graph': [
      {
        '@type': 'Organization',
        '@id': `${SITE_URL}/#organization`,
        name: ORG_NAME,
        url: `${SITE_URL}/`,
        logo: LOGO_URL,
        email: `mailto:${SUPPORT_EMAIL}`,
        contactPoint: [
          {
            '@type': 'ContactPoint',
            email: SUPPORT_EMAIL,
            contactType: 'customer support',
            availableLanguage: isChinesePage ? ['Chinese', 'English'] : ['English']
          }
        ],
        sameAs: ['https://github.com/KnoWhiz/DoWhiz']
      },
      {
        '@type': 'FAQPage',
        '@id': `${content.metadata.canonicalUrl}#faq`,
        url: content.metadata.canonicalUrl,
        inLanguage: content.metadata.htmlLang,
        mainEntity: content.faqItems.map((item) => ({
          '@type': 'Question',
          name: item.question,
          acceptedAnswer: {
            '@type': 'Answer',
            text: item.answer
          }
        }))
      }
    ]
  };

  const oliverContactHref = buildMailtoLink('oliver@dowhiz.com', content.hero.contactSubject, content.hero.contactBody);
  const heroPrimaryHref = isAuthenticated
    ? getLocalizedDashboardPath(pathname)
    : getLocalizedAuthPath('', pathname);
  const settingsHref = isAuthenticated
    ? getLocalizedAuthPath(AUTHENTICATED_SETTINGS_SUFFIX, pathname)
    : getLocalizedAuthPath(LANDING_SETTINGS_SUFFIX, pathname);
  const manageSetupLabel = isAuthenticated
    ? content.hero.manageAuthenticated
    : content.hero.manageAnonymous;

  const trackCtaClick = (eventName, properties) => {
    trackAnalyticsEvent(eventName, properties);
  };

  const startProviderConnect = async (provider) => {
    const endpoint = OAUTH_ENDPOINTS[provider];
    if (!endpoint) {
      window.location.href = settingsHref;
      return;
    }

    try {
      const {
        data: { session }
      } = await supabase.auth.getSession();

      if (!session?.access_token) {
        window.location.href = settingsHref;
        return;
      }

      const response = await fetch(`${getDoWhizApiBaseUrl()}${endpoint}`, {
        headers: { Authorization: `Bearer ${session.access_token}` }
      });
      const data = await response.json().catch(() => null);

      if (!response.ok || !data?.redirect_url) {
        throw new Error(data?.error || `Failed to start ${provider} connection`);
      }

      window.location.href = data.redirect_url;
    } catch (error) {
      console.error(`Landing: failed to start ${provider} connect flow`, error);
      setLoadingToolKey(null);
      window.location.href = settingsHref;
    }
  };

  const getHeroToolActionLabel = (tool) => {
    if (loadingToolKey === tool.key) {
      return content.hero.actionLabels.loading;
    }

    if (tool.key === 'email') {
      return tool.authenticatedActionLabel || tool.anonymousActionLabel;
    }

    if (isAuthenticated) {
      return tool.authenticatedActionLabel || content.hero.actionLabels.connect;
    }

    return tool.anonymousActionLabel;
  };

  const getHeroToolStatus = (tool) => {
    return isAuthenticated ? tool.authenticatedStatus : tool.anonymousStatus;
  };

  const getHeroToolAction = (tool) => {
    if (tool.key === 'email') {
      return {
        href: oliverContactHref,
        type: 'mailto',
        openInNewTab: false
      };
    }

    if (!isAuthenticated) {
      return getAnonymousHeroToolAction(tool);
    }

    return {
      type: 'oauth',
      provider: tool.key
    };
  };

  const handleHeroToolAction = async (tool, ctaLocation = 'hero_channel_widget') => {
    if (loadingToolKey) {
      return;
    }

    const action = getHeroToolAction(tool);

    trackCtaClick('hero_tool_action_click', {
      cta_location: ctaLocation,
      cta_text: tool.label,
      tool: tool.key,
      action_type: action.type,
      landing_page_variant: LANDING_PAGE_VARIANT
    });

    if (typeof window === 'undefined') {
      return;
    }

    if (action.type !== 'oauth') {
      if (action.openInNewTab) {
        openExternalHref(action.href);
      } else if (action.href) {
        window.location.href = action.href;
      }
      return;
    }

    setLoadingToolKey(tool.key);
    await startProviderConnect(tool.key);
  };

  const [openFaq, setOpenFaq] = useState(null);
  const toggleFaq = (idx) => setOpenFaq((prev) => (prev === idx ? null : idx));

  return (
    <div className={`app-container landing-page${isChinesePage ? ' landing-page-cn' : ''}`}>
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }}
      />
      <div className="content-layer">
        <nav className={`navbar${navHidden ? ' nav-hidden' : ''}`}>
          <div className="nav-content">
            <a href={localizedHomePath} className="logo">
              <img src="/assets/DoWhiz.svg" alt="" className="brand-mark" aria-hidden="true" />
              <span>Do<span className="text-gradient">Whiz</span></span>
            </a>
            <div className="nav-links">
              {content.nav.links.map((link) => (
                <a key={link.href} href={link.href} className="nav-btn">
                  {link.label}
                </a>
              ))}
            </div>
            <div className="nav-actions">
              <div className="social-links">
                <a
                  href="https://github.com/KnoWhiz/DoWhiz"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="btn-small"
                  aria-label={content.nav.githubAriaLabel}
                >
                  <svg viewBox="0 0 24 24" width="20" height="20" stroke="currentColor" strokeWidth="2" fill="none" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 0 0-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0 0 20 4.77 5.07 5.07 0 0 0 19.91 1S18.73.65 16 2.48a13.38 13.38 0 0 0-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 0 0 5 4.77a5.44 5.44 0 0 0-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 0 0 9 18.13V22"></path>
                  </svg>
                </a>
                <a
                  href="https://discord.gg/7ucnweCKk8"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="btn-small"
                  aria-label={content.nav.discordAriaLabel}
                >
                  <svg viewBox="0 0 24 24" width="20" height="20" stroke="currentColor" strokeWidth="2" fill="none" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z"></path>
                  </svg>
                </a>
                <a
                  className="btn-small"
                  href={oliverContactHref}
                  aria-label={content.nav.contactAriaLabel}
                  onClick={() =>
                    trackCtaClick('secondary_cta_click', {
                      cta_location: 'nav_contact',
                      cta_text: content.nav.contactAriaLabel
                    })
                  }
                >
                  <svg viewBox="0 0 24 24" width="20" height="20" stroke="currentColor" strokeWidth="2" fill="none" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"></path>
                    <polyline points="22,6 12,13 2,6"></polyline>
                  </svg>
                </a>
                {user ? (
                  <div className="user-menu-container" ref={userMenuRef}>
                    <button
                      type="button"
                      className="user-profile-btn"
                      onClick={() => setShowUserMenu(!showUserMenu)}
                      aria-label={user.user_metadata?.full_name || user.email || (isChinesePage ? '用户菜单' : 'User menu')}
                      title={user.user_metadata?.full_name || user.email || (isChinesePage ? '用户菜单' : 'User menu')}
                    >
                      <img
                        src={user.user_metadata?.avatar_url || user.user_metadata?.picture}
                        alt={user.user_metadata?.full_name || user.email}
                        className="user-avatar"
                      />
                    </button>
                    {showUserMenu && (
                      <div className="user-dropdown">
                        <a
                          href={getLocalizedDashboardPath(pathname)}
                          className="dropdown-item"
                          onClick={async (e) => {
                            e.preventDefault();
                            const {
                              data: { session }
                            } = await supabase.auth.getSession();
                            window.location.href = session
                              ? getLocalizedDashboardPath(window.location.pathname)
                              : getLocalizedAuthPath('', window.location.pathname);
                          }}
                        >
                          <svg viewBox="0 0 24 24" width="16" height="16" stroke="currentColor" strokeWidth="2" fill="none">
                            <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
                          </svg>
                          {content.nav.dashboard}
                        </a>
                        <button
                          className="dropdown-item"
                          onClick={async () => {
                            await supabase.auth.signOut();
                            setUser(null);
                            setShowUserMenu(false);
                          }}
                        >
                          <svg viewBox="0 0 24 24" width="16" height="16" stroke="currentColor" strokeWidth="2" fill="none">
                            <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
                            <polyline points="16 17 21 12 16 7" />
                            <line x1="21" y1="12" x2="9" y2="12" />
                          </svg>
                          {content.nav.signOut}
                        </button>
                      </div>
                    )}
                  </div>
                ) : (
                  <a
                    className="btn-small"
                    href={getLocalizedAuthPath('', pathname)}
                    aria-label={content.nav.signIn}
                    onClick={() =>
                      trackCtaClick('primary_cta_click', {
                        cta_location: 'nav_sign_in',
                        cta_text: content.nav.signIn
                      })
                    }
                  >
                    <svg viewBox="0 0 24 24" width="20" height="20" stroke="currentColor" strokeWidth="2" fill="none" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"></path>
                      <circle cx="12" cy="7" r="4"></circle>
                    </svg>
                  </a>
                )}
              </div>
            </div>
          </div>
        </nav>

        <main>
          <section id="channels" className="hero-section">
          {enableMouseField ? <MouseField theme={theme} /> : null}
          <div className="halo-effect"></div>
          <div className="container hero-content hero-showcase-layout">
            <div className="hero-copy hero-copy-showcase">
              <p className="hero-eyebrow">{content.hero.eyebrow}</p>
              <h1 className="hero-title">{content.hero.title}</h1>
              <p className="hero-subtitle">{content.hero.subtitle}</p>
              <div className="hero-value-strip" aria-label={content.hero.entryEyebrow}>
                {content.hero.proofPills.map((item) => (
                  <span key={item} className="hero-value-pill">
                    {item}
                  </span>
                ))}
              </div>
              <div className="hero-cta-row">
                <a
                  className="btn btn-primary hero-primary-cta"
                  href={heroPrimaryHref}
                  onClick={() =>
                    trackCtaClick('primary_cta_click', {
                      cta_location: 'hero_primary_get_dowhiz_free',
                      cta_text: content.hero.primaryCta,
                      landing_page_variant: LANDING_PAGE_VARIANT
                    })
                  }
                >
                  {content.hero.primaryCta}
                </a>
              </div>
              <p className="hero-support-note">{content.hero.supportNote || content.hero.toolsFootnote}</p>
            </div>

            <div
              className={`hero-channel-showcase${showcasePaused || prefersReducedMotion ? ' is-paused' : ''}`}
              onMouseEnter={() => setShowcasePaused(true)}
              onMouseLeave={() => setShowcasePaused(false)}
              onFocusCapture={() => setShowcasePaused(true)}
              onBlurCapture={(event) => {
                if (!event.currentTarget.contains(event.relatedTarget)) {
                  setShowcasePaused(false);
                }
              }}
            >
              <div className="hero-showcase-topbar">
                <div className="hero-showcase-heading hero-showcase-heading-stacked">
                  <span className="hero-panel-kicker">{content.hero.previewLabel}</span>
                  <p className="hero-showcase-summary">{content.hero.showcaseSummary}</p>
                </div>
                <div className="hero-operator-chip">
                  <div className="hero-operator-portrait">
                    <img src={oliverImg} alt={content.hero.operatorName || 'Oliver'} className="hero-portrait" />
                  </div>
                  <div className="hero-operator-copy">
                    <strong>{content.hero.operatorName || 'Oliver'}</strong>
                    <span>{content.hero.operatorRole || (isChinesePage ? '可信 AI operator' : 'Trusted AI operator')}</span>
                  </div>
                </div>
              </div>

              <div className="hero-channel-rail" role="group" aria-label={content.hero.toolsEyebrow}>
                <div className="hero-channel-group">
                  <div className="hero-channel-group-head">
                    <span className="hero-channel-group-label">{content.hero.primaryChannelsLabel}</span>
                    <span className="hero-channel-group-caption">
                      {showcasePaused || prefersReducedMotion
                        ? content.hero.pausedLabel
                        : content.hero.autoplayLabel}
                    </span>
                  </div>
                  <div className="hero-channel-dock hero-channel-dock-primary">
                    {primaryHeroTools.map((tool) => (
                      <button
                        key={tool.key}
                        type="button"
                        className={`hero-channel-pill${tool.index === activeShowcaseIndex ? ' is-active' : ''}`}
                        style={{ '--tool-accent': tool.accent }}
                        onMouseEnter={() => setActiveShowcaseIndex(tool.index)}
                        onFocus={() => setActiveShowcaseIndex(tool.index)}
                        onClick={() => handleHeroToolAction(tool, 'hero_channel_primary')}
                        disabled={Boolean(loadingToolKey)}
                      >
                        <span className="hero-channel-pill-main">
                          <span className="hero-tool-badge hero-tool-badge-pill" aria-hidden="true">
                            <HeroToolIcon toolKey={tool.key} />
                          </span>
                          <span className="hero-channel-pill-copy">
                            <strong>{tool.label}</strong>
                            <span>{tool.pillLabel || tool.anonymousActionLabel}</span>
                          </span>
                        </span>
                        <span className="hero-channel-pill-progress" aria-hidden="true">
                          <span className="hero-channel-pill-progress-fill"></span>
                        </span>
                      </button>
                    ))}
                  </div>
                </div>

                <div className="hero-channel-group hero-channel-group-secondary">
                  <div className="hero-channel-group-head">
                    <span className="hero-channel-group-label">{content.hero.secondaryChannelsLabel}</span>
                  </div>
                  <div className="hero-channel-dock hero-channel-dock-secondary">
                    {secondaryHeroTools.map((tool) => (
                      <button
                        key={tool.key}
                        type="button"
                        className={`hero-channel-pill hero-channel-pill-secondary${tool.index === activeShowcaseIndex ? ' is-active' : ''}`}
                        style={{ '--tool-accent': tool.accent }}
                        onMouseEnter={() => setActiveShowcaseIndex(tool.index)}
                        onFocus={() => setActiveShowcaseIndex(tool.index)}
                        onClick={() => handleHeroToolAction(tool, 'hero_channel_secondary')}
                        disabled={Boolean(loadingToolKey)}
                      >
                        <span className="hero-channel-pill-main">
                          <span className="hero-tool-badge hero-tool-badge-pill" aria-hidden="true">
                            <HeroToolIcon toolKey={tool.key} />
                          </span>
                          <span className="hero-channel-pill-copy">
                            <strong>{tool.label}</strong>
                            <span>{tool.authenticatedStatus || tool.anonymousStatus}</span>
                          </span>
                        </span>
                      </button>
                    ))}
                  </div>
                </div>
              </div>

              {activeHeroTool ? (
                <article
                  key={activeHeroTool.key}
                  className={`hero-channel-stage hero-channel-stage-${activeHeroTool.key}`}
                  style={{ '--tool-accent': activeHeroTool.accent }}
                >
                  <div className="hero-channel-stage-head">
                    <div className="hero-channel-stage-title">
                      <span className="hero-tool-badge hero-tool-badge-stage" aria-hidden="true">
                        <HeroToolIcon toolKey={activeHeroTool.key} />
                      </span>
                      <div className="hero-channel-stage-copy">
                        <h2>{activeHeroTool.label}</h2>
                        <p>{getHeroToolStatus(activeHeroTool)}</p>
                      </div>
                    </div>
                    <button
                      type="button"
                      className="btn btn-primary hero-stage-action"
                      onClick={() => handleHeroToolAction(activeHeroTool, 'hero_channel_stage')}
                      disabled={Boolean(loadingToolKey)}
                    >
                      {getHeroToolActionLabel(activeHeroTool)}
                    </button>
                  </div>

                  {renderHeroStageBody(activeHeroTool)}
                </article>
              ) : null}

              <div className="hero-channel-footnote">
                <p>{content.hero.toolsFootnote}</p>
                <a
                  href={settingsHref}
                  onClick={() =>
                    trackCtaClick('secondary_cta_click', {
                      cta_location: 'hero_manage_setup',
                      cta_text: manageSetupLabel,
                      landing_page_variant: LANDING_PAGE_VARIANT
                    })
                  }
                >
                  {manageSetupLabel}
                </a>
              </div>
            </div>
          </div>
        </section>

          {isChinesePage ? (
            <>
              <section id="watch" className="section demo-showcase-section">
                <div className="container">
                  <div className="section-heading-shell">
                    <span className="section-kicker">{content.demo.eyebrow}</span>
                    <h2 className="section-title section-title-left">{content.demo.title}</h2>
                    <p className="section-intro section-intro-left">{content.demo.intro}</p>
                  </div>

                  <div className="demo-showcase-grid">
                    <article className="demo-feature-card">
                      <div className="demo-card-head">
                        <div>
                          <h3>{content.demo.desktopTitle}</h3>
                          <p>{content.demo.desktopDescription}</p>
                        </div>
                        <a
                          className="demo-inline-link"
                          href={content.demo.desktopVideoHref}
                          target="_blank"
                          rel="noopener noreferrer"
                        >
                          {content.demo.desktopCta}
                        </a>
                      </div>
                      <div className="frame-shell frame-landscape">
                        <iframe
                          src={`https://www.youtube.com/embed/${content.demo.desktopVideoId}?rel=0`}
                          title={content.demo.desktopTitle}
                          loading="lazy"
                          allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
                          referrerPolicy="strict-origin-when-cross-origin"
                          allowFullScreen
                        ></iframe>
                      </div>
                    </article>

                    <aside className="demo-short-rail">
                      <div className="demo-short-head">
                        <h3>{content.demo.shortsTitle}</h3>
                        <p>{content.demo.shortsDescription}</p>
                      </div>
                      <div className="demo-short-grid">
                        {content.demo.shorts.map((item) => (
                          <article key={item.videoId} className="demo-short-card">
                            <div className="frame-shell frame-portrait">
                              <iframe
                                src={`https://www.youtube.com/embed/${item.videoId}?rel=0`}
                                title={item.title}
                                loading="lazy"
                                allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
                                referrerPolicy="strict-origin-when-cross-origin"
                                allowFullScreen
                              ></iframe>
                            </div>
                            <a
                              className="demo-short-link"
                              href={item.href}
                              target="_blank"
                              rel="noopener noreferrer"
                            >
                              {item.title}
                            </a>
                          </article>
                        ))}
                      </div>
                    </aside>
                  </div>
                </div>
              </section>

              <section id="examples" className="section example-showcase-section">
                <div className="container story-stack">
                  <div className="section-heading-shell">
                    <span className="section-kicker">{content.examples.eyebrow}</span>
                    <h2 className="section-title section-title-left">{content.examples.title}</h2>
                    <p className="section-intro section-intro-left">{content.examples.intro}</p>
                  </div>

                  <div className="example-card-grid">
                    {content.examples.cards.map((item) => (
                      <a
                        key={item.title}
                        className={`example-card${item.href ? ' example-card-link' : ''}`}
                        href={item.href || undefined}
                      >
                        <span className="example-card-tag">{item.tag}</span>
                        <h3>{item.title}</h3>
                        <p>{item.description}</p>
                        {item.ctaLabel ? <span className="example-card-cta">{item.ctaLabel}</span> : null}
                      </a>
                    ))}
                  </div>
                </div>
              </section>
            </>
          ) : (
            <>
              <section id="workflow" className="section workflow-visual-section">
                <div className="container story-stack">
                  <div className="section-heading-shell">
                    <span className="section-kicker">{content.workflowVisual.eyebrow}</span>
                    <h2 className="section-title section-title-left">{content.workflowVisual.title}</h2>
                    <p className="section-intro section-intro-left">{content.workflowVisual.subtitle}</p>
                  </div>

                  <div className="workflow-visual-flow">
                    {content.workflowVisual.stages.map((stage, index) => (
                      <article
                        key={stage.key}
                        className="workflow-visual-step"
                        data-app={stage.key}
                      >
                        <div className="workflow-visual-card">
                          <div className="workflow-visual-card-head">
                            <span className="workflow-visual-app">{stage.app}</span>
                            <span className="workflow-visual-index">{String(index + 1).padStart(2, '0')}</span>
                          </div>
                          <h3>{stage.title}</h3>
                          <p className="workflow-visual-copy">{stage.copy}</p>
                        </div>
                      </article>
                    ))}
                  </div>

                  <p className="workflow-visual-note">{content.workflowVisual.note}</p>
                </div>
              </section>

              <section id="outputs" className="section outputs-compact-section">
                <div className="container story-stack">
                  <div className="section-heading-shell">
                    <span className="section-kicker">{content.artifactRail.eyebrow}</span>
                    <h2 className="section-title section-title-left">{content.artifactRail.title}</h2>
                    <p className="section-intro section-intro-left">{content.artifactRail.subtitle}</p>
                  </div>

                  <div className="artifact-rail">
                    {content.artifactRail.items.map((item) => (
                      <article key={item.title} className="example-card artifact-card">
                        <h3>{item.title}</h3>
                        <p>{item.description}</p>
                      </article>
                    ))}
                  </div>
                </div>
              </section>
            </>
          )}

          {isChinesePage ? (
            <section id="faq" className="section faq-section">
              <div className="container">
                <div className="section-heading-shell">
                  <span className="section-kicker">{content.labels.faqEyebrow}</span>
                  <h2 className="section-title section-title-left">{content.labels.faqTitle}</h2>
                  <p className="section-intro section-intro-left">{content.labels.faqIntro}</p>
                </div>
                <div className="faq-accordion faq-compact">
                  {content.faqItems.map((item, idx) => {
                    const isOpen = openFaq === idx;
                    return (
                      <article key={item.question} className={`faq-accordion-item ${isOpen ? 'open' : ''}`}>
                        <button
                          type="button"
                          className="faq-accordion-header"
                          onClick={() => toggleFaq(idx)}
                          aria-expanded={isOpen}
                          aria-controls={`faq-panel-${idx}`}
                        >
                          <span className="faq-question">{item.question}</span>
                          <span className="faq-toggle" aria-hidden="true">
                            {isOpen ? '−' : '+'}
                          </span>
                        </button>
                        <div
                          id={`faq-panel-${idx}`}
                          className="faq-accordion-panel"
                          style={{ display: isOpen ? 'block' : 'none' }}
                        >
                          <p>{item.answer}</p>
                        </div>
                      </article>
                    );
                  })}
                </div>
                <div className="faq-link-row">
                  <a className="faq-text-link" href="/help-center/">
                    {content.labels.faqLinkLabel}
                  </a>
                </div>
              </div>
            </section>
          ) : null}

        </main>

        <footer className="site-footer">
          <div className="container footer-content">
            <div className="footer-brand">
              <a href={localizedHomePath} className="footer-logo">
                <img src="/assets/DoWhiz.svg" alt="" className="footer-brand-mark" aria-hidden="true" />
                <span>Do<span className="text-gradient">Whiz</span></span>
              </a>
              <p className="footer-tagline">{content.labels.footerTagline}</p>
              <div className="footer-pill">{content.labels.footerPill}</div>
            </div>
            <div className="footer-links">
              <span className="footer-title">{content.labels.footerTitle}</span>
              <div className="footer-link-grid">
                {content.footerLinks.map((link) => (
                  <a key={link.href} href={link.href} className="footer-link">
                    {link.label}
                  </a>
                ))}
                <a href={`mailto:${SUPPORT_EMAIL}`} className="footer-link">
                  {content.labels.footerContactLabel || (isChinesePage ? '联系 Oliver' : 'Contact Oliver')}
                </a>
              </div>
            </div>
          </div>
          <div className="container footer-bottom">
            <span>
              &copy; {new Date().getFullYear()} DoWhiz. {isChinesePage ? '保留所有权利。' : 'All rights reserved.'}
            </span>
            <span>{content.labels.footerBottomSecondary}</span>
          </div>
        </footer>
      </div>
    </div>
  );
}

export default LandingPage;
