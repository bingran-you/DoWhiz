---
name: used-car-search
description: Search for used cars from reputable large dealership sources and present a curated catalog. Use when the user asks about buying a used car, finding a car within a budget, searching for vehicles in their area, or any variant like "find me a car under $X", "used cars near me", "looking for a reliable sedan", or "what's available at CarMax". Trigger even when the user doesn't explicitly say "used car search" but is clearly shopping for a vehicle purchase.
---

# Used Car Search

A structured framework for finding used cars from reputable large dealership sources. The output is a markdown table the user can scan in under a minute to identify promising options, with direct links to each listing.

## Required inputs

1. **Location** — City, state, or ZIP code where the user plans to purchase.
2. **Price range** — Maximum budget. Default to $5,000 if not specified.
3. **Purchase timeline** — When the user plans to buy. Assume within the next 2 months if not specified.

## Optional inputs

- **Car type** — Sedan, SUV, truck, hatchback, etc. Consider if specified, otherwise search across types.
- **Make/model preferences** — Specific brands or models the user prefers.
- **Mileage limit** — Maximum acceptable mileage.
- **Year range** — Minimum model year.

## Approved sources

**IMPORTANT: Only search from these reputable large dealership and marketplace sources:**

- **CarMax** (carmax.com)
- **Carvana** (carvana.com)
- **AutoTrader** (autotrader.com)
- **Cars.com** (cars.com)
- **CarGurus** (cargurus.com)
- **TrueCar** (truecar.com)

**Do NOT include under any circumstances:**
- Facebook Marketplace
- Craigslist
- Private party sales
- Small independent dealerships
- Auction sites

## Workflow

1. **Clarify constraints** — If location or budget is missing, ask before searching.
2. **Web search** — Use web search to find current inventory matching user constraints from approved sources.
3. **Filter and rank** — Prioritize by value (price vs. mileage vs. year), reliability reputation of the make/model, and deal ratings if available.
4. **Present as table** — Output a markdown table with the columns specified below.

## Output format

**IMPORTANT: Always present results as a markdown table with these exact columns:**

| Year | Make | Model | Price | Mileage | Location | Source | URL |
|------|------|-------|-------|---------|----------|--------|-----|
| 2019 | Honda | Civic | $4,800 | 62,000 | Austin, TX | CarMax | [View listing](https://...) |

The URL column must contain a clickable link to the actual listing where Oliver found the car.

Include 5-10 options that best match the user's constraints. Below the table, add:

1. **Top pick** — One sentence on which car offers the best overall value and why.
2. **Budget note** — If the budget is tight for the location/preferences, note what trade-offs exist (higher mileage, older year, different type).
3. **Next steps** — Remind user to check vehicle history (Carfax/AutoCheck), schedule a test drive, and verify the listing is still available.

## What this framework rejects

- Listings from non-approved sources
- Cars without verifiable pricing
- Salvage or rebuilt title vehicles (unless user explicitly requests)
- Results older than 7 days (inventory changes fast)
