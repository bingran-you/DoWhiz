#!/usr/bin/env python3

"""Jurisdiction context builders for Alice AI Step 5."""

from __future__ import annotations

from pathlib import Path
from typing import Any
from uuid import NAMESPACE_URL, uuid5

from alice_registry import county_override_path, get_county_row, load_json, load_all_sources
from alice_subject_resolution import (
    dedupe_strings,
    load_request,
    load_session_state,
    now_iso,
    resolve_county_hint,
    resolve_state_hint,
)


JURISDICTION_CONTEXT_VERSION = "alice.jurisdiction_context.v1"
SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
SCHEMAS_ROOT = SKILL_ROOT / "schemas"
JURISDICTION_CONTEXT_SCHEMA_PATH = SCHEMAS_ROOT / "jurisdiction_context.schema.json"

DERIVATION_PRIORITY = {
    "explicit_request_geography_fips": 1,
    "explicit_request_geography_text": 2,
    "explicit_subject_hint": 3,
    "inherited_session_state": 4,
    "subject_resolution_hint": 5,
    "address_parse_hint": 6,
    "listing_url_slug_hint": 7,
    "geography_context_rollup": 8,
}

DERIVATION_CONFIDENCE = {
    "explicit_request_geography_fips": 0.98,
    "explicit_request_geography_text": 0.93,
    "explicit_subject_hint": 0.9,
    "inherited_session_state": 0.88,
    "subject_resolution_hint": 0.79,
    "address_parse_hint": 0.68,
    "listing_url_slug_hint": 0.61,
    "geography_context_rollup": 0.55,
}

COUNTY_CONFIDENT_TYPES = {
    "explicit_request_geography_fips",
    "explicit_request_geography_text",
    "explicit_subject_hint",
    "inherited_session_state",
}


def load_subject_resolution(path: str | Path) -> dict[str, Any]:
    return load_json(Path(path))


def _candidate_key(*values: str | None) -> str:
    return "::".join("" if value is None else value for value in values)


def _state_from_values(
    state_fips: str | None = None,
    state_code: str | None = None,
    state_name: str | None = None,
) -> dict[str, str] | None:
    return resolve_state_hint(state_code, state_fips, state_name)


def _county_from_values(
    county_fips: str | None = None,
    county_name: str | None = None,
    state_fips: str | None = None,
    state_code: str | None = None,
    state_name: str | None = None,
) -> dict[str, Any] | None:
    if county_fips:
        try:
            return get_county_row(county_fips)
        except KeyError:
            return None
    if not county_name:
        return None
    state = _state_from_values(state_fips=state_fips, state_code=state_code, state_name=state_name)
    if state is None:
        return None
    return resolve_county_hint(state["state_code"], county_name)


def _make_derivation(
    *,
    derivation_type: str,
    scope: str,
    source_ref: str,
    subject_ids: list[str],
    state_fips: str | None = None,
    state_code: str | None = None,
    state_name: str | None = None,
    county_fips: str | None = None,
    county_name: str | None = None,
    city_name: str | None = None,
    notes: list[str] | None = None,
) -> dict[str, Any]:
    state = _state_from_values(state_fips=state_fips, state_code=state_code, state_name=state_name)
    county = _county_from_values(
        county_fips=county_fips,
        county_name=county_name,
        state_fips=state["state_fips"] if state else state_fips,
        state_code=state["state_code"] if state else state_code,
        state_name=state["state_name"] if state else state_name,
    )
    return {
        "derivation_id": str(
            uuid5(
                NAMESPACE_URL,
                "|".join(
                    [
                        "alice-jurisdiction-derivation",
                        derivation_type,
                        scope,
                        source_ref,
                        ",".join(subject_ids),
                        state["state_fips"] if state else (state_fips or ""),
                        county["county_fips"] if county else (county_fips or ""),
                        city_name or "",
                    ]
                ),
            )
        ),
        "derivation_type": derivation_type,
        "scope": scope,
        "subject_ids": sorted(set(subject_ids)),
        "state_fips": state["state_fips"] if state else state_fips,
        "state_code": state["state_code"] if state else state_code,
        "state_name": state["state_name"] if state else state_name,
        "county_fips": county["county_fips"] if county else county_fips,
        "county_name": county["county_name"] if county else county_name,
        "city_name": city_name,
        "source_ref": source_ref,
        "confidence": DERIVATION_CONFIDENCE[derivation_type],
        "notes": notes or [],
    }


def _request_subject_by_index(request: dict[str, Any], index: int) -> dict[str, Any] | None:
    subjects = request.get("subjects", [])
    if index >= len(subjects):
        return None
    return subjects[index]


def _session_target_geographies(session_state: dict[str, Any] | None) -> list[dict[str, Any]]:
    if not session_state:
        return []
    return session_state.get("thesis_snapshot", {}).get("target_geographies", [])


