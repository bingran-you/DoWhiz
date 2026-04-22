import { useEffect, useState } from 'react';
import {
  getOrCreateSessionId,
  persistAttributionFromLocation,
  trackAnalyticsEvent
} from '../analytics';
import { getThemeForLocalTime, LOCAL_THEME_CHANGE_EVENT, THEME_META_COLORS } from '../theme/localTheme';
import {
  OLIVER_AUTH_OVERVIEW_HREF,
  OLIVER_AUTH_WORK_HREF,
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

function CheckLineIcon() {
  return (
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <path
        d="M4.5 10.5l3.3 3.3L15.5 6"
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
    <div className="app-container oliver-hook-page">
      <div className="oliver-hook-noise" aria-hidden="true" />
      <div className="content-layer">
        <header className="oliver-hook-nav">
          <div className="oliver-hook-nav-inner">
            <a href="/" className="oliver-hook-brand" aria-label="Back to the DoWhiz homepage">
              <img src="/assets/DoWhiz.svg" alt="" className="oliver-hook-brand-mark" aria-hidden="true" />
              <span className="oliver-hook-brand-copy">
                Do<span className="text-gradient">Whiz</span>
              </span>
            </a>

            <nav className="oliver-hook-links" aria-label="Oliver page sections">
              {content.nav.links.map((link) => (
                <a
                  key={link.href}
                  href={link.href}
                  className="oliver-hook-link"
                  onClick={() =>
                    trackLandingInteraction('secondary_cta_click', {
                      cta_location: 'oliver_nav',
                      cta_text: link.label
                    })
                  }
                >
                  {link.label}
                </a>
              ))}
            </nav>

            <div className="oliver-hook-nav-actions">
              <a
                className="btn btn-secondary oliver-hook-nav-secondary"
                href={OLIVER_AUTH_WORK_HREF}
                onClick={() =>
                  trackLandingInteraction('secondary_cta_click', {
                    cta_location: 'oliver_nav_work',
                    cta_text: content.nav.secondaryCta
                  })
                }
              >
                {content.nav.secondaryCta}
              </a>
              <a
                className="btn btn-primary oliver-hook-nav-primary"
                href={OLIVER_AUTH_OVERVIEW_HREF}
                onClick={() =>
                  trackLandingInteraction('primary_cta_click', {
                    cta_location: 'oliver_nav_open',
                    cta_text: content.nav.primaryCta
                  })
                }
              >
                {content.nav.primaryCta}
              </a>
            </div>
          </div>
        </header>

        <main className="oliver-hook-main">
          <section className="oliver-hook-hero" id="top">
            <div className="container oliver-hook-hero-grid">
              <div className="oliver-hook-copy">
                <p className="oliver-hook-eyebrow">{content.hero.eyebrow}</p>
                <h1 className="oliver-hook-title">{content.hero.title}</h1>
                <p className="oliver-hook-subtitle">{content.hero.subtitle}</p>

                <div className="oliver-hook-pill-row" aria-label="Key TPM outputs">
                  {content.hero.proofPills.map((item) => (
                    <span key={item} className="oliver-hook-pill">
                      {item}
                    </span>
                  ))}
                </div>

                <div className="oliver-hook-cta-row">
                  <a
                    className="btn btn-primary oliver-hook-primary-button"
                    href={OLIVER_AUTH_OVERVIEW_HREF}
                    onClick={() =>
                      trackLandingInteraction('primary_cta_click', {
                        cta_location: 'hero_primary_open_oliver',
                        cta_text: content.hero.primaryCta
                      })
                    }
                  >
                    {content.hero.primaryCta}
                  </a>
                  <a
                    className="btn btn-secondary oliver-hook-secondary-button"
                    href="#proof"
                    onClick={() =>
                      trackLandingInteraction('secondary_cta_click', {
                        cta_location: 'hero_secondary_outputs',
                        cta_text: content.hero.secondaryCta
                      })
                    }
                  >
                    {content.hero.secondaryCta}
                  </a>
                </div>

                <p className="oliver-hook-note">{content.hero.note}</p>
              </div>

              <aside className="oliver-hook-hero-panel" aria-label="Oliver launch review preview">
                <div className="oliver-hook-hero-panel-head">
                  <div>
                    <p className="oliver-hook-panel-label">{content.hero.artifact.label}</p>
                    <h2>{content.hero.artifact.title}</h2>
                  </div>
                  <div className="oliver-hook-health">
                    <span className="oliver-hook-health-badge">{content.hero.artifact.healthLabel}</span>
                    <span className="oliver-hook-health-detail">{content.hero.artifact.healthDetail}</span>
                  </div>
                </div>

                <div className="oliver-hook-signal-band" aria-label="Signal sources">
                  {content.hero.artifact.signalSources.map((item) => (
                    <span key={item} className="oliver-hook-signal-chip">
                      {item}
                    </span>
                  ))}
                </div>

                <div className="oliver-hook-hero-panel-grid">
                  <section className="oliver-hook-artifact-card oliver-hook-artifact-card-update">
                    <p className="oliver-hook-card-kicker">{content.hero.artifact.summary.label}</p>
                    <p className="oliver-hook-card-copy">{content.hero.artifact.summary.text}</p>
                  </section>

                  <section className="oliver-hook-artifact-card oliver-hook-artifact-card-actions">
                    <p className="oliver-hook-card-kicker">Actions ready</p>
                    <ul className="oliver-hook-checklist">
                      {content.hero.artifact.actions.map((item) => (
                        <li key={`${item.owner}-${item.task}`}>
                          <span className="oliver-hook-check-icon">
                            <CheckLineIcon />
                          </span>
                          <div>
                            <strong>{item.owner}</strong>
                            <span>{item.task}</span>
                          </div>
                          <small>{item.due}</small>
                        </li>
                      ))}
                    </ul>
                  </section>
                </div>

                <section className="oliver-hook-artifact-card oliver-hook-artifact-card-risks">
                  <div className="oliver-hook-card-row">
                    <p className="oliver-hook-card-kicker">Current risks</p>
                    <a
                      href={OLIVER_AUTH_WORK_HREF}
                      className="oliver-hook-inline-link"
                      onClick={() =>
                        trackLandingInteraction('secondary_cta_click', {
                          cta_location: 'hero_risk_register',
                          cta_text: 'Open current work view'
                        })
                      }
                    >
                      Open current work view
                      <ArrowUpRightIcon />
                    </a>
                  </div>
                  <ul className="oliver-hook-risk-list">
                    {content.hero.artifact.risks.map((item) => (
                      <li key={item.label}>
                        <span className={`oliver-hook-risk-level oliver-hook-risk-level-${item.level.toLowerCase()}`}>
                          {item.level}
                        </span>
                        <span>{item.label}</span>
                      </li>
                    ))}
                  </ul>
                </section>
              </aside>
            </div>
          </section>

          <section className="oliver-hook-section" id="proof">
            <div className="container">
              <div className="oliver-hook-section-head">
                <p className="oliver-hook-section-eyebrow">{content.proof.eyebrow}</p>
                <h2 className="oliver-hook-section-title">{content.proof.title}</h2>
                <p className="oliver-hook-section-intro">{content.proof.intro}</p>
              </div>

              <div className="oliver-hook-proof-grid">
                {content.proof.cards.map((card) => (
                  <article key={card.title} className="oliver-hook-proof-card">
                    <p className="oliver-hook-card-kicker">{card.eyebrow}</p>
                    <h3>{card.title}</h3>
                    <p>{card.summary}</p>
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

          <section className="oliver-hook-section" id="ownership">
            <div className="container">
              <div className="oliver-hook-section-head">
                <p className="oliver-hook-section-eyebrow">{content.ownership.eyebrow}</p>
                <h2 className="oliver-hook-section-title">{content.ownership.title}</h2>
                <p className="oliver-hook-section-intro">{content.ownership.intro}</p>
              </div>

              <div className="oliver-hook-outcome-grid">
                {content.ownership.cards.map((card) => (
                  <article key={card.title} className="oliver-hook-outcome-card">
                    <h3>{card.title}</h3>
                    <p>{card.description}</p>
                  </article>
                ))}
              </div>
            </div>
          </section>

          <section className="oliver-hook-section oliver-hook-workflow-section" id="workflow">
            <div className="container oliver-hook-workflow-layout">
              <div className="oliver-hook-section-head oliver-hook-section-head-left">
                <p className="oliver-hook-section-eyebrow">{content.workflow.eyebrow}</p>
                <h2 className="oliver-hook-section-title">{content.workflow.title}</h2>
              </div>

              <div className="oliver-hook-workflow-list">
                {content.workflow.steps.map((step) => (
                  <article key={step.label} className="oliver-hook-step-card">
                    <div className="oliver-hook-step-label">{step.label}</div>
                    <div className="oliver-hook-step-copy">
                      <h3>{step.title}</h3>
                      <p>{step.description}</p>
                    </div>
                  </article>
                ))}
              </div>
            </div>
          </section>

          <section className="oliver-hook-section oliver-hook-tools-section" id="tools">
            <div className="container oliver-hook-surface-card">
              <div className="oliver-hook-surface-copy">
                <p className="oliver-hook-section-eyebrow">{content.tools.eyebrow}</p>
                <h2 className="oliver-hook-section-title">{content.tools.title}</h2>
                <p className="oliver-hook-section-intro oliver-hook-section-intro-left">{content.tools.intro}</p>
              </div>

              <div className="oliver-hook-tool-chip-grid" aria-label="Signal sources Oliver can work across">
                {content.tools.items.map((item) => (
                  <span key={item} className="oliver-hook-tool-chip">
                    {item}
                  </span>
                ))}
              </div>
            </div>
          </section>

          <section className="oliver-hook-section" id="controls">
            <div className="container">
              <div className="oliver-hook-section-head">
                <p className="oliver-hook-section-eyebrow">{content.controls.eyebrow}</p>
                <h2 className="oliver-hook-section-title">{content.controls.title}</h2>
                <p className="oliver-hook-section-intro">{content.controls.intro}</p>
              </div>

              <div className="oliver-hook-control-grid">
                {content.controls.cards.map((card) => (
                  <article key={card.title} className="oliver-hook-control-card">
                    <h3>{card.title}</h3>
                    <p>{card.description}</p>
                  </article>
                ))}
              </div>
            </div>
          </section>

          <section className="oliver-hook-section" id="faq">
            <div className="container">
              <div className="oliver-hook-section-head">
                <p className="oliver-hook-section-eyebrow">{content.faq.eyebrow}</p>
                <h2 className="oliver-hook-section-title">{content.faq.title}</h2>
              </div>

              <div className="oliver-hook-faq-grid">
                {content.faq.items.map((item) => (
                  <article key={item.question} className="oliver-hook-faq-card">
                    <h3>{item.question}</h3>
                    <p>{item.answer}</p>
                  </article>
                ))}
              </div>
            </div>
          </section>

          <section className="oliver-hook-section oliver-hook-final-section">
            <div className="container">
              <div className="oliver-hook-final-card">
                <p className="oliver-hook-section-eyebrow">{content.finalCta.eyebrow}</p>
                <h2 className="oliver-hook-section-title">{content.finalCta.title}</h2>
                <p className="oliver-hook-section-intro">{content.finalCta.description}</p>

                <div className="oliver-hook-cta-row oliver-hook-final-actions">
                  <a
                    className="btn btn-primary oliver-hook-primary-button"
                    href={OLIVER_AUTH_OVERVIEW_HREF}
                    onClick={() =>
                      trackLandingInteraction('primary_cta_click', {
                        cta_location: 'final_primary_open_oliver',
                        cta_text: content.finalCta.primaryCta
                      })
                    }
                  >
                    {content.finalCta.primaryCta}
                  </a>
                  <a
                    className="btn btn-secondary oliver-hook-secondary-button"
                    href={OLIVER_AUTH_WORK_HREF}
                    onClick={() =>
                      trackLandingInteraction('secondary_cta_click', {
                        cta_location: 'final_secondary_work_view',
                        cta_text: content.finalCta.secondaryCta
                      })
                    }
                  >
                    {content.finalCta.secondaryCta}
                  </a>
                </div>

                <p className="oliver-hook-disclaimer">{content.finalCta.disclaimer}</p>
              </div>
            </div>
          </section>
        </main>
      </div>
    </div>
  );
}

export default OliverLandingPage;
