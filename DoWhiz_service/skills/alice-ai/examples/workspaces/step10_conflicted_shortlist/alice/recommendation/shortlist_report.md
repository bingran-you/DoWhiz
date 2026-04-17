# Alice Recommendation Shortlist: Compare observed acreage listings for solar first, with rural hold as a fallback, while preserving parcel-identity tradeoffs.

## Advisory Boundary
This shortlist is drawn from the candidate universe Alice was able to observe and process. It is not exhaustive market coverage.

## Request Thesis
- Summary: Compare observed acreage listings for solar first, with rural hold as a fallback, while preserving parcel-identity tradeoffs.
- Active thesis families: energy, recreational_rural_hold
- Budget ceiling: $700,000.00

## Candidate Universe
- Observed candidates: 3
- Acquisition mode: user supplied ranking
- Platforms: landwatch, land_com
- Limitation note: This shortlist is limited to the 3 candidates Alice was able to observe and process. It is not exhaustive market coverage.
- Observation limit: Recommendation v0 ranks candidates only from user-supplied URLs or Alice's observed listing catalog.
- Observation limit: This candidate universe is not a live or exhaustive market crawl.

## Shortlist Summary
- From the candidates Alice was able to observe, LandWatch listing 12345678 near 80 Acres near Sierra Blanca (TX) in Hudspeth County, TX ranks first because recreational / rural hold screening is favorable.
- Warning: Alice is ranking only the observed candidate universe and does not claim exhaustive market coverage.
- Warning: Shortlist ordering reflects thesis-fit tradeoffs plus evidence quality rather than one single score.

## Ranked Candidates
### 1. LandWatch listing 12345678 near 80 Acres near Sierra Blanca (TX) in Hudspeth County, TX
- Why it surfaced: LandWatch listing 12345678 near 80 Acres near Sierra Blanca (TX) in Hudspeth County, TX ranks with an overall favorable thesis fit from the observed candidate universe. Primary support: Requested use-case modules land in favorable or clearly supportive territory at screening grade. Primary drag: 2 material blockers are already visible in the current screening set.
- Fit summary: favorable across recreational_rural_hold, energy_solar.
- Geography: County-level context only; city or unincorporated place not yet confirmed for Hudspeth County., Hudspeth County, TX
- Market snapshot: $240,000.00; 80 acres
- Evidence footing: parcel_confirmed parcel identity, partial coverage tier, 44% overall confidence.
- Major blockers:
  - Flood screening was not completed or lacked a defensible point/parcel geometry.
  - No authoritative zoning or local solar-compatibility check is attached yet.
- Major unknowns:
  - Boundary-based access and environmental confirmation would still be needed before a final buy call.
  - Boundary-based environmental checks are still weaker than parcel geometry would allow.
  - Carrying costs, tax burden, and resale liquidity are not yet well constrained.
- Ranking factor (strong positive): Requested use-case modules land in favorable or clearly supportive territory at screening grade.
- Ranking factor (strong positive): Parcel identity is parcel-confirmed, which reduces subject mismatch risk.
- Ranking factor (neutral): Coverage footing is usable but still partial.
- Ranking factor (neutral): Evidence quality is usable but incomplete.
- Candidate memo artifact: DoWhiz_service/skills/alice-ai/examples/workspaces/step7_strong_curated_hudspeth/alice/parcel_memo.json
- Candidate report artifact: DoWhiz_service/skills/alice-ai/examples/workspaces/step7_strong_curated_hudspeth/alice/report.md

### 2. Land.com listing 60606060 near Bakersfield, Kern County, CA
- Why it surfaced: Land.com listing 60606060 near Bakersfield, Kern County, CA ranks with an overall mixed thesis fit from the observed candidate universe. Primary support: Requested use-case modules show usable support, but the thesis still carries material blockers or diligence gaps. Primary drag: 3 material blockers are already visible in the current screening set.
- Fit summary: mixed across recreational_rural_hold, energy_solar.
- Geography: Bakersfield, Kern County, CA
- Market snapshot: $640,000.00; 160 acres
- Evidence footing: candidate_corroborated parcel identity, full coverage tier, 45% overall confidence.
- Major blockers:
  - Competing parcel candidates remain active, so parcel-specific thesis conclusions cannot be treated as final.
  - Flood screening was not completed or lacked a defensible point/parcel geometry.
  - The active subject is not yet anchored to one official parcel record.
