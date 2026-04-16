# Alice AI Architecture and Product Design

- Status: Proposed
- Last updated: 2026-04-16
- Audience: Product, backend, data/source infrastructure, evaluation
- Scope: New DoWhiz land acquisition and due diligence skill for U.S. vacant land

## 0. DoWhiz Integration Baseline

This design is intentionally shaped around the current DoWhiz runtime rather than assuming a new standalone service.

Current codebase anchors that matter:

1. Inbound messages are routed by channel and key to an employee in `DoWhiz_service/gateway.toml`, then processed through `DoWhiz_service/scheduler_module/src/service/ingestion.rs`.
2. Each conversation thread gets a persistent workspace with `incoming_email/`, `incoming_attachments/`, `memory/`, and `references/` in `DoWhiz_service/scheduler_module/src/service/workspace.rs`.
3. Employee persona files (`AGENTS.md`, `SOUL.md`) and shared skills are copied into every workspace, with skills available under `.agents/skills/`.
4. The runtime prompt already enforces channel-specific reply artifacts such as `reply_message.txt` and `reply_email_draft.html` in `DoWhiz_service/run_task_module/src/run_task/prompt.rs`.
5. The current channel abstraction is already broad: email, Slack, Discord, SMS, Telegram, WhatsApp, Google Docs/Sheets/Slides, BlueBubbles, Notion, WeChat, Lark, and Zoom are modeled in `DoWhiz_service/scheduler_module/src/channel.rs`.

Architectural implication:

1. Alice should be designed first as a shared skill package plus a structured workspace artifact contract.
2. A dedicated "Alice" employee identity is optional product packaging, not the core intelligence boundary.
3. Alice should reuse the existing channel and workspace model instead of introducing a new queue, new service binary, or special runtime path in v1.

## 1. Alice AI System Definition

### 1.1 What Alice is inside DoWhiz

Alice AI should be implemented as a domain-specialized DoWhiz skill and orchestration layer for buyer-side U.S. land sourcing and public-data-first due diligence.

Recommended product shape:

1. Core intelligence: shared skill package at `DoWhiz_service/skills/alice-ai/`.
2. Product interface: optional `agents/openai.yaml` with a concise surface prompt for future agent marketplaces or UI surfacing.
3. Workspace contract: structured JSON and markdown artifacts written under a dedicated `alice/` directory inside each thread workspace.
4. Optional branded persona: later, a dedicated employee profile and channel routing such as `alice@dowhiz.com`, but only after the skill contract is stable.

Preferred v1 position:

1. Alice is not a separate backend service.
2. Alice is not just a generic prompt alias.
3. Alice is a reusable land-research capability with explicit schemas, source registry logic, scoring rules, and reporting contracts.

### 1.2 System boundary

Inside the Alice boundary:

1. User request normalization.
2. Parcel, listing, address, APN, and coordinate resolution.
3. Jurisdiction resolution: county, city, state, federal, and special-district relevance.
4. Public-source discovery and retrieval planning.
5. Structured extraction from APIs, GIS services, downloadable datasets, and web-only sources.
6. Universal land research synthesis.
7. Use-case-specific screening modules.
8. Confidence, completeness, conflict, and missing-data reporting.
9. Recommendation shortlist generation from observed candidate supply.
10. Report generation with source-linked provenance.

Outside the Alice boundary:

1. Legal advice.
2. Brokerage advice or agency representation.
3. Formal appraisal.
4. Guaranteed development entitlement.
5. Guaranteed interconnection feasibility.
6. Guaranteed water-rights confirmation.
7. Title insurance, lien, easement, deed, or mineral-rights final determination.
8. Final investment committee underwriting.
9. Automated purchase negotiation or offer execution.

### 1.3 In scope for v1

V1 should cover:

1. U.S. vacant land and lightly improved land where the primary value thesis is land rather than existing structures.
2. Two main flows:
   - recommendation flow
   - deep research flow
3. Single-parcel or parcel-group deep research.
4. Small batch deep research for comparison, with a recommended cap of 10 subjects per request.
5. Public-data-first diligence across all U.S. counties, with honest coverage-level reporting.
6. Universal land screening plus initial use-case modules for:
   - energy
   - agriculture
   - residential/light development
   - industrial/storage/commercial-type land
   - recreational/rural lifestyle/hold
7. Channel-agnostic report generation, with tailored output formatting for Slack, email, and longer markdown artifacts.

### 1.4 Explicitly out of scope for v1

1. Full off-market lead generation from raw county ownership data.
2. Direct broker contact or owner outreach.
3. Paid-data-vendor dependence as a hard requirement.
4. Parcel boundary correction or cadastral adjudication.
5. Guaranteed comp-based valuation.
6. Permit application drafting.
7. Wetland delineation, geotechnical, survey, or engineering conclusions.
8. Advanced interconnection queue analysis beyond public directional signals.
9. Fully autonomous ongoing monitoring of every parcel after report generation.

### 1.5 How Alice differs from a generic research agent

Alice should differ in five concrete ways:

1. Canonical subject resolution.
   - Alice must resolve "what land are we talking about?" before research begins.
   - A generic research agent often starts summarizing before the parcel identity is stable.

2. Jurisdiction-aware source planning.
   - Alice chooses sources based on county, city, state, federal, and special-district context.
   - A generic research agent mostly uses freeform web browsing.

3. Structured evidence contracts.
   - Alice outputs a normalized land-research object with scores, unknowns, and provenance.
   - A generic research agent usually outputs narrative prose only.

4. Use-case screening modules.
   - Alice runs domain-specific screens for energy, ag, residential, industrial, and recreational theses.
   - A generic research agent does not have reusable land-feasibility modules.

5. Honest coverage logic.
   - Alice must say "minimal county coverage" or "zoning unresolved" instead of fabricating completeness.
   - A generic research agent tends to over-compress uncertainty.

## 2. Input Surface

### 2.1 Canonical request envelope

All inbound Alice requests should normalize into the same internal request object:

```json
{
  "schema_version": "alice.request.v1",
  "request_id": "uuid",
  "thread_id": "string",
  "channel": "email|slack|discord|...",
  "request_mode": "recommendation|deep_research|batch_compare|follow_up",
  "raw_user_message": "string",
  "thesis": {
    "summary": "string|null",
    "use_case_hypotheses": ["energy", "agriculture"],
    "target_geographies": [],
    "filters": {},
    "budget": {},
    "hold_period": {},
    "return_preferences": {}
  },
  "subjects": [],
  "output_preferences": {
    "verbosity": "short|standard|full",
    "comparison_requested": true
  },
  "conversation_state_ref": "alice/session_state.json"
}
```

### 2.2 Supported input types and normalization rules

