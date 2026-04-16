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
