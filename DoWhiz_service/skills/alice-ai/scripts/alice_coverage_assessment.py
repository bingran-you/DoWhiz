#!/usr/bin/env python3

"""Coverage assessment builders for Alice AI Step 5."""

from __future__ import annotations

from pathlib import Path
from typing import Any
from uuid import NAMESPACE_URL, uuid5

from alice_jurisdiction import JURISDICTION_CONTEXT_SCHEMA_PATH, build_jurisdiction_context, load_subject_resolution
from alice_registry import get_effective_county_entry, load_all_sources, load_json
from alice_subject_resolution import dedupe_strings, load_request, now_iso


COVERAGE_ASSESSMENT_VERSION = "alice.coverage_assessment.v1"
SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
SCHEMAS_ROOT = SKILL_ROOT / "schemas"
COVERAGE_ASSESSMENT_SCHEMA_PATH = SCHEMAS_ROOT / "coverage_assessment.schema.json"

COUNTY_CAPABILITY_MAP = {
    "parcel_identity": "parcel_identity",
    "parcel_geometry": None,
    "zoning": "zoning",
    "planning_docs": "planning_docs",
    "tax_roll": "tax_roll",
    "environmental_baseline": "environmental",
    "water_signals": "water",
    "utilities": "utilities",
    "transmission": "transmission",
    "market_context": "listings",
}

CAPABILITY_TO_SOURCE_CAPABILITIES = {
    "parcel_identity": {"parcel_lookup_by_apn", "parcel_lookup_by_address", "owner_name", "site_address"},
    "parcel_geometry": {"parcel_geometry"},
    "zoning": {"zoning"},
    "planning_docs": {"future_land_use", "development_code"},
    "tax_roll": {"tax_roll"},
    "environmental_baseline": {"flood", "wetlands", "soil", "topography_elevation", "environmental_screening"},
    "water_signals": {"water_rights", "soil"},
    "utilities": {"utility_territory", "broadband"},
    "transmission": {"transmission"},
    "market_context": {"listing_search", "market_context"},
}

CAPABILITY_ORDER = [
    "parcel_identity",
    "parcel_geometry",
    "tax_roll",
    "zoning",
    "planning_docs",
    "environmental_baseline",
    "water_signals",
    "utilities",
    "transmission",
    "market_context",
]


def load_jurisdiction_context(path: str | Path) -> dict[str, Any]:
    return load_json(Path(path))


def _map_registry_level(level: str | None) -> str:
    return {
        "full": "available",
        "partial": "partial",
        "minimal": "minimal",
        "none": "unavailable",
        None: "unavailable",
    }[level]


def _platform_source_id(platform: str) -> str:
    return f"listing_platform_{platform}"


def _has_geography_anchor(subject_resolution: dict[str, Any], jurisdiction_context: dict[str, Any]) -> bool:
    if jurisdiction_context["county_fips"] or jurisdiction_context["state_fips"]:
        return True
    if subject_resolution.get("geography_context", {}).get("scope_status") in {
        "point_scoped",
        "region_scoped",
        "state_scoped",
        "county_scoped",
        "multi_county",
    }:
        return True
    return any(
        subject["identifiers"].get("coordinates") is not None
        for subject in subject_resolution["active_subjects"]
    )


def resolve_relevant_sources(
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
    jurisdiction_context: dict[str, Any],
) -> list[dict[str, Any]]:
    county_fips = jurisdiction_context["county_fips"]
    state_fips = jurisdiction_context["state_fips"]
    city_name = (
        jurisdiction_context["city_or_place"]["name"]
        if jurisdiction_context["city_or_place"]["resolution_status"] == "resolved"
        else None
    )
    listing_platforms = subject_resolution.get("listing_context", {}).get("platforms", [])

    relevant_sources = []
    for source in load_all_sources():
        geography = source["geography_scope"]
        if source["category"] == "listing_platform":
            if listing_platforms:
                if source["source_id"] in {_platform_source_id(platform) for platform in listing_platforms}:
                    relevant_sources.append(source)
            elif request["request_mode"] == "recommendation":
                relevant_sources.append(source)
            continue

        if geography["national"] is True:
            relevant_sources.append(source)
            continue

        if county_fips is not None and geography["county_fips"] == county_fips:
            if geography["city_name"] is not None and geography["city_name"] != city_name:
                continue
            relevant_sources.append(source)
            continue

        if state_fips is not None and geography["county_fips"] is None and geography["state_fips"] == state_fips:
            if geography["city_name"] is not None and geography["city_name"] != city_name:
                continue
            relevant_sources.append(source)

    deduped: dict[str, dict[str, Any]] = {}
    for source in relevant_sources:
        deduped[source["source_id"]] = source
    return list(deduped.values())


