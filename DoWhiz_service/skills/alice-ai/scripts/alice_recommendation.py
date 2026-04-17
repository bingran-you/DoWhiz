#!/usr/bin/env python3

"""Step 10 recommendation, shortlist, and ranking engine for Alice AI."""

from __future__ import annotations

import json
import math
import re
from copy import deepcopy
from pathlib import Path
from typing import Any
from uuid import NAMESPACE_URL, uuid5

from alice_registry import load_json
from alice_subject_resolution import build_subject_resolution, load_request, now_iso
from alice_use_case_modules import FIT_ORDER, evaluate_use_case_modules, select_module_ids


RECOMMENDATION_CANDIDATES_VERSION = "alice.recommendation_candidates.v1"
SHORTLIST_VERSION = "alice.shortlist.v1"
RANKING_POLICY_VERSION = "alice.ranking.v0"

SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
SCHEMAS_ROOT = SKILL_ROOT / "schemas"
WORKSPACE_EXAMPLES_ROOT = SKILL_ROOT / "examples" / "workspaces"
RECOMMENDATION_CANDIDATES_SCHEMA_PATH = SCHEMAS_ROOT / "recommendation_candidates.schema.json"
SHORTLIST_SCHEMA_PATH = SCHEMAS_ROOT / "shortlist.schema.json"
REPO_ROOT = SKILL_ROOT.parents[2]

DEFAULT_SHORTLIST_LIMIT = 5

REQUEST_MODE_ORDER = {
    "user_supplied_ranking": 0,
    "open_discovery": 1,
    "prior_set_refinement": 2,
}

OBSERVATION_STATUS_ORDER = {
    "observed_with_research_artifacts": 0,
    "observed_listing_only": 1,
    "unsupported": 2,
}

CONFIRMATION_ORDER = {
    "parcel_confirmed": 0,
    "candidate_corroborated": 1,
    "candidate_unconfirmed": 2,
    "listing_hint_only": 3,
    "geography_only": 4,
    "unresolved": 5,
}

COVERAGE_ORDER = {
    "full": 0,
    "partial": 1,
    "minimal": 2,
    None: 3,
}

FOOTING_ORDER = {
    "strong_local_footing": 0,
    "partial_local_footing": 1,
    "deep_research_ready": 1,
    "partially_ready": 2,
    "minimal_local_footing": 3,
    "federal_baseline_only": 4,
    "not_ready": 4,
    None: 5,
}

PLATFORM_ORDER = {
    "landwatch": 0,
    "land_com": 1,
    "redfin": 2,
    "zillow": 3,
    "broker_site": 4,
    "county_listing": 5,
    "unknown": 6,
    None: 7,
}

REGION_COUNTY_ALIASES = {
    "west texas": {
        "48059",  # Callahan
        "48075",  # Childress
        "48103",  # Crane
        "48105",  # Crockett
        "48109",  # Culberson
        "48141",  # El Paso
        "48173",  # Glasscock
        "48227",  # Howard
        "48229",  # Hudspeth
        "48301",  # Loving
        "48329",  # Midland
        "48335",  # Mitchell
        "48371",  # Pecos
        "48377",  # Presidio
        "48383",  # Reagan
        "48389",  # Reeves
        "48443",  # Terrell
        "48461",  # Upton
        "48475",  # Ward
        "48495",  # Winkler
    },
}

EXHAUSTIVE_LANGUAGE = (
    "best available land in the market",
    "entire market",
    "whole market",
    "all available land",
    "all available listings",
    "complete market coverage",
)


def deterministic_uuid(*parts: str) -> str:
    return str(uuid5(NAMESPACE_URL, "|".join(parts)))


def bounded(value: float) -> float:
    return max(0.0, min(1.0, round(value, 3)))


def sorted_unique_strings(values: list[str]) -> list[str]:
    return sorted({value for value in values if value})


def _workspace_alice_root(workspace_root: str | Path) -> Path:
    alice_root = Path(workspace_root) / "alice"
    alice_root.mkdir(parents=True, exist_ok=True)
    return alice_root


def _recommendation_root(workspace_root: str | Path) -> Path:
    recommendation_root = _workspace_alice_root(workspace_root) / "recommendation"
    recommendation_root.mkdir(parents=True, exist_ok=True)
    return recommendation_root


def _load_optional_json(path: Path) -> dict[str, Any] | None:
    return load_json(path) if path.exists() else None


def _repo_relative(path: str | Path | None) -> str | None:
    if path is None:
        return None
    resolved = Path(path).resolve()
    try:
        return str(resolved.relative_to(REPO_ROOT))
    except ValueError:
        return str(resolved)


def _field_value(field: dict[str, Any] | None) -> Any:
    if not isinstance(field, dict):
        return None
    return field.get("value")


def _field_status(field: dict[str, Any] | None) -> str:
    if not isinstance(field, dict):
        return "missing"
    return field.get("status") or "missing"


def _field_text(field: dict[str, Any] | None) -> str:
    value = _field_value(field)
    if value is None:
        return ""
    if isinstance(value, str):
        return value.strip()
    if isinstance(value, dict):
        parts = []
        for nested_value in value.values():
            if isinstance(nested_value, str) and nested_value.strip():
                parts.append(nested_value.strip())
        return " ".join(parts)
    if isinstance(value, list):
        parts = []
        for nested_value in value:
            if isinstance(nested_value, str) and nested_value.strip():
                parts.append(nested_value.strip())
        return " ".join(parts)
    return str(value)


def _format_currency(value: float | int | None) -> str | None:
    if not isinstance(value, (int, float)):
        return None
    return f"${float(value):,.2f}"


def _format_number(value: float | int | None) -> str | None:
    if not isinstance(value, (int, float)):
        return None
    float_value = float(value)
    if float_value.is_integer():
        return f"{int(float_value):,}"
    return f"{float_value:,.2f}"


def _format_confidence(value: float) -> str:
    return f"{round(bounded(value) * 100):.0f}%"


def _safe_title(value: str | None, fallback: str) -> str:
    clean = (value or "").strip()
    return clean or fallback


def _annotated_title(title: str, county_name: str | None, state_code: str | None) -> str:
    lowered = title.lower()
    county_lower = county_name.lower() if isinstance(county_name, str) else None
    state_lower = state_code.lower() if isinstance(state_code, str) else None
    if county_lower and county_lower in lowered:
        return title
    suffix = ", ".join(part for part in [county_name, state_code] if part)
    if not suffix:
        return title
    if state_lower and state_lower in lowered and not county_name:
        return title
    return f"{title} in {suffix}"


