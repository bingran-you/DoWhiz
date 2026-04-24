import { useEffect, useState } from 'react';
import {
  getOrCreateSessionId,
  persistAttributionFromLocation,
  trackAnalyticsEvent
} from '../analytics';
import { getThemeForLocalTime, LOCAL_THEME_CHANGE_EVENT, THEME_META_COLORS } from '../theme/localTheme';
import oliverAvatar from '../assets/Oliver-Avatar-Apr-23-2026.png';
import {
  OLIVER_AUTH_SIGN_IN_HREF,
  OLIVER_ENTRY_SURFACE,
  OLIVER_HOME_HREF,
  OLIVER_LAUNCH_HREF,
  OLIVER_LANDING_VARIANT,
  oliverLandingContent
} from './oliverLandingContent';

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

function DocumentIcon() {
  return (
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <path
        d="M6 3.5h5.8L15 6.7V16a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4.5a1 1 0 0 1 1-1Z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path d="M11.5 3.5V7H15" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      <path d="M7.5 10H12.5M7.5 13H12.5" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
    </svg>
  );
}

function AlertIcon() {
  return (
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <path
        d="M10 3.2 16.1 15a1 1 0 0 1-.9 1.5H4.8A1 1 0 0 1 3.9 15L10 3.2Z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinejoin="round"
      />
      <path d="M10 7.3v4.1M10 14.2h.01" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}

function CheckCircleIcon() {
  return (
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <circle cx="10" cy="10" r="6.5" fill="none" stroke="currentColor" strokeWidth="1.5" />
      <path
        d="m7.2 10.2 1.8 1.9 3.8-4"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function ArrowUpRightIcon() {
  return (
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <path
        d="M6 14L14 6M8 6h6v6"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function ProofCardIcon({ type }) {
  if (type === 'update') {
    return <DocumentIcon />;
  }
  if (type === 'risks') {
    return <AlertIcon />;
  }
  return <CheckCircleIcon />;
}

function ProofVisual({ visual }) {
  if (visual.type === 'update') {
    return (
      <div className="oliver-proof-visual oliver-proof-visual-update">
        <div className="oliver-proof-window-bar" aria-hidden="true">
          <span></span>
          <span></span>
          <span></span>
        </div>
        <div className="oliver-proof-update-head">
          <div>
            <p className="oliver-proof-window-kicker">Weekly update</p>
            <h4>{visual.windowTitle}</h4>
          </div>
          <span className="oliver-proof-status-pill is-risk">{visual.status}</span>
        </div>
        <div className="oliver-proof-metric-row">
          {visual.metrics.map((metric) => (
            <span key={metric} className="oliver-proof-metric-pill">
              {metric}
            </span>
          ))}
        </div>
        <div className="oliver-proof-bars" aria-hidden="true">
          {visual.lines.map((line) => (
            <div key={line.label} className="oliver-proof-bar-row">
              <span>{line.label}</span>
              <div className="oliver-proof-bar-track">
                <div className="oliver-proof-bar-fill" style={{ width: `${line.value}%` }}></div>
              </div>
            </div>
          ))}
        </div>
        <ul className="oliver-proof-note-list">
          {visual.bullets.map((bullet) => (
            <li key={bullet}>{bullet}</li>
          ))}
        </ul>
      </div>
    );
  }

  if (visual.type === 'risks') {
    return (
      <div className="oliver-proof-visual oliver-proof-visual-risks">
        <div className="oliver-proof-window-bar" aria-hidden="true">
          <span></span>
          <span></span>
          <span></span>
        </div>
        <div className="oliver-proof-table-head">
          <span>Risk</span>
          <span>Owner</span>
          <span>Review</span>
        </div>
        <div className="oliver-proof-risk-rows">
          {visual.rows.map((row) => (
            <div key={row.label} className="oliver-proof-risk-row">
              <div className="oliver-proof-risk-main">
                <span className={`oliver-proof-status-pill is-${row.tone}`}>{row.tone}</span>
                <strong>{row.label}</strong>
              </div>
              <span>{row.owner}</span>
              <span>{row.review}</span>
            </div>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="oliver-proof-visual oliver-proof-visual-follow-up">
      <div className="oliver-proof-window-bar" aria-hidden="true">
        <span></span>
        <span></span>
        <span></span>
      </div>
      <div className="oliver-proof-follow-up-list">
        {visual.rows.map((row) => (
          <div key={`${row.tool}-${row.owner}-${row.action}`} className="oliver-proof-follow-up-row">
            <span className="oliver-proof-tool-chip">{row.tool}</span>
            <div className="oliver-proof-follow-up-copy">
              <strong>{row.owner}</strong>
              <span>{row.action}</span>
            </div>
            <span className="oliver-proof-follow-up-state">{row.state}</span>
          </div>
        ))}
      </div>
      <div className="oliver-proof-follow-up-footer">
        <span>All follow-ups stay attached to the same program.</span>
      </div>
    </div>
  );
}

function OliverLandingPage() {
  const [theme, setTheme] = useState(() => getThemeForLocalTime());
  const content = oliverLandingContent;

  useEffect(() => {
    persistAttributionFromLocation();
    const sessionId = getOrCreateSessionId();

    trackAnalyticsEvent(
      'landing_page_view',
      {
        landing_page_variant: OLIVER_LANDING_VARIANT,
        landing_story: 'launch_execution',
        entry_surface: OLIVER_ENTRY_SURFACE,
        role_focus: 'launch_execution'
      },
      {
        eventKey: `landing_page_view:${sessionId}:/oliver`,
        routePath: '/oliver',
        pagePath:
          typeof window !== 'undefined'
            ? `${window.location.pathname}${window.location.search}`
            : '/oliver'
      }
    );
  }, []);

  useEffect(() => {
    if (typeof document === 'undefined') {
      return;
    }

    document.documentElement.lang = content.metadata.htmlLang;
    document.title = content.metadata.title;

    updateMetaContent('meta[name="description"]', content.metadata.description);
    updateMetaContent('meta[name="robots"]', content.metadata.robots);
    updateMetaContent('meta[property="og:title"]', content.metadata.title);
    updateMetaContent('meta[property="og:description"]', content.metadata.description);
    updateMetaContent('meta[property="og:url"]', content.metadata.canonicalUrl);
    updateMetaContent('meta[property="og:locale"]', content.metadata.ogLocale);
    updateMetaContent('meta[property="og:image"]', content.metadata.ogImage);
    updateMetaContent('meta[property="og:image:alt"]', content.metadata.ogImageAlt);
    updateMetaContent('meta[name="twitter:title"]', content.metadata.title);
    updateMetaContent('meta[name="twitter:description"]', content.metadata.description);
    updateMetaContent('meta[name="twitter:image"]', content.metadata.ogImage);
    updateLinkHref('link[rel="canonical"]', content.metadata.canonicalUrl);
  }, [content.metadata]);

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
    updateMetaContent('meta[name="theme-color"]', THEME_META_COLORS[theme] || content.metadata.themeColor);
  }, [content.metadata.themeColor, theme]);

  const trackLandingInteraction = (eventName, properties = {}) => {
    trackAnalyticsEvent(eventName, {
      landing_page_variant: OLIVER_LANDING_VARIANT,
      landing_story: 'launch_execution',
      entry_surface: OLIVER_ENTRY_SURFACE,
      ...properties
    });
  };

  return (
    <div className="app-container oliver-clarity-page">
      <div className="content-layer">
        <header className="oliver-clarity-header">
          <div className="container oliver-clarity-topbar">
            <a href={OLIVER_HOME_HREF} className="oliver-clarity-brand" aria-label="Back to the DoWhiz homepage">
              <img src="/assets/DoWhiz.svg" alt="" className="oliver-clarity-brand-mark" aria-hidden="true" />
              <span className="oliver-clarity-brand-copy">DoWhiz</span>
            </a>

            <div className="oliver-clarity-topbar-actions">
              <a
                href={OLIVER_HOME_HREF}
                className="oliver-clarity-inline-link oliver-clarity-topbar-link"
                onClick={() =>
                  trackLandingInteraction('secondary_cta_click', {
                    cta_location: 'header_home',
                    cta_text: content.nav.homeLabel
                  })
                }
              >
                {content.nav.homeLabel}
              </a>
              <a
                href={OLIVER_AUTH_SIGN_IN_HREF}
                className="btn btn-secondary oliver-clarity-secondary-button"
                onClick={() =>
                  trackLandingInteraction('secondary_cta_click', {
                    cta_location: 'header_sign_in',
                    cta_text: content.nav.signInLabel
                  })
                }
              >
                {content.nav.signInLabel}
              </a>
              <a
                href={OLIVER_LAUNCH_HREF}
                className="btn btn-primary oliver-clarity-primary-button"
                onClick={() =>
                  trackLandingInteraction('primary_cta_click', {
                    cta_location: 'header_primary',
                    cta_text: content.nav.primaryCta
                  })
                }
              >
                {content.nav.primaryCta}
              </a>
            </div>
          </div>
        </header>

        <main className="oliver-clarity-main">
          <section className="oliver-clarity-hero">
            <div className="container oliver-clarity-hero-shell">
              <div className="oliver-clarity-badge">{content.hero.badge}</div>

              <div className="oliver-clarity-portrait-stage" aria-hidden="true">
                {content.hero.orbitSignals.map((signal, index) => (
                  <span
                    key={signal}
                    className={`oliver-clarity-orbit-chip is-${index + 1}`}
                  >
                    {signal}
                  </span>
                ))}
                <div className="oliver-clarity-portrait-ring">
                  <div className="oliver-clarity-portrait-ring-outer"></div>
                  <div className="oliver-clarity-portrait-ring-inner">
                    <img src={oliverAvatar} alt="" className="oliver-clarity-portrait" />
                  </div>
                  <span className="oliver-clarity-portrait-status"></span>
                </div>
              </div>

              <div className="oliver-clarity-hero-copy">
                <p className="oliver-clarity-eyebrow">{content.hero.eyebrow}</p>
                <h1 className="oliver-clarity-title">{content.hero.title}</h1>
                <p className="oliver-clarity-subtitle">{content.hero.subtitle}</p>
                <div className="oliver-clarity-activity-pill">{content.hero.activity}</div>
                <div className="oliver-clarity-cta-row">
                  <a
                    className="btn btn-primary oliver-clarity-primary-button"
                    href={OLIVER_LAUNCH_HREF}
                    onClick={() =>
                      trackLandingInteraction('primary_cta_click', {
                        cta_location: 'hero_primary',
                        cta_text: content.hero.primaryCta
                      })
                    }
                  >
                    {content.hero.primaryCta}
                  </a>
                  <a
                    className="oliver-clarity-inline-link"
                    href="#output-samples"
                    onClick={() =>
                      trackLandingInteraction('secondary_cta_click', {
                        cta_location: 'hero_secondary',
                        cta_text: content.hero.secondaryCta
                      })
                    }
                  >
                    {content.hero.secondaryCta}
                    <ArrowUpRightIcon />
                  </a>
                </div>
                <div className="oliver-clarity-proof-strip" aria-label="Primary outputs">
                  {content.hero.proofPills.map((item) => (
                    <span key={item} className="oliver-clarity-proof-pill">
                      {item}
                    </span>
                  ))}
                </div>
              </div>
            </div>
          </section>

          <section className="oliver-clarity-section oliver-clarity-workflow-section">
            <div className="container">
              <div className="oliver-clarity-section-head oliver-clarity-section-head-centered">
                <p className="oliver-clarity-section-eyebrow">{content.workflow.eyebrow}</p>
                <h2 className="oliver-clarity-section-title">{content.workflow.title}</h2>
                <p className="oliver-clarity-section-intro">{content.workflow.subtitle}</p>
              </div>

              <div className="oliver-clarity-orchestration-card">
                <div className="oliver-clarity-orchestration-column">
                  <p className="oliver-clarity-column-label">{content.workflow.signalsLabel}</p>
                  <div className="oliver-clarity-signal-stack">
                    {content.workflow.signals.map((signal) => (
                      <article key={signal.title} className={`oliver-clarity-signal-card is-${signal.tone}`}>
                        <strong>{signal.title}</strong>
                        <span>{signal.meta}</span>
                      </article>
                    ))}
                  </div>
                </div>

                <div className="oliver-clarity-orchestration-core">
                  <div className="oliver-clarity-core-avatar">
                    <img src={oliverAvatar} alt="" aria-hidden="true" className="oliver-clarity-core-portrait" />
                  </div>
                  <p className="oliver-clarity-column-label">{content.workflow.coreLabel}</p>
                  <h3>{content.workflow.coreTitle}</h3>
                  <p>{content.workflow.coreSubtitle}</p>
                </div>

                <div className="oliver-clarity-orchestration-column">
                  <p className="oliver-clarity-column-label">{content.workflow.outputsLabel}</p>
                  <div className="oliver-clarity-output-stack">
                    {content.workflow.outputs.map((output) => (
                      <article key={output.title} className={`oliver-clarity-output-stack-card is-${output.tone}`}>
                        <strong>{output.title}</strong>
                        <span>{output.meta}</span>
                      </article>
                    ))}
                  </div>
                </div>
              </div>
            </div>
          </section>

          <section className="oliver-clarity-section oliver-clarity-proof-section" id="output-samples">
            <div className="container">
              <div className="oliver-clarity-section-head">
                <p className="oliver-clarity-section-eyebrow">{content.proof.eyebrow}</p>
                <h2 className="oliver-clarity-section-title">{content.proof.title}</h2>
              </div>

              <div className="oliver-clarity-proof-grid">
                {content.proof.cards.map((card) => (
                  <article key={card.key} className="oliver-clarity-proof-card">
                    <div className="oliver-clarity-proof-copy">
                      <span className="oliver-clarity-proof-icon">
                        <ProofCardIcon type={card.visual.type} />
                      </span>
                      <p className="oliver-clarity-section-eyebrow">{card.eyebrow}</p>
                      <h3>{card.title}</h3>
                      <p>{card.description}</p>
                    </div>
                    <ProofVisual visual={card.visual} />
                  </article>
                ))}
              </div>
            </div>
          </section>

          <section className="oliver-clarity-section oliver-clarity-trust-section">
            <div className="container">
              <div className="oliver-clarity-trust-band">
                <div className="oliver-clarity-section-head oliver-clarity-section-head-compact">
                  <p className="oliver-clarity-section-eyebrow">{content.trust.eyebrow}</p>
                  <h2 className="oliver-clarity-section-title">{content.trust.title}</h2>
                </div>

                <div className="oliver-clarity-trust-row">
                  {content.trust.items.map((item) => (
                    <span key={item} className="oliver-clarity-trust-pill">
                      {item}
                    </span>
                  ))}
                </div>

                <div className="oliver-clarity-trust-actions">
                  <a
                    className="btn btn-primary oliver-clarity-primary-button"
                    href={OLIVER_LAUNCH_HREF}
                    onClick={() =>
                      trackLandingInteraction('primary_cta_click', {
                        cta_location: 'trust_primary',
                        cta_text: content.trust.primaryCta
                      })
                    }
                  >
                    {content.trust.primaryCta}
                  </a>
                  <a
                    className="btn btn-secondary oliver-clarity-secondary-button"
                    href={OLIVER_AUTH_SIGN_IN_HREF}
                    onClick={() =>
                      trackLandingInteraction('secondary_cta_click', {
                        cta_location: 'trust_secondary',
                        cta_text: content.trust.secondaryCta
                      })
                    }
                  >
                    {content.trust.secondaryCta}
                  </a>
                </div>
              </div>
            </div>
          </section>
        </main>
      </div>
    </div>
  );
}

export default OliverLandingPage;