def _category_bucket(source: dict[str, Any]) -> str:
    if source["category"] == "federal":
        return "federal"
    if source["category"] == "state":
        return "state"
    if source["category"] == "county":
        return "county"
    if source["category"] == "city_local":
        return "local"
    if source["category"] == "listing_platform":
        return "listing_platform"
    return "other"


def _source_summary(
    relevant_sources: list[dict[str, Any]],
    county_entry: dict[str, Any] | None,
) -> dict[str, Any]:
    ids_by_bucket = {
        "federal": [],
        "state": [],
        "county": [],
        "local": [],
        "listing_platform": [],
    }
    official_count = 0
    for source in relevant_sources:
        bucket = _category_bucket(source)
        if bucket in ids_by_bucket:
            ids_by_bucket[bucket].append(source["source_id"])
        if source["category"] != "listing_platform":
            official_count += 1

    relevant_ids = sorted({source["source_id"] for source in relevant_sources})
    non_listing_categories = {
        source["category"]
        for source in relevant_sources
        if source["category"] != "listing_platform"
    }
    only_federal_baseline_available = bool(relevant_ids) and non_listing_categories == {"federal"}

    preferred_source_ids = []
    if county_entry is not None:
        preferred_source_ids = [
            source_id
            for source_id in county_entry["preferred_source_ids"]
            if source_id in relevant_ids
        ]

    return {
        "relevant_source_count": len(relevant_ids),
        "relevant_source_ids": relevant_ids,
        "preferred_source_ids": preferred_source_ids,
        "federal_source_ids": sorted(set(ids_by_bucket["federal"])),
        "state_source_ids": sorted(set(ids_by_bucket["state"])),
        "county_source_ids": sorted(set(ids_by_bucket["county"])),
        "local_source_ids": sorted(set(ids_by_bucket["local"])),
        "listing_platform_source_ids": sorted(set(ids_by_bucket["listing_platform"])),
        "official_source_count": official_count,
        "only_federal_baseline_available": only_federal_baseline_available,
    }


def _supporting_sources_for_capability(capability: str, relevant_sources: list[dict[str, Any]]) -> list[str]:
    supporting_caps = CAPABILITY_TO_SOURCE_CAPABILITIES[capability]
    source_ids = []
    for source in relevant_sources:
        if supporting_caps & set(source["capabilities"]):
            source_ids.append(source["source_id"])
    return sorted(set(source_ids))


def _critical_surfaces(
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
) -> list[str]:
    request_mode = request["request_mode"]
    use_cases = request["thesis"]["use_case_hypotheses"]
    surfaces: list[str] = []

    if request_mode in {"deep_research", "follow_up"}:
        surfaces.extend(
            [
                "parcel_identity",
                "parcel_geometry",
                "tax_roll",
                "zoning",
                "planning_docs",
                "environmental_baseline",
            ]
        )
    elif request_mode == "batch_compare":
        surfaces.extend(
            [
                "market_context",
                "parcel_identity",
                "parcel_geometry",
                "tax_roll",
                "environmental_baseline",
            ]
        )
    else:
        surfaces.extend(["market_context", "environmental_baseline"])

    if "energy" in use_cases:
        surfaces.extend(["utilities", "transmission"])
    if "agriculture" in use_cases:
        surfaces.append("water_signals")
    if any(
        use_case in use_cases
        for use_case in [
            "residential_light_development",
            "industrial_storage_commercial",
        ]
    ):
        surfaces.extend(["utilities", "zoning", "planning_docs"])
    if subject_resolution.get("listing_context", {}).get("has_listing_inputs"):
        surfaces.append("market_context")

    return dedupe_strings(surfaces)


