#!/usr/bin/env python3

"""Source planning builders for Alice AI Step 5."""

from __future__ import annotations

from pathlib import Path
from typing import Any
from uuid import NAMESPACE_URL, uuid5

from alice_coverage_assessment import (
    CAPABILITY_ORDER,
    CAPABILITY_TO_SOURCE_CAPABILITIES,
    COVERAGE_ASSESSMENT_SCHEMA_PATH,
    build_coverage_assessment,
    load_jurisdiction_context,
    resolve_relevant_sources,
)
from alice_jurisdiction import build_jurisdiction_context, load_subject_resolution
from alice_registry import get_effective_county_entry, load_all_sources, load_json
from alice_subject_resolution import dedupe_strings, load_request, now_iso


SOURCE_PLAN_VERSION = "alice.source_plan.v1"
SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
SCHEMAS_ROOT = SKILL_ROOT / "schemas"
SOURCE_PLAN_SCHEMA_PATH = SCHEMAS_ROOT / "source_plan.schema.json"

LOCAL_CAPABILITIES = {"parcel_identity", "parcel_geometry", "tax_roll", "zoning", "planning_docs"}

FALLBACK_MESSAGES = {
    "parcel_identity": [
        "If county parcel search is missing, fall back to statewide parcel programs where available or request explicit county/APN clarification."
    ],
    "parcel_geometry": [
        "If county geometry is unavailable, fall back to statewide parcel datasets or keep geometry unresolved."
    ],
    "tax_roll": [
        "If tax roll access is missing, keep assessed value and ownership context unresolved until a county source is added."
    ],
    "zoning": [
        "If direct zoning visibility is missing, mark zoning unresolved and continue with procedural planning sources only."
    ],
    "planning_docs": [
        "If local planning documents are thin, rely on county procedural pages and keep entitlement interpretation conservative."
    ],
    "environmental_baseline": [
        "If one national overlay is degraded, continue with the remaining federal baseline sources and record the missing layer explicitly."
    ],
    "water_signals": [
        "If local water-rights sources are absent, use soils and other baseline signals only as directional context."
    ],
    "utilities": [
        "If local utility service areas are unclear, fall back to statewide utility maps and keep service conclusions directional."
    ],
    "transmission": [
        "If state or local transmission context is thin, retain transmission proximity as a directional screen only."
    ],
    "market_context": [
        "If one listing platform is missing, use the remaining listing or market context sources and mark any platform-specific gaps."
    ],
}


def load_coverage_assessment(path: str | Path) -> dict[str, Any]:
    return load_json(Path(path))