def _build_subject_derivations(
    *,
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
    session_state: dict[str, Any] | None,
    subject_index: int,
    active_subject: dict[str, Any],
) -> list[dict[str, Any]]:
    subject_id = active_subject["subject_id"]
    normalized_input = subject_resolution["normalized_inputs"][subject_index]
    request_subject = _request_subject_by_index(request, subject_index)
    derivations: list[dict[str, Any]] = []

    for index, geography in enumerate(request.get("thesis", {}).get("target_geographies", [])):
        state = _state_from_values(
            state_fips=geography.get("state_fips"),
            state_code=geography.get("state_code"),
        )
        county = _county_from_values(
            county_fips=geography.get("county_fips"),
            county_name=geography.get("county_name"),
            state_fips=state["state_fips"] if state else None,
            state_code=state["state_code"] if state else None,
        )
        if county is not None:
            derivations.append(
                _make_derivation(
                    derivation_type="explicit_request_geography_fips"
                    if geography.get("county_fips")
                    else "explicit_request_geography_text",
                    scope="county",
                    source_ref=f"request:thesis.target_geographies[{index}]",
                    subject_ids=[subject_id],
                    state_fips=county["state_fips"],
                    state_code=county["state_code"],
                    state_name=county["state_name"],
                    county_fips=county["county_fips"],
                    county_name=county["county_name"],
                    city_name=geography.get("city_name"),
                    notes=["Request-level target geography supplied county context."],
                )
            )
        elif state is not None:
            derivations.append(
                _make_derivation(
                    derivation_type="explicit_request_geography_fips"
                    if geography.get("state_fips")
                    else "explicit_request_geography_text",
                    scope="state",
                    source_ref=f"request:thesis.target_geographies[{index}]",
                    subject_ids=[subject_id],
                    state_fips=state["state_fips"],
                    state_code=state["state_code"],
                    state_name=state["state_name"],
                    city_name=geography["label"] if geography.get("kind") == "city" else None,
                    notes=["Request-level target geography supplied state context."],
                )
            )
        if geography.get("kind") == "city" and geography.get("label"):
            derivations.append(
                _make_derivation(
                    derivation_type="explicit_request_geography_text",
                    scope="city_or_place",
                    source_ref=f"request:thesis.target_geographies[{index}]",
                    subject_ids=[subject_id],
                    state_fips=state["state_fips"] if state else None,
                    state_code=state["state_code"] if state else geography.get("state_code"),
                    state_name=state["state_name"] if state else None,
                    city_name=geography["label"],
                    notes=["Request-level geography explicitly named a city or place."],
                )
            )

    if request_subject is not None:
        hints = request_subject.get("resolution_hints", {})
        if hints:
            derivations.append(
                _make_derivation(
                    derivation_type="explicit_subject_hint",
                    scope="county" if hints.get("county_fips") or hints.get("county_name") else "state",
                    source_ref=f"request:subjects[{subject_index}].resolution_hints",
                    subject_ids=[subject_id],
                    state_fips=hints.get("state_fips"),
                    state_code=hints.get("state_code"),
                    county_fips=hints.get("county_fips"),
                    county_name=hints.get("county_name"),
                    city_name=hints.get("city_name"),
                    notes=["Request-level subject hints provided geography context directly."],
                )
            )

    if session_state is not None:
        for index, geography in enumerate(_session_target_geographies(session_state)):
            state = _state_from_values(
                state_fips=geography.get("state_fips"),
                state_code=geography.get("state_code"),
            )
            county = _county_from_values(
                county_fips=geography.get("county_fips"),
                county_name=geography.get("county_name"),
                state_fips=state["state_fips"] if state else None,
                state_code=state["state_code"] if state else None,
            )
            derivations.append(
                _make_derivation(
                    derivation_type="inherited_session_state",
                    scope="county" if county else "state",
                    source_ref=f"session_state:thesis_snapshot.target_geographies[{index}]",
                    subject_ids=[subject_id],
                    state_fips=county["state_fips"] if county else (state["state_fips"] if state else None),
                    state_code=county["state_code"] if county else (state["state_code"] if state else None),
                    state_name=county["state_name"] if county else (state["state_name"] if state else None),
                    county_fips=county["county_fips"] if county else None,
                    county_name=county["county_name"] if county else None,
                    city_name=geography.get("label") if geography.get("city_name") else None,
                    notes=["Follow-up inherited geography from prior Alice session state."],
                )
            )

    identifiers = active_subject["identifiers"]
    if identifiers.get("county_fips") or identifiers.get("state_fips"):
        derivations.append(
            _make_derivation(
                derivation_type="subject_resolution_hint",
                scope="county" if identifiers.get("county_fips") else "state",
                source_ref=f"subject_resolution:active_subjects[{subject_index}]",
                subject_ids=[subject_id],
                state_fips=identifiers.get("state_fips"),
                state_code=identifiers.get("state_code"),
                state_name=identifiers.get("state_name"),
                county_fips=identifiers.get("county_fips"),
                county_name=identifiers.get("county_name"),
                city_name=identifiers.get("city_name"),
                notes=["Step 4 subject identifiers already contained geography hints."],
            )
        )

    normalized = normalized_input["normalized"]
    if active_subject["input_kind"] == "address" and (
        normalized.get("state_code_hint") or normalized.get("city_hint")
    ):
        derivations.append(
            _make_derivation(
                derivation_type="address_parse_hint",
                scope="city_or_place" if normalized.get("city_hint") else "state",
                source_ref=f"subject_resolution:normalized_inputs[{subject_index}]",
                subject_ids=[subject_id],
                state_code=normalized.get("state_code_hint"),
                city_name=normalized.get("city_hint"),
                notes=["Step 4 address parsing yielded non-authoritative jurisdiction clues."],
            )
        )

    if active_subject["input_kind"] == "listing_url" and (
        normalized.get("county_text_hint") or normalized.get("state_code_hint")
    ):
        derivations.append(
            _make_derivation(
                derivation_type="listing_url_slug_hint",
                scope="county" if normalized.get("county_text_hint") else "state",
                source_ref=f"subject_resolution:normalized_inputs[{subject_index}]",
                subject_ids=[subject_id],
                state_code=normalized.get("state_code_hint"),
                county_name=normalized.get("county_text_hint"),
                notes=["Listing URL path hints provided best-effort geography clues."],
            )
        )

    geography_context = subject_resolution.get("geography_context", {})
    if not derivations:
        counties = geography_context.get("counties", [])
        states = geography_context.get("states", [])
        cities = geography_context.get("cities", [])
        if counties:
            county = counties[0]
            derivations.append(
                _make_derivation(
                    derivation_type="geography_context_rollup",
                    scope="county",
                    source_ref="subject_resolution:geography_context",
                    subject_ids=[subject_id],
                    state_fips=county.get("state_fips"),
                    state_code=county.get("state_code"),
                    state_name=county.get("state_name"),
                    county_fips=county.get("county_fips"),
                    county_name=county.get("county_name"),
                    city_name=cities[0] if cities else None,
                    notes=["Used only as the weakest geography rollup when no better subject-specific clue existed."],
                )
            )
        elif states:
            state = states[0]
            derivations.append(
                _make_derivation(
                    derivation_type="geography_context_rollup",
                    scope="state",
                    source_ref="subject_resolution:geography_context",
                    subject_ids=[subject_id],
                    state_fips=state.get("state_fips"),
                    state_code=state.get("state_code"),
                    state_name=state.get("state_name"),
                    city_name=cities[0] if cities else None,
                    notes=["Used only as the weakest geography rollup when no better subject-specific clue existed."],
                )
            )

    deduped: dict[str, dict[str, Any]] = {}
    for derivation in derivations:
        key = "|".join(
            [
                derivation["derivation_type"],
                derivation["scope"],
                ",".join(derivation["subject_ids"]),
                derivation["state_fips"] or "",
                derivation["county_fips"] or "",
                derivation["city_name"] or "",
                derivation["source_ref"],
            ]
        )
        deduped[key] = derivation
    return list(deduped.values())


