# Alice Source Fetching

Step 6 introduces the first live retrieval layer for Alice.

This layer is intentionally narrow:

1. it executes only a small supported subset of Step 5 source plans
2. it writes raw evidence and a schema-backed fetch log
3. it keeps retrieval separate from extraction and separate from research assembly

## What counts as a fetch

A fetch is any concrete attempt to retrieve source material after Step 5 planning.

Examples:

1. GET a listing page URL
2. query a federal ArcGIS endpoint at a point geometry
3. POST a USDA SDA SOAP query for soil attributes
4. load a county planning or parcel-search landing page
5. record that a planned source was blocked because coordinates or county context were still missing

`source_fetch_log.json` is the system of record for these attempts.

If Alice did not fetch it, Alice should not imply it was checked.

## Descriptor access modes

Step 10 still uses `interface_type`, but the stabilization patch adds descriptor `access_mode` so evaluation can distinguish:

1. direct machine endpoints
2. landing pages
3. viewers
4. mixed source families

Examples:

1. `machine_endpoint`
   - a real API or GIS endpoint that Alice can query directly
2. `landing_page`
   - a human-facing page or download entry point that may still be useful evidence, but is not the same thing as a direct queryable endpoint
3. `viewer`
   - an interactive map or viewer surface
4. `mixed`
   - a source family where Alice plans from the program page or descriptor, but executes a derived machine endpoint when available

This is intentionally separate from `interface_type`.

## Source execution policy

Step 6 consumes `alice/source_plan.json` and applies a conservative support matrix.

Preferred execution behavior:

1. execute supported sources in plan priority order
2. reuse one fetch result across multiple requested capabilities when the same source is reused
3. record unsupported sources explicitly as `unsupported` or `deferred`
4. record missing-prerequisite cases explicitly as `blocked`
5. preserve raw payload references for every successful live or fixture-backed fetch

`landing_page` or `viewer` access should not be counted later as machine-endpoint coverage just because Alice reached the page successfully.

Step 6 does not yet do dynamic replanning from newly extracted listing clues. If a listing page later reveals coordinates or APN hints, future steps can use that to unlock additional source groups.

## Supported retrieval surface in this step

### Listing platforms

Step 6 supports direct page fetch attempts plus conservative HTML parsing for:

1. Redfin
2. Zillow
3. LandWatch
4. Land.com

Extraction remains conservative:

1. title
2. canonical URL
3. asking price if clearly present
4. acreage if clearly present
5. address-like text if clearly present
6. descriptive text
7. APN clues if clearly present
8. coordinates if clearly present in page data

Important:

1. a listing page never confirms parcel identity by itself
2. some platforms may block direct non-browser HTTP; this must be logged honestly as a failed attempt, not treated as “checked”

### Federal baseline

Step 6 supports point-based retrieval when defensible coordinates are already known:

1. FEMA NFHL flood zone query
2. USFWS/USGS wetlands query
3. USGS 3DEP elevation sample query
4. USDA Soil Data Access soil lookup plus soil attribute lookup

These are point-based directional screens, not parcel-boundary conclusions.

If Alice does not have a point geometry yet, these sources should be logged as blocked by missing prerequisite rather than guessed from county context alone.

### Pilot county/state/local pages

Step 6 supports conservative page access retrieval for a small set of curated pilot sources, such as:

1. Hudspeth CAD property-search landing page
2. Hudspeth CAD public-information page
3. Hudspeth County forms and applications page
4. Texas PUC electricity maps landing page
5. Kern County and Bakersfield planning/gis landing pages when present in the source plan

In Step 6 these page fetches primarily support:

1. verifying that the local surface exists and was reachable
2. capturing titles, descriptions, and public-entrypoint evidence
3. informing unknowns and next actions

They do not yet perform full parcel-specific local search workflows.

## Raw evidence rules

Runtime workspaces should store raw evidence under:

```text
alice/
  raw_evidence/
    <source_id>/
      <entry_id>.html
      <entry_id>.json
      <entry_id>.xml
      <entry_id>.txt
      <entry_id>.headers.json
```

Rules:

1. store the response body only when it materially supports later extraction or auditing
2. store headers when status, content type, redirects, or anti-bot behavior matters
3. avoid dumping irrelevant assets such as images, CSS, JS bundles, or huge binary blobs unless they are the only evidence path
4. use deterministic, entry-linked filenames so fetch-log references remain stable

## Retrieval guardrails

Step 6 must preserve Alice truthfulness rules:

1. listing fetch success does not equal parcel confirmation
2. county page access does not equal zoning certainty
3. state utility map access does not equal service availability or interconnection feasibility
4. flood, wetlands, soil, and elevation point queries are geography-linked screens, not full parcel-boundary determinations
5. missing or unsupported local source execution must stay visible in the fetch log and later unknowns

## What Step 6 does not solve yet

Step 6 does not yet implement:

1. browser-automation fallbacks for anti-bot listing sites
2. parcel-fabric intersection at county or national scale
3. robust county parcel search workflows driven by APN/address form posts
4. deep parsing of downloadable statewide or local datasets
5. final narrative report rendering quality