def _safe_slug(value: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")
    return slug or "candidate"


def _match_county_fips(candidate: dict[str, Any], county_fips: str) -> bool:
    return candidate.get("county_fips") == county_fips


def _match_state_code(candidate: dict[str, Any], state_code: str) -> bool:
    return candidate.get("state_code") == state_code


def _distance_miles(
    latitude_one: float,
    longitude_one: float,
    latitude_two: float,
    longitude_two: float,
) -> float:
    radius_miles = 3958.756
    lat_one = math.radians(latitude_one)
    lon_one = math.radians(longitude_one)
    lat_two = math.radians(latitude_two)
    lon_two = math.radians(longitude_two)
    d_lat = lat_two - lat_one
    d_lon = lon_two - lon_one
    haversine = (
        math.sin(d_lat / 2) ** 2
        + math.cos(lat_one) * math.cos(lat_two) * math.sin(d_lon / 2) ** 2
    )
    return radius_miles * 2 * math.asin(min(1.0, math.sqrt(haversine)))


def _primary_parcel(parcel_memo: dict[str, Any]) -> dict[str, Any] | None:
    parcel_identity = parcel_memo.get("parcel_identity", {})
    primary_candidate_id = parcel_identity.get("primary_candidate_id")
    for parcel in parcel_identity.get("parcels", []):
        if parcel.get("candidate_id") == primary_candidate_id:
            return parcel
    parcels = parcel_identity.get("parcels", [])
    return parcels[0] if parcels else None


def _listing_url_from_memo(parcel_memo: dict[str, Any]) -> str | None:
    canonical_url = parcel_memo.get("listing", {}).get("canonical_url")
    if isinstance(canonical_url, str) and canonical_url:
        return canonical_url
    for source_url in parcel_memo.get("source_urls", []):
        source_id = source_url.get("source_id")
        if isinstance(source_id, str) and (
            source_id.startswith("listing_platform_")
            or source_id == "subject_input_listing_url"
        ):
            url = source_url.get("url")
            if isinstance(url, str) and url:
                return url
    return None


def _step_priority(workspace_name: str) -> int:
    match = re.match(r"step(\d+)_", workspace_name)
    if match:
        return int(match.group(1))
    return 0


def _bundle_priority(bundle: dict[str, Any]) -> tuple[int, int, float]:
    return (
        bundle["step_priority"],
        -CONFIRMATION_ORDER.get(bundle["parcel_identity_level"], 99),
        bundle["overall_confidence"],
    )


def load_workspace_candidate_bundle(workspace_dir: str | Path) -> dict[str, Any] | None:
    workspace_dir = Path(workspace_dir)
    alice_root = workspace_dir / "alice"
    parcel_memo_path = alice_root / "parcel_memo.json"
    if not parcel_memo_path.exists():
        return None

    parcel_memo = load_json(parcel_memo_path)
    canonical_url = _listing_url_from_memo(parcel_memo)
    listing_present = bool(parcel_memo.get("listing", {}).get("listing_present"))
    if not listing_present or not canonical_url:
        return None

    coverage_assessment = _load_optional_json(alice_root / "coverage_assessment.json")
    parcel_candidates = _load_optional_json(alice_root / "parcel_candidates.json")
    report_summary = _load_optional_json(alice_root / "report_summary.json")
    primary_parcel = _primary_parcel(parcel_memo)
    subject = parcel_memo["subject"]
    listing = parcel_memo["listing"]
    jurisdiction = parcel_memo["jurisdiction"]
    scores = parcel_memo["scores"]
    coordinates = _field_value(subject.get("coordinates"))
    title = _safe_title(
        subject.get("primary_subject_label"),
        _field_text(listing.get("listing_text_summary")) or canonical_url,
    )
    title = _annotated_title(title, jurisdiction.get("county", {}).get("name"), bundle_state_code if (bundle_state_code := (
        (_field_value(subject.get("canonical_address")) or {}).get("state_code")
        if isinstance(_field_value(subject.get("canonical_address")), dict)
        else None
    )) else None)
    bundle = {
        "workspace_dir": workspace_dir,
        "workspace_name": workspace_dir.name,
        "step_priority": _step_priority(workspace_dir.name),
        "canonical_url": canonical_url,
        "platform": _field_value(listing.get("platform")),
        "listing_id": listing.get("listing_id"),
        "title": title,
        "asking_price": _field_value(listing.get("asking_price")),
        "acreage": _field_value(listing.get("listed_acreage")),
        "listing_text_summary": _field_text(listing.get("listing_text_summary")) or None,
        "county_name": jurisdiction.get("county", {}).get("name"),
        "county_fips": jurisdiction.get("county", {}).get("fips"),
        "state_name": jurisdiction.get("state", {}).get("name"),
        "state_fips": jurisdiction.get("state", {}).get("fips"),
        "state_code": bundle_state_code,
        "city_or_place": _field_text(jurisdiction.get("city_or_unincorporated")) or None,
        "coordinates": coordinates if isinstance(coordinates, dict) else None,
        "parcel_identity_level": parcel_memo["parcel_identity"]["overall_confirmation_level"],
        "candidate_set_status": parcel_memo["parcel_identity"]["candidate_set_status"],
        "overall_confidence": float(scores.get("overall_confidence") or 0.0),
        "overall_completeness": float(scores.get("overall_completeness") or 0.0),
        "coverage_tier": (coverage_assessment or {}).get("effective_coverage_tier") or scores.get("coverage_tier"),
        "local_footing_status": (coverage_assessment or {}).get("local_footing_status"),
        "parcel_memo": parcel_memo,
        "coverage_assessment": coverage_assessment,
        "parcel_candidates": parcel_candidates,
        "report_summary": report_summary,
        "observed_at": parcel_memo.get("created_at"),
        "primary_apn": _field_value(primary_parcel.get("apn")) if isinstance(primary_parcel, dict) else None,
    }
    if not bundle["state_code"] and bundle["state_fips"] == "48":
        bundle["state_code"] = "TX"
    elif not bundle["state_code"] and bundle["state_fips"] == "06":
        bundle["state_code"] = "CA"
    elif not bundle["state_code"] and bundle["state_fips"] == "01":
        bundle["state_code"] = "AL"
    return bundle


def build_observed_candidate_catalog(
    *,
    workspaces_root: str | Path = WORKSPACE_EXAMPLES_ROOT,
) -> dict[str, dict[str, Any]]:
    workspaces_root = Path(workspaces_root)
    catalog: dict[str, dict[str, Any]] = {}
    for workspace_dir in sorted(workspaces_root.iterdir()):
        if not workspace_dir.is_dir():
            continue
        bundle = load_workspace_candidate_bundle(workspace_dir)
        if bundle is None:
            continue
        key = bundle["canonical_url"]
        current = catalog.get(key)
        if current is None or _bundle_priority(bundle) > _bundle_priority(current):
            catalog[key] = bundle
    return catalog


def _candidate_execution_mode(request: dict[str, Any]) -> str:
    follow_up_context = request.get("follow_up_context") or {}
    if follow_up_context:
        if follow_up_context.get("update_kind") in {"filter_subject_set", "refine_thesis", "compare_with_previous"}:
            return "prior_set_refinement"
    if request.get("subjects"):
        return "user_supplied_ranking"
    return "open_discovery"


def _thesis_summary(request: dict[str, Any]) -> str:
    summary = request.get("thesis", {}).get("summary")
    if isinstance(summary, str) and summary.strip():
        return summary.strip()
    return request["raw_user_message"].strip()


def _search_scope(request: dict[str, Any]) -> dict[str, Any]:
    thesis = request.get("thesis", {})
    return {
        "target_geographies": deepcopy(thesis.get("target_geographies", [])),
        "filters": deepcopy(thesis.get("filters", {})),
        "budget_max_purchase_price": thesis.get("budget", {}).get("max_purchase_price"),
        "active_use_case_hypotheses": list(thesis.get("use_case_hypotheses", [])),
        "notes": sorted_unique_strings(
            [
                *(thesis.get("return_preferences", {}).get("notes", []) or []),
                *(thesis.get("filters", {}).get("notes", []) or []),
                *(request.get("follow_up_context", {}).get("notes", []) or []),
            ]
        ),
    }


def _matches_region(candidate: dict[str, Any], label: str) -> bool:
    alias_counties = REGION_COUNTY_ALIASES.get(label.lower())
    if alias_counties is not None:
        return candidate.get("county_fips") in alias_counties
    haystack = " ".join(
        part.lower()
        for part in [
            candidate.get("county_name"),
            candidate.get("state_name"),
            candidate.get("title"),
            candidate.get("listing_text_summary"),
        ]
        if isinstance(part, str) and part
    )
    return label.lower() in haystack


def _matches_target_geography(candidate: dict[str, Any], geography: dict[str, Any]) -> bool:
    kind = geography.get("kind")
    if kind == "nationwide":
        return True
    if kind == "state":
        state_code = geography.get("state_code")
        return bool(state_code) and _match_state_code(candidate, state_code)
    if kind == "county":
        county_fips = geography.get("county_fips")
        if county_fips:
            return _match_county_fips(candidate, county_fips)
        state_code = geography.get("state_code")
        county_name = geography.get("county_name")
        return (
            bool(state_code)
            and bool(county_name)
            and _match_state_code(candidate, state_code)
            and candidate.get("county_name") == county_name
        )
    if kind == "region":
        label = geography.get("label")
        return bool(label) and _matches_region(candidate, label)
    if kind == "city":
        label = (geography.get("city_name") or geography.get("label") or "").lower()
        city_name = (candidate.get("city_or_place") or "").lower()
        return bool(label) and label in city_name
    if kind == "coordinates_radius":
        center = geography.get("center_coordinates")
        radius = geography.get("radius_miles")
        coordinates = candidate.get("coordinates")
        if not isinstance(center, dict) or not isinstance(coordinates, dict):
            return False
        if not isinstance(radius, (int, float)):
            return False
        return (
            _distance_miles(
                float(center["latitude"]),
                float(center["longitude"]),
                float(coordinates["latitude"]),
                float(coordinates["longitude"]),
            )
            <= float(radius)
        )
    return False


def _passes_geography_scope(candidate: dict[str, Any], request: dict[str, Any]) -> bool:
    target_geographies = request.get("thesis", {}).get("target_geographies", [])
    if not target_geographies:
        return True
    return any(_matches_target_geography(candidate, geography) for geography in target_geographies)


def _primary_budget_limit(request: dict[str, Any]) -> float | None:
    thesis = request.get("thesis", {})
    filters = thesis.get("filters", {})
    budget = thesis.get("budget", {})
    candidates: list[float] = []
    for value in [filters.get("max_price"), budget.get("max_purchase_price")]:
        if isinstance(value, (int, float)):
            candidates.append(float(value))
    return min(candidates) if candidates else None


def _min_budget_limit(request: dict[str, Any]) -> float | None:
    thesis = request.get("thesis", {})
    filters = thesis.get("filters", {})
    budget = thesis.get("budget", {})
    candidates: list[float] = []
    for value in [filters.get("min_price"), budget.get("min_purchase_price")]:
        if isinstance(value, (int, float)):
            candidates.append(float(value))
    return max(candidates) if candidates else None


def _price_per_acre(asking_price: float | None, acreage: float | None) -> float | None:
    if not isinstance(asking_price, (int, float)) or not isinstance(acreage, (int, float)):
        return None
    if acreage <= 0:
        return None
    return float(asking_price) / float(acreage)


def _budget_mismatch_reason(candidate: dict[str, Any], request: dict[str, Any]) -> str | None:
    ask_price = candidate.get("asking_price")
    if not isinstance(ask_price, (int, float)):
        return None
    min_budget = _min_budget_limit(request)
    if min_budget is not None and float(ask_price) < min_budget:
        return f"Listing ask of {_format_currency(float(ask_price))} falls below the user-stated minimum acquisition basis."
    max_budget = _primary_budget_limit(request)
    if max_budget is not None and float(ask_price) > max_budget:
        return f"Listing ask of {_format_currency(float(ask_price))} is above the user-stated budget ceiling."
    return None


def _acreage_mismatch_reason(candidate: dict[str, Any], request: dict[str, Any]) -> str | None:
    acreage = candidate.get("acreage")
    if not isinstance(acreage, (int, float)):
        return None
    filters = request.get("thesis", {}).get("filters", {})
    minimum = filters.get("acreage_min")
    if isinstance(minimum, (int, float)) and float(acreage) < float(minimum):
        return f"Observed acreage of {_format_number(float(acreage))} acres is below the requested minimum."
    maximum = filters.get("acreage_max")
    if isinstance(maximum, (int, float)) and float(acreage) > float(maximum):
        return f"Observed acreage of {_format_number(float(acreage))} acres is above the requested maximum."
    return None


def _price_per_acre_mismatch_reason(candidate: dict[str, Any], request: dict[str, Any]) -> str | None:
    filters = request.get("thesis", {}).get("filters", {})
    per_acre = _price_per_acre(candidate.get("asking_price"), candidate.get("acreage"))
    if per_acre is None:
        return None
    minimum = filters.get("min_price_per_acre")
    if isinstance(minimum, (int, float)) and per_acre < float(minimum):
        return f"Observed ask per acre of {_format_currency(per_acre)} is below the requested floor."
    maximum = filters.get("max_price_per_acre")
    if isinstance(maximum, (int, float)) and per_acre > float(maximum):
        return f"Observed ask per acre of {_format_currency(per_acre)} is above the requested ceiling."
    return None


def _initial_eligibility_status(candidate: dict[str, Any], request: dict[str, Any]) -> str:
    if not _passes_geography_scope(candidate, request):
        return "hard_filtered"
    if _budget_mismatch_reason(candidate, request):
        return "hard_filtered"
    if _acreage_mismatch_reason(candidate, request):
        return "hard_filtered"
    if _price_per_acre_mismatch_reason(candidate, request):
        return "hard_filtered"
    if candidate.get("observation_status") == "unsupported":
        return "needs_review"
    return "eligible"


def _workspace_artifact_refs(bundle: dict[str, Any] | None) -> dict[str, Any] | None:
    if bundle is None:
        return None
    alice_root = bundle["workspace_dir"] / "alice"
    return {
        "workspace_root": _repo_relative(bundle["workspace_dir"]),
        "parcel_memo_ref": _repo_relative(alice_root / "parcel_memo.json"),
        "coverage_assessment_ref": _repo_relative(alice_root / "coverage_assessment.json")
        if (alice_root / "coverage_assessment.json").exists()
        else None,
        "parcel_candidates_ref": _repo_relative(alice_root / "parcel_candidates.json")
        if (alice_root / "parcel_candidates.json").exists()
        else None,
        "report_ref": _repo_relative(alice_root / "report.md")
        if (alice_root / "report.md").exists()
        else None,
    }


def _user_supplied_candidate_seeds(
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
    observed_catalog: dict[str, dict[str, Any]],
) -> tuple[list[dict[str, Any]], int, int]:
    normalized_by_id = {
        item["subject_id"]: item
        for item in subject_resolution.get("normalized_inputs", [])
    }
    seeds: list[dict[str, Any]] = []
    seen_dedupe_keys: dict[str, dict[str, Any]] = {}
    duplicate_input_count = 0
    unmatched_input_count = 0

    for subject in subject_resolution.get("active_subjects", []):
        subject_id = subject["subject_id"]
        normalized_input = normalized_by_id.get(subject_id, {})
        identifiers = subject.get("identifiers", {})
        canonical_url = identifiers.get("canonical_url")
        platform = identifiers.get("listing_platform")
        raw_value = normalized_input.get("raw_value") or subject.get("display_label") or subject_id
        bundle = observed_catalog.get(canonical_url) if canonical_url else None
        dedupe_key = canonical_url or f"subject-ref:{subject_id}"
        if dedupe_key in seen_dedupe_keys:
            duplicate_input_count += 1
            retained = seen_dedupe_keys[dedupe_key]
            retained["notes"].append(
                "Duplicate user-supplied candidate input was collapsed by canonical listing URL."
            )
            continue

        observation_status = "observed_with_research_artifacts" if bundle is not None else "unsupported"
        if bundle is None:
            unmatched_input_count += 1

        title = (
            bundle["title"]
            if bundle is not None
            else subject.get("display_label") or canonical_url or raw_value
        )
        seed = {
            "candidate_subject_id": f"candidate-{deterministic_uuid(request['request_id'], dedupe_key)[:8]}",
            "raw_value": raw_value,
            "canonical_url": canonical_url,
            "platform": platform,
            "listing_id": identifiers.get("listing_id"),
            "county_fips": bundle["county_fips"] if bundle is not None else identifiers.get("county_fips"),
            "county_name": bundle["county_name"] if bundle is not None else identifiers.get("county_name"),
            "state_fips": bundle["state_fips"] if bundle is not None else identifiers.get("state_fips"),
            "state_code": bundle["state_code"] if bundle is not None else identifiers.get("state_code"),
            "state_name": bundle["state_name"] if bundle is not None else identifiers.get("state_name"),
            "asking_price": bundle["asking_price"] if bundle is not None else None,
            "acreage": bundle["acreage"] if bundle is not None else None,
            "listing_text_summary": bundle["listing_text_summary"] if bundle is not None else None,
            "title": title,
            "bundle": bundle,
            "acquisition_path": "user_supplied_listing_url",
            "observation_status": observation_status,
            "observed_at": bundle["observed_at"] if bundle is not None else None,
            "initial_match_rationale": (
                "User supplied this listing directly, and Alice matched it to an observed research workspace."
                if bundle is not None
                else "User supplied this listing directly, but Alice does not yet have reusable evaluation artifacts for it."
            ),
            "notes": [],
        }
        seed["initial_eligibility_status"] = _initial_eligibility_status(seed, request)
        seen_dedupe_keys[dedupe_key] = seed
        seeds.append(seed)

    return seeds, duplicate_input_count, unmatched_input_count


def _open_discovery_candidate_seeds(
    request: dict[str, Any],
    observed_catalog: dict[str, dict[str, Any]],
) -> list[dict[str, Any]]:
    seeds: list[dict[str, Any]] = []
    for bundle in sorted(
        observed_catalog.values(),
        key=lambda entry: (
            entry["state_code"] or "",
            entry["county_name"] or "",
            entry["canonical_url"],
        ),
    ):
        if not _passes_geography_scope(bundle, request):
            continue
        seed = {
            "candidate_subject_id": f"candidate-{deterministic_uuid(request['request_id'], bundle['canonical_url'])[:8]}",
            "raw_value": bundle["canonical_url"],
            "canonical_url": bundle["canonical_url"],
            "platform": bundle["platform"],
            "listing_id": bundle["listing_id"],
            "county_fips": bundle["county_fips"],
            "county_name": bundle["county_name"],
            "state_fips": bundle["state_fips"],
            "state_code": bundle["state_code"],
            "state_name": bundle["state_name"],
            "asking_price": bundle["asking_price"],
            "acreage": bundle["acreage"],
            "listing_text_summary": bundle["listing_text_summary"],
            "title": bundle["title"],
            "bundle": bundle,
            "acquisition_path": "open_discovery_catalog",
            "observation_status": "observed_with_research_artifacts",
            "observed_at": bundle["observed_at"],
            "initial_match_rationale": "Observed listing catalog entry falls inside the active discovery geography scope and can be reused for thesis-aware ranking.",
            "notes": [],
        }
        seed["initial_eligibility_status"] = _initial_eligibility_status(seed, request)
        seeds.append(seed)
    return seeds


def build_candidate_universe(
    request: dict[str, Any],
    *,
    subject_resolution: dict[str, Any],
    workspace_root: str | Path,
    observed_catalog: dict[str, dict[str, Any]] | None = None,
    generated_at: str | None = None,
    write_outputs: bool = True,
) -> dict[str, Any]:
    observed_catalog = observed_catalog or build_observed_candidate_catalog()
    request_mode = _candidate_execution_mode(request)
    duplicate_input_count = 0
    unmatched_input_count = 0

    if request_mode == "user_supplied_ranking":
        seeds, duplicate_input_count, unmatched_input_count = _user_supplied_candidate_seeds(
            request,
            subject_resolution,
            observed_catalog,
        )
    else:
        seeds = _open_discovery_candidate_seeds(request, observed_catalog)

    candidates: list[dict[str, Any]] = []
    for seed in seeds:
        normalized_seed = {
            "input_kind": "listing_url",
            "raw_value": seed["raw_value"],
            "canonical_url": seed["canonical_url"],
            "listing_platform": seed["platform"],
            "listing_id": seed["listing_id"],
            "county_fips": seed["county_fips"],
            "county_name": seed["county_name"],
            "state_fips": seed["state_fips"],
            "state_code": seed["state_code"],
            "state_name": seed["state_name"],
        }
        notes = list(seed["notes"])
        if seed["initial_eligibility_status"] == "hard_filtered":
            notes.append("Current request filters will likely exclude this candidate before ranking.")
        if seed["observation_status"] != "observed_with_research_artifacts":
            notes.append("Alice does not yet have reusable Step 6-9 artifacts for this candidate.")
        candidates.append(
            {
                "candidate_subject_id": seed["candidate_subject_id"],
                "source_listing_url": seed["canonical_url"],
                "platform": seed["platform"],
                "acquisition_path": seed["acquisition_path"],
                "candidate_observation_status": seed["observation_status"],
                "observed_at": seed["observed_at"],
                "raw_listing_hints": {
                    "title": seed["title"],
                    "asking_price": seed["asking_price"],
                    "acreage": seed["acreage"],
                    "county_name": seed["county_name"],
                    "state_code": seed["state_code"],
                    "county_fips": seed["county_fips"],
                    "listing_id": seed["listing_id"],
                    "listing_text_summary": seed["listing_text_summary"],
                },
                "normalized_subject_seed": normalized_seed,
                "dedupe_group_id": f"dedupe-{deterministic_uuid(seed['canonical_url'] or seed['candidate_subject_id'])[:8]}",
                "initial_match_rationale": seed["initial_match_rationale"],
                "initial_eligibility_status": seed["initial_eligibility_status"],
                "linked_workspace_artifacts": _workspace_artifact_refs(seed.get("bundle")),
                "notes": sorted_unique_strings(notes),
            }
        )

    observed_platforms = sorted(
        {
            candidate["platform"]
            for candidate in candidates
            if candidate.get("platform")
        },
        key=lambda platform: PLATFORM_ORDER.get(platform, 99),
    )
    acquisition_limitations = [
        "Recommendation v0 ranks candidates only from user-supplied URLs or Alice's observed listing catalog.",
        "This candidate universe is not a live or exhaustive market crawl.",
    ]
    if request_mode == "open_discovery":
        acquisition_limitations.append(
            "Open discovery currently depends on the listing candidates Alice has already observed and processed."
        )
    if unmatched_input_count:
        acquisition_limitations.append(
            "Some user-supplied candidates do not yet map to reusable Step 6-9 research workspaces."
        )

    acquisition_summary = {
        "observed_platforms": observed_platforms,
        "input_subject_count": len(request.get("subjects", [])),
        "duplicate_input_count": duplicate_input_count,
        "matched_existing_research_count": sum(
            1
            for candidate in candidates
            if candidate["candidate_observation_status"] == "observed_with_research_artifacts"
        ),
        "unmatched_input_count": unmatched_input_count,
        "candidate_observation_limitations": sorted_unique_strings(acquisition_limitations),
        "acquisition_notes": sorted_unique_strings(
            [
                "Candidate acquisition preserves the observed universe before shortlist pruning.",
                "Alice keeps hard-filtered and thin-evidence candidates visible instead of silently dropping them.",
            ]
        ),
    }

    candidate_universe = {
        "schema_version": RECOMMENDATION_CANDIDATES_VERSION,
        "recommendation_candidates_id": deterministic_uuid(
            "alice-recommendation-candidates",
            request["request_id"],
            request_mode,
        ),
        "request_id": request["request_id"],
        "subject_resolution_id": subject_resolution["resolution_id"],
        "generated_at": generated_at or now_iso(),
        "request_mode": request_mode,
        "thesis_summary": _thesis_summary(request),
        "search_scope": _search_scope(request),
        "acquisition_summary": acquisition_summary,
        "candidate_count": len(candidates),
        "candidates": candidates,
    }

    if write_outputs:
        recommendation_root = _recommendation_root(workspace_root)
        (recommendation_root / "candidate_universe.json").write_text(
            json.dumps(candidate_universe, indent=2),
            encoding="utf-8",
        )

    return candidate_universe


def _module_outcomes(modules: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [
        {
            "module_id": module["module_id"],
            "module_label": module["module_label"],
            "fit_assessment": module["fit_assessment"],
            "module_status": module["module_status"],
            "confidence": bounded(float(module["confidence"])),
        }
        for module in modules
    ]


def _overall_fit(modules: list[dict[str, Any]]) -> str:
    fits = [module["fit_assessment"] for module in modules]
    if not fits:
        return "unknown"
    if any(fit == "favorable" for fit in fits) and all(fit != "weak" for fit in fits):
        return "favorable"
    if any(fit in {"favorable", "mixed"} for fit in fits):
        return "mixed"
    if any(fit == "weak" for fit in fits):
        return "weak"
    if all(fit == "not_applicable" for fit in fits):
        return "not_applicable"
    return "unknown"


def _module_summary(modules: list[dict[str, Any]]) -> str:
    if not modules:
        return "Alice does not yet have enough reusable thesis screening to rank this candidate confidently."
    parts = [
        f"{module['module_label']} is {module['fit_assessment']}"
        for module in modules[:3]
    ]
    if len(modules) > 3:
        parts.append(f"+{len(modules) - 3} more modules")
    return "; ".join(parts)


def _evaluate_flood_burden(parcel_memo: dict[str, Any]) -> bool | None:
    flood_text = _field_text(parcel_memo["environmental_constraints"]["flood_zone"]).lower()
    if not flood_text:
        return None
    if "no intersecting" in flood_text or "outside sfha" in flood_text or "zone x" in flood_text:
        return False
    if re.search(r"\bzone\s*(a|ae|ah|ao|ve|v)\b", flood_text) or "sfha" in flood_text:
        return True
    return None


def _evaluate_access_signal(parcel_memo: dict[str, Any]) -> bool | None:
    text = " ".join(
        part.lower()
        for part in [
            _field_text(parcel_memo["infrastructure_and_utilities"]["legal_or_physical_access_signal"]),
            _field_text(parcel_memo["listing"]["listing_text_summary"]),
        ]
        if part
    )
    if not text:
        return None
    if "no legal access" in text or "landlocked" in text:
        return False
    if any(token in text for token in ["road", "frontage", "county-road", "county road", "access"]):
        return True
    return None


def _evaluate_transmission_signal(parcel_memo: dict[str, Any]) -> bool | None:
    text = " ".join(
        part.lower()
        for part in [
            _field_text(parcel_memo["infrastructure_and_utilities"]["transmission_proximity_signal"]),
            _field_text(parcel_memo["infrastructure_and_utilities"]["infrastructure_summary"]),
            _field_text(parcel_memo["listing"]["listing_text_summary"]),
        ]
        if part
    )
    if not text:
        return None
    if any(token in text for token in ["transmission", "substation", "power", "utility"]):
        return True
    return None


def _factor(
    factor_type: str,
    effect: str,
    rationale: str,
) -> dict[str, Any]:
    return {
        "factor_type": factor_type,
        "effect": effect,
        "rationale": rationale,
    }


def _top_strings(values: list[str], *, limit: int = 3) -> list[str]:
    return sorted_unique_strings(values)[:limit]


def _candidate_evidence_quality(bundle: dict[str, Any]) -> dict[str, Any]:
    parcel_memo = bundle["parcel_memo"]
    coverage_assessment = bundle.get("coverage_assessment") or {}
    scores = parcel_memo["scores"]
    return {
        "overall_confidence": bounded(float(scores.get("overall_confidence") or 0.0)),
        "overall_completeness": bounded(float(scores.get("overall_completeness") or 0.0)),
        "coverage_tier": coverage_assessment.get("effective_coverage_tier") or scores.get("coverage_tier"),
        "local_footing_status": coverage_assessment.get("local_footing_status"),
        "parcel_identity_level": parcel_memo["parcel_identity"]["overall_confirmation_level"],
    }


def _candidate_geography(bundle: dict[str, Any]) -> dict[str, Any]:
    return {
        "county_name": bundle.get("county_name"),
        "state_code": bundle.get("state_code"),
        "state_name": bundle.get("state_name"),
        "city_or_place": bundle.get("city_or_place"),
    }


def _major_blockers(
    request: dict[str, Any],
    bundle: dict[str, Any],
    modules: list[dict[str, Any]],
) -> list[str]:
    blockers: list[str] = []
    blockers.extend(
        blocker
        for module in modules
        for blocker in module.get("blocking_flags", [])
    )
    confirmation_level = bundle["parcel_identity_level"]
    risk_descriptions = []
    for risk in bundle["parcel_memo"].get("risks", []):
        if not risk.get("blocking"):
            continue
        if (
            confirmation_level == "parcel_confirmed"
            and (
                risk.get("risk_id") == "parcel_identity_unconfirmed"
                or "not yet anchored to one official parcel record" in (risk.get("description") or "").lower()
            )
        ):
            continue
        risk_descriptions.append(risk["description"])
    blockers.extend(risk_descriptions)

    budget_reason = _budget_mismatch_reason(bundle, request)
    acreage_reason = _acreage_mismatch_reason(bundle, request)
    price_per_acre_reason = _price_per_acre_mismatch_reason(bundle, request)
    blockers.extend(
        reason
        for reason in [budget_reason, acreage_reason, price_per_acre_reason]
        if reason
    )
    return _top_strings(blockers, limit=4)


def _major_unknowns(bundle: dict[str, Any], modules: list[dict[str, Any]]) -> list[str]:
    unknowns: list[str] = []
    unknowns.extend(
        unknown
        for module in modules
        for unknown in module.get("key_unknowns", [])
    )
    unknowns.extend(
        unknown["question"]
        for unknown in bundle["parcel_memo"].get("unknowns", [])
    )
    return _top_strings(unknowns, limit=4)


def _candidate_positioning_note(
    primary_label: str,
    overall_fit: str,
    ranking_factors: list[dict[str, Any]],
) -> str:
    positive = [
        factor["rationale"]
        for factor in ranking_factors
        if factor["effect"] in {"strong_positive", "positive"}
    ]
    negative = [
        factor["rationale"]
        for factor in ranking_factors
        if factor["effect"] in {"negative", "hard_filter"}
    ]
    lead = f"{primary_label} ranks with an overall {overall_fit} thesis fit from the observed candidate universe."
    parts = [lead]
    if positive:
        parts.append(f"Primary support: {positive[0]}")
    if negative:
        parts.append(f"Primary drag: {negative[0]}")
    return " ".join(parts)


def _evaluate_supported_candidate(
    request: dict[str, Any],
    bundle: dict[str, Any],
) -> dict[str, Any]:
    parcel_memo = deepcopy(bundle["parcel_memo"])
    coverage_assessment = bundle.get("coverage_assessment")
    parcel_candidates = bundle.get("parcel_candidates")
    use_case_results = evaluate_use_case_modules(
        request,
        parcel_memo,
        parcel_candidates=parcel_candidates,
        coverage_assessment=coverage_assessment,
    )
    modules = use_case_results["use_case_modules"]
    overall_fit = _overall_fit(modules)
    fit_summary = {
        "overall_fit": overall_fit,
        "summary": _module_summary(modules),
        "module_outcomes": _module_outcomes(modules),
    }
    evidence_quality = _candidate_evidence_quality(bundle)
    ranking_factors: list[dict[str, Any]] = []

    if overall_fit == "favorable":
        ranking_factors.append(
            _factor(
                "module_fit",
                "strong_positive",
                "Requested use-case modules land in favorable or clearly supportive territory at screening grade.",
            )
        )
    elif overall_fit == "mixed":
        ranking_factors.append(
            _factor(
                "module_fit",
                "positive",
                "Requested use-case modules show usable support, but the thesis still carries material blockers or diligence gaps.",
            )
        )
    elif overall_fit == "weak":
        ranking_factors.append(
            _factor(
                "module_fit",
                "negative",
                "Requested use-case modules are already constrained by current blockers or weak footing.",
            )
        )
    else:
        ranking_factors.append(
            _factor(
                "module_fit",
                "negative",
                "Alice does not yet have enough thesis-specific module support to make a stronger call.",
            )
        )

    confirmation_level = bundle["parcel_identity_level"]
    if confirmation_level == "parcel_confirmed":
        ranking_factors.append(
            _factor(
                "parcel_identity",
                "strong_positive",
                "Parcel identity is parcel-confirmed, which reduces subject mismatch risk.",
            )
        )
    elif confirmation_level == "candidate_corroborated":
        ranking_factors.append(
            _factor(
                "parcel_identity",
                "positive",
                "One parcel candidate is locally corroborated, though parcel-specific conclusions remain provisional.",
            )
        )
    else:
        ranking_factors.append(
            _factor(
                "parcel_identity",
                "negative",
                "Parcel identity is still weak or competing, which limits parcel-specific ranking confidence.",
            )
        )

    coverage_tier = evidence_quality["coverage_tier"]
    footing = evidence_quality["local_footing_status"]
    if coverage_tier == "full" or footing == "strong_local_footing":
        ranking_factors.append(
            _factor(
                "coverage_footing",
                "positive",
                "Local coverage footing is stronger than federal-baseline-only screening.",
            )
        )
    elif coverage_tier == "minimal" or footing == "federal_baseline_only":
        ranking_factors.append(
            _factor(
                "coverage_footing",
                "negative",
                "Local diligence footing is thin, so ranking confidence remains limited.",
            )
        )
    else:
        ranking_factors.append(
            _factor(
                "coverage_footing",
                "neutral",
                "Coverage footing is usable but still partial.",
            )
        )

    if evidence_quality["overall_confidence"] >= 0.55 and evidence_quality["overall_completeness"] >= 0.24:
        ranking_factors.append(
            _factor(
                "evidence_quality",
                "positive",
                "Reusable evidence quality is strong enough for comparative screening.",
            )
        )
    elif evidence_quality["overall_confidence"] < 0.35 or evidence_quality["overall_completeness"] < 0.15:
        ranking_factors.append(
            _factor(
                "evidence_quality",
                "negative",
                "Evidence quality is still directional, which limits shortlist confidence.",
            )
        )
    else:
        ranking_factors.append(
            _factor(
                "evidence_quality",
                "neutral",
                "Evidence quality is usable but incomplete.",
            )
        )

    ask_price = bundle.get("asking_price")
    if _budget_mismatch_reason(bundle, request):
        ranking_factors.append(
            _factor("budget_match", "hard_filter", _budget_mismatch_reason(bundle, request) or "")
        )
    elif isinstance(ask_price, (int, float)) and _primary_budget_limit(request) is not None:
        ranking_factors.append(
            _factor(
                "budget_match",
                "positive",
                f"Listing ask of {_format_currency(float(ask_price))} fits inside the active budget ceiling.",
            )
        )
    elif _primary_budget_limit(request) is not None:
        ranking_factors.append(
            _factor(
                "budget_match",
                "neutral",
                "No current listing ask is attached, so budget fit remains unresolved.",
            )
        )

    acreage = bundle.get("acreage")
    if _acreage_mismatch_reason(bundle, request):
        ranking_factors.append(
            _factor("acreage_match", "hard_filter", _acreage_mismatch_reason(bundle, request) or "")
        )
    elif isinstance(acreage, (int, float)) and any(
        isinstance(request.get("thesis", {}).get("filters", {}).get(key), (int, float))
        for key in ["acreage_min", "acreage_max"]
    ):
        ranking_factors.append(
            _factor(
                "acreage_match",
                "positive",
                f"Observed acreage of {_format_number(float(acreage))} acres fits the active acreage target.",
            )
        )

    if request.get("thesis", {}).get("target_geographies"):
        if _passes_geography_scope(bundle, request):
            ranking_factors.append(
                _factor(
                    "geography_match",
                    "positive",
                    "This candidate sits inside the active geography scope.",
                )
            )
        else:
            ranking_factors.append(
                _factor(
                    "geography_match",
                    "hard_filter",
                    "This candidate falls outside the active geography scope.",
                )
            )

    economics_status = use_case_results["directional_economics"]["status"]
    if economics_status == "available":
        ranking_factors.append(
            _factor(
                "directional_economics",
                "positive",
                "Directional economics are available enough to frame acquisition and exit scenarios.",
            )
        )
    elif economics_status == "limited":
        ranking_factors.append(
            _factor(
                "directional_economics",
                "neutral",
                "Directional economics remain limited and should not be mistaken for underwriting.",
            )
        )
    else:
        ranking_factors.append(
            _factor(
                "directional_economics",
                "negative",
                "Directional economics are too thin to contribute much ranking support yet.",
            )
        )

    transmission_preference = request.get("thesis", {}).get("filters", {}).get("prefer_transmission_proximity")
    if transmission_preference:
        transmission_signal = _evaluate_transmission_signal(parcel_memo)
        if transmission_signal is True:
            ranking_factors.append(
                _factor(
                    "transmission_preference",
                    "positive",
                    "Current listing or infrastructure context carries at least a directional transmission or utility signal.",
                )
            )
        else:
            ranking_factors.append(
                _factor(
                    "transmission_preference",
                    "negative",
                    "Transmission or utility proximity is still thin at current screening depth.",
                )
            )

    if request.get("thesis", {}).get("filters", {}).get("exclude_major_floodplain"):
        flood_burden = _evaluate_flood_burden(parcel_memo)
        if flood_burden is True:
            ranking_factors.append(
                _factor(
                    "flood_burden",
                    "hard_filter",
                    "Current flood evidence suggests a major floodplain burden relative to the active request.",
                )
            )
        elif flood_burden is False:
            ranking_factors.append(
                _factor(
                    "flood_burden",
                    "positive",
                    "Available flood screening does not currently show a major floodplain burden.",
                )
            )
        else:
            ranking_factors.append(
                _factor(
                    "flood_burden",
                    "neutral",
                    "Floodplain burden remains unresolved at current screening depth.",
                )
            )

    if request.get("thesis", {}).get("filters", {}).get("require_paved_access") or request.get("thesis", {}).get("filters", {}).get("require_legal_access"):
        access_signal = _evaluate_access_signal(parcel_memo)
        if access_signal is True:
            ranking_factors.append(
                _factor(
                    "access_requirement",
                    "positive",
                    "Listing or infrastructure context includes at least a directional access signal.",
                )
            )
        else:
            ranking_factors.append(
                _factor(
                    "access_requirement",
                    "negative",
                    "Access remains unconfirmed for the active request requirement.",
                )
            )

    blockers = _major_blockers(request, bundle, modules)
    unknowns = _major_unknowns(bundle, modules)
    if len(blockers) >= 2:
        ranking_factors.append(
            _factor(
                "blocker_burden",
                "negative",
                f"{len(blockers)} material blockers are already visible in the current screening set.",
            )
        )
    if len(unknowns) >= 3:
        ranking_factors.append(
            _factor(
                "unknown_burden",
                "negative",
                f"{len(unknowns)} major unknowns still separate this candidate from stronger conviction.",
            )
        )

    hard_filter_reasons = [
        factor["rationale"]
        for factor in ranking_factors
        if factor["effect"] == "hard_filter"
    ]
    ranking_tuple = (
        len(hard_filter_reasons),
        FIT_ORDER.get(overall_fit, 99),
        len(blockers),
        len(unknowns),
        COVERAGE_ORDER.get(coverage_tier, 99),
        FOOTING_ORDER.get(footing, 99),
        CONFIRMATION_ORDER.get(confirmation_level, 99),
        -evidence_quality["overall_confidence"],
        -evidence_quality["overall_completeness"],
        float(bundle.get("asking_price") or 0.0),
        bundle.get("title") or "",
    )
    return {
        "candidate_subject_id": bundle["candidate_subject_id"],
        "primary_label": bundle["title"],
        "source_listing_url": bundle["canonical_url"],
        "platform": bundle["platform"],
        "geography": _candidate_geography(bundle),
        "asking_price": bundle["asking_price"],
        "acreage": bundle["acreage"],
        "fit_assessment_summary": fit_summary,
        "active_module_ids": [module["module_id"] for module in modules],
        "ranking_factors": ranking_factors,
        "major_blockers": blockers,
        "major_unknowns": unknowns,
        "evidence_quality_summary": evidence_quality,
        "candidate_universe_positioning_note": _candidate_positioning_note(
            bundle["title"],
            overall_fit,
            ranking_factors,
        ),
        "linked_workspace_artifacts": _workspace_artifact_refs(bundle),
        "ranking_tuple": ranking_tuple,
        "hard_filter_reasons": hard_filter_reasons,
        "directional_economics_status": economics_status,
    }


def _evaluate_unsupported_candidate(
    request: dict[str, Any],
    candidate: dict[str, Any],
) -> dict[str, Any]:
    ranking_factors = [
        _factor(
            "candidate_universe_limit",
            "hard_filter",
            "Alice does not yet have reusable Step 6-9 research artifacts for this candidate, so it cannot be ranked on the same footing as observed candidates.",
        )
    ]
    overall_fit = "unknown"
    if _budget_mismatch_reason(candidate, request):
        ranking_factors.append(
            _factor("budget_match", "hard_filter", _budget_mismatch_reason(candidate, request) or "")
        )
    if _acreage_mismatch_reason(candidate, request):
        ranking_factors.append(
            _factor("acreage_match", "hard_filter", _acreage_mismatch_reason(candidate, request) or "")
        )
    if request.get("thesis", {}).get("target_geographies") and not _passes_geography_scope(candidate, request):
        ranking_factors.append(
            _factor(
                "geography_match",
                "hard_filter",
                "This candidate falls outside the active geography scope.",
            )
        )
    hard_filter_reasons = [
        factor["rationale"]
        for factor in ranking_factors
        if factor["effect"] == "hard_filter"
    ]
    ranking_tuple = (
        len(hard_filter_reasons) or 1,
        FIT_ORDER[overall_fit],
        99,
        99,
        99,
        99,
        99,
        0,
        0,
        float(candidate.get("asking_price") or 0.0),
        candidate.get("title") or "",
    )
    return {
        "candidate_subject_id": candidate["candidate_subject_id"],
        "primary_label": candidate["title"],
        "source_listing_url": candidate["canonical_url"],
        "platform": candidate["platform"],
        "geography": {
            "county_name": candidate.get("county_name"),
            "state_code": candidate.get("state_code"),
            "state_name": candidate.get("state_name"),
            "city_or_place": None,
        },
        "asking_price": candidate.get("asking_price"),
        "acreage": candidate.get("acreage"),
        "fit_assessment_summary": {
            "overall_fit": "unknown",
            "summary": "Candidate is visible in the universe, but Alice does not yet have reusable research artifacts to rank it confidently.",
            "module_outcomes": [],
        },
        "active_module_ids": select_module_ids(request),
        "ranking_factors": ranking_factors,
        "major_blockers": hard_filter_reasons,
        "major_unknowns": [
            "Step 6-9 research artifacts would need to be generated before this candidate can be compared on the same footing."
        ],
        "evidence_quality_summary": {
            "overall_confidence": 0.0,
            "overall_completeness": 0.0,
            "coverage_tier": None,
            "local_footing_status": None,
            "parcel_identity_level": "unresolved",
        },
        "candidate_universe_positioning_note": (
            "Alice observed this candidate input, but it remains outside the current reusable research footing."
        ),
        "linked_workspace_artifacts": None,
        "ranking_tuple": ranking_tuple,
        "hard_filter_reasons": hard_filter_reasons or [
            "Candidate lacks reusable Step 6-9 research artifacts."
        ],
        "directional_economics_status": "not_enough_data",
    }


def _candidate_from_universe(candidate: dict[str, Any], observed_catalog: dict[str, dict[str, Any]]) -> dict[str, Any]:
    bundle = None
    workspace_refs = candidate.get("linked_workspace_artifacts")
    if workspace_refs and workspace_refs.get("workspace_root"):
        bundle = observed_catalog.get(candidate.get("source_listing_url"))
    return {
        "candidate_subject_id": candidate["candidate_subject_id"],
        "canonical_url": candidate.get("source_listing_url"),
        "platform": candidate.get("platform"),
        "listing_id": candidate["normalized_subject_seed"].get("listing_id"),
        "county_fips": candidate["normalized_subject_seed"].get("county_fips"),
        "county_name": candidate["normalized_subject_seed"].get("county_name"),
        "state_fips": candidate["normalized_subject_seed"].get("state_fips"),
        "state_code": candidate["normalized_subject_seed"].get("state_code"),
        "state_name": candidate["normalized_subject_seed"].get("state_name"),
        "asking_price": candidate["raw_listing_hints"].get("asking_price"),
        "acreage": candidate["raw_listing_hints"].get("acreage"),
        "listing_text_summary": candidate["raw_listing_hints"].get("listing_text_summary"),
        "title": candidate["raw_listing_hints"].get("title") or candidate["candidate_subject_id"],
        "bundle": bundle,
        "observation_status": candidate["candidate_observation_status"],
    }


def build_shortlist(
    request: dict[str, Any],
    candidate_universe: dict[str, Any],
    *,
    subject_resolution: dict[str, Any],
    workspace_root: str | Path,
    observed_catalog: dict[str, dict[str, Any]] | None = None,
    generated_at: str | None = None,
    shortlist_limit: int = DEFAULT_SHORTLIST_LIMIT,
    write_outputs: bool = True,
) -> dict[str, Any]:
    observed_catalog = observed_catalog or build_observed_candidate_catalog()
    evaluated: list[dict[str, Any]] = []
    hard_filtered_count = 0

    for candidate in candidate_universe["candidates"]:
        normalized_candidate = _candidate_from_universe(candidate, observed_catalog)
        bundle = normalized_candidate.get("bundle")
        if bundle is not None:
            bundle = {
                **bundle,
                "candidate_subject_id": candidate["candidate_subject_id"],
            }
            evaluated_candidate = _evaluate_supported_candidate(request, bundle)
        else:
            evaluated_candidate = _evaluate_unsupported_candidate(request, normalized_candidate)
        evaluated.append(evaluated_candidate)
        if evaluated_candidate["hard_filter_reasons"]:
            hard_filtered_count += 1

    sorted_candidates = sorted(evaluated, key=lambda item: item["ranking_tuple"])
    ranked_items: list[dict[str, Any]] = []
    filtered_out_items: list[dict[str, Any]] = []

    for evaluated_candidate in sorted_candidates:
        if evaluated_candidate["hard_filter_reasons"]:
            filtered_out_items.append(
                {
                    "candidate_subject_id": evaluated_candidate["candidate_subject_id"],
                    "primary_label": evaluated_candidate["primary_label"],
                    "source_listing_url": evaluated_candidate["source_listing_url"],
                    "filter_reason": evaluated_candidate["hard_filter_reasons"][0],
                    "major_blockers": _top_strings(evaluated_candidate["major_blockers"]),
                    "major_unknowns": _top_strings(evaluated_candidate["major_unknowns"]),
                    "notes": _top_strings(
                        [
                            "Candidate stayed visible in the universe for transparency.",
                            *evaluated_candidate["hard_filter_reasons"][1:],
                        ]
                    ),
                }
            )
            continue
        if len(ranked_items) < shortlist_limit:
            ranked_items.append(
                {
                    "rank": len(ranked_items) + 1,
                    "candidate_subject_id": evaluated_candidate["candidate_subject_id"],
                    "primary_label": evaluated_candidate["primary_label"],
                    "source_listing_url": evaluated_candidate["source_listing_url"],
                    "platform": evaluated_candidate["platform"],
                    "geography": evaluated_candidate["geography"],
                    "asking_price": evaluated_candidate["asking_price"],
                    "acreage": evaluated_candidate["acreage"],
                    "fit_assessment_summary": evaluated_candidate["fit_assessment_summary"],
                    "active_module_ids": evaluated_candidate["active_module_ids"],
                    "ranking_factors": evaluated_candidate["ranking_factors"],
                    "major_blockers": _top_strings(evaluated_candidate["major_blockers"]),
                    "major_unknowns": _top_strings(evaluated_candidate["major_unknowns"]),
                    "evidence_quality_summary": evaluated_candidate["evidence_quality_summary"],
                    "candidate_universe_positioning_note": evaluated_candidate["candidate_universe_positioning_note"],
                    "linked_workspace_artifacts": evaluated_candidate["linked_workspace_artifacts"],
                }
            )
            continue
        filtered_out_items.append(
            {
                "candidate_subject_id": evaluated_candidate["candidate_subject_id"],
                "primary_label": evaluated_candidate["primary_label"],
                "source_listing_url": evaluated_candidate["source_listing_url"],
                "filter_reason": "Candidate was outranked by stronger fits within the surfaced shortlist limit.",
                "major_blockers": _top_strings(evaluated_candidate["major_blockers"]),
                "major_unknowns": _top_strings(evaluated_candidate["major_unknowns"]),
                "notes": _top_strings(
                    [
                        evaluated_candidate["candidate_universe_positioning_note"],
                    ]
                ),
            }
        )

    observed_platforms = sorted(
        {
            candidate["platform"]
            for candidate in candidate_universe["candidates"]
            if candidate.get("platform")
        },
        key=lambda platform: PLATFORM_ORDER.get(platform, 99),
    )
    limitation_note = (
        f"This shortlist is limited to the {candidate_universe['candidate_count']} candidates Alice was able to observe and process. "
        "It is not exhaustive market coverage."
    )
    top_pick = ranked_items[0] if ranked_items else None
    top_pick_label = top_pick["primary_label"] if top_pick else None
    if top_pick is not None:
        top_line = (
            f"From the candidates Alice was able to observe, {top_pick['primary_label']} ranks first because "
            f"{top_pick['fit_assessment_summary']['summary'].split(';')[0].lower()}."
        )
    else:
        top_line = (
            "Alice did not surface a shortlist candidate on current evidence and request constraints from the observed universe."
        )

    warnings = [
        {
            "warning_type": "non_exhaustive_market_coverage",
            "message": "Alice is ranking only the observed candidate universe and does not claim exhaustive market coverage.",
        }
    ]
    if candidate_universe["acquisition_summary"]["duplicate_input_count"]:
        warnings.append(
            {
                "warning_type": "duplicate_inputs_collapsed",
                "message": "Duplicate user-supplied candidate inputs were collapsed by canonical listing URL.",
            }
        )
    if candidate_universe["acquisition_summary"]["unmatched_input_count"]:
        warnings.append(
            {
                "warning_type": "unmatched_user_candidates",
                "message": "Some user-supplied candidates did not map to reusable Step 6-9 artifacts and were filtered conservatively.",
            }
        )
    if candidate_universe["candidate_count"] < 3:
        warnings.append(
            {
                "warning_type": "thin_candidate_universe",
                "message": "The observed candidate universe is thin, so ranking confidence is constrained by coverage.",
            }
        )
    if hard_filtered_count:
        warnings.append(
            {
                "warning_type": "hard_filtered_candidates_present",
                "message": "At least one observed candidate failed a hard request constraint and stayed visible only for transparency.",
            }
        )
    if ranked_items and any(
        item["fit_assessment_summary"]["overall_fit"] != ranked_items[0]["fit_assessment_summary"]["overall_fit"]
        for item in ranked_items[1:]
    ):
        warnings.append(
            {
                "warning_type": "mixed_evidence_rankings",
                "message": "Shortlist ordering reflects thesis-fit tradeoffs plus evidence quality rather than one single score.",
            }
        )

    source_coverage_notes = sorted_unique_strings(
        [
            "Recommendations reuse Step 6-9 candidate artifacts rather than inventing a parallel shallow scorer.",
            "Official local coverage still matters: weak local footing can drag a candidate below stronger-data peers even when listing optics look attractive.",
            "Listing platforms remain market context; they do not override parcel-confirmed local evidence.",
        ]
    )

    shortlist = {
        "schema_version": SHORTLIST_VERSION,
        "shortlist_id": deterministic_uuid(
            "alice-shortlist",
            request["request_id"],
            candidate_universe["recommendation_candidates_id"],
        ),
        "request_id": request["request_id"],
        "subject_resolution_id": subject_resolution["resolution_id"],
        "recommendation_candidates_id": candidate_universe["recommendation_candidates_id"],
        "generated_at": generated_at or now_iso(),
        "ranking_basis": {
            "request_mode": candidate_universe["request_mode"],
            "active_module_ids": select_module_ids(request),
            "ranking_policy_notes": [
                "Hard request mismatches are filtered before softer comparative ranking.",
                "Alice then prioritizes thesis-fit modules, blocker burden, unknown burden, evidence quality, parcel identity strength, and coverage footing.",
                "This ranking is comparative within the observed universe, not a market-wide score.",
            ],
            "applied_filters": sorted_unique_strings(
                [
                    *(f"geography:{geography['label']}" for geography in request.get("thesis", {}).get("target_geographies", [])),
                    *(
                        f"{key}:{value}"
                        for key, value in request.get("thesis", {}).get("filters", {}).items()
                        if value not in (None, "", [], {})
                    ),
                    *(
                        [f"max_purchase_price:{_primary_budget_limit(request)}"]
                        if _primary_budget_limit(request) is not None
                        else []
                    ),
                ]
            ),
        },
        "shortlist_summary": {
            "selected_count": len(ranked_items),
            "filtered_out_count": len(filtered_out_items),
            "top_pick_candidate_subject_id": top_pick["candidate_subject_id"] if top_pick else None,
            "top_pick_label": top_pick_label,
            "one_line_summary": top_line,
            "limitation_note": limitation_note,
        },
        "candidate_universe_limits": {
            "observed_candidate_count": candidate_universe["candidate_count"],
            "eligible_candidate_count": len(ranked_items),
            "ranked_candidate_count": len(ranked_items),
            "filtered_candidate_count": len(filtered_out_items),
            "observed_platforms": observed_platforms,
            "exhaustive_market_scan": False,
            "limitation_note": limitation_note,
        },
        "ranked_items": ranked_items,
        "filtered_out_items": filtered_out_items,
        "warnings": warnings,
        "source_coverage_notes": source_coverage_notes,
    }

    if write_outputs:
        recommendation_root = _recommendation_root(workspace_root)
        (recommendation_root / "shortlist.json").write_text(
            json.dumps(shortlist, indent=2),
            encoding="utf-8",
        )

    return shortlist


def render_shortlist_markdown(
    request: dict[str, Any],
    candidate_universe: dict[str, Any],
    shortlist: dict[str, Any],
) -> str:
    lines = [
        f"# Alice Recommendation Shortlist: {_thesis_summary(request)}",
        "",
        "## Advisory Boundary",
        "This shortlist is drawn from the candidate universe Alice was able to observe and process. It is not exhaustive market coverage.",
        "",
        "## Request Thesis",
        f"- Summary: {_thesis_summary(request)}",
    ]

    use_case_hypotheses = request.get("thesis", {}).get("use_case_hypotheses", [])
    if use_case_hypotheses:
        lines.append(f"- Active thesis families: {', '.join(use_case_hypotheses)}")
    if request.get("thesis", {}).get("target_geographies"):
        labels = ", ".join(
            geography["label"]
            for geography in request["thesis"]["target_geographies"]
        )
        lines.append(f"- Geography scope: {labels}")
    budget_limit = _primary_budget_limit(request)
    if budget_limit is not None:
        lines.append(f"- Budget ceiling: {_format_currency(budget_limit)}")
    lines.extend(
        [
            "",
            "## Candidate Universe",
            f"- Observed candidates: {candidate_universe['candidate_count']}",
            f"- Acquisition mode: {candidate_universe['request_mode'].replace('_', ' ')}",
            f"- Platforms: {', '.join(candidate_universe['acquisition_summary']['observed_platforms']) or 'none'}",
            f"- Limitation note: {shortlist['candidate_universe_limits']['limitation_note']}",
        ]
    )
    for limitation in candidate_universe["acquisition_summary"]["candidate_observation_limitations"]:
        lines.append(f"- Observation limit: {limitation}")

    lines.extend(["", "## Shortlist Summary", f"- {shortlist['shortlist_summary']['one_line_summary']}"])
    if shortlist["warnings"]:
        for warning in shortlist["warnings"]:
            lines.append(f"- Warning: {warning['message']}")

    lines.extend(["", "## Ranked Candidates"])
    if shortlist["ranked_items"]:
        for item in shortlist["ranked_items"]:
            geography = item["geography"]
            place = ", ".join(
                part
                for part in [
                    geography.get("city_or_place"),
                    geography.get("county_name"),
                    geography.get("state_code"),
                ]
                if part
            )
            lines.append(f"### {item['rank']}. {item['primary_label']}")
            lines.append(f"- Why it surfaced: {item['candidate_universe_positioning_note']}")
            lines.append(
                f"- Fit summary: {item['fit_assessment_summary']['overall_fit']} across "
                f"{', '.join(item['active_module_ids']) or 'no active modules'}."
            )
            if place:
                lines.append(f"- Geography: {place}")
            if isinstance(item["asking_price"], (int, float)) or isinstance(item["acreage"], (int, float)):
                price_text = _format_currency(item["asking_price"]) or "price unresolved"
                acreage_text = (
                    f"{_format_number(item['acreage'])} acres"
                    if isinstance(item["acreage"], (int, float))
                    else "acreage unresolved"
                )
                lines.append(f"- Market snapshot: {price_text}; {acreage_text}")
            evidence = item["evidence_quality_summary"]
            lines.append(
                f"- Evidence footing: {evidence['parcel_identity_level']} parcel identity, "
                f"{evidence['coverage_tier'] or 'unresolved'} coverage tier, "
                f"{_format_confidence(evidence['overall_confidence'])} overall confidence."
            )
            if item["major_blockers"]:
                lines.append("- Major blockers:")
                for blocker in item["major_blockers"]:
                    lines.append(f"  - {blocker}")
            if item["major_unknowns"]:
                lines.append("- Major unknowns:")
                for unknown in item["major_unknowns"]:
                    lines.append(f"  - {unknown}")
            for factor in item["ranking_factors"][:4]:
                lines.append(
                    f"- Ranking factor ({factor['effect'].replace('_', ' ')}): {factor['rationale']}"
                )
            linked_report = item["linked_workspace_artifacts"].get("report_ref") if item["linked_workspace_artifacts"] else None
            linked_memo = item["linked_workspace_artifacts"].get("parcel_memo_ref") if item["linked_workspace_artifacts"] else None
            if linked_memo:
                lines.append(f"- Candidate memo artifact: {linked_memo}")
            if linked_report:
                lines.append(f"- Candidate report artifact: {linked_report}")
            lines.append("")
    else:
        lines.append("- No shortlist candidates were surfaced on current evidence and request constraints.")
        lines.append("")

    if shortlist["filtered_out_items"]:
        lines.extend(["## Filtered or Deprioritized Candidates"])
        for item in shortlist["filtered_out_items"]:
            lines.append(f"- {item['primary_label']}: {item['filter_reason']}")
            for blocker in item["major_blockers"][:2]:
                lines.append(f"  - Blocker: {blocker}")
            for unknown in item["major_unknowns"][:2]:
                lines.append(f"  - Unknown: {unknown}")
        lines.append("")

    lines.extend(["## Coverage and Ranking Notes"])
    for note in shortlist["source_coverage_notes"]:
        lines.append(f"- {note}")

    lines.extend(["", "## Recommended Next Actions"])
    next_actions: list[str] = []
    if shortlist["ranked_items"]:
        top_item = shortlist["ranked_items"][0]
        next_actions.append(
            f"Advance parcel-specific diligence on {top_item['primary_label']} only after reviewing the linked candidate memo and its remaining blockers."
        )
        if top_item["major_unknowns"]:
            next_actions.append(f"Resolve the top unknown: {top_item['major_unknowns'][0]}")
    if shortlist["filtered_out_items"]:
        next_actions.append(
            "Review filtered candidates only if the active budget, acreage, or thesis constraints intentionally change."
        )
    next_actions.append("Treat this shortlist as a comparative screen from the observed universe, not a full market search.")
    for action in next_actions:
        lines.append(f"- {action}")

    report_text = "\n".join(lines).rstrip() + "\n"
    return report_text


def render_shortlist_slack(shortlist: dict[str, Any]) -> str:
    lines = [
        "Alice recommendation shortlist",
        shortlist["shortlist_summary"]["one_line_summary"],
        f"Observed universe: {shortlist['candidate_universe_limits']['observed_candidate_count']} candidates. Not exhaustive market coverage.",
    ]
    if shortlist["ranked_items"]:
        lines.append("Top candidates:")
        for item in shortlist["ranked_items"][:5]:
            lines.append(
                f"{item['rank']}. {item['primary_label']} - {item['fit_assessment_summary']['overall_fit']}; "
                f"{item['candidate_universe_positioning_note']}"
            )
    else:
        lines.append("No shortlist candidates surfaced from the observed universe on current constraints.")
    if shortlist["filtered_out_items"]:
        lines.append(f"Filtered or deprioritized: {len(shortlist['filtered_out_items'])}")
    if shortlist["warnings"]:
        lines.append(f"Top blocker/theme: {shortlist['warnings'][0]['message']}")
    lines.append("Full memo: alice/recommendation/shortlist_report.md")
    return "\n".join(lines) + "\n"


def render_shortlist_email(
    request: dict[str, Any],
    shortlist: dict[str, Any],
) -> str:
    lines = [
        f"Alice recommendation shortlist for: {_thesis_summary(request)}",
        "",
        shortlist["shortlist_summary"]["one_line_summary"],
        "",
        f"Candidate universe note: {shortlist['candidate_universe_limits']['limitation_note']}",
        "",
    ]
    if shortlist["ranked_items"]:
        lines.append("Top ranked candidates:")
        for item in shortlist["ranked_items"][:5]:
            geography = item["geography"]
            place = ", ".join(
                part
                for part in [geography.get("county_name"), geography.get("state_code")]
                if part
            )
            lines.append(
                f"- #{item['rank']} {item['primary_label']} ({place or 'location unresolved'})"
            )
            lines.append(f"  Fit: {item['fit_assessment_summary']['summary']}")
            if item["major_blockers"]:
                lines.append(f"  Main blocker: {item['major_blockers'][0]}")
            if item["major_unknowns"]:
                lines.append(f"  Main unknown: {item['major_unknowns'][0]}")
        lines.append("")
    else:
        lines.append("No shortlist candidate cleared the current evidence and request constraints from the observed universe.")
        lines.append("")

    if shortlist["filtered_out_items"]:
        lines.append("Filtered or deprioritized candidates:")
        for item in shortlist["filtered_out_items"][:3]:
            lines.append(f"- {item['primary_label']}: {item['filter_reason']}")
        lines.append("")

    lines.append("Warnings:")
    for warning in shortlist["warnings"]:
        lines.append(f"- {warning['message']}")
    lines.append("")
    lines.append("Full memo: alice/recommendation/shortlist_report.md")
    return "\n".join(lines) + "\n"


def render_recommendation_workspace(
    request: dict[str, Any],
    *,
    subject_resolution: dict[str, Any] | None = None,
    workspace_root: str | Path,
    observed_catalog: dict[str, dict[str, Any]] | None = None,
    generated_at: str | None = None,
    shortlist_limit: int = DEFAULT_SHORTLIST_LIMIT,
    write_outputs: bool = True,
) -> dict[str, Any]:
    if request.get("request_mode") != "recommendation":
        raise ValueError("Step 10 recommendation rendering expects an Alice request_mode of 'recommendation'.")

    subject_resolution = subject_resolution or build_subject_resolution(request)
    observed_catalog = observed_catalog or build_observed_candidate_catalog()

    alice_root = _workspace_alice_root(workspace_root)
    if write_outputs:
        (alice_root / "request_normalized.json").write_text(json.dumps(request, indent=2), encoding="utf-8")
        (alice_root / "subject_resolution.json").write_text(
            json.dumps(subject_resolution, indent=2),
            encoding="utf-8",
        )

    candidate_universe = build_candidate_universe(
        request,
        subject_resolution=subject_resolution,
        workspace_root=workspace_root,
        observed_catalog=observed_catalog,
        generated_at=generated_at,
        write_outputs=write_outputs,
    )
    shortlist = build_shortlist(
        request,
        candidate_universe,
        subject_resolution=subject_resolution,
        workspace_root=workspace_root,
        observed_catalog=observed_catalog,
        generated_at=generated_at,
        shortlist_limit=shortlist_limit,
        write_outputs=write_outputs,
    )
    markdown = render_shortlist_markdown(request, candidate_universe, shortlist)
    slack_text = render_shortlist_slack(shortlist)
    email_text = render_shortlist_email(request, shortlist)

    for rendered_text in [markdown.lower(), slack_text.lower(), email_text.lower()]:
        for forbidden in EXHAUSTIVE_LANGUAGE:
            if forbidden in rendered_text:
                raise ValueError(
                    f"Recommendation rendering produced forbidden exhaustive-market language: {forbidden!r}"
                )

    if write_outputs:
        recommendation_root = _recommendation_root(workspace_root)
        (recommendation_root / "shortlist_report.md").write_text(markdown, encoding="utf-8")
        (recommendation_root / "shortlist_slack.txt").write_text(slack_text, encoding="utf-8")
        (recommendation_root / "shortlist_email.txt").write_text(email_text, encoding="utf-8")

    return {
        "candidate_universe": candidate_universe,
        "shortlist": shortlist,
        "shortlist_report": markdown,
        "shortlist_slack": slack_text,
        "shortlist_email": email_text,
    }


def main() -> int:
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--request", required=True, help="Path to a normalized Alice recommendation request.")
    parser.add_argument("--workspace-root", required=True, help="Workspace root where alice/ recommendation artifacts will be written.")
    parser.add_argument("--generated-at", help="Optional fixed timestamp for deterministic example generation.")
    parser.add_argument(
        "--shortlist-limit",
        type=int,
        default=DEFAULT_SHORTLIST_LIMIT,
        help="Maximum number of ranked shortlist items to surface.",
    )
    parser.add_argument(
        "--no-write",
        action="store_true",
        help="Build recommendation artifacts without writing them into the workspace.",
    )
    parser.add_argument(
        "--compact",
        action="store_true",
        help="Emit compact JSON instead of pretty JSON.",
    )
    args = parser.parse_args()

    request = load_request(args.request)
    rendered = render_recommendation_workspace(
        request,
        workspace_root=args.workspace_root,
        generated_at=args.generated_at,
        shortlist_limit=args.shortlist_limit,
        write_outputs=not args.no_write,
    )
    shortlist = rendered["shortlist"]
    output = {
        "shortlist_id": shortlist["shortlist_id"],
        "top_pick_label": shortlist["shortlist_summary"]["top_pick_label"],
        "selected_count": shortlist["shortlist_summary"]["selected_count"],
        "limitation_note": shortlist["shortlist_summary"]["limitation_note"],
        "report_ref": "alice/recommendation/shortlist_report.md",
        "slack_ref": "alice/recommendation/shortlist_slack.txt",
        "email_ref": "alice/recommendation/shortlist_email.txt",
    }
    if args.compact:
        print(json.dumps(output, separators=(",", ":")))
    else:
        print(json.dumps(output, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
