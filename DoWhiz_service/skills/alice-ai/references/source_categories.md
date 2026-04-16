# Alice Source Categories

Alice uses a source taxonomy that separates *what kind of authority a source is* from *how the source is accessed*.

Source categories are modeled by the `category` field in `schemas/source_descriptor.schema.json`.

## Categories

### `federal`

Nationwide or multi-state official sources used for baseline overlays and national screening.

Typical uses:

1. flood baseline
2. wetlands baseline
3. topography and hydrography
4. soils and land capability
5. environmental screening
6. broadband baseline

These sources are often strong for consistent baseline coverage, but they do not replace local parcel or zoning diligence.

### `state`

Official state-level sources used for statewide context or when county capability is weak.

Typical uses:

1. state parcel layers where available
2. state water-rights systems
3. state environmental overlays
4. state utility or energy-siting information
5. state DOT and infrastructure context

### `county`

Official county-level sources. These are often the highest-value local sources for land diligence.

Typical uses:

1. assessor parcel search
2. parcel GIS
3. tax roll
4. county zoning or land-use maps
5. county planning pages and development code

### `city_local`

City, town, ETJ-like, or other local planning sources below the county level.

Typical uses:

1. municipal zoning maps
2. future land use maps
3. local planning code
4. utility district maps
5. service-area boundaries

### `listing_platform`

Market-discovery and listing-context sources.

Typical uses:

1. listing URL intake
2. ask price
3. stated acreage
4. listing text and marketing claims
5. nearby listing context

Listing platforms are useful, but they are not the final authority on parcel identity, zoning, utilities, or environmental constraints when official sources exist.

### `infrastructure_utility_market`

Directional feasibility and operating-context sources that do not fit the core federal/state/county/city/local buckets.

Typical uses:

1. utility territory maps
2. transmission and substation viewers
3. logistics maps
4. market-signal sources
5. public infrastructure context

## Important distinction: category vs interface type

Category answers:

1. what level or class of authority the source represents

Interface type answers:

1. how Alice accesses the source

The same category can appear through different interface types:

1. `formal_api`
2. `gis_service`
3. `downloadable_dataset`
4. `web_only`

Alice should reason about both, not just one.