def _build_capability_item(
    *,
    capability: str,
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
    jurisdiction_context: dict[str, Any],
    county_entry: dict[str, Any] | None,
    relevant_sources: list[dict[str, Any]],
) -> dict[str, Any]:
    county_fips = jurisdiction_context["county_fips"]
    state_fips = jurisdiction_context["state_fips"]
    has_anchor = _has_geography_anchor(subject_resolution, jurisdiction_context)
    supporting_source_ids = _supporting_sources_for_capability(capability, relevant_sources)
    registry_key = COUNTY_CAPABILITY_MAP[capability]
    county_registry_level = county_entry["capabilities"].get(registry_key) if county_entry and registry_key else None

    status = "unavailable"
    basis = "unresolved"
    notes: list[str] = []

    if capability == "market_context":
        if subject_resolution.get("listing_context", {}).get("has_listing_inputs"):
            status = "partial" if supporting_source_ids else "minimal"
            basis = "listing_platform"
            notes.append("Listing inputs already provide a directional market context entry point.")
        elif request["request_mode"] == "recommendation" and supporting_source_ids:
            status = "minimal"
            basis = "listing_platform"
            notes.append("Recommendation mode can use listing platforms later even before a parcel is chosen.")
        else:
            status = "unavailable"
            basis = "unresolved"
    elif capability in {"parcel_identity", "zoning", "planning_docs", "tax_roll"}:
        if county_fips is None:
            status = "blocked_by_unresolved_jurisdiction"
            basis = "unresolved"
            notes.append("County attachment is required before local parcel, zoning, planning, or tax sources can be used safely.")
        else:
            status = _map_registry_level(county_registry_level)
            basis = "county_registry"
            if not supporting_source_ids and status != "unavailable":
                status = "unavailable"
                notes.append("County registry suggests capability exists, but no matching registered source was found yet.")
            elif status == "unavailable":
                notes.append("County registry does not currently provide strong footing for this local surface.")
    elif capability == "parcel_geometry":
        if county_fips is not None:
            if supporting_source_ids:
                county_or_local = [
                    source_id
                    for source_id in supporting_source_ids
                    if source_id in {
                        source["source_id"]
                        for source in relevant_sources
                        if source["category"] in {"county", "city_local"}
                    }
                ]
                if county_or_local:
                    status = "available" if county_entry and county_entry["coverage_tier"] == "full" else "partial"
                    basis = "county_registry" if county_entry else "state_source_presence"
                else:
                    status = "partial"
                    basis = "state_source_presence"
            else:
                status = "unavailable"
                basis = "unresolved"
                notes.append("No registered parcel geometry source is currently attached to this county.")
        elif state_fips is not None and supporting_source_ids:
            status = "minimal"
            basis = "state_source_presence"
            notes.append("Statewide parcel geometry may help later, but county attachment is still missing.")
        else:
            status = "blocked_by_unresolved_jurisdiction"
            basis = "unresolved"
            notes.append("Parcel geometry planning is blocked until jurisdiction is more stable.")
    elif capability == "environmental_baseline":
        if not has_anchor:
            status = "blocked_by_unresolved_jurisdiction"
            basis = "unresolved"
            notes.append("Environmental overlays still need a defensible geography anchor.")
        elif county_fips is not None and county_entry is not None:
            status = _map_registry_level(county_registry_level)
            basis = "county_registry"
            if status == "unavailable" and supporting_source_ids:
                status = "minimal"
                basis = "federal_baseline"
            notes.append("Environmental baseline uses federal overlays even when county-local depth is thin.")
        elif supporting_source_ids:
            status = "minimal"
            basis = "federal_baseline"
            notes.append("Only national baseline overlays are currently attachable.")
        else:
            status = "unavailable"
            basis = "unresolved"
    elif capability == "water_signals":
        if not has_anchor:
            status = "blocked_by_unresolved_jurisdiction"
            basis = "unresolved"
            notes.append("Water-related screening still needs stable geography.")
        elif county_fips is not None and county_entry is not None and county_registry_level != "none":
            status = _map_registry_level(county_registry_level)
            basis = "county_registry"
        elif supporting_source_ids:
            status = "minimal"
            basis = "federal_baseline"
            notes.append("Current water or agriculture footing is directional and mostly baseline-level.")
        else:
            status = "unavailable"
            basis = "unresolved"
    elif capability in {"utilities", "transmission"}:
        if not state_fips and not county_fips:
            status = "blocked_by_unresolved_jurisdiction"
            basis = "unresolved"
            notes.append("Utilities and transmission planning still needs at least state-level geography.")
        elif county_fips is not None and county_entry is not None and county_registry_level != "none":
            status = _map_registry_level(county_registry_level)
            basis = "county_registry"
        elif supporting_source_ids:
            status = "minimal" if county_fips is None else "partial"
            basis = "state_source_presence"
            notes.append("State-level infrastructure sources are available, but local feasibility remains directional.")
        else:
            status = "unavailable"
            basis = "unresolved"

    return {
        "status": status,
        "county_registry_level": county_registry_level,
        "supporting_source_ids": supporting_source_ids,
        "coverage_basis": basis,
        "notes": dedupe_strings(notes),
    }