def _select_state_candidate(derivations: list[dict[str, Any]]) -> tuple[dict[str, Any] | None, list[str]]:
    candidates = []
    for derivation in derivations:
        state = _state_from_values(
            state_fips=derivation.get("state_fips"),
            state_code=derivation.get("state_code"),
            state_name=derivation.get("state_name"),
        )
        if state is None:
            continue
        candidates.append(
            {
                "fips": state["state_fips"],
                "code": state["state_code"],
                "name": state["state_name"],
                "derivation_type": derivation["derivation_type"],
                "precedence": DERIVATION_PRIORITY[derivation["derivation_type"]],
            }
        )
    if not candidates:
        return None, []

    candidates.sort(key=lambda item: (item["precedence"], item["fips"]))
    strongest_precedence = candidates[0]["precedence"]
    strongest = [item for item in candidates if item["precedence"] == strongest_precedence]
    state_values = sorted({item["fips"] for item in strongest})
    if len(state_values) > 1:
        return None, state_values
    return strongest[0], []


def _select_county_candidate(
    derivations: list[dict[str, Any]],
    chosen_state: dict[str, Any] | None,
) -> tuple[dict[str, Any] | None, list[str]]:
    candidates = []
    for derivation in derivations:
        county = _county_from_values(
            county_fips=derivation.get("county_fips"),
            county_name=derivation.get("county_name"),
            state_fips=derivation.get("state_fips"),
            state_code=derivation.get("state_code"),
            state_name=derivation.get("state_name"),
        )
        if county is None:
            continue
        precedence = DERIVATION_PRIORITY[derivation["derivation_type"]]
        if chosen_state is not None and chosen_state["fips"] != county["state_fips"] and precedence >= chosen_state["precedence"]:
            continue
        candidates.append(
            {
                "fips": county["county_fips"],
                "name": county["county_name"],
                "state_fips": county["state_fips"],
                "state_code": county["state_code"],
                "state_name": county["state_name"],
                "derivation_type": derivation["derivation_type"],
                "precedence": precedence,
            }
        )
    if not candidates:
        return None, []

    candidates.sort(key=lambda item: (item["precedence"], item["fips"]))
    strongest_precedence = candidates[0]["precedence"]
    strongest = [item for item in candidates if item["precedence"] == strongest_precedence]
    county_values = sorted({item["fips"] for item in strongest})
    if len(county_values) > 1:
        return None, county_values
    return strongest[0], []