| Input type | Normalization goal | Required extracted fields | Fallback behavior |
| --- | --- | --- | --- |
| Natural language investment intent | Convert a freeform thesis into a search plan | use-case hypotheses, geography constraints, acreage range, budget, exclusions, timeline, risk tolerance | If geography or thesis is too vague, ask only the minimum narrowing questions |
| Listing URL | Convert a listing page into canonical listing metadata plus parcel candidates | platform, canonical URL, listing ID if present, asking price, stated acreage, coordinates/address/APNs if present | If the URL cannot be parsed, snapshot page content and move to manual extraction mode |
| APN / parcel number | Resolve a parcel identifier to a county parcel record | raw APN, normalized APN, county/state hint, candidate matches | If county is missing, infer from thread context or ask for county/state before claiming resolution |
| Address | Geocode and parcel-match the site | normalized address, coordinates, county, city, parcel candidates | If multiple parcel matches exist, return ranked candidates and keep report in unresolved state |
| Coordinates | Resolve point to parcel and jurisdictions | lat/lon, county, city, parcel intersection candidates, nearby listings if needed | If parcel fabric is unavailable, proceed with geography-only report and lower completeness |
| Multiple URLs / batch input | Convert many subjects into a comparable batch | list of normalized subject entries with a batch comparison mode | If batch exceeds limit, split into chunks and tell the user which chunk was processed |
| Follow-up conversational refinement | Update prior request state instead of restarting | delta constraints, changed priorities, newly supplied subject identifiers, prior report references | If the prior subject is ambiguous, restate current assumed subject before continuing |

### 2.3 Normalization by input type

#### A. Natural language investment intent

Example user input:

> I want 40 to 200 acres in West Texas for solar or hold, under $500k, not too far from transmission, not in a floodplain.

Normalized output should contain:

1. `request_mode = recommendation`
2. `use_case_hypotheses = ["energy", "recreational_hold"]`
3. Geography constraints:
   - state = Texas
   - region hint = West Texas
4. Filters:
   - acreage_min = 40
   - acreage_max = 200
   - max_price = 500000
   - exclude_major_floodplain = true
   - prefer_transmission_proximity = true
5. Missing items:
   - county preference unknown
   - utility preference unknown
   - access tolerance unknown

#### B. Listing URL

Normalization stages:

1. Canonicalize URL.
2. Detect platform.
3. Extract listing metadata from URL, page HTML, structured metadata, and visible text.
4. Extract any parcel clues:
   - APN
   - address
   - coordinates
   - acreage
   - county
5. Assign `request_mode = deep_research` unless user explicitly asks for comparison or shortlist generation.
6. Store a listing snapshot because listing pages change.

#### C. APN / parcel number

Normalization stages:

1. Preserve the raw APN exactly as supplied.
2. Generate a normalized APN with county-specific formatting stripped only for matching, not for display.
3. Require a county context or infer it from:
   - listing URL
   - address
   - coordinates
   - prior thread state
4. Search county assessor and parcel GIS sources first.
5. If multiple candidate parcels match, do not collapse them silently.

#### D. Address

Normalization stages:

1. Standardize address string.
2. Geocode to coordinates.
3. Resolve county, city, state, ZIP, and census geography.
4. Attempt parcel intersection or nearest parcel match.
5. Record whether the report is:
   - address-resolved and parcel-resolved
   - address-resolved but parcel-unresolved

#### E. Coordinates

Normalization stages:

1. Validate coordinate format.
2. Resolve county, city, state, and special districts by overlay.
3. Intersect with parcel fabric where available.
4. If no parcel fabric is available:
   - proceed with a geography-based research object
   - set parcel identity status to `unresolved`
   - downgrade completeness and confidence for parcel-specific conclusions

#### F. Multiple URLs / batch input

Normalization stages:

1. Normalize every subject independently.
2. Group by request purpose:
   - compare candidate deals
   - rank a user-supplied list
   - produce light diligence on each subject
3. Preserve per-subject confidence and coverage; do not flatten scores across the batch.

#### G. Follow-up conversational refinement

Follow-ups should update a persisted `alice/session_state.json` in the thread workspace with:

1. active subject(s)
2. latest thesis
3. last report artifact
4. unresolved questions
5. user-confirmed assumptions

This allows follow-ups like:

1. "Now only show me parcels with paved access."
2. "Redo this assuming small battery storage instead of solar."
3. "Compare this listing against the two we discussed yesterday."

## 3. Core Flows

### 3.1 Shared architecture stages

Both main flows should reuse the same seven internal layers:

1. Request normalization.
2. Subject resolution.
3. Jurisdiction resolution.
4. Source discovery and planning.
5. Source retrieval and normalization.
6. Universal land research assembly.
7. Use-case screening, scoring, and report generation.

Recommended internal components:

1. `alice_request_normalizer`
2. `alice_subject_resolver`
3. `alice_jurisdiction_resolver`
4. `alice_source_registry`
5. `alice_retrieval_orchestrator`
6. `alice_land_research_assembler`
7. `alice_use_case_modules`
8. `alice_report_renderer`

Recommended workspace artifact layout:

```text
alice/
  session_state.json
  request_normalized.json
  subject_resolution.json
  jurisdiction_context.json
  source_plan.json
  source_fetch_log.json
  coverage_assessment.json
  parcel_memo.json
  report.md
  report_summary.json
  recommendation/
    candidate_universe.json
    shortlist.json
```

### 3.2 Recommendation flow

#### Purpose

Given a user thesis, return a ranked shortlist of candidate parcels or listings from the observable market universe.

Important truthfulness rule:

The recommendation flow does **not** promise exhaustive coverage of every land opportunity in the market. It promises a transparent shortlist from the candidate universe Alice was actually able to observe and score.

#### Entry points

1. Slack or email message with a natural language investment thesis.
2. Follow-up message that refines earlier criteria.
3. Batch comparison request against a user-supplied candidate list.

#### Orchestration stages

1. Thesis normalization.
   - Extract geography, budget, acreage, risk filters, and intended land use.

2. Search-universe planning.
   - Determine which listing platforms and geographic scopes to query.
   - Decide whether the user asked for broad discovery or ranking of known candidates.

3. Candidate acquisition.
   - Fetch listing candidates from supported listing sources.
   - Dedupe by canonical listing URL, APN, address, and coordinates.

4. Subject resolution.
   - Resolve each candidate to parcel identity or unresolved parcel cluster.

5. Universal land enrichment.
   - Run baseline parcel, jurisdiction, environmental, access, infrastructure, and zoning screens.

6. Use-case-specific screening.
   - Run only modules that match the thesis.

7. Ranking and pruning.
   - Compute:
     - thesis-fit score
     - blocker severity
     - data confidence
     - completeness
   - Penalize unresolved critical blockers heavily.

8. Shortlist packaging.
   - Return top candidates with concise rationale, top risks, missing items, and source coverage notes.

#### Intermediate artifacts

