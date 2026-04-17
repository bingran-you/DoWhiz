#!/usr/bin/env python3

"""Shared registry helpers for Alice AI county and source metadata."""

from __future__ import annotations

import json
from functools import lru_cache
from pathlib import Path
from typing import Any


COUNTY_REGISTRY_VERSION = "alice.county_coverage_registry.v1"
COUNTY_INDEX_VERSION = "alice.us_counties.v1"
FALLBACK_CAPABILITIES = {
    "parcel_identity": "minimal",
    "zoning": "none",
    "planning_docs": "none",
    "tax_roll": "none",
    "utilities": "none",
    "environmental": "partial",
    "water": "none",
    "transmission": "none",
    "broadband": "partial",
    "listings": "none",
}
CAPABILITY_LEVEL_ORDER = {
    "none": 0,
    "minimal": 1,
    "partial": 2,
    "full": 3,
}
FEDERAL_BASELINE_SOURCE_IDS = [
    "federal_fema_nfhl",
    "federal_usfws_nwi",
    "federal_usda_soil_data_access",
    "federal_usgs_3dep_elevation",
    "federal_fcc_bdc_national",
    "federal_epa_ejscreen",
]

SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
REGISTRY_ROOT = SKILL_ROOT / "registry"
SCHEMAS_ROOT = SKILL_ROOT / "schemas"
COUNTY_INDEX_PATH = REGISTRY_ROOT / "us_counties.json"
COUNTY_OVERRIDES_ROOT = REGISTRY_ROOT / "county_overrides"
SOURCES_ROOT = REGISTRY_ROOT / "sources"
COUNTY_SCHEMA_PATH = SCHEMAS_ROOT / "county_coverage_registry.schema.json"
SOURCE_SCHEMA_PATH = SCHEMAS_ROOT / "source_descriptor.schema.json"


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


@lru_cache(maxsize=1)
def load_county_index() -> dict[str, Any]:
    return load_json(COUNTY_INDEX_PATH)


@lru_cache(maxsize=1)
def county_rows() -> dict[str, dict[str, Any]]:
    counties = load_county_index().get("counties", {})
    return {county_fips: dict(row) for county_fips, row in counties.items()}


def get_county_row(county_fips: str) -> dict[str, Any]:
    row = county_rows().get(county_fips)
    if row is None:
        raise KeyError(f"Unknown county_fips: {county_fips}")
    return row


def iter_county_override_paths() -> list[Path]:
    return sorted(COUNTY_OVERRIDES_ROOT.glob("*/*.json"))


def iter_source_paths() -> list[Path]:
    return sorted(SOURCES_ROOT.glob("*/*.json"))


def build_fallback_county_entry(county_row: dict[str, Any]) -> dict[str, Any]:
    county_name = county_row["county_name"]
    state_name = county_row["state_name"]
    return {
        "county_fips": county_row["county_fips"],
        "state_fips": county_row["state_fips"],
        "county_name": county_name,
        "registry_version": COUNTY_REGISTRY_VERSION,
        "coverage_tier": "minimal",
        "capabilities": dict(FALLBACK_CAPABILITIES),
        "preferred_source_ids": list(FEDERAL_BASELINE_SOURCE_IDS),
        "discovered_source_ids": list(FEDERAL_BASELINE_SOURCE_IDS),
        "known_limitations": [
            (
                f"{county_name}, {state_name} is not yet curated in the Alice "
                "county registry; this entry is a generated minimal fallback."
            ),
            (
                "Local parcel, zoning, planning, tax roll, and utility sources "
                "have not yet been verified for this county."
            ),
            (
                "Current county coverage depends on national baseline sources "
                "until county-specific discovery and curation are added."
            ),
        ],
        "last_verified_at": None,
    }


def county_override_path(county_fips: str) -> Path:
    county = get_county_row(county_fips)
    return COUNTY_OVERRIDES_ROOT / county["state_code"] / f"{county_fips}.json"


def load_county_override(county_fips: str) -> dict[str, Any] | None:
    path = county_override_path(county_fips)
    if not path.exists():
        return None
    return load_json(path)