def _select_city_candidate(
    derivations: list[dict[str, Any]],
    chosen_state: dict[str, Any] | None,
) -> tuple[dict[str, Any] | None, list[str]]:
    candidates = []
    for derivation in derivations:
        city_name = derivation.get("city_name")
        if not city_name:
            continue
        precedence = DERIVATION_PRIORITY[derivation["derivation_type"]]
        if chosen_state is not None and derivation.get("state_fips") not in {None, chosen_state["fips"]} and precedence >= chosen_state["precedence"]:
            continue
        candidates.append(
            {
                "name": city_name,
                "derivation_type": derivation["derivation_type"],
                "precedence": precedence,
            }
        )
    if not candidates:
        return None, []

    candidates.sort(key=lambda item: (item["precedence"], item["name"].lower()))
    strongest_precedence = candidates[0]["precedence"]
    strongest = [item for item in candidates if item["precedence"] == strongest_precedence]
    values = sorted({item["name"] for item in strongest})
    if len(values) > 1:
        return None, values
    return strongest[0], []


def _subject_geography_status(
    *,
    active_subject: dict[str, Any],
    chosen_state: dict[str, Any] | None,
    chosen_county: dict[str, Any] | None,
    chosen_city: dict[str, Any] | None,
) -> str:
    if chosen_county is not None:
        if chosen_county["derivation_type"] in COUNTY_CONFIDENT_TYPES:
            return "county_resolved"
        return "county_inferred"
    if chosen_state is not None and chosen_city is not None:
        return "city_state_partial"
    if chosen_state is not None:
        return "state_resolved"
    if active_subject["identifiers"].get("coordinates") is not None:
        return "geography_only"
    return "unresolved"


def _subject_incorporated_status(derivations: list[dict[str, Any]]) -> str:
    for derivation in derivations:
        if derivation["scope"] == "city_or_place" and derivation["derivation_type"] in {
            "explicit_request_geography_fips",
            "explicit_request_geography_text",
        }:
            return "incorporated"
    return "unknown"


def _strongest_derivation_type(
    derivations: list[dict[str, Any]],
    *,
    state_fips: str | None = None,
    county_fips: str | None = None,
    city_name: str | None = None,
) -> str | None:
    matching = []
    for derivation in derivations:
        if state_fips is not None and derivation.get("state_fips") != state_fips:
            continue
        if county_fips is not None and derivation.get("county_fips") != county_fips:
            continue
        if city_name is not None and derivation.get("city_name") != city_name:
            continue
        matching.append(derivation)
    if not matching:
        return None
    matching.sort(key=lambda item: DERIVATION_PRIORITY[item["derivation_type"]])
    return matching[0]["derivation_type"]