def _local_footing_status(
    *,
    county_fips: str | None,
    county_entry: dict[str, Any] | None,
    capability_assessment: dict[str, Any],
    source_summary: dict[str, Any],
) -> str:
    if county_fips is None:
        return "blocked"
    if source_summary["only_federal_baseline_available"]:
        return "federal_baseline_only"
    if county_entry is not None and county_entry["coverage_tier"] == "full":
        return "strong_local_footing"
    if any(
        capability_assessment[key]["status"] in {"available", "partial"}
        for key in ["parcel_identity", "tax_roll", "zoning", "planning_docs"]
    ):
        return "partial_local_footing"
    return "federal_baseline_only"


def _constraints(
    *,
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
    jurisdiction_context: dict[str, Any],
    county_entry: dict[str, Any] | None,
    source_summary: dict[str, Any],
    capability_assessment: dict[str, Any],
) -> list[dict[str, str]]:
    constraints = []
    if jurisdiction_context["county_fips"] is None and jurisdiction_context["state_fips"] is None:
        constraints.append(
            {
                "constraint_type": "needs_county_resolution",
                "message": "County or state context is still unresolved, so local parcel-specific planning should not proceed yet.",
            }
        )
    elif jurisdiction_context["county_fips"] is None:
        constraints.append(
            {
                "constraint_type": "state_only_research_footing",
                "message": "Only state-level geography is attached, so local parcel, zoning, and tax surfaces remain blocked.",
            }
        )
    if jurisdiction_context["effective_county_registry_mode"] == "generated_fallback":
        constraints.append(
            {
                "constraint_type": "county_on_fallback_registry",
                "message": "The effective county uses a generated fallback registry entry, so local coverage should be treated as minimal.",
            }
        )
    if source_summary["only_federal_baseline_available"]:
        constraints.append(
            {
                "constraint_type": "federal_baseline_only",
                "message": "Only federal baseline sources are currently attached for this context.",
            }
        )
    if subject_resolution["listing_context"]["has_listing_inputs"]:
        constraints.append(
            {
                "constraint_type": "listing_not_parcel_confirmed",
                "message": "Listing inputs still need parcel confirmation before parcel-level conclusions are safe.",
            }
        )
    if subject_resolution["subject_kind"] == "batch_subject_set" and jurisdiction_context["geography_status"] == "mixed_subjects":
        constraints.append(
            {
                "constraint_type": "batch_mixed_jurisdiction_certainty",
                "message": "Active batch subjects do not yet share one stable county attachment.",
            }
        )
    if capability_assessment["zoning"]["status"] == "unavailable" and county_entry is not None:
        constraints.append(
            {
                "constraint_type": "local_zoning_missing",
                "message": "Local zoning visibility is still weak or absent for this county.",
            }
        )
    if source_summary["relevant_source_count"] == 0:
        constraints.append(
            {
                "constraint_type": "no_relevant_sources",
                "message": "No relevant sources were selected from the current registry inputs.",
            }
        )
    return constraints


def _suitability_for_deep_research(
    *,
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
    jurisdiction_context: dict[str, Any],
    county_entry: dict[str, Any] | None,
    critical_surfaces: list[str],
    capability_assessment: dict[str, Any],
) -> str:
    if request["request_mode"] == "recommendation" and subject_resolution["subject_kind"] == "thesis_request":
        return "not_ready"
    if jurisdiction_context["county_fips"] is None and request["request_mode"] in {"deep_research", "follow_up", "batch_compare"}:
        return "not_ready"
    if subject_resolution["resolution_status"] == "unresolved" and jurisdiction_context["county_fips"] is None:
        return "not_ready"

    critical_statuses = [capability_assessment[capability]["status"] for capability in critical_surfaces]
    if any(status == "blocked_by_unresolved_jurisdiction" for status in critical_statuses):
        return "not_ready"
    if county_entry is not None and county_entry["coverage_tier"] == "full" and all(
        status in {"available", "partial", "minimal"} for status in critical_statuses
    ):
        return "deep_research_ready"
    if any(status == "unavailable" for status in critical_statuses):
        return "partially_ready"
    return "partially_ready"


