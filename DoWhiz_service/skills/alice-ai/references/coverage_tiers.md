# Alice Coverage Tiers

Coverage tier is a county- or subject-level statement about **how much relevant local diligence surface Alice could inspect**.

Coverage tier is not the same thing as confidence, and it is not the same thing as completeness, even though they are related.

## Coverage tiers

### `full`

Use `full` when Alice can usually obtain all of the following for the subject county or parcel context:

1. parcel identity from official local sources
2. parcel geometry
3. local zoning or land-use visibility
4. local planning or code context
5. strong baseline environmental overlays
6. at least one meaningful local infrastructure or utility signal

`full` does not mean the parcel is fully diligenced. It means the local source environment is strong enough to support robust parcel-level research.

### `partial`

Use `partial` when Alice can obtain:

1. meaningful parcel identity and geography
2. some local planning, zoning, tax, or infrastructure context
3. strong state or federal baseline overlays

but one or more important local surfaces are weak, incomplete, stale, or difficult to verify directly.

### `minimal`

Use `minimal` when Alice can still produce a useful report path, but the local source environment is thin.

Typical `minimal` conditions:

1. weak or absent county parcel interfaces
2. no machine-readable zoning or planning map
3. only national and state overlays available
4. limited local utility or infrastructure visibility

`minimal` does not mean "unsupported." Alice should still produce an honest output, but that output must surface the limits clearly.

## Mechanical rubric

The stabilization patch formalizes a lightweight rubric so registry validation can infer whether a county deserves `full`, `partial`, or `minimal`.

The rubric uses the Step 3 county capability fields already present in the registry.

### Bucket logic

1. baseline footing
   - `environmental >= partial`
2. identity footing
   - `parcel_identity >= partial` or `tax_roll >= partial`
3. planning footing
   - `zoning >= partial` or `planning_docs >= partial`
4. infrastructure footing
   - `utilities >= partial` or `transmission >= partial`

### `full` rubric

Use `full` only when all of the following are true:

1. baseline footing is present
2. `parcel_identity = full`
3. tax-roll footing is at least `partial`
4. zoning footing is present
5. infrastructure footing is present

Registry note:

1. Step 3 county capabilities do not encode parcel geometry as a separate flag, so the rubric proxies strong local parcel geometry support through strong parcel-identity curation plus the curated local source set.

### `partial` rubric

Use `partial` when:

1. baseline footing is present
2. identity footing is present
3. either planning footing or infrastructure footing is present
4. but the county does not meet the stricter `full` rule

### `minimal` rubric

Use `minimal` when the county does not meet the `partial` rule.

Typical cases:

1. generated fallback counties
2. counties with baseline overlays but no meaningful local parcel or planning footing
3. counties where only broadband or other weak directional signals exist without local parcel diligence

`broadband` alone does not move a county from `minimal` to `partial`.

## Coverage tier vs confidence vs completeness

These concepts must stay separate:

### Coverage tier

What local source environment Alice had access to.

### Confidence

How likely the reported findings are materially correct, given the evidence that was actually reviewed.

### Completeness

How much of the desired diligence surface Alice was able to inspect for this parcel or request.

## Practical examples

1. High confidence + low completeness:
   - Alice is confident about a few verified facts, but many local diligence surfaces remain unavailable.

2. Partial coverage + moderate confidence:
   - Alice found strong parcel identity and some overlays, but zoning or utilities remain only partly visible.

3. Minimal coverage + low completeness:
   - Alice can place the parcel geographically and run baseline federal/state screens, but local parcel-level conclusions are limited.