1. `alice/request_normalized.json`
2. `alice/recommendation/candidate_universe.json`
3. `alice/subject_resolution.json`
4. `alice/source_plan.json`
5. `alice/coverage_assessment.json`
6. `alice/parcel_memo.json` for each shortlisted candidate
7. `alice/recommendation/shortlist.json`
8. `alice/report.md`

#### Failure modes

1. Thesis too vague to search.
   - Example: "Find me good land in the U.S."
   - Response: ask for geography and use-case narrowing before pretending to shortlist.

2. Search universe too broad for observed supply.
   - Response: explain the constraint and ask for one narrowing dimension.

3. Listing platform extraction failure.
   - Response: continue with other sources if possible, mark candidate universe incomplete.

4. Parcel identity unresolved for many candidates.
   - Response: keep them in shortlist only if the user asked for rough screening; otherwise pause for more identifiers.

5. County coverage too weak.
   - Response: return lower-confidence candidates with explicit unknowns.

#### Expected outputs

Primary output should be a shortlist, not a giant market dump.

Recommended shortlist item fields:

1. Candidate name or parcel shorthand.
2. Geography.
3. Asking price and acreage.
4. Thesis-fit summary.
5. Top 3 reasons it made the shortlist.
6. Top 3 blockers or unknowns.
7. Confidence score.
8. Completeness score.
9. Coverage tier: full, partial, or minimal.
10. Listing/source links.

### 3.3 Deep research flow

#### Purpose

Given a specific listing URL, APN, address, or coordinates, return a detailed public-data-first diligence memo.

#### Entry points

1. Listing URL from Redfin, Zillow, LandWatch, Land.com, or similar sites.
2. APN / parcel number.
3. Address.
4. Coordinates.
5. Small batch of known candidate properties for comparison.

#### Orchestration stages

1. Subject normalization and resolution.
   - Resolve to a parcel, parcel group, or unresolved candidate set.

2. Jurisdiction context assembly.
   - Determine:
     - county
     - city or unincorporated area
     - state
     - relevant special districts
     - relevant federal overlays

3. Source-plan generation.
   - Select official sources first.
   - Then select listing and secondary sources for market context or missing clues.

4. Retrieval and evidence capture.
   - Fetch and normalize evidence.
   - Preserve provenance and timestamps.

5. Universal land research assembly.
   - Parcel identity
   - listing metadata
   - zoning and planning
   - environmental overlays
   - access
   - infrastructure and utility signals
   - water and ag signals

6. Use-case module execution.
   - Run modules requested by the user or inferred from the thesis.

7. Economics and risk synthesis.
   - Build directional economics only where assumptions are explicit.

8. Report rendering.
   - Produce a structured memo plus user-facing narrative.

#### Intermediate artifacts

1. `alice/request_normalized.json`
2. `alice/subject_resolution.json`
3. `alice/jurisdiction_context.json`
4. `alice/source_plan.json`
5. `alice/source_fetch_log.json`
6. `alice/coverage_assessment.json`
7. `alice/parcel_memo.json`
8. `alice/report.md`
9. `alice/report_summary.json`

#### Failure modes

1. Listing resolves to multiple parcels.
   - Response: keep a parcel-group memo and mark parcel-level details as partially unresolved.

2. APN supplied without county.
   - Response: ask for county if it cannot be inferred.

3. County parcel data unavailable.
   - Response: proceed with minimal-coverage memo from national and state data.

4. Zoning documents available but map not available.
   - Response: return code-text and land use context with lower parcel-specific certainty.

5. Conflicting official records.
   - Response: report the conflict explicitly, preserve both citations, and lower confidence.

6. Economics unsupported by evidence.
   - Response: omit numeric output rather than inventing assumptions.

#### Expected outputs

The deep research flow should return:

1. Executive summary.
2. Parcel or parcel-group identity resolution.
3. Evidence-quality panel.
4. Public-data findings by domain.
5. Use-case feasibility sections.
6. Directional economics framework.
7. Risks, unknowns, and next diligence actions.
8. Source links and citations.

## 4. Canonical Parcel Research Schema

The canonical contract should be a normalized object called `AliceLandResearchObject`.

This object may represent:

1. a single parcel
2. a parcel group tied to one listing
3. an unresolved candidate set where identity is not yet fully stable

### 4.1 Top-level schema

```json
{
  "schema_version": "alice.land_research.v1",
  "object_id": "uuid",
  "report_type": "recommendation_candidate|deep_research|batch_compare_item",
  "created_at": "datetime",
  "request_context": {},
  "subject": {},
  "listing": {},
  "source_urls": [],
  "jurisdiction": {},
  "parcel_identity": {},
  "planning_and_land_use": {},
  "environmental_constraints": {},
  "water_and_agriculture": {},
  "infrastructure_and_utilities": {},
  "market_signals": {},
  "use_case_modules": [],
  "directional_economics": {},
  "risks": [],
  "unknowns": [],
  "next_actions": [],
  "citations": [],
  "scores": {}
}
```

### 4.2 Reusable field types

Most factual fields should use the same typed wrapper:

```json
{
  "value": "any|null",
  "status": "confirmed|estimated|conflicting|missing|not_applicable",
  "confidence": 0.0,
  "citation_ids": ["c1", "c2"],
  "as_of": "date|null",
  "notes": "string|null"
}
```

This is the core anti-hallucination pattern. Every material fact should be:

1. typed
2. status-labeled
3. confidence-scored
4. citation-linked

### 4.3 `request_context`

```json
{
  "request_id": "uuid",
  "thread_id": "string",
  "request_mode": "recommendation|deep_research|batch_compare|follow_up",
  "channel": "email|slack|discord|...",
  "user_thesis_summary": "string|null",
  "use_case_hypotheses": ["energy"],
  "user_constraints": {},
  "batch_parent_id": "uuid|null"
}
```

### 4.4 `subject`

```json
{
  "research_unit_type": "single_parcel|parcel_group|unresolved_candidate_set",
  "resolution_status": "resolved|partially_resolved|unresolved",
  "primary_subject_label": "string",
  "raw_inputs": [],
  "canonical_address": {},
  "coordinates": {},
  "parcel_count": 1
}
```

### 4.5 `listing`

```json
{
  "platform": {
    "value": "zillow",
    "status": "confirmed",
    "confidence": 0.99,
    "citation_ids": ["c_listing"]
  },
  "canonical_url": "string|null",
  "listing_id": "string|null",
  "status": "active|pending|sold|off_market|unknown",
  "asking_price": {},
  "listed_acreage": {},
  "days_on_market": {},
  "listing_agent_name": {},
  "listing_brokerage": {},
  "listing_text_summary": {
    "value": "string|null",
    "status": "confirmed",
    "confidence": 0.8,
    "citation_ids": ["c_listing"]
  },
  "listing_snapshot_at": "datetime|null"
}
```

### 4.6 `source_urls`

This should be a simple normalized list for quick display:

```json
[
  {
    "label": "County parcel viewer",
    "url": "string",
    "source_id": "county_tx_xxx_parcel_gis"
  }
]
```

### 4.7 `jurisdiction`

```json
{
  "state": {
    "name": "Texas",
    "fips": "48"
  },
  "county": {
    "name": "Hudspeth",
    "fips": "48229"
  },
  "city_or_unincorporated": {
    "value": "unincorporated county",
    "status": "confirmed",
    "confidence": 0.95,
    "citation_ids": ["c_boundary"]
  },
  "special_districts": [],
  "federal_relevance": [
    "FEMA flood data",
    "USFWS wetlands"
  ],
  "coverage_tier": "full|partial|minimal"
}
```

### 4.8 `parcel_identity`

```json
{
  "parcels": [
    {
      "apn": {},
      "county_parcel_id": {},
      "assessor_site_address": {},
      "assessor_acreage": {},
      "geometry_source_id": "string|null",
      "geometry_confidence": 0.0,
      "owner_name_public": {},
      "tax_status_summary": {}
    }
  ],
  "access_point_estimate": {},
  "boundary_notes": "string|null",
  "identity_conflicts": []
}
```

Notes:

1. Public owner name can be included only when it comes directly from public county records and should remain factual, not personalized.
2. Ownership details should never be treated as title evidence.

### 4.9 `planning_and_land_use`

```json
{
  "zoning_designation": {},
  "zoning_description": {},
  "future_land_use_designation": {},
  "plan_area": {},
  "overlay_districts": [],
  "minimum_lot_size": {},
  "setback_signals": [],
  "subdivision_signals": [],
  "permitted_use_signals": [],
  "conditional_use_signals": [],
  "development_constraints_summary": {
    "value": "string|null",
    "status": "estimated",
    "confidence": 0.0,
    "citation_ids": []
  },
  "interpretation_limits": []
}
```

### 4.10 `environmental_constraints`

```json
{
  "flood_zone": {},
  "wetlands_signal": {},
  "slope_signal": {},
  "elevation_signal": {},
  "soil_constraints_signal": {},
  "fire_risk_signal": {},
  "habitat_or_conservation_signal": {},
  "superfund_or_contamination_signal": {},
  "environmental_summary": {},
  "regulatory_notes": []
}
```

### 4.11 `water_and_agriculture`

```json
{
  "surface_water_proximity": {},
  "groundwater_or_well_signal": {},
  "irrigation_district_signal": {},
  "water_rights_status": {
    "value": null,
    "status": "missing",
    "confidence": 0.0,
    "citation_ids": [],
    "notes": "Public water-rights confirmation not completed"
  },
  "soil_productivity_signal": {},
  "cropland_or_pasture_signal": {},
  "ag_exemption_signal": {},
  "water_and_ag_summary": {}
}
```

### 4.12 `infrastructure_and_utilities`

```json
{
  "legal_or_physical_access_signal": {},
  "road_frontage_signal": {},
  "power_service_signal": {},
  "utility_territory_signal": {},
  "substation_proximity_signal": {},
  "transmission_proximity_signal": {},
  "broadband_signal": {},
  "water_service_signal": {},
  "wastewater_or_septic_signal": {},
  "rail_or_highway_logistics_signal": {},
  "infrastructure_summary": {}
}
```

Important rule:

Utility and transmission proximity are directional siting signals, not service guarantees.

### 4.13 `market_signals`

```json
{
  "ask_price_per_acre": {},
  "nearby_listing_context": [],
  "county_market_context": {},
  "tax_burden_signal": {},
  "liquidity_signal": {},
  "market_summary": {}
}
```

### 4.14 `use_case_modules`

Every module should return the same shape:

```json
{
  "module_id": "energy_solar",
  "module_label": "Solar",
  "fit_assessment": "favorable|mixed|weak|not_applicable|unknown",
  "confidence": 0.0,
  "blocking_flags": [],
  "supporting_signals": [],
  "key_unknowns": [],
  "economics_inputs_required": [],
  "summary": "string",
  "citation_ids": []
}
```

### 4.15 `directional_economics`

```json
{
  "status": "available|limited|not_enough_data",
  "basis": "asking_price_plus_public_proxies",
  "assumptions": [
    {
      "name": "closing_cost_pct",
      "value": "3%",
      "source": "user_assumption|market_proxy|public_record"
    }
  ],
  "carry_costs": {},
  "improvement_capex_proxies": {},
  "revenue_or_exit_cases": [],
  "scenario_summary": [],
  "limitations": []
}
```

Required philosophy:

1. No pseudo-precise IRR from thin data.
2. No hidden assumptions.
3. If economics are weakly supported, `status` must be `limited` or `not_enough_data`.

### 4.16 `risks`

Each risk item should be a discrete object:

```json
{
  "risk_id": "flood_and_access",
  "label": "Flood exposure may constrain access and development",
  "severity": "high|medium|low",
  "category": "zoning|environmental|utilities|market|identity|legal_unknown",
  "description": "string",
  "impact": "string",
  "mitigation_or_next_check": "string",
  "blocking": true,
  "citation_ids": ["c1", "c2"]
}
```

### 4.17 `unknowns`

Unknowns are not the same as risks. They are unresolved diligence gaps.

```json
{
  "unknown_id": "exact_utility_availability",
  "question": "Is three-phase power actually available at the parcel edge?",
  "why_it_matters": "Critical for energy and industrial use cases",
  "recommended_next_source": "Utility territory map or utility engineer confirmation",
  "blocking": true
}
```

### 4.18 `next_actions`

```json
[
  {
    "priority": 1,
    "action": "Confirm parcel APN directly from county assessor",
    "reason": "Listing and county acreage disagree"
  }
]
```

### 4.19 `citations`

Every source used should be recorded here:

```json
{
  "citation_id": "c_fema_nfhl",
  "source_id": "federal_fema_nfhl",
  "source_name": "FEMA National Flood Hazard Layer",
  "source_category": "federal",
  "interface_type": "gis_service",
  "url": "string",
  "retrieved_at": "datetime",
  "published_or_effective_at": "date|null",
  "authority_rank": 1,
  "content_hash": "string|null",
  "notes": "string|null"
}
```

### 4.20 `scores`

```json
{
  "overall_confidence": 0.0,
  "overall_completeness": 0.0,
  "section_confidence": {
    "identity": 0.0,
    "planning": 0.0,
    "environmental": 0.0,
    "infrastructure": 0.0,
    "economics": 0.0
  },
  "section_completeness": {
    "identity": 0.0,
    "planning": 0.0,
    "environmental": 0.0,
    "infrastructure": 0.0,
    "economics": 0.0
  },
  "coverage_tier": "full|partial|minimal",
  "score_notes": []
}
```

### 4.21 Score semantics

Confidence and completeness must be treated separately.