def _aggregate_top_level(
    *,
    active_subject_ids: list[str],
    subject_jurisdictions: list[dict[str, Any]],
    derivations: list[dict[str, Any]],
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any], str, str | None, str | None, str | None]:
    state_values = sorted({item["state_fips"] for item in subject_jurisdictions if item["state_fips"]})
    county_values = sorted({item["county_fips"] for item in subject_jurisdictions if item["county_fips"]})
    city_values = sorted({item["city_or_place"] for item in subject_jurisdictions if item["city_or_place"]})

    has_unknown_county = any(item["county_fips"] is None for item in subject_jurisdictions)
    has_unknown_state = any(item["state_fips"] is None for item in subject_jurisdictions)

    geography_status = "unresolved"
    effective_state_fips = None
    effective_county_fips = None
    effective_city_name = None

    if len(state_values) > 1 or len(county_values) > 1:
        geography_status = "mixed_subjects"
    elif len(county_values) == 1 and not has_unknown_county:
        effective_county_fips = county_values[0]
        county = get_county_row(effective_county_fips)
        effective_state_fips = county["state_fips"]
        effective_city_name = city_values[0] if len(city_values) == 1 else None
        county_type = _strongest_derivation_type(derivations, county_fips=effective_county_fips)
        geography_status = "county_resolved" if county_type in COUNTY_CONFIDENT_TYPES else "county_inferred"
    elif len(state_values) == 1 and (has_unknown_county or not county_values):
        effective_state_fips = state_values[0]
        effective_city_name = city_values[0] if len(city_values) == 1 else None
        if effective_city_name:
            geography_status = "mixed_subjects" if len(active_subject_ids) > 1 and has_unknown_county else "city_state_partial"
        else:
            geography_status = "mixed_subjects" if len(active_subject_ids) > 1 and has_unknown_county else "state_resolved"
    elif len(county_values) == 1 and has_unknown_county:
        effective_state_fips = get_county_row(county_values[0])["state_fips"]
        geography_status = "mixed_subjects"
    elif any(item["geography_status"] == "geography_only" for item in subject_jurisdictions):
        geography_status = "geography_only"

    state_candidates = {}
    for item in subject_jurisdictions:
        if item["state_fips"]:
            state_candidates[item["state_fips"]] = state_candidates.get(item["state_fips"], 0) + 1

    county_candidates = {}
    for item in subject_jurisdictions:
        if item["county_fips"]:
            county_candidates[item["county_fips"]] = county_candidates.get(item["county_fips"], 0) + 1

    state_context = {
        "fips": effective_state_fips or (state_values[0] if len(state_values) == 1 else None),
        "code": None,
        "name": None,
        "resolution_status": "resolved" if effective_state_fips else ("candidate" if len(state_values) == 1 else ("conflicted" if len(state_values) > 1 else "unresolved")),
        "derivation_type": None,
        "subject_ids": [item["subject_id"] for item in subject_jurisdictions if item["state_fips"] == (effective_state_fips or (state_values[0] if len(state_values) == 1 else None))],
        "notes": [],
    }
    if state_context["fips"]:
        state = _state_from_values(state_fips=state_context["fips"])
        if state is not None:
            state_context["code"] = state["state_code"]
            state_context["name"] = state["state_name"]
            state_context["derivation_type"] = _strongest_derivation_type(derivations, state_fips=state["state_fips"])
    elif len(state_values) > 1:
        state_context["notes"].append("Active subjects do not agree on one state context.")

    county_context = {
        "fips": effective_county_fips or (county_values[0] if len(county_values) == 1 else None),
        "name": None,
        "resolution_status": "resolved" if effective_county_fips else ("candidate" if len(county_values) == 1 else ("conflicted" if len(county_values) > 1 else "unresolved")),
        "derivation_type": None,
        "subject_ids": [item["subject_id"] for item in subject_jurisdictions if item["county_fips"] == (effective_county_fips or (county_values[0] if len(county_values) == 1 else None))],
        "notes": [],
    }
    if county_context["fips"]:
        county = get_county_row(county_context["fips"])
        county_context["name"] = county["county_name"]
        county_context["derivation_type"] = _strongest_derivation_type(derivations, county_fips=county["county_fips"])
        if county_context["resolution_status"] == "candidate":
            county_context["notes"].append("At least one active subject points at this county, but the full active subject set does not yet share it.")
    elif len(county_values) > 1:
        county_context["notes"].append("Active subjects do not agree on one county context.")

    city_context = {
        "name": effective_city_name or (city_values[0] if len(city_values) == 1 else None),
        "resolution_status": "resolved" if effective_city_name else ("candidate" if len(city_values) == 1 and len(active_subject_ids) > 1 else ("conflicted" if len(city_values) > 1 else "unresolved")),
        "derivation_type": None,
        "subject_ids": [item["subject_id"] for item in subject_jurisdictions if item["city_or_place"] == (effective_city_name or (city_values[0] if len(city_values) == 1 else None))],
        "notes": [],
    }
    if city_context["name"]:
        city_context["derivation_type"] = _strongest_derivation_type(derivations, city_name=city_context["name"])
        if city_context["resolution_status"] == "candidate":
            city_context["notes"].append("City or place context is only attached to part of the active subject set.")
    elif len(city_values) > 1:
        city_context["notes"].append("Active subjects do not agree on one city or place context.")

    incorporated_status = "unknown"
    subject_incorporated = {item["incorporated_status"] for item in subject_jurisdictions}
    if subject_incorporated == {"incorporated"}:
        incorporated_status = "incorporated"
    elif subject_incorporated == {"unincorporated"}:
        incorporated_status = "unincorporated"
    elif len(subject_incorporated - {"unknown"}) > 1:
        incorporated_status = "mixed"

    return (
        state_context,
        county_context,
        city_context,
        incorporated_status,
        effective_state_fips,
        effective_county_fips,
        state_context["code"],
    )


