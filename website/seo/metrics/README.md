# SEO Metrics Inputs

Drop real data exports into this directory when you want `npm run seo:report` to run against current metrics.

Preferred file names:

- `search_console_latest.csv`
- `keyword_volume_latest.csv`

Accepted input formats:

1. Search Console export

Required columns, with alias support:

- `query` or `keyword`
- `page`, `url`, or `target_url`
- `clicks`
- `impressions`
- `ctr`
- `position` or `avg_position`

2. Search volume export

Required columns, with alias support:

- `keyword` or `query`
- `search_volume`, `avg_monthly_searches`, or `volume`

Optional columns:

- `country`
- `language`
- `source`

Notes:

- These files are intentionally not checked in with live metrics.
- Sample fixtures for testing live under `website/seo/fixtures/`.
- If the preferred file names are missing, the report script now auto-discovers the newest matching file in this directory.
- Search Console data is enough to generate ranking, CTR, and cannibalization recommendations.
- Keyword volume data remains important for prioritization, but its absence no longer needs to fully block the report if Search Console data exists.
- If no live files are present, the report script will generate a blocker-style report unless explicit fixture paths are passed.
