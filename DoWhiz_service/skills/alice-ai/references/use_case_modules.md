# Alice Use-Case Modules

Step 9 adds a thesis-aware interpretation layer on top of the universal `alice_land_research` object.

The goal is screening-grade usefulness, not final feasibility.

## What a use-case module does

Each module asks:

1. what signals currently support the thesis
2. what blockers currently weaken the thesis
3. what critical unknowns still prevent a stronger conclusion
4. what economics inputs are still missing

The output lives inside `alice/parcel_memo.json` under `use_case_modules`.

It is downstream from:

1. subject resolution
2. jurisdiction context
3. coverage assessment
4. source planning
5. retrieval and extracted evidence
6. parcel candidates
7. universal research assembly

## Step 9 module families

Step 9 supports these screening modules:

1. `energy_solar`
2. `energy_wind`
3. `energy_battery`
4. `agriculture_general`
5. `residential_light_development`
6. `industrial_storage`
7. `recreational_rural_hold`

Each module also carries a `module_family` so later recommendation or rendering layers can group related modules cleanly.

## How modules are selected

Step 9 keeps module selection simple and deterministic:

1. explicit `thesis.use_case_hypotheses` are the primary driver
2. request text and thesis summary can add narrower module hints such as:
   `solar`, `battery`, `grazing`, `subdivision`, `industrial`, `rural hold`
3. if no clear thesis is supplied, Alice falls back to broad screening across the v0 module set

This is still a contract-driven heuristic layer, not a conversational planner.

## Fit assessment meaning

Step 9 uses these fit labels:

1. `favorable`
   enough supporting signals exist that the thesis looks directionally plausible at screening grade
2. `mixed`
   meaningful support exists, but blockers or missing diligence still materially limit the thesis
3. `weak`
   current blockers or missing footing materially constrain the thesis
4. `unknown`
   evidence is too thin for a defensible call
5. `not_applicable`
   reserved for future cases where a requested thesis clearly does not apply

These are not standalone ranking scores.

Step 10 uses them as one ranking input alongside blockers, unknowns, evidence quality, parcel identity strength, and request-constraint fit.

They do not mean:

1. feasible
2. entitled
3. financeable
4. buildable
5. profitable

## Module status meaning

Each module also carries a `module_status`:

1. `evaluated`
   enough evidence exists for a meaningful screening-grade interpretation
2. `partially_evaluated`
   some evidence exists, but critical surfaces are still missing
3. `insufficient_data`
   there is not enough evidence to interpret the thesis meaningfully
4. `blocked`
   reserved for cases where parcel or jurisdiction instability stops the module almost entirely

`fit_assessment` and `module_status` are related but different.

Examples:

1. a module can be `mixed` and still `evaluated`
2. a module can be `weak` and still `evaluated`
3. a module can be `unknown` because it is `insufficient_data`

## Evidence scope summary

Modules do not erase field-level truthfulness.

Each module includes `evidence_scope_summary` so later renderers can say whether the thesis is mainly supported by:

1. `parcel_confirmed`
2. `parcel_candidate`
3. `county_level`
4. `geography_only`
5. `listing_derived`
6. `inferred`
7. `unresolved`

The module summary may rely on mixed scopes.

Examples:

1. parcel identity can be `parcel_confirmed` while transmission clues remain `geography_only`
2. zoning can be `parcel_candidate` while flood is only `geography_only`
3. listing acreage can remain `listing_derived` even when parcel identity later strengthens

## Blocking flags vs unknowns

Step 9 keeps these separate:

1. `blocking_flags`
   what already weakens the thesis now
2. `key_unknowns`
   what still needs to be learned before a stronger conclusion is possible

Examples:

1. "zoning still unresolved" is a blocker
2. "actual interconnection cost is unknown" is an unknown
3. "water rights remain unresolved" is an unknown that may become a blocker for ag or residential use

## Module families in v0

### Energy

Signals used in Step 9:

1. acreage sufficiency
2. slope or terrain hints
3. flood / wetlands burden
4. power / transmission entry points
5. access hints
6. zoning or planning clues when available

Step 9 does not claim:

1. interconnection feasibility
2. queue viability
3. upgrade cost certainty
4. final power-service capacity

### Agriculture

Signals used in Step 9:

1. acreage sufficiency
2. soils or NRCS-style productivity clues
3. flood / drainage hints
4. grazing or ag-language context
5. water and irrigation clues when available

Step 9 does not claim:

1. transferable water rights
2. crop profitability
3. yield certainty
4. lease-rate certainty

### Residential / Light Development

Signals used in Step 9:

1. acreage range
2. access / frontage hints
3. listing or thesis language about development
4. zoning, water, septic, and subdivision signals when available

Step 9 does not claim:

1. entitlement certainty
2. buildability certainty
3. utility availability certainty
4. plat approval certainty

### Industrial / Storage

Signals used in Step 9:

1. acreage sufficiency
2. road / logistics hints
3. power / transmission clues
4. zoning compatibility clues

Step 9 does not claim:

1. final truck access quality
2. service-capacity certainty
3. environmental permitting certainty

### Recreational / Rural Hold

Signals used in Step 9:

1. acreage sufficiency
2. hold framing from the thesis
3. access hints
4. environmental burden screens
5. price and budget signals when available

Step 9 does not claim:

1. carry-cost precision
2. exit-liquidity certainty
3. build rights

## Downstream use

Step 10 already uses these modules for:

1. recommendation ranking
2. thesis comparison
3. memo section rendering
4. diligence task generation
5. stronger economics modules

But Step 9 itself remains a conservative screening layer rather than a final decision engine.