| Score | Meaning |
| --- | --- |
| Confidence | How likely the reported findings are materially correct, given the sources actually reviewed |
| Completeness | How much of the desired diligence surface Alice could actually inspect |

Interpretation example:

1. High confidence, low completeness:
   - Alice is confident about the few facts it found, but many local questions remain open.

2. Low confidence, high completeness:
   - Rare, but possible when many sources conflict or are stale.

3. Low confidence, low completeness:
   - Minimal county coverage, weak parcel identity, or major source outages.

## 5. Data-Source Architecture

### 5.1 Source categories

Alice should maintain a source model with the following top-level categories:

#### A. Federal

Use for nationwide baseline overlays and nationally consistent signals.

Examples:

1. FEMA flood hazard layers
2. USGS topography, elevation, hydrography, and boundaries
3. USDA soil and land capability data
4. U.S. Fish and Wildlife wetlands or habitat layers
5. EPA environmental screening and contamination-related datasets
6. FCC broadband availability data

#### B. State

Use for state-wide policy, parcel, water, environmental, and infrastructure sources.

Examples:

1. State GIS clearinghouses
2. State parcel fabrics where available
3. State water-rights systems
4. State environmental quality layers
5. State DOT, state energy siting, or state utility commission data

#### C. County

These are often the highest-value local sources.

Examples:

1. County assessor parcel search
2. County parcel GIS
3. County zoning map
4. County tax rolls
5. County planning and development code
6. County floodplain administration

#### D. City / local planning

These matter whenever the parcel sits inside municipal or ETJ-like jurisdiction.

Examples:

1. City zoning map
2. General plan or future land use map
3. Development code
4. Utility district maps
5. Special district or service-area maps

#### E. Listing platforms

Use these as market-discovery and listing-context sources, not as the final authority on parcel facts.

Examples:

1. Zillow
2. Redfin
3. LandWatch
4. Land.com
5. Other listing or brokerage pages

#### F. Infrastructure / utility / market signal sources

Use for directional feasibility and operating context.

Examples:

1. Utility territory maps
2. Transmission and substation viewers
3. Broadband availability maps
4. Road, rail, and logistics layers
5. Public market context datasets and listing comps

### 5.2 Source interface types

Alice must explicitly model how a source is accessed.

| Interface type | Description | Expected reliability | Notes |
| --- | --- | --- | --- |
| `formal_api` | Documented API returning structured data | Highest | Best for stable parsing and cache keys |
| `gis_service` | ArcGIS FeatureServer/MapServer, WMS, WFS, WMTS, or similar | High but heterogeneous | Critical for county and overlay work |
| `downloadable_dataset` | Bulk downloads such as CSV, shapefile, GeoJSON, PDF, or geodatabase | Medium to high | Good for cached enrichment or offline refresh |
| `web_only` | Human-facing page, viewer, PDF, or search form | Lowest | Last resort, but must still be supported |

### 5.3 Source registration model

Every source should be registered with a `SourceDescriptor`:

```json
{
  "source_id": "county_tx_hudspeth_parcel_gis",
  "name": "Hudspeth County Parcel GIS",
  "category": "county",
  "jurisdiction_level": "county",
  "geography_scope": {
    "state_fips": "48",
    "county_fips": "48229"
  },
  "interface_type": "gis_service",
  "capabilities": [
    "parcel_geometry",
    "parcel_lookup_by_apn",
    "owner_name",
    "site_address"
  ],
  "base_url": "string",
  "priority": 100,
  "authority_rank": 1,
  "ttl_hours": 168,
  "auth_required": false,
  "status": "active|degraded|offline|manual_review",
  "last_verified_at": "datetime|null",
  "notes": "string|null"
}
```

#### Required source capabilities vocabulary

Capabilities should be standardized so Alice can query the registry by need:

1. `parcel_lookup_by_apn`
2. `parcel_lookup_by_address`
3. `parcel_geometry`
4. `zoning`
5. `future_land_use`
6. `development_code`
7. `tax_roll`
8. `flood`
9. `wetlands`
10. `soil`
11. `water_rights`
12. `utility_territory`
13. `transmission`
14. `broadband`
15. `listing_search`

### 5.4 Source discovery

Source discovery should be parcel-first and jurisdiction-aware:

1. Resolve coordinates and county FIPS first.
2. Load the county/source registry entry for that parcel.
3. Determine municipal context and special districts.
4. Build a source plan by capability:
   - identity
   - zoning/planning
   - environmental
   - utilities/infrastructure
   - market/listing
5. Rank sources within each capability by:
   - official authority
   - specificity to the parcel
   - interface reliability
   - last verification
6. Execute the plan in priority order.

### 5.5 Source prioritization rules

Default precedence should be explicit, not implicit.

Examples:

1. Parcel identity:
   - county assessor or county parcel GIS
   - state parcel layer
   - listing platform

2. Zoning:
   - municipal or county zoning map and code
   - planning department PDF or zoning table
   - listing text

3. Flood:
   - FEMA flood data
   - county floodplain map if parcel-specific and more current
   - listing disclosure text

4. Utilities:
   - official utility territory or service map
   - public infrastructure map
   - listing claim

5. Pricing and acreage:
   - current listing page for ask price
   - county record for parcel acreage
   - user-supplied statement

### 5.6 Caching strategy

Alice should cache by source, query, and geometry context.

Recommended cache keys:

1. `source_id + request_signature`
2. `source_id + parcel_geometry_hash`
3. `source_id + canonical_url`
4. `source_id + published_release_id`

Recommended freshness defaults:

1. Listing pages: 24 hours
2. Official GIS query results: 7 days unless source metadata says otherwise
3. Bulk datasets: until a new published release is detected
4. Static planning PDFs and code pages: 30 days with hash comparison

Important rule:

Cache is for performance and reproducibility, not for silently hiding staleness. Every report should still carry `retrieved_at` and, when known, `published_or_effective_at`.

### 5.7 Provenance tracking

Every source fetch should be recorded in `alice/source_fetch_log.json` with:

1. `source_id`
2. request parameters
3. retrieved timestamp
4. status code or retrieval result
5. raw artifact pointer or hash
6. extracted fields
7. parser version

This creates an audit trail and makes unsupported claims easier to detect.

### 5.8 Handling stale or missing sources

When a source is stale or missing:

1. mark the source as `degraded`, `offline`, or `manual_review`
2. try the next lower-priority source
3. downgrade completeness
4. downgrade confidence only if the missing source affects factual certainty
5. add a user-visible unknown when the source gap is material

Alice must never silently replace a missing local source with a broad national proxy and then report the result as if local diligence were complete.

## 6. Nationwide County Coverage Strategy

This is a core product requirement: Alice should support all U.S. counties in principle, while being honest about uneven source quality.

### 6.1 Design principle

Do not skip counties. Instead, classify county capability and respond with the correct coverage tier.