def _relevant_sources_by_id(relevant_sources: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    return {source["source_id"]: source for source in relevant_sources}


def _category_rank(capability: str, source: dict[str, Any]) -> int:
    category = source["category"]
    if capability == "market_context":
        return {
            "listing_platform": 0,
            "county": 1,
            "city_local": 1,
            "state": 2,
            "federal": 3,
        }.get(category, 4)
    if capability in {"environmental_baseline", "water_signals"}:
        return {
            "federal": 0,
            "state": 1,
            "county": 2,
            "city_local": 2,
            "listing_platform": 4,
        }.get(category, 3)
    if capability in {"utilities", "transmission"}:
        return {
            "county": 0,
            "city_local": 0,
            "state": 1,
            "federal": 2,
            "listing_platform": 4,
        }.get(category, 3)
    return {
        "county": 0,
        "city_local": 0,
        "state": 1,
        "federal": 2,
        "listing_platform": 4,
    }.get(category, 3)


def _sorted_source_ids_for_capability(
    capability: str,
    relevant_sources: list[dict[str, Any]],
    county_entry: dict[str, Any] | None,
) -> list[str]:
    source_ids = []
    supporting_caps = CAPABILITY_TO_SOURCE_CAPABILITIES[capability]
    preferred_order = county_entry["preferred_source_ids"] if county_entry is not None else []

    for source in relevant_sources:
        if supporting_caps & set(source["capabilities"]):
            source_ids.append(source["source_id"])

    by_id = _relevant_sources_by_id(relevant_sources)

    def sort_key(source_id: str) -> tuple[int, int, int, int, str]:
        source = by_id[source_id]
        preferred_rank = preferred_order.index(source_id) if source_id in preferred_order else 999
        return (
            preferred_rank,
            _category_rank(capability, source),
            source["authority_rank"],
            -source["priority"],
            source_id,
        )

    return sorted(set(source_ids), key=sort_key)


def _authority_level(source_ids: list[str], relevant_sources: list[dict[str, Any]]) -> str:
    by_id = _relevant_sources_by_id(relevant_sources)
    categories = {by_id[source_id]["category"] for source_id in source_ids}
    if categories == {"listing_platform"}:
        return "platform"
    if categories == {"federal"}:
        return "official_federal"
    if categories == {"state"}:
        return "official_state"
    if categories <= {"county", "city_local"}:
        return "official_local_or_county"
    return "mixed"


def _execution_preconditions(
    capability: str,
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
    jurisdiction_context: dict[str, Any],
) -> list[str]:
    preconditions = []
    if capability in LOCAL_CAPABILITIES:
        preconditions.append("county_fips must be stable before this local source group runs")
    if capability == "parcel_identity":
        preconditions.append("subject must retain at least one parcel clue such as APN, address, listing URL, or inherited parcel reference")
    if capability == "market_context":
        if subject_resolution["listing_context"]["has_listing_inputs"]:
            preconditions.append("listing URL inputs remain in the active subject set")
        else:
            preconditions.append("candidate acquisition should be active before market-planning retrieval begins")
    if capability in {"environmental_baseline", "water_signals"}:
        preconditions.append("a defensible geography anchor such as county, state, or coordinates must be present")
    if capability in {"utilities", "transmission"}:
        preconditions.append("at least state-level geography should be stable before infrastructure retrieval begins")
    if request["request_mode"] == "recommendation" and capability in LOCAL_CAPABILITIES:
        preconditions.append("recommendation flow usually defers this local parcel surface until shortlisted candidates exist")
    return preconditions


def _source_selection_rationale(
    capability: str,
    source_ids: list[str],
    relevant_sources: list[dict[str, Any]],
    county_entry: dict[str, Any] | None,
) -> str:
    by_id = _relevant_sources_by_id(relevant_sources)
    top_source = by_id[source_ids[0]]
    if capability == "market_context":
        return (
            f"Start with {top_source['name']} because listing-platform context is the fastest supported way to recover seller-facing listing or market clues for this capability."
        )
    if capability == "environmental_baseline":
        return (
            f"Start with {top_source['name']} because national baseline overlays should be retrieved before narrower parcel-specific interpretation."
        )
    if county_entry is not None and source_ids[0] in county_entry["preferred_source_ids"]:
        return (
            f"Start with {top_source['name']} because the effective county registry prefers this source for {capability} in the current jurisdiction."
        )
    return (
        f"Start with {top_source['name']} because it is the strongest currently registered source group for {capability} given the present jurisdiction footing."
    )


def _city_scoped_gap(capability: str, jurisdiction_context: dict[str, Any]) -> bool:
    county_fips = jurisdiction_context["county_fips"]
    city_name = jurisdiction_context["city_or_place"]["name"]
    if county_fips is None or city_name is not None:
        return False
    supporting_caps = CAPABILITY_TO_SOURCE_CAPABILITIES[capability]
    for source in load_all_sources():
        geography = source["geography_scope"]
        if geography["county_fips"] != county_fips or geography["city_name"] is None:
            continue
        if supporting_caps & set(source["capabilities"]):
            return True
    return False


def _determine_required_capabilities(
    request: dict[str, Any],
    coverage_assessment: dict[str, Any],
) -> list[str]:
    return [capability for capability in CAPABILITY_ORDER if capability in coverage_assessment["critical_surfaces"]]


def _deferred_step(
    capability: str,
    subject_ids: list[str],
    blocker_type: str,
    reason: str,
    dependency: str | None,
) -> dict[str, Any]:
    return {
        "capability": capability,
        "subject_ids": subject_ids,
        "blocker_type": blocker_type,
        "reason": reason,
        "dependency": dependency,
    }


def build_source_plan(
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
    jurisdiction_context: dict[str, Any],
    coverage_assessment: dict[str, Any],
) -> dict[str, Any]:
    county_entry = (
        get_effective_county_entry(jurisdiction_context["county_fips"])
        if jurisdiction_context["county_fips"] is not None
        else None
    )
    relevant_sources = resolve_relevant_sources(request, subject_resolution, jurisdiction_context)
    required_capabilities = _determine_required_capabilities(request, coverage_assessment)
    active_subject_ids = [subject["subject_id"] for subject in subject_resolution["active_subjects"]]

    planned_source_groups = []
    deferred_steps = []
    blocked_steps = []
    missing_capabilities = []

    for priority_order, capability in enumerate(required_capabilities, start=1):
        assessment = coverage_assessment["capability_assessment"][capability]
        source_ids = _sorted_source_ids_for_capability(capability, relevant_sources, county_entry)
        execution_preconditions = _execution_preconditions(
            capability,
            request,
            subject_resolution,
            jurisdiction_context,
        )

        if assessment["status"] == "blocked_by_unresolved_jurisdiction":
            blocker_type = "county_required" if capability in LOCAL_CAPABILITIES else "jurisdiction_unresolved"
            blocked_steps.append(
                _deferred_step(
                    capability,
                    active_subject_ids,
                    blocker_type,
                    f"{capability} is blocked because jurisdiction is not stable enough yet.",
                    "county_fips" if capability in LOCAL_CAPABILITIES else "stable_geography_anchor",
                )
            )
            missing_capabilities.append(capability)
            continue

        if not source_ids:
            blocked_steps.append(
                _deferred_step(
                    capability,
                    active_subject_ids,
                    "local_source_gap",
                    f"No registered source currently supports {capability} strongly enough for this context.",
                    None,
                )
            )
            missing_capabilities.append(capability)
            continue

        group_status = "ready"
        if request["request_mode"] == "recommendation" and capability in LOCAL_CAPABILITIES:
            group_status = "deferred"
            deferred_steps.append(
                _deferred_step(
                    capability,
                    active_subject_ids,
                    "recommendation_flow_deferred",
                    f"{capability} is usually deferred until recommendation candidates narrow to parcel-level subjects.",
                    "candidate_shortlist",
                )
            )
        elif jurisdiction_context["geography_status"] == "mixed_subjects" and capability in LOCAL_CAPABILITIES:
            group_status = "deferred"
            deferred_steps.append(
                _deferred_step(
                    capability,
                    active_subject_ids,
                    "mixed_batch_jurisdiction",
                    f"{capability} is deferred because the active batch does not yet share one stable county.",
                    "shared_county_context",
                )
            )
        elif _city_scoped_gap(capability, jurisdiction_context):
            group_status = "deferred"
            deferred_steps.append(
                _deferred_step(
                    capability,
                    active_subject_ids,
                    "city_specific_context_missing",
                    f"{capability} has city-scoped sources that should wait until city applicability is confirmed.",
                    "city_or_place_resolution",
                )
            )

        planned_source_groups.append(
            {
                "capability": capability,
                "subject_ids": active_subject_ids,
                "source_ids": source_ids,
                "source_selection_rationale": _source_selection_rationale(
                    capability,
                    source_ids,
                    relevant_sources,
                    county_entry,
                ),
                "priority_order": priority_order,
                "authority_level": _authority_level(source_ids, relevant_sources),
                "execution_preconditions": execution_preconditions,
                "fallback_if_missing": FALLBACK_MESSAGES[capability],
                "group_status": group_status,
                "notes": assessment["notes"],
            }
        )

    plan_status = "ready"
    if blocked_steps and not planned_source_groups:
        plan_status = "blocked"
    elif blocked_steps or deferred_steps or any(group["group_status"] != "ready" for group in planned_source_groups):
        plan_status = "partially_blocked"

    notes = [
        "This artifact is a retrieval plan only. No live source fetches have been executed in Step 5.",
    ]
    if coverage_assessment["source_summary"]["only_federal_baseline_available"]:
        notes.append("Current source plan is dominated by federal baseline sources because local coverage is still thin.")
    if request["request_mode"] == "recommendation":
        notes.append("Recommendation-mode planning emphasizes market and screening inputs before parcel-specific local diligence.")

    return {
        "schema_version": SOURCE_PLAN_VERSION,
        "source_plan_id": str(
            uuid5(
                NAMESPACE_URL,
                f"alice-source-plan:{request['request_id']}:{subject_resolution['resolution_id']}:{jurisdiction_context['jurisdiction_context_id']}:{coverage_assessment['assessment_id']}",
            )
        ),
        "request_id": request["request_id"],
        "subject_resolution_id": subject_resolution["resolution_id"],
        "jurisdiction_context_id": jurisdiction_context["jurisdiction_context_id"],
        "coverage_assessment_id": coverage_assessment["assessment_id"],
        "plan_status": plan_status,
        "request_mode": request["request_mode"],
        "active_use_cases": request["thesis"]["use_case_hypotheses"],
        "required_capabilities": required_capabilities,
        "planned_source_groups": planned_source_groups,
        "deferred_steps": deferred_steps,
        "blocked_steps": blocked_steps,
        "missing_capabilities": dedupe_strings(missing_capabilities),
        "notes": dedupe_strings(notes),
        "generated_at": now_iso(),
    }


def summarize_source_plan(plan: dict[str, Any]) -> dict[str, Any]:
    return {
        "plan_status": plan["plan_status"],
        "required_capabilities": plan["required_capabilities"],
        "planned_group_count": len(plan["planned_source_groups"]),
        "deferred_step_count": len(plan["deferred_steps"]),
        "blocked_step_count": len(plan["blocked_steps"]),
        "missing_capabilities": plan["missing_capabilities"],
    }