def get_effective_county_entry(county_fips: str) -> dict[str, Any]:
    override = load_county_override(county_fips)
    if override is not None:
        return override
    return build_fallback_county_entry(get_county_row(county_fips))


@lru_cache(maxsize=1)
def load_all_sources() -> list[dict[str, Any]]:
    sources = []
    for path in iter_source_paths():
        descriptor = load_json(path)
        descriptor["_path"] = str(path)
        sources.append(descriptor)
    sources.sort(key=lambda item: (item["category"], item["source_id"]))
    return sources


@lru_cache(maxsize=1)
def source_by_id() -> dict[str, dict[str, Any]]:
    return {source["source_id"]: dict(source) for source in load_all_sources()}


def list_sources(
    *,
    category: str | None = None,
    state_fips: str | None = None,
    county_fips: str | None = None,
    capability: str | None = None,
    source_id: str | None = None,
) -> list[dict[str, Any]]:
    applicable_state_fips = state_fips
    if county_fips is not None:
        applicable_state_fips = get_county_row(county_fips)["state_fips"]

    results = []
    for source in load_all_sources():
        if category and source["category"] != category:
            continue
        geography = source["geography_scope"]
        if county_fips is not None:
            matches_county = geography["county_fips"] == county_fips
            matches_state = (
                geography["county_fips"] is None
                and applicable_state_fips is not None
                and geography["state_fips"] == applicable_state_fips
            )
            matches_national = geography["national"] is True
            if not (matches_county or matches_state or matches_national):
                continue
        elif state_fips:
            matches_state = geography["state_fips"] == state_fips
            matches_national = geography["national"] is True
            if not (matches_state or matches_national):
                continue
        if capability and capability not in source["capabilities"]:
            continue
        if source_id and source["source_id"] != source_id:
            continue
        results.append(source)
    return results


def compact_source_view(source: dict[str, Any]) -> dict[str, Any]:
    return {
        "source_id": source["source_id"],
        "name": source["name"],
        "category": source["category"],
        "jurisdiction_level": source["jurisdiction_level"],
        "state_fips": source["geography_scope"]["state_fips"],
        "county_fips": source["geography_scope"]["county_fips"],
        "interface_type": source["interface_type"],
        "access_mode": source["access_mode"],
        "capabilities": source["capabilities"],
        "status": source["status"],
    }


def capability_level_at_least(level: str | None, minimum: str) -> bool:
    return CAPABILITY_LEVEL_ORDER.get(level or "none", 0) >= CAPABILITY_LEVEL_ORDER[minimum]


def coverage_tier_rubric(capabilities: dict[str, Any]) -> dict[str, Any]:
    baseline_footing = capability_level_at_least(capabilities.get("environmental"), "partial")
    identity_footing = (
        capability_level_at_least(capabilities.get("parcel_identity"), "partial")
        or capability_level_at_least(capabilities.get("tax_roll"), "partial")
    )
    zoning_footing = capability_level_at_least(capabilities.get("zoning"), "partial")
    planning_footing = (
        zoning_footing
        or capability_level_at_least(capabilities.get("planning_docs"), "partial")
    )
    infrastructure_footing = (
        capability_level_at_least(capabilities.get("utilities"), "partial")
        or capability_level_at_least(capabilities.get("transmission"), "partial")
    )

    if (
        baseline_footing
        and capabilities.get("parcel_identity") == "full"
        and capability_level_at_least(capabilities.get("tax_roll"), "partial")
        and zoning_footing
        and infrastructure_footing
    ):
        inferred_tier = "full"
    elif baseline_footing and identity_footing and (planning_footing or infrastructure_footing):
        inferred_tier = "partial"
    else:
        inferred_tier = "minimal"

    return {
        "baseline_footing": baseline_footing,
        "identity_footing": identity_footing,
        "zoning_footing": zoning_footing,
        "planning_footing": planning_footing,
        "infrastructure_footing": infrastructure_footing,
        "inferred_tier": inferred_tier,
    }
