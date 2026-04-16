# Alice Use-Case Modules

Alice is layered:

1. universal land research layer
2. use-case-specific modules

The universal layer is the base contract for every parcel memo. Use-case modules add thesis-specific screening on top of that same canonical object.

## Universal layer first

Before any module-specific opinion, Alice should have a structured baseline for:

1. subject identity
2. parcel and jurisdiction context
3. listing metadata
4. planning and land use baseline
5. environmental baseline
6. water and agriculture baseline
7. infrastructure and utilities baseline
8. risks, unknowns, citations, confidence, and completeness

## Standard module pattern

Each module should eventually produce:

1. `module_id`
2. `module_label`
3. `fit_assessment`
4. `confidence`
5. `blocking_flags`
6. `supporting_signals`
7. `key_unknowns`
8. `economics_inputs_required`
9. `summary`
10. `citation_ids`

The module contract is defined in `schemas/alice_land_research.schema.json` under `use_case_modules`.

## Initial module families

### Energy

Initial family examples:

1. `energy_solar`
2. `energy_wind`
3. `energy_battery_storage`

Typical focus:

1. acreage sufficiency
2. slope and terrain proxies
3. flood and wetlands burden
4. transmission and substation proximity
5. utility territory
6. access
7. zoning fit signals

Important guardrail:

Energy modules may report directional siting signals, but must not claim guaranteed interconnection or queue viability.

### Agriculture

Initial family examples:

1. `agriculture_general`
2. `agriculture_irrigated`
3. `agriculture_rangeland`

Typical focus:

1. soils and land capability
2. cropland or pasture signals
3. groundwater and irrigation signals
4. flood and drainage
5. ag exemption signals

Important guardrail:

Agriculture modules must not infer transferable water rights or crop economics from soils alone.

### Residential / light development

Initial family examples:

1. `residential_light_development`
2. `rural_subdivision_light`

Typical focus:

1. zoning and future land use
2. lot-size and subdivision signals
3. access and frontage
4. water and septic proxies
5. flood, slope, and fire burden
6. nearby development pattern

Important guardrail:

These modules must not imply guaranteed entitlement or buildability.

### Industrial / storage / commercial-type land

Initial family examples:

1. `industrial_storage_commercial`
2. `yard_storage_logistics`

Typical focus:

1. zoning compatibility
2. logistics access
3. utility signals
4. environmental burden
5. adjacency to industrial uses or corridors

Important guardrail:

These modules must not imply guaranteed capacity, service, or commercial entitlement.

### Recreational / rural hold

Initial family examples:

1. `recreational_rural_hold`
2. `rural_lifestyle_hold`

Typical focus:

1. access and remoteness
2. topography and scenery proxies
3. water feature proximity
4. habitat and recreation context
5. holding-cost profile

Important guardrail:

These modules must not overstate usable or buildable area when access or local rules are still unclear.
