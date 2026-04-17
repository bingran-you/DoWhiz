# Alice Subject Resolution

Subject resolution is the contract layer between request normalization and land research.

Its job is to answer one question before Alice starts making land conclusions:

1. What land subject or subject set are we actually talking about right now?

This layer exists because listing URLs, APNs, addresses, coordinates, and follow-up prompts do not all imply the same level of parcel certainty.

## Why Subject Resolution Comes First

Alice should not start zoning, utility, or feasibility claims until the subject is stable enough for those claims to attach to something real.

Subject resolution protects against four common failure modes:

1. treating a listing URL as if it always maps to one parcel
2. treating an APN normalization result as if it were already an assessor-confirmed parcel
3. treating an address as if it automatically implies parcel boundaries
4. treating a follow-up like "compare this with the last one" as if the active subject is obvious

## Three Different Layers

Keep these layers separate:

### Input normalization

Normalization cleans and classifies what the user supplied.

Examples:

1. canonicalizing a listing URL
2. stripping APN delimiters into matching variants
3. validating a latitude/longitude pair
4. standardizing freeform address text

Normalization does **not** mean the parcel is resolved.

### Subject resolution

Subject resolution decides the current subject state.

Examples:

1. thesis-only request with no parcel yet
2. listing identified but parcel still unknown
3. APN identified and county-scoped, but parcel still awaiting official confirmation
4. coordinates known, but only at geography-only scope
5. parcel group recognized instead of silently collapsing to one parcel

### Parcel confirmation

Parcel confirmation happens later, when Alice can connect the subject to authoritative parcel evidence.

Examples:

1. confirmed parcel/APN match from county sources
2. confirmed parcel geometry
3. confirmed parcel group that must stay grouped

Step 4 formalizes only the first two layers.

## Resolution Status

The top-level `resolution_status` is intentionally coarse:

### `resolved`

Use when the active subject is stable enough to carry forward without re-asking "which parcel?"

Typical examples:

1. a previously confirmed parcel reused in a follow-up
2. an explicitly confirmed parcel group

### `partially_resolved`

Use when Alice knows a lot about the subject, but parcel-level certainty is still incomplete.

Typical examples:

1. listing identified, parcel not yet confirmed
2. APN normalized and county-scoped, but not yet checked against official parcel sources
3. address standardized, but geocoding and parcel lookup still pending
4. coordinates validated, but parcel fabric intersection still pending

### `unresolved`

Use when the active subject set is not stable enough for confident parcel-level research handoff.

Typical examples:

1. APN provided without county and no reliable county hint
2. batch compare request where the subjects do not yet normalize to comparable land targets
3. follow-up subject reference is ambiguous

## Resolution Levels

Per-subject `resolution_level` is more specific than top-level status.

Important distinctions:

### `thesis_only`

Recommendation-oriented setup. No parcel yet.

### `geography_only`

Alice knows a point or geography, but not a parcel.

### `address_identified`

Alice has a normalized address string and maybe city/state hints. This is **not** parcel resolution.

### `listing_identified`

Alice has a classified and canonicalized listing URL plus best-effort listing identifiers. This is **not** parcel resolution.

### `parcel_candidate_identified`

Alice has a strong parcel clue, such as APN plus county context, but still needs later confirmation.

### `parcel_resolved`

The active subject is a confirmed parcel.

### `parcel_group_resolved`

The active subject is intentionally a parcel group and must not be flattened.

### `subject_reference_only`

Follow-up input refers to prior thread state rather than providing a fresh parcel identifier.

## Address-Resolved vs Parcel-Resolved vs Geography-Only

These states are not interchangeable.

### Address-resolved

Alice can normalize the address text and maybe infer city/state, but still needs later geocoding and parcel lookup.

### Parcel-resolved

Alice has a confirmed parcel identity. Only this state is safe for confident parcel-specific downstream claims.

### Geography-only

Alice has a point, region, county, or state context but no parcel identity. This is still useful, but downstream research must stay honest about the missing parcel layer.

## Parcel Groups and Batch Requests

Alice must not silently collapse multi-subject or multi-parcel inputs.

Rules:

1. every active subject keeps its own `subject_id`
2. every subject keeps its own ambiguity flags and next actions
3. parcel groups stay explicit as parcel groups
4. batch compare requests use `subject_kind = batch_subject_set`
5. batch status can remain `unresolved` even when some individual subjects are only partially resolved
6. batching in Step 4 is represented as repeated concrete `subjects[]` entries, not a generic batch placeholder input kind

## Follow-Up Handling

Follow-ups should attach to prior Alice state instead of restarting blindly.

Use `alice/session_state.json` to carry:

1. active subject IDs
2. active resolution reference
3. current thesis snapshot
4. unresolved questions
5. user-confirmed assumptions

Use `alice/request_normalized.json` `follow_up_context` to say how the new request should interact with prior state.

Examples:

1. `refine_thesis`
2. `reuse_active_subjects`
3. `compare_with_previous`
4. `append_subjects`
5. `filter_subject_set`

If the follow-up reference is ambiguous, keep `resolution_status = unresolved` and ask for the minimum clarifying input.

In Step 4, `subject_ref` is only a pointer. The lightweight resolver does not silently dereference that pointer yet. That means a follow-up request may remain `unresolved` even when the prior `session_state.json` is present, until later orchestration explicitly reattaches the active subject set. Once that reattachment happens, the same follow-up can legitimately become `parcel_resolved` without changing the original input kind.

## Recommendation vs Deep Research

At the resolution layer:

### Recommendation mode

Recommendation mode can be valid with zero resolved parcels.

That usually means:

1. `subject_kind = thesis_request`
2. `resolution_level = thesis_only`
3. geography and use-case constraints are the subject, not a parcel

### Deep research mode

Deep research mode usually expects a concrete subject candidate:

1. listing URL
2. APN
3. address
4. coordinates
5. follow-up reference to a prior parcel

Deep research can still proceed later from a partially resolved or geography-only state, but the report must stay narrow until parcel confirmation exists.

## APN Normalization Caveats

APN normalization is for matching, not proof.

Rules:

1. preserve the raw APN exactly
2. uppercase and collapse delimiter noise into a conservative alphanumeric matching key
3. create a small matching variant set, typically canonical, hyphenated, and digits-only when valid
4. use county or state context when known to pick a formatting strategy, but do not fabricate missing jurisdiction
5. never infer ownership, geometry, or final parcel identity from formatting alone

## Listing URL Caveats

Listing URL normalization is also limited.

Step 4 only:

1. classifies the platform
2. canonicalizes the URL
3. extracts best-effort listing IDs and path hints

It does **not** scrape the page or confirm that the listing maps to one parcel.
