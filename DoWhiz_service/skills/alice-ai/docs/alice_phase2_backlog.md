# Alice Phase 2 Backlog

This document consolidates work that is intentionally **not** solved in Alice phase 1.

The goal is to keep unfinished work explicit instead of half-starting it in the phase-1 codebase.

## Broader source coverage

1. Expand county and local retrieval far beyond the current pilot counties.
2. Improve state-level source breadth where planning, utilities, or parcel identity are currently weak.
3. Add broader live-source calibration and freshness tracking instead of relying so heavily on deterministic fixtures.
4. Support more robust listing-source acquisition and search patterns without implying full market coverage.

## Stronger parcel confirmation

1. Add parcel-fabric and geometry-backed confirmation paths.
2. Improve map-based parcel-candidate narrowing and parcel-group handling.
3. Calibrate when `parcel_confirmed` should remain text-corroborated versus when geometry-backed confirmation is required.
4. Broaden parcel-identity strengthening beyond the current listing-plus-local-record pilot paths.

## Stronger local diligence depth

1. Add more zoning, planning, utility, frontage, subdivision, and site-constraint depth across more counties.
2. Improve local ordinance and development-surface coverage without overclaiming entitlement certainty.
3. Add richer infrastructure and access evidence where current signals are mostly directional.

## Economics and investment interpretation

1. Move from directional economics scaffolding toward structured underwriting inputs.
2. Add stronger carry-cost, capex, revenue, lease, and exit-case modeling.
3. Calibrate module heuristics against reviewed deals and benchmark outcomes.
4. Add scenario comparison and portfolio-style tradeoff tooling.

## Recommendation expansion

1. Broaden observed candidate acquisition without losing candidate-universe traceability.
2. Improve dedupe and candidate linking across broader listing surfaces.
3. Add better ranking calibration, preference tuning, and evidence-weighting review loops.
4. Support richer refinement workflows over prior shortlist runs.

## Productization and workflow

1. Add polished UI and export surfaces.
2. Add reviewer workflow support, approval loops, and collaborative memo refinement.
3. Add richer operational automation only after evidence quality and evaluation breadth improve.
4. Improve packaging for external release once the underlying evidence surface is broader and better calibrated.

## Evaluation and calibration backlog

1. Expand the benchmark set beyond the current deterministic scenario suite.
2. Add broader live-source smoke tests for selected source families.
3. Add reviewer-scored truthfulness and usefulness evaluation over more parcel types and geographies.
4. Add module-calibration benchmarks so favorable, mixed, weak, and unknown are better grounded.

## What should stay out of phase 1

1. Exhaustive national source coverage
2. Guaranteed parcel confirmation
3. Final ROI underwriting
4. Interconnection feasibility claims
5. Water-rights legal determination
6. Autonomous outreach or negotiation
7. Polished end-user frontend delivery