def build_coverage_assessment(
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
    jurisdiction_context: dict[str, Any],
) -> dict[str, Any]:
    county_entry = (
        get_effective_county_entry(jurisdiction_context["county_fips"])
        if jurisdiction_context["county_fips"] is not None
        else None
    )
    relevant_sources = resolve_relevant_sources(request, subject_resolution, jurisdiction_context)
    capability_assessment = {
        capability: _build_capability_item(
            capability=capability,
            request=request,
            subject_resolution=subject_resolution,
            jurisdiction_context=jurisdiction_context,
            county_entry=county_entry,
            relevant_sources=relevant_sources,
        )
        for capability in CAPABILITY_ORDER
    }
    critical_surfaces = _critical_surfaces(request, subject_resolution)
    missing_surfaces = [
        capability
        for capability in critical_surfaces
        if capability_assessment[capability]["status"] in {"unavailable", "blocked_by_unresolved_jurisdiction"}
    ]
    source_summary = _source_summary(relevant_sources, county_entry)
    constraints = _constraints(
        request=request,
        subject_resolution=subject_resolution,
        jurisdiction_context=jurisdiction_context,
        county_entry=county_entry,
        source_summary=source_summary,
        capability_assessment=capability_assessment,
    )
    local_footing_status = _local_footing_status(
        county_fips=jurisdiction_context["county_fips"],
        county_entry=county_entry,
        capability_assessment=capability_assessment,
        source_summary=source_summary,
    )
    suitability = _suitability_for_deep_research(
        request=request,
        subject_resolution=subject_resolution,
        jurisdiction_context=jurisdiction_context,
        county_entry=county_entry,
        critical_surfaces=critical_surfaces,
        capability_assessment=capability_assessment,
    )

    notes = []
    if county_entry is not None:
        notes.extend(county_entry["known_limitations"])
    if local_footing_status == "federal_baseline_only":
        notes.append("Current footing is dominated by federal baseline coverage rather than local parcel or planning visibility.")
    if request["request_mode"] == "recommendation":
        notes.append("Coverage assessment is still useful in recommendation mode, but it should not be mistaken for parcel-specific diligence readiness.")

    return {
        "schema_version": COVERAGE_ASSESSMENT_VERSION,
        "assessment_id": str(
            uuid5(
                NAMESPACE_URL,
                f"alice-coverage-assessment:{request['request_id']}:{subject_resolution['resolution_id']}:{jurisdiction_context['jurisdiction_context_id']}",
            )
        ),
        "request_id": request["request_id"],
        "subject_resolution_id": subject_resolution["resolution_id"],
        "jurisdiction_context_id": jurisdiction_context["jurisdiction_context_id"],
        "request_mode": request["request_mode"],
        "effective_county_fips": jurisdiction_context["county_fips"],
        "effective_coverage_tier": county_entry["coverage_tier"] if county_entry is not None else None,
        "county_registry_mode": jurisdiction_context["effective_county_registry_mode"],
        "local_footing_status": local_footing_status,
        "capability_assessment": capability_assessment,
        "critical_surfaces": critical_surfaces,
        "missing_surfaces": missing_surfaces,
        "source_summary": source_summary,
        "constraints_on_next_steps": constraints,
        "suitability_for_deep_research": suitability,
        "notes": dedupe_strings(notes),
        "generated_at": now_iso(),
    }


def summarize_coverage_assessment(assessment: dict[str, Any]) -> dict[str, Any]:
    return {
        "effective_county_fips": assessment["effective_county_fips"],
        "effective_coverage_tier": assessment["effective_coverage_tier"],
        "local_footing_status": assessment["local_footing_status"],
        "suitability_for_deep_research": assessment["suitability_for_deep_research"],
        "missing_surfaces": assessment["missing_surfaces"],
    }
