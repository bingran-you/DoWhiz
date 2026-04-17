# Alice Jurisdiction Context

Jurisdiction context is the layer between subject resolution and coverage/source planning.

It answers a narrower question than research:

1. Which governing geography can Alice defend strongly enough to attach county coverage and source selection?

## Why This Layer Exists

Subject resolution tells Alice what the user is talking about.

Jurisdiction context tells Alice which county/state/city context is safe to use next.

Those are not the same thing.

Examples:

1. a listing URL may be a valid subject candidate while county attachment is still inferred
2. an address can be normalized while county remains unresolved
3. a follow-up can inherit a county from prior session state even when the new message does not restate it

## Explicit vs Inferred vs Inherited

Step 5 preserves how geography was derived.

### Explicit

Use when the user directly supplied county/state information in the request contract.

Examples:

1. `target_geographies[].county_fips`
2. request-level county/state text already normalized into request hints

### Inferred

Use when geography came from subject-level clues rather than a direct user county declaration.

Examples:

1. listing URL path hints
2. address parsing that yields city/state but not county confirmation
3. subject-resolution rollups

### Inherited

Use when a follow-up or reused session attaches to prior Alice state.

Examples:

1. `subject_ref` paired with `session_state.json`
2. follow-up prompts that keep the previously confirmed parcel or county context

### Unresolved

Use when Alice does not yet have a defensible county or state attachment.

## Precedence Rules

Preferred precedence in this step:

1. explicit request county/state FIPS
2. explicit request county/state text
3. explicit subject hints already normalized into the request
4. inherited session-state context
5. subject-resolution identifiers and hints
6. listing URL slug hints
7. unresolved

Important guardrails:

1. APN normalization does not imply county
2. address text alone does not imply county unless county/state was explicitly carried in the request contract
3. coordinates alone do not imply county in Step 5 because no offline boundary lookup is attached yet
4. lower-precedence hints should not silently override stronger prior context

## County, City, and Unincorporated Status

County context is the key attachment point for registry coverage.

City or place context is still tracked because future planning, zoning, and utility research may depend on it.

`incorporated_status` should stay `unknown` unless Alice has an explicit reason to mark the place as incorporated or unincorporated. Step 5 intentionally does not guess this from address text alone.

## Batch Behavior

Batch cases should not be flattened into one county if only some subjects have stable county clues.

Use:

1. top-level `geography_status = mixed_subjects` when active subjects do not share the same defensible county attachment
2. `subject_jurisdictions[]` to preserve per-subject geography states
3. `county_fips = null` when no single county should govern the whole batch plan yet

## Follow-Up Reuse

Follow-up inheritance is allowed here even when Step 4 kept the subject unresolved.

That means:

1. subject resolution can remain conservative
2. jurisdiction context can still recover prior county/state footing from `session_state.json`
3. later planning can explain that the county was inherited rather than restated
