import { useEffect, useState } from 'react';
import {
  getOrCreateSessionId,
  persistAttributionFromLocation,
  trackAnalyticsEvent
} from '../analytics';
import { getThemeForLocalTime, LOCAL_THEME_CHANGE_EVENT, THEME_META_COLORS } from '../theme/localTheme';
import {
  OLIVER_AUTH_OVERVIEW_HREF,
  OLIVER_ENTRY_SURFACE,
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

function CheckLineIcon() {
  return (
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <path
        d="M4.5 10.5l3.3 3.3L15.5 6"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
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
        landing_story: 'tpm_hook',
        entry_surface: OLIVER_ENTRY_SURFACE,
        role_focus: 'tpm'
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
      landing_story: 'tpm_hook',
      entry_surface: OLIVER_ENTRY_SURFACE,
      ...properties
    });
  };

  return (
    <div className="app-container oliver-clarity-page">
      <div className="content-layer">
        <header className="oliver-clarity-header">
          <div className="container oliver-clarity-topbar">
            <a href="/" className="oliver-clarity-brand" aria-label="Back to the DoWhiz homepage">
              <img src="/assets/DoWhiz.svg" alt="" className="oliver-clarity-brand-mark" aria-hidden="true" />
              <span className="oliver-clarity-brand-copy">
                Do<span className="text-gradient">Whiz</span>
              </span>
            </a>

            <div className="oliver-clarity-topbar-actions">
              <a
                href="#sample-update"
                className="oliver-clarity-proof-link"
                onClick={() =>
                  trackLandingInteraction('secondary_cta_click', {
                    cta_location: 'topbar_proof',
                    cta_text: content.nav.proofLink
                  })
                }
              >
                {content.nav.proofLink}
              </a>
              <a
                className="btn btn-primary oliver-clarity-nav-cta"
                href={OLIVER_AUTH_OVERVIEW_HREF}
                onClick={() =>
                  trackLandingInteraction('primary_cta_click', {
                    cta_location: 'topbar_primary',
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
            <div className="container oliver-clarity-hero-grid">
              <div className="oliver-clarity-copy">
                <p className="oliver-clarity-eyebrow">{content.hero.eyebrow}</p>
                <h1 className="oliver-clarity-title">{content.hero.title}</h1>
                <p className="oliver-clarity-subtitle">{content.hero.subtitle}</p>

                <ul className="oliver-clarity-highlight-list" aria-label="Key outputs">
                  {content.hero.highlights.map((item) => (
                    <li key={item} className="oliver-clarity-highlight-item">
                      <span className="oliver-clarity-highlight-icon">
                        <CheckLineIcon />
                      </span>
                      <span>{item}</span>
                    </li>
                  ))}
                </ul>

                <div className="oliver-clarity-cta-row">
                  <a
                    className="btn btn-primary oliver-clarity-primary-button"
                    href={OLIVER_AUTH_OVERVIEW_HREF}
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
                    className="btn btn-secondary oliver-clarity-secondary-button"
                    href="#sample-update"
                    onClick={() =>
                      trackLandingInteraction('secondary_cta_click', {
                        cta_location: 'hero_secondary',
                        cta_text: content.hero.secondaryCta
                      })
                    }
                  >
                    {content.hero.secondaryCta}
                  </a>
                </div>
              </div>

              <aside
                className="oliver-clarity-update-card"
                id="sample-update"
                aria-label="Sample weekly update"
              >
                <div className="oliver-clarity-update-head">
                  <div>
                    <p className="oliver-clarity-card-kicker">{content.hero.artifact.label}</p>
                    <h2 className="oliver-clarity-card-title">{content.hero.artifact.program}</h2>
                  </div>
                  <div className="oliver-clarity-health-group">
                    <span className="oliver-clarity-health-badge">{content.hero.artifact.healthLabel}</span>
                    <span className="oliver-clarity-health-detail">{content.hero.artifact.healthDetail}</span>
                  </div>
                </div>

                <section className="oliver-clarity-card-section">
                  <p className="oliver-clarity-card-label">{content.hero.artifact.summaryLabel}</p>
                  <p className="oliver-clarity-card-summary">{content.hero.artifact.summary}</p>
                </section>

                <div className="oliver-clarity-update-grid">
                  <section className="oliver-clarity-card-section">
                    <p className="oliver-clarity-card-label">{content.hero.artifact.risksLabel}</p>
                    <ul className="oliver-clarity-issue-list">
                      {content.hero.artifact.risks.map((risk) => (
                        <li key={risk}>{risk}</li>
                      ))}
                    </ul>
                  </section>

                  <section className="oliver-clarity-card-section">
                    <p className="oliver-clarity-card-label">{content.hero.artifact.ownersLabel}</p>
                    <ul className="oliver-clarity-owner-list">
                      {content.hero.artifact.owners.map((item) => (
                        <li key={`${item.owner}-${item.action}`}>
                          <strong>{item.owner}</strong>
                          <span>{item.action}</span>
                        </li>
                      ))}
                    </ul>
                  </section>
                </div>

                <p className="oliver-clarity-source-note">
                  <span>{content.hero.artifact.sourceLabel}</span>
                  {content.hero.artifact.sources}
                </p>
              </aside>
            </div>
          </section>

          <section className="oliver-clarity-section oliver-clarity-section-outputs">
            <div className="container">
              <div className="oliver-clarity-section-head">
                <p className="oliver-clarity-section-eyebrow">{content.outputs.eyebrow}</p>
                <h2 className="oliver-clarity-section-title">{content.outputs.title}</h2>
              </div>

              <div className="oliver-clarity-output-grid">
                {content.outputs.cards.map((card) => (
                  <article key={card.title} className="oliver-clarity-surface-card">
                    <h3>{card.title}</h3>
                    <p>{card.description}</p>
                    <ul>
                      {card.bullets.map((bullet) => (
                        <li key={bullet}>{bullet}</li>
                      ))}
                    </ul>
                  </article>
                ))}
              </div>
            </div>
          </section>

          <section className="oliver-clarity-section oliver-clarity-section-setup">
            <div className="container">
              <div className="oliver-clarity-section-head">
                <p className="oliver-clarity-section-eyebrow">{content.setup.eyebrow}</p>
                <h2 className="oliver-clarity-section-title">{content.setup.title}</h2>
              </div>

              <div className="oliver-clarity-step-grid">
                {content.setup.steps.map((step) => (
                  <article key={step.label} className="oliver-clarity-step-card">
                    <div className="oliver-clarity-step-label">{step.label}</div>
                    <div className="oliver-clarity-step-copy">
                      <h3>{step.title}</h3>
                      <p>{step.description}</p>
                    </div>
                  </article>
                ))}
              </div>
            </div>
          </section>

          <section className="oliver-clarity-section oliver-clarity-section-trust">
            <div className="container">
              <div className="oliver-clarity-trust-panel">
                <div className="oliver-clarity-section-head oliver-clarity-section-head-compact">
                  <p className="oliver-clarity-section-eyebrow">{content.trust.eyebrow}</p>
                  <h2 className="oliver-clarity-section-title">{content.trust.title}</h2>
                  <p className="oliver-clarity-section-intro">{content.trust.intro}</p>
                </div>

                <div className="oliver-clarity-trust-grid">
                  {content.trust.items.map((item) => (
                    <article key={item.title} className="oliver-clarity-trust-card">
                      <h3>{item.title}</h3>
                      <p>{item.description}</p>
                    </article>
                  ))}
                </div>

                <div className="oliver-clarity-trust-actions">
                  <a
                    className="btn btn-primary oliver-clarity-primary-button"
                    href={OLIVER_AUTH_OVERVIEW_HREF}
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
                    href="#sample-update"
                    className="oliver-clarity-inline-link"
                    onClick={() =>
                      trackLandingInteraction('secondary_cta_click', {
                        cta_location: 'trust_secondary',
                        cta_text: 'See sample update'
                      })
                    }
                  >
                    See sample update
                    <ArrowUpRightIcon />
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