def _build_special_district_placeholders(county_fips: str | None, city_name: str | None) -> list[dict[str, Any]]:
    if county_fips is None:
        return []

    placeholders = []
    for source in load_all_sources():
        geography = source["geography_scope"]
        if geography["county_fips"] != county_fips:
            continue
        if geography["city_name"] is not None and geography["city_name"] != city_name:
            continue
        if source["jurisdiction_level"] == "special_district":
            district_type = "other"
            if any(capability in source["capabilities"] for capability in ["parcel_lookup_by_apn", "tax_roll", "owner_name"]):
                district_type = "appraisal_district"
            elif "utility_territory" in source["capabilities"]:
                district_type = "utility_service_area"
            elif "water_rights" in source["capabilities"]:
                district_type = "water_district"
            placeholders.append(
                {
                    "district_type": district_type,
                    "status": "candidate",
                    "name": geography.get("special_district_name"),
                    "source_ids": [source["source_id"]],
                    "notes": [
                        "Relevant special-district source is registered for this county context.",
                    ],
                }
            )
        elif source["category"] == "city_local" and geography.get("city_name") and city_name is None:
            placeholders.append(
                {
                    "district_type": "planning_authority",
                    "status": "deferred",
                    "name": geography["city_name"],
                    "source_ids": [source["source_id"]],
                    "notes": [
                        "City-specific planning source exists, but city applicability is not yet confirmed.",
                    ],
                }
            )

    deduped: dict[str, dict[str, Any]] = {}
    for placeholder in placeholders:
        key = "|".join(
            [
                placeholder["district_type"],
                placeholder["status"],
                placeholder["name"] or "",
                ",".join(placeholder["source_ids"]),
            ]
        )
        deduped[key] = placeholder
    return list(deduped.values())


def _build_conflicts(
    *,
    subject_jurisdictions: list[dict[str, Any]],
    derivations: list[dict[str, Any]],
    request_mode: str,
) -> list[dict[str, Any]]:
    conflicts = []

    state_values = sorted({item["state_fips"] for item in subject_jurisdictions if item["state_fips"]})
    county_values = sorted({item["county_fips"] for item in subject_jurisdictions if item["county_fips"]})
    city_values = sorted({item["city_or_place"] for item in subject_jurisdictions if item["city_or_place"]})

    if len(state_values) > 1:
        conflicts.append(
            {
                "conflict_type": "state_conflict",
                "subject_ids": [item["subject_id"] for item in subject_jurisdictions if item["state_fips"]],
                "candidate_values": state_values,
                "notes": ["Active subjects point at multiple states."],
            }
        )
    if len(county_values) > 1:
        conflicts.append(
            {
                "conflict_type": "county_conflict" if request_mode != "batch_compare" else "batch_subject_mismatch",
                "subject_ids": [item["subject_id"] for item in subject_jurisdictions if item["county_fips"]],
                "candidate_values": county_values,
                "notes": ["Active subjects do not share one county context."],
            }
        )
    if len(city_values) > 1:
        conflicts.append(
            {
                "conflict_type": "city_conflict",
                "subject_ids": [item["subject_id"] for item in subject_jurisdictions if item["city_or_place"]],
                "candidate_values": city_values,
                "notes": ["Active subjects do not share one city or place context."],
            }
        )

    explicit_request_counties = {
        derivation["county_fips"]
        for derivation in derivations
        if derivation["derivation_type"] in {"explicit_request_geography_fips", "explicit_request_geography_text", "explicit_subject_hint"}
        and derivation["county_fips"] is not None
    }
    inherited_counties = {
        derivation["county_fips"]
        for derivation in derivations
        if derivation["derivation_type"] == "inherited_session_state" and derivation["county_fips"] is not None
    }
    if explicit_request_counties and inherited_counties and explicit_request_counties != inherited_counties:
        conflicts.append(
            {
                "conflict_type": "session_state_conflict",
                "subject_ids": sorted(
                    {
                        subject_id
                        for derivation in derivations
                        if derivation["derivation_type"] == "inherited_session_state"
                        for subject_id in derivation["subject_ids"]
                    }
                ),
                "candidate_values": sorted(explicit_request_counties | inherited_counties),
                "notes": ["Inherited session-state county context disagrees with stronger request-level geography."],
            }
        )

    return conflicts