The product promise is:

1. every county gets a report path
2. not every county gets the same depth
3. lower depth must be visible, not hidden

### 6.2 County/source registry

Alice should maintain a `CountyCoverageRegistry` keyed by county FIPS.

Recommended shape:

```json
{
  "county_fips": "48229",
  "state_fips": "48",
  "county_name": "Hudspeth",
  "registry_version": "v1",
  "coverage_tier": "full|partial|minimal",
  "capabilities": {
    "parcel_identity": "full|partial|minimal|none",
    "zoning": "full|partial|minimal|none",
    "planning_docs": "full|partial|minimal|none",
    "tax_roll": "full|partial|minimal|none",
    "utilities": "full|partial|minimal|none"
  },
  "preferred_source_ids": [],
  "discovered_source_ids": [],
  "known_limitations": [],
  "last_verified_at": "datetime|null"
}
```

### 6.3 Coverage tiers

#### Full coverage

A county is `full` when Alice can usually obtain:

1. parcel identity from official county or local sources
2. parcel geometry
3. local zoning or land-use data
4. local planning or code context
5. baseline environmental overlays
6. at least one local infrastructure or utility signal

#### Partial coverage

A county is `partial` when Alice can usually obtain:

1. parcel identity and geometry
2. some local planning or zoning signals
3. strong national/state overlays

But one or more critical local surfaces are missing, unstructured, or unreliable.

#### Minimal coverage

A county is `minimal` when Alice can usually obtain:

1. geographic placement
2. some parcel clue or approximate subject identity
3. national and state overlays

But local parcel, zoning, or planning interfaces are absent or too weak for robust parcel-level conclusions.

### 6.4 How to support all counties in principle

Alice should use a layered fallback model:

1. Universal baseline for all counties.
   - flood
   - topography
   - soils
   - wetlands
   - broadband
   - state/federal context

2. State-level discovery for all counties.
   - state parcel, environmental, water, and utility registries where available

3. County/local discovery where available.
   - assessor, parcel GIS, zoning, planning, tax, local utilities

4. Web-only local fallback.
   - planning PDFs, code pages, public viewers, and manual forms where APIs do not exist

5. Honest degradation path.
   - if steps 3 and 4 fail, still produce a minimal-coverage memo

### 6.5 Discovery for counties not yet curated

The registry should support both curated and heuristic discovery.

Recommended flow for an uncatalogued county:

1. Resolve county FIPS.
2. Check for existing curated registry entry.
3. If none exists, create a synthetic entry with `coverage_tier = minimal`.
4. Run heuristic discovery:
   - county assessor search
   - county GIS
   - ArcGIS REST endpoint patterns
   - zoning or planning pages
   - tax assessor pages
5. Promote the county to `partial` or `full` only when the sources actually work.

This avoids the trap of waiting for full county curation before serving users.

### 6.6 County capability should be visible in the output

Every Alice report should state:

1. county coverage tier
2. which critical surfaces were available
3. which were not
4. whether local zoning was directly verified or inferred

Example:

> Coverage tier: partial. Parcel identity and county parcel geometry were verified. Local zoning map was unavailable through machine-readable sources, so zoning conclusions are based on county code text plus listing context and should be confirmed with the planning department.

## 7. Use-Case Architecture

Alice should be layered:

1. Universal land research layer
2. Use-case-specific modules on top of that base

This keeps the system scalable. New investment theses should usually add modules, not rewrite the whole diligence engine.

### 7.1 Universal land research layer

The universal layer should always attempt to produce:

1. parcel identity
2. jurisdiction
3. listing snapshot
4. planning and zoning baseline
5. environmental baseline
6. water and ag baseline
7. access and infrastructure baseline
8. risks, unknowns, citations, and scores

Use-case modules consume this base object and add:

1. domain-specific signals
2. domain-specific blocker logic
3. domain-specific economics templates

### 7.2 Standard module contract

Every use-case module should follow the same pattern:

| Field | Purpose |
| --- | --- |
| `module_id` | Stable identifier |
| `activation_conditions` | When to run the module |
| `required_inputs` | Universal-layer data dependencies |
| `optional_sources` | Additional source categories if needed |
| `fit_assessment` | Favorable, mixed, weak, unknown, or not applicable |
| `blocking_flags` | Hard-stop issues |
| `supporting_signals` | Positive or neutral evidence |
| `key_unknowns` | Missing facts that could change the result |
| `economics_inputs_required` | Inputs needed for directional ROI |
| `summary` | User-facing explanation |

### 7.3 Energy module family

Initial scope:

1. solar
2. wind
3. battery / storage

Energy module focus:

1. acreage sufficiency
2. slope and terrain proxies
3. flood and wetland burden
4. transmission and substation proximity
5. utility territory
6. road access
7. zoning or conditional use fit

Guardrail:

Do not claim interconnection feasibility. The module should only provide directional siting signals and explicitly list interconnection as a separate diligence step.

### 7.4 Agriculture module

Focus:

1. soils and land capability proxies
2. cropland or pasture context
3. water availability signals
4. irrigation district or well signals
5. flood and drainage context
6. ag exemption signals

Guardrail:

Do not infer transferable water rights or profitable crop economics from soil alone.

### 7.5 Residential / light development module

Focus:

1. zoning and future land use
2. minimum lot size and subdivision signals
3. access and frontage
4. water and septic proxies
5. flood, slope, and fire constraints
6. nearby development pattern

Guardrail:

Do not claim entitlement certainty or buildability without survey, utility, and local planning confirmation.

### 7.6 Industrial / storage / commercial-type land module

Focus:

1. zoning compatibility
2. road and logistics access
3. utility availability signals
4. flood and environmental burden
5. adjacency to industrial uses or transport corridors
6. site shape and usable acreage proxies

Guardrail:

Do not imply guaranteed truck, rail, or utility capacity.

### 7.7 Recreational / rural lifestyle / hold module

Focus:

1. access and remoteness
2. topography and scenery proxies
3. water feature proximity
4. recreation and habitat signals
5. flood burden and build-envelope constraints
6. holding-cost context

Guardrail:

Do not oversell "usable" or "buildable" recreational land when access or local rules are unclear.

## 8. Reporting Contract

Alice should have one canonical report structure, then render channel-specific views from it.

### 8.1 Canonical report structure

Recommended long-form order:

1. Title and scope
2. Advisory boundary
3. Executive summary
4. Subject snapshot
5. Evidence quality and coverage panel
6. Parcel and jurisdiction profile
7. Public-data findings
8. Use-case feasibility modules
9. Directional economics / ROI framework
10. Risks and unknowns
11. Recommended next actions
12. Sources and citations
13. Appendix: source log, conflicting records, raw notes if needed

### 8.2 Section details

#### 1. Title and scope

Should clearly state:

1. parcel or listing identifier
2. report mode
3. date generated
4. whether the subject is fully resolved or still partial

