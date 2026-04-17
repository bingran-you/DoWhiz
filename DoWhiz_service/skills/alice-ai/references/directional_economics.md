# Alice Directional Economics

Step 9 adds the first honest economics scaffolding to Alice.

This is still not underwriting.

It is a scenario-framing layer that sits on top of:

1. universal research assembly
2. parcel candidates
3. use-case modules

## What Step 9 directional economics is for

Directional economics should help Alice say:

1. what economic frame is even discussable from current evidence
2. what cost buckets are obviously still missing
3. what exit or revenue paths the active thesis implies
4. what assumptions are user-supplied vs public-data-backed vs inferred

It should not pretend to know:

1. IRR
2. NPV
3. guaranteed ROI
4. final capex
5. final revenue
6. interconnection cost
7. entitlement cost certainty

## Status values

Step 9 uses three statuses:

1. `available`
   enough evidence exists to frame screening-grade scenario cases from current modules plus a live price basis
2. `limited`
   some economics framing is possible, but major assumptions or missing surfaces still dominate
3. `not_enough_data`
   current evidence is too thin for meaningful economics beyond explicit limitations

`available` still does not mean underwritten.

It only means Alice can state a clearer screening-grade scenario basis.

## Contract shape

`directional_economics` in `alice_land_research` should include:

1. `status`
2. `basis`
3. `active_module_ids`
4. `assumptions`
5. `carry_costs`
6. `improvement_capex_proxies`
7. `revenue_or_exit_cases`
8. `scenario_summary`
9. `limitations`

## Assumptions

Assumptions should stay explicit and typed.

Common Step 9 examples:

1. listing ask placeholder
2. working acreage placeholder
3. hold-period assumption
4. budget ceiling
5. lead screening module

Assumptions must label their source as:

1. `user_assumption`
2. `market_proxy`
3. `public_record`
4. `listing_source`
5. `derived_inference`

## Carry costs

Step 9 keeps carry costs lightweight.

Expected examples:

1. hold-period context
2. property-tax and assessment gap
3. carry/liquidity framing for hold theses

If taxes are not directly known, Alice should say so.

## Improvement capex proxies

Step 9 uses placeholder cost buckets, not estimates.

Typical buckets:

1. utility or interconnection
2. water or well improvements
3. entitlement and local permits
4. water / septic / subdivision improvements
5. site access or civil work

These fields are meant to tell later steps what the economics still depends on.

## Revenue or exit cases

Each scenario case should point back to a module where possible.

Examples:

1. solar option value / ground lease
2. battery storage option value
3. working-land / grazing hold
4. low-density homesite or small subdivision
5. industrial yard / storage use
6. rural hold / recreation resale

These are directional scenario frames, not forecasts.

## How Step 9 decides status

The current implementation keeps this conservative:

1. if Alice has a live price basis plus meaningful module output and local footing is not extremely thin, status can be `available`
2. if Alice only has partial module evidence or only a weak acquisition basis, status should be `limited`
3. if Alice lacks both a usable price basis and meaningful module output, status should be `not_enough_data`

## Truthfulness rules

Directional economics must never imply:

1. a completed financial model
2. entitlement certainty
3. power-delivery certainty
4. transferable water-right certainty
5. guaranteed resale timing

If Alice only has listing ask plus thesis framing, the economics layer should say that directly.

If Alice only has geography-only signals for a module, the economics layer should stay cautious.

## Rendering rule

Rendered memos should present directional economics as:

1. a status
2. a short basis statement
3. explicit assumptions
4. missing cost buckets
5. scenario framing
6. explicit limitations

This section should help a user understand what still has to be diligenced before real underwriting begins.
