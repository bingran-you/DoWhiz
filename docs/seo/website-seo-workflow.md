# Website SEO Workflow

This document describes the SEO content operations foundation for the DoWhiz website.

## Source of Truth

The website SEO workflow now centers on the files under `website/seo/`.

Primary files:

- `website/seo/content-registry.json`
  - Canonical inventory of indexable public pages.
  - Drives sitemap generation.
  - Drives the generated blog index sections for published blog posts.
- `website/seo/keywords.json`
  - Canonical list of tracked keywords, clusters, intent, priority, and target URLs.
  - Used by the reporting layer and future automation agents.

Supporting files:

- `website/seo/templates/blog-post.template.html`
  - Reusable blog post scaffold for future manual or agent-authored articles.
- `website/seo/fixtures/`
  - Sample local input files for validating the keyword report script without live credentials.
- `website/seo/metrics/`
  - Intended drop location for live Search Console and search-volume exports.
- `website/seo/exports/`
  - Generated machine-readable exports for downstream automation.

## Generated Artifacts

Run the generator from `website/`:

```bash
npm run seo:build
```

This command regenerates:

- `website/public/sitemap.xml`
- `website/public/blog/index.html`
  - Only the generated blog-link and blog-card sections
- `website/seo/exports/content-inventory.generated.json`
- `website/seo/exports/keyword-target-map.generated.json`

The generator also validates that every `file_path` in `website/seo/content-registry.json` exists before it updates the sitemap or blog index. This is intended to fail fast if a future agent adds a registry row without actually publishing the page file.

`website/public/blog/index.html` contains explicit generation markers. Do not remove them unless you also update `website/scripts/build-seo-artifacts.mjs`.

## Keyword Reporting

The reporting script joins:

- tracked keywords from `website/seo/keywords.json`
- content targets from `website/seo/content-registry.json`
- Search Console export data
- keyword search-volume export data

Default live-input paths:

- `website/seo/metrics/search_console_latest.csv`
- `website/seo/metrics/keyword_volume_latest.csv`

Run from `website/`:

```bash
npm run seo:report
```

For local validation with fixtures:

```bash
npm run seo:report -- \
  --search-console seo/fixtures/search_console_sample.csv \
  --keyword-volume seo/fixtures/keyword_volume_sample.csv
```

Outputs:

- `website/reports/seo-keyword-report-YYYY-MM-DD.md`
- `website/reports/seo-keyword-report-YYYY-MM-DD.json`

The markdown report includes:

- keyword
- target page
- average position
- impressions
- clicks
- CTR
- search volume
- recommended action

It also breaks out:

- keywords with no Search Console data
- missing target pages
- keywords with no volume data
- keywords ranking in positions 4-20
- high-impression low-CTR pages
- likely cannibalization candidates

Behavior notes:

- The report auto-discovers the newest matching metrics file in `website/seo/metrics/` if the preferred `*_latest.csv` name is missing.
- Search Console data is sufficient for ranking and CTR analysis.
- Keyword volume still informs prioritization, but missing volume alone should not fully block ranking-based recommendations.

## Adding a Keyword

1. Open `website/seo/keywords.json`.
2. Add a new row with:
   - `keyword`
   - `cluster`
   - `intent`
   - `target_url`
   - `language`
   - `country`
   - `priority`
   - `status`
   - `notes`
3. If the keyword has no target page yet, leave `target_url` empty and set `status` to something like `planned` or `research`.
4. Re-run:

```bash
cd website
npm run seo:build
```

## Adding a Blog Page

Recommended workflow:

1. Scaffold a page from the template:

```bash
cd website
npm run seo:new-blog -- \
  --slug your-new-slug \
  --headline "Your headline" \
  --description "Your meta description" \
  --owner Oliver \
  --primary-keyword "your target keyword"
```

2. Replace the placeholder sections in the generated `website/public/blog/<slug>/index.html`.
3. Add a matching page entry to `website/seo/content-registry.json`.
   - Make sure `include_in_sitemap` is true for a real published page.
   - Add the blog-index fields if the article should appear in the blog index:
     - `include_in_blog_index`
     - `blog_tag`
     - `blog_index_title`
     - `blog_related_description`
     - `blog_summary`
     - `blog_highlights`
4. Add or update a keyword entry in `website/seo/keywords.json`.
5. Re-run:

```bash
cd website
npm run seo:build
```

6. Validate:

```bash
cd website
npm run lint
npm run build
npm run seo:crawl
```

## Chinese `/cn` Decision

Current policy:

- `/cn` remains available as a user-facing localized entry point.
- `/cn` is not yet treated as a fully supported SEO surface.
- Chinese localized pages are currently marked `noindex, follow`.
- The English source pages remain the canonical SEO targets.
- The homepage no longer advertises `/cn` as an alternate hreflang target.

This reduces conflicting signals between:

- runtime localization
- canonical URLs
- sitemap coverage
- actual crawlable static content

If DoWhiz later decides to support Chinese SEO as a first-class surface, the follow-up work should include:

- a dedicated Chinese content inventory
- Chinese sitemap coverage
- stable Chinese canonical URLs
- page-by-page localized metadata and structured data
- a clear hreflang strategy across all public pages

## Guidance for Future Automation Agents

Future recurring SEO agents should:

1. Treat `website/seo/content-registry.json` and `website/seo/keywords.json` as the canonical control plane.
2. Update page files and those JSON files together.
3. Run `npm run seo:build` after changing public content or tracked keywords.
4. Run `npm run seo:report` only against real metric exports unless explicitly testing fixtures.
5. Avoid creating new pages when an existing target page already owns the same core intent.
6. Use the generated report to decide whether to:
   - refresh an existing page
   - improve title/meta for CTR
   - consolidate competing pages
   - create a genuinely new page

## Common Commands

From `website/`:

```bash
npm run seo:build
npm run seo:report
npm run seo:new-blog -- --slug my-post --headline "My headline" --description "Meta description" --owner Oliver
npm run seo:crawl
```