def _determine_effective_registry(county_fips: str | None) -> tuple[str | None, str]:
    if county_fips is None:
        return None, "none"
    path = county_override_path(county_fips)
    if path.exists():
        state_code = get_county_row(county_fips)["state_code"]
        return f"registry/county_overrides/{state_code}/{county_fips}.json", "curated_override"
    return f"generated:county_fallback:{county_fips}", "generated_fallback"


def _build_scores(
    *,
    geography_status: str,
    subject_jurisdictions: list[dict[str, Any]],
    effective_state_fips: str | None,
    effective_county_fips: str | None,
) -> dict[str, Any]:
    confidence_map = {
        "county_resolved": 0.94,
        "county_inferred": 0.8,
        "state_resolved": 0.72,
        "city_state_partial": 0.66,
        "geography_only": 0.46,
        "mixed_subjects": 0.4,
        "conflicted": 0.18,
        "unresolved": 0.12,
    }
    if effective_county_fips is not None:
        matched_subjects = sum(1 for item in subject_jurisdictions if item["county_fips"] == effective_county_fips)
        subject_alignment = round(matched_subjects / len(subject_jurisdictions), 2)
        county_attachment_readiness = 1.0 if matched_subjects == len(subject_jurisdictions) else 0.45
    elif effective_state_fips is not None:
        matched_subjects = sum(1 for item in subject_jurisdictions if item["state_fips"] == effective_state_fips)
        subject_alignment = round(matched_subjects / len(subject_jurisdictions), 2)
        county_attachment_readiness = 0.42
    else:
        subject_alignment = round(
            sum(1 for item in subject_jurisdictions if item["geography_status"] != "unresolved")
            / len(subject_jurisdictions),
            2,
        )
        county_attachment_readiness = 0.18 if geography_status == "geography_only" else 0.0

    return {
        "jurisdiction_confidence": confidence_map[geography_status],
        "county_attachment_readiness": round(county_attachment_readiness, 2),
        "subject_alignment_score": subject_alignment,
    }