#### 2. Advisory boundary

Short, visible disclaimer:

1. public-data-first research
2. not legal advice
3. not brokerage advice
4. not appraisal
5. not entitlement or interconnection guarantee

#### 3. Executive summary

Should answer:

1. what this property is
2. why it might fit the thesis
3. what the main blockers are
4. how much confidence Alice has

#### 4. Subject snapshot

Compact table:

1. APN(s)
2. address
3. acreage
4. county / city / state
5. asking price
6. coverage tier
7. overall confidence
8. overall completeness

#### 5. Evidence quality and coverage panel

This is important enough to deserve its own section.

Show:

1. overall confidence
2. overall completeness
3. county coverage tier
4. which critical surfaces were directly verified
5. which are still missing

#### 6. Parcel and jurisdiction profile

Should summarize:

1. parcel identity
2. parcel grouping issues
3. local jurisdiction
4. municipal or unincorporated status
5. relevant special districts

#### 7. Public-data findings

Organize by domain:

1. land use and zoning
2. environmental constraints
3. water and ag signals
4. infrastructure and utilities
5. market context

#### 8. Use-case feasibility

For each active module:

1. fit assessment
2. supporting signals
3. blockers
4. unknowns

#### 9. Directional economics / ROI framework

Should include:

1. explicit assumptions
2. what can and cannot be estimated from current data
3. scenario framing rather than false precision

#### 10. Risks and unknowns

Keep these separate:

1. risks = things Alice sees
2. unknowns = things Alice still cannot verify

#### 11. Recommended next actions

This should turn the report into an actionable diligence checklist.

#### 12. Sources and citations

Should group sources by authority level where helpful:

1. official local
2. official state/federal
3. listing/market context

### 8.3 Channel-specific rendering

#### Slack

Slack output should be a concise decision-support summary, not the full memo inline.

Recommended Slack structure:

1. one-line conclusion
2. parcel snapshot
3. top positives
4. top blockers
5. confidence and coverage tier
6. prompt to open attached or linked full memo

Slack should usually include:

1. inline summary in `reply_message.txt`
2. attached markdown or PDF only if the user wants a longer memo or comparison artifact

#### Email

Email should be the best default long-form human-readable channel.

Recommended email structure:

1. short intro
2. executive summary
3. snapshot table
4. major findings
5. risks and unknowns
6. next actions
7. link or attachment for the full memo if long

#### Long-form markdown artifact

Markdown should be the system-of-record narrative artifact because:

1. it is easy to diff
2. it is easy to store in the workspace
3. it is easy to attach or convert later

The markdown artifact should always be generated for deep research and batch comparison, even if the channel receives a shorter summary.

### 8.4 Rendering principle

Structured JSON is the canonical machine contract.

Markdown and channel-specific summaries are views over the same underlying object.

This prevents drift between what Alice "knows" and what Alice "says."

## 9. Safety, Compliance, and Truthfulness

Alice should bias toward honest uncertainty over false precision.

### 9.1 Overclaiming guardrails

Alice must not say:

1. "This parcel is definitely buildable."
2. "This is zoned for your project" unless the local source explicitly supports that exact statement.
3. "Utilities are available" when only proximity is known.
4. "Water rights exist" from wells, irrigation districts, or nearby water features alone.
5. "Interconnection is feasible" from transmission proximity alone.

Preferred wording:

1. "Public-data screening suggests..."
2. "The county zoning map appears to indicate..."
3. "Utility proximity is favorable, but actual service availability is not yet confirmed."
4. "Water-rights status remains unresolved from current public sources."

### 9.2 Legal and compliance boundaries

Alice should always make these boundaries explicit:

1. not legal advice
2. not brokerage advice
3. not appraisal
4. not title review
5. not engineering or survey certification

### 9.3 Ambiguous zoning interpretation

When zoning is ambiguous:

1. cite both the map and the code if both exist
2. do not jump from a district label to a guaranteed use outcome
3. explicitly separate:
   - permitted
   - conditional
   - prohibited
   - unresolved
4. recommend planning-department confirmation when parcel-specific interpretation is uncertain

### 9.4 Interconnection and utility uncertainty

Alice may report:

1. proximity to transmission
2. utility territory
3. visible substations or line classes where public sources support it

Alice may not report:

1. queue viability
2. available capacity
3. cost to interconnect
4. construction timeline

unless those facts come from explicit public utility materials, and even then they should still be framed as directional.

### 9.5 Water-rights uncertainty

Alice may report:

1. surface water presence
2. groundwater/well regulatory context
3. irrigation-district presence
4. publicly visible water-right registry hits

Alice may not infer:

1. transferable water rights
2. usable permitted volume
3. priority or enforceability of rights

without explicit public evidence.

### 9.6 Missing local data

If local data is missing:

1. say it plainly
2. reduce completeness
3. add an unknown
4. avoid backfilling with generic county assumptions

### 9.7 Conflicting public records

When public records conflict:

1. preserve both citations
2. explain the conflict
3. lower confidence
4. do not force a single answer unless one source is clearly more authoritative and current

Default source precedence:

1. official current local source for the relevant question
2. official state source
3. official federal source
4. listing or secondary source

### 9.8 Unsupported claim rule

Every material claim in the final report should be one of:

1. directly supported by one or more citations
2. explicitly labeled as an inference
3. explicitly labeled as unknown

Anything else is a failure.

## 10. Evaluation Plan

Alice needs a purpose-built evaluation harness, not just spot checks.

### 10.1 Parcel-level benchmark

Build a benchmark set of parcels and listings across:

1. multiple states
2. multiple coverage tiers
3. multiple land types
4. multiple use cases

Recommended benchmark design:

1. 150 to 300 parcel subjects
2. stratified across:
   - full coverage counties
   - partial coverage counties
   - minimal coverage counties
3. include:
   - clean single-parcel cases
   - multi-parcel listing cases
   - ambiguous APN cases

Key metrics:

1. parcel resolution accuracy
2. zoning summary correctness
3. critical-risk recall
4. unsupported-claim rate
5. unknown identification quality

### 10.2 County coverage benchmark

Build a coverage benchmark that is county-centric rather than parcel-centric.

Recommended design:

1. sample counties from every state
2. stratify by:
   - population density
   - GIS maturity
   - region
   - urban / rural

Key metrics:

1. county registry classification accuracy
2. source discovery success rate
3. parcel identity resolution rate
4. planning/zoning availability rate
5. false "full coverage" rate

That last metric matters a lot. Overstating county capability is worse than understating it.

### 10.3 Recommendation quality benchmark

Recommendation quality needs its own eval set.

Recommended design:

1. 50 to 100 investment theses
2. each with:
   - geography
   - budget
   - acreage
   - use case
   - explicit exclusions
3. human-reviewed relevance judgments for candidate parcels or listings

Key metrics:

1. shortlist precision at K
2. blocker recall
3. data-confidence-weighted ranking quality
4. user-perceived usefulness from review rubrics

### 10.4 Hallucination and unsupported-claim checks

Build automated checks over the final report:

1. every material sentence should map to citation IDs or inference tags
2. detect unsupported numeric claims
3. detect prohibited phrases:
   - guaranteed
   - definitely zoned
   - entitled
   - buildable
   - interconnection available

without supporting evidence

### 10.5 Source citation checks

Check:

1. citation presence
2. citation freshness metadata
3. correct source category and authority level
4. whether high-impact claims rely only on listing pages when official sources exist

### 10.6 Confidence calibration checks

Confidence is only useful if calibrated.

Recommended calibration measures:

1. compare predicted confidence buckets to verified claim accuracy
2. track overconfidence by county tier
3. track overconfidence by use-case module
4. use simple calibration metrics such as bucket accuracy and expected calibration error

Desired behavior:

1. full-coverage counties should support higher calibrated confidence
2. minimal-coverage counties should rarely produce high-confidence outputs

## 11. Delivery Roadmap

The implementation should be staged so the core contract is stable before the source surface expands.

### Stage 1. Schema and workspace contracts

Deliver:

1. `AliceLandResearchObject` schema
2. request envelope schema
3. workspace artifact layout
4. score semantics
5. report template contract

Why first:

Without stable contracts, source integration and evaluation will drift.

### Stage 2. Source registry and county registry

Deliver:

1. `SourceDescriptor` model
2. `CountyCoverageRegistry` model
3. source capability vocabulary
4. curated seed entries for pilot geographies
5. heuristic discovery flow for uncatalogued counties

### Stage 3. Parcel and listing resolution

Deliver:

1. listing URL normalization
2. APN normalization
3. address and coordinate resolution
4. parcel-group handling
5. subject-resolution artifact

### Stage 4. Single-parcel deep research v0

Deliver:

1. official-source-first retrieval planner
2. universal land research layer
3. full report rendering
4. confidence/completeness scoring
5. pilot support for a limited but representative county set

Success criterion:

Alice can generate credible single-parcel memos before recommendation is attempted at scale.

### Stage 5. Nationwide county fallback expansion

Deliver:

1. minimal-coverage path for any county
2. partial/full tier promotion logic
3. registry update workflow
4. missing-data and stale-source handling

### Stage 6. Use-case module rollout

Deliver modules in this order:

1. energy
2. agriculture
3. residential/light development
4. industrial/storage/commercial
5. recreational/rural hold

Reason:

Energy and ag tend to benefit early from broad geospatial screening, while residential and industrial require heavier local-rule interpretation.

### Stage 7. Recommendation engine

Deliver:

1. thesis normalization for discovery
2. listing-universe search adapters
3. candidate scoring and ranking
4. shortlist contract

Important constraint:

Recommendation should launch only after deep research produces trustworthy parcel objects, because the recommendation engine depends on that same core object.

### Stage 8. Channel productization

Deliver:

1. polished Slack/email response templates
2. batch comparison UX
3. optional Alice-branded employee identity and routing
4. memory and conversational refinement behavior

### Stage 9. Evaluation harness and operational review

Deliver:

1. benchmark datasets
2. claim-verification checks
3. citation audits
4. confidence calibration dashboards
5. source health monitoring

## 12. Recommended Alice Skill Package Shape

Recommended future repo shape:

```text
DoWhiz_service/skills/alice-ai/
  SKILL.md
  agents/
    openai.yaml
  references/
    source_categories.md
    coverage_tiers.md
    safety_guardrails.md
    use_case_modules.md
  schemas/
    alice_request.schema.json
    alice_land_research.schema.json
    county_coverage_registry.schema.json
    source_descriptor.schema.json
  scripts/
    parcel_resolution.py
    source_registry_cli.py
    render_report.py
```

This keeps Alice aligned with the existing DoWhiz shared-skill model.

## 13. Key Design Decisions

1. Alice should be a shared skill package plus structured artifact contract inside the existing DoWhiz worker model, not a separate backend service in v1.
2. The canonical system-of-record should be a structured `AliceLandResearchObject`, not freeform prose.
3. Confidence and completeness must be separate scores.
4. County coverage must be tiered as full, partial, or minimal instead of binary supported/unsupported.
5. Recommendation flow should rank the candidate universe Alice actually observed, not pretend to be a complete market crawler.
6. Deep research should ship before broad recommendation because trustworthy recommendation depends on trustworthy parcel objects.
7. Universal land research should be separated from use-case-specific modules so the product can scale across many investor intents.
8. Official local, state, and federal sources should outrank listing pages for factual claims.
9. Missing local data should result in explicit unknowns and degraded completeness, not silent inference.
10. Markdown reports should be rendered from structured JSON so channel summaries and long-form artifacts stay consistent.

## 14. Open Risks

1. County heterogeneity is extreme, so source-registry maintenance could become a large operational surface.
2. Local zoning interpretation is often document-heavy and ambiguous, which increases the risk of overstated conclusions.
3. Listing platforms may change page structures often, which can weaken recommendation-candidate acquisition.
4. Parcel groups and listing-to-parcel mismatches are common in land deals and can break naive identity resolution.
5. Utilities, water rights, and interconnection are exactly the areas where users most want certainty but public data is weakest.
6. Recommendation quality may be constrained by observable listing supply unless Alice later expands into stronger parcel-first discovery.
7. Confidence calibration may be difficult until a large verified benchmark set exists.
8. Batch comparison can create pressure to over-compress diligence and hide missing-data variance across parcels.

## 15. Assumptions

1. Alice will operate inside the current DoWhiz workspace, routing, and shared-skill architecture.
2. Public-data-first means Alice should work without paid vendor lock-in, even if paid sources are added later.
3. The initial product will prioritize English-language U.S. land diligence workflows.
4. Early launch priority is email, Slack, and Discord presentation, even though the runtime can support more channels later.
5. The user value is highest when Alice is explicit about unknowns rather than maximizing narrative smoothness.
6. Recommendation flow is acceptable if it is transparent about candidate-universe limits.
7. A small curated county registry plus heuristic fallback is a better starting strategy than waiting for nationwide manual county curation.

## 16. Recommended Next Implementation Step

The next implementation step should be:

1. formalize the schema and workspace contracts first
2. specifically create the initial `alice-ai` skill scaffold with:
   - `SKILL.md`
   - `agents/openai.yaml`
   - JSON schemas for request, parcel memo, source descriptor, and county coverage registry
   - a minimal workspace artifact layout contract

Why this is the right next step:

1. it matches the current DoWhiz shared-skill architecture
2. it stabilizes the contract before source adapters are built
3. it enables later implementation prompts to work against a precise object model
4. it creates the foundation for both deep research and recommendation without prematurely solving county integration details in code