- Major unknowns:
  - Boundary-based access and environmental confirmation would still be needed before a final buy call.
  - Boundary-based environmental checks are still weaker than parcel geometry would allow.
  - Carrying costs, tax burden, and resale liquidity are not yet well constrained.
- Ranking factor (positive): Requested use-case modules show usable support, but the thesis still carries material blockers or diligence gaps.
- Ranking factor (positive): One parcel candidate is locally corroborated, though parcel-specific conclusions remain provisional.
- Ranking factor (positive): Local coverage footing is stronger than federal-baseline-only screening.
- Ranking factor (neutral): Evidence quality is usable but incomplete.
- Candidate memo artifact: DoWhiz_service/skills/alice-ai/examples/workspaces/step7_competing_kern/alice/parcel_memo.json
- Candidate report artifact: DoWhiz_service/skills/alice-ai/examples/workspaces/step7_competing_kern/alice/report.md

### 3. Land.com listing 55555555 near 25 Acres in Autauga County Alabama (AL)
- Why it surfaced: Land.com listing 55555555 near 25 Acres in Autauga County Alabama (AL) ranks with an overall mixed thesis fit from the observed candidate universe. Primary support: Requested use-case modules show usable support, but the thesis still carries material blockers or diligence gaps. Primary drag: Parcel identity is still weak or competing, which limits parcel-specific ranking confidence.
- Fit summary: mixed across recreational_rural_hold, energy_solar.
- Geography: County-level context only; city or unincorporated place not yet confirmed for Autauga County., Autauga County, AL
- Market snapshot: $185,000.00; 25 acres
- Evidence footing: candidate_unconfirmed parcel identity, minimal coverage tier, 34% overall confidence.
- Major blockers:
  - Flood screening was not completed or lacked a defensible point/parcel geometry.
  - Local county planning and utility footing is still thin, so solar screening stays preliminary.
  - Local parcel, tax, and market footing is still thin, so the hold thesis stays mostly directional.
- Major unknowns:
  - Boundary-based access and environmental confirmation would still be needed before a final buy call.
  - Boundary-based environmental checks are still weaker than parcel geometry would allow.
  - Carrying costs, tax burden, and resale liquidity are not yet well constrained.
- Ranking factor (positive): Requested use-case modules show usable support, but the thesis still carries material blockers or diligence gaps.
- Ranking factor (negative): Parcel identity is still weak or competing, which limits parcel-specific ranking confidence.
- Ranking factor (negative): Local diligence footing is thin, so ranking confidence remains limited.
- Ranking factor (negative): Evidence quality is still directional, which limits shortlist confidence.
- Candidate memo artifact: DoWhiz_service/skills/alice-ai/examples/workspaces/step7_fallback_weak_autauga/alice/parcel_memo.json
- Candidate report artifact: DoWhiz_service/skills/alice-ai/examples/workspaces/step7_fallback_weak_autauga/alice/report.md

## Coverage and Ranking Notes
- Listing platforms remain market context; they do not override parcel-confirmed local evidence.
- Official local coverage still matters: weak local footing can drag a candidate below stronger-data peers even when listing optics look attractive.
- Recommendations reuse Step 6-9 candidate artifacts rather than inventing a parallel shallow scorer.

## Recommended Next Actions
- Advance parcel-specific diligence on LandWatch listing 12345678 near 80 Acres near Sierra Blanca (TX) in Hudspeth County, TX only after reviewing the linked candidate memo and its remaining blockers.
- Resolve the top unknown: Boundary-based access and environmental confirmation would still be needed before a final buy call.
- Treat this shortlist as a comparative screen from the observed universe, not a full market search.