def build_jurisdiction_context(
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
    session_state: dict[str, Any] | None = None,
) -> dict[str, Any]:
    active_subjects = subject_resolution["active_subjects"]
    active_subject_ids = [subject["subject_id"] for subject in active_subjects]

    subject_jurisdictions = []
    derivations: list[dict[str, Any]] = []
    for index, active_subject in enumerate(active_subjects):
        subject_derivations = _build_subject_derivations(
            request=request,
            subject_resolution=subject_resolution,
            session_state=session_state,
            subject_index=index,
            active_subject=active_subject,
        )
        derivations.extend(subject_derivations)

        chosen_state, _ = _select_state_candidate(subject_derivations)
        chosen_county, _ = _select_county_candidate(subject_derivations, chosen_state)
        if chosen_county is not None and chosen_state is None:
            chosen_state = {
                "fips": chosen_county["state_fips"],
                "code": chosen_county["state_code"],
                "name": chosen_county["state_name"],
                "derivation_type": chosen_county["derivation_type"],
                "precedence": chosen_county["precedence"],
            }
        chosen_city, _ = _select_city_candidate(subject_derivations, chosen_state)

        geography_status = _subject_geography_status(
            active_subject=active_subject,
            chosen_state=chosen_state,
            chosen_county=chosen_county,
            chosen_city=chosen_city,
        )

        derivation_types = dedupe_strings([item["derivation_type"] for item in subject_derivations])
        notes = []
        if chosen_county is None and chosen_state is None and active_subject["input_kind"] == "apn":
            notes.append("APN remained jurisdiction-unresolved because Step 5 will not infer county from APN format alone.")
        if active_subject["input_kind"] == "coordinates" and chosen_county is None:
            notes.append("Coordinates are preserved as geography-only context until offline county lookup exists.")
        if active_subject["input_kind"] == "subject_ref" and session_state is not None:
            notes.append("County/state context was inherited from session state rather than restated in the follow-up prompt.")

        subject_jurisdictions.append(
            {
                "subject_id": active_subject["subject_id"],
                "geography_status": geography_status,
                "state_fips": chosen_state["fips"] if chosen_state else None,
                "state_code": chosen_state["code"] if chosen_state else None,
                "state_name": chosen_state["name"] if chosen_state else None,
                "county_fips": chosen_county["fips"] if chosen_county else None,
                "county_name": chosen_county["name"] if chosen_county else None,
                "city_or_place": chosen_city["name"] if chosen_city else None,
                "incorporated_status": _subject_incorporated_status(subject_derivations),
                "derivation_types": derivation_types,
                "notes": notes,
            }
        )

    (
        state_context,
        county_context,
        city_context,
        incorporated_status,
        effective_state_fips,
        effective_county_fips,
        effective_state_code,
    ) = _aggregate_top_level(
        active_subject_ids=active_subject_ids,
        subject_jurisdictions=subject_jurisdictions,
        derivations=derivations,
    )

    geography_status = "conflicted" if _build_conflicts(
        subject_jurisdictions=subject_jurisdictions,
        derivations=derivations,
        request_mode=request["request_mode"],
    ) else (
        "geography_only"
        if any(item["geography_status"] == "geography_only" for item in subject_jurisdictions)
        and effective_state_fips is None
        and effective_county_fips is None
        else (
            "mixed_subjects"
            if len(active_subject_ids) > 1
            and any(item["county_fips"] is None for item in subject_jurisdictions)
            and effective_county_fips is None
            else (
                "county_resolved"
                if effective_county_fips is not None and county_context["derivation_type"] in COUNTY_CONFIDENT_TYPES
                else (
                    "county_inferred"
                    if effective_county_fips is not None
                    else (
                        "city_state_partial"
                        if effective_state_fips is not None and city_context["name"] is not None
                        else ("state_resolved" if effective_state_fips is not None else "unresolved")
                    )
                )
            )
        )
    )

    effective_county_registry_ref, effective_county_registry_mode = _determine_effective_registry(effective_county_fips)
    conflicts = _build_conflicts(
        subject_jurisdictions=subject_jurisdictions,
        derivations=derivations,
        request_mode=request["request_mode"],
    )

    unresolved_questions = []
    if effective_county_fips is None and effective_state_fips is None:
        unresolved_questions.append("What county or state should govern local parcel-level research before county registry coverage is attached?")
    elif effective_county_fips is None and effective_state_fips is not None:
        unresolved_questions.append("Which county should govern local parcel, zoning, and tax research for this subject?")
    if geography_status == "mixed_subjects":
        unresolved_questions.append("Should Alice narrow this batch to subjects that share one county before local source planning proceeds?")
    if city_context["name"] is not None and incorporated_status == "unknown":
        unresolved_questions.append("Does city-specific planning jurisdiction apply here, or is this still unincorporated county land?")
    unresolved_questions = dedupe_strings(unresolved_questions)

    notes = []
    if effective_county_registry_mode == "generated_fallback":
        notes.append("County attachment is stable enough to use the county registry, but only the generated fallback entry exists today.")
    elif effective_county_registry_mode == "curated_override":
        notes.append("A curated county registry override is available for this jurisdiction.")
    if session_state is not None and request["request_mode"] == "follow_up":
        notes.append("Jurisdiction context reused prior session-state geography rather than restarting from the follow-up prompt alone.")
    if any(item["geography_status"] == "geography_only" for item in subject_jurisdictions):
        notes.append("At least one active subject remains geography-only, so parcel-level local planning must stay conservative.")

    context = {
        "schema_version": JURISDICTION_CONTEXT_VERSION,
        "jurisdiction_context_id": str(
            uuid5(
                NAMESPACE_URL,
                f"alice-jurisdiction-context:{request['request_id']}:{subject_resolution['resolution_id']}",
            )
        ),
        "request_id": request["request_id"],
        "subject_resolution_id": subject_resolution["resolution_id"],
        "session_state_ref": subject_resolution.get("state_refs", {}).get("previous_session_state_ref")
        or subject_resolution.get("state_refs", {}).get("conversation_state_ref"),
        "request_mode": request["request_mode"],
        "active_subject_ids": active_subject_ids,
        "subject_jurisdictions": subject_jurisdictions,
        "geography_status": geography_status,
        "state": state_context,
        "county": county_context,
        "city_or_place": city_context,
        "incorporated_status": incorporated_status,
        "county_fips": effective_county_fips,
        "state_fips": effective_state_fips,
        "state_code": effective_state_code,
        "derivation_sources": derivations,
        "jurisdiction_conflicts": conflicts,
        "special_district_placeholders": _build_special_district_placeholders(
            effective_county_fips,
            city_context["name"] if city_context["resolution_status"] == "resolved" else None,
        ),
        "effective_county_registry_ref": effective_county_registry_ref,
        "effective_county_registry_mode": effective_county_registry_mode,
        "unresolved_questions": unresolved_questions,
        "notes": dedupe_strings(notes),
        "scores": _build_scores(
            geography_status=geography_status,
            subject_jurisdictions=subject_jurisdictions,
            effective_state_fips=effective_state_fips,
            effective_county_fips=effective_county_fips,
        ),
        "generated_at": now_iso(),
    }
    return context


def summarize_jurisdiction_context(context: dict[str, Any]) -> dict[str, Any]:
    return {
        "geography_status": context["geography_status"],
        "state_fips": context["state_fips"],
        "county_fips": context["county_fips"],
        "effective_county_registry_mode": context["effective_county_registry_mode"],
        "unresolved_questions": context["unresolved_questions"],
        "scores": context["scores"],
    }
