#!/usr/bin/env python3

"""Step 8 report-summary extraction and memo rendering for Alice AI."""

from __future__ import annotations

import json
from collections import defaultdict
from pathlib import Path
from typing import Any
from uuid import NAMESPACE_URL, uuid5

from alice_registry import load_json, source_by_id
from alice_subject_resolution import now_iso


REPORT_SUMMARY_VERSION = "alice.report_summary.v1"
SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
SCHEMAS_ROOT = SKILL_ROOT / "schemas"
REPORT_SUMMARY_SCHEMA_PATH = SCHEMAS_ROOT / "report_summary.schema.json"

SCOPE_LABELS = {
    "parcel_confirmed": "Parcel-confirmed",
    "parcel_candidate": "Parcel-candidate",
    "listing_derived": "Listing-derived",
    "county_level": "County-level",
    "geography_only": "Geography-only",
    "inferred": "Inferred",
    "unresolved": "Unresolved",
}

SOURCE_CATEGORY_ORDER = [
    "county",
    "city_local",
    "state",
    "federal",
    "listing_platform",
    "infrastructure_utility_market",
]

SOURCE_GROUP_LABELS = {
    "county": "County official sources",
    "city_local": "City / local official sources",
    "state": "State official sources",
    "federal": "Federal official sources",
    "listing_platform": "Listing and seller-facing sources",
    "infrastructure_utility_market": "Secondary infrastructure / market sources",
}

SOURCE_GROUP_NOTES = {
    "county": "Preferred for parcel, tax, and county public-record context.",
    "city_local": "Useful for city or local planning context.",
    "state": "Useful for statewide parcel, planning, or utility context.",
    "federal": "Baseline environmental and infrastructure screens.",
    "listing_platform": "Seller-facing market and listing context, not parcel confirmation by itself.",
    "infrastructure_utility_market": "Secondary context rather than parcel-confirmed factual support.",
}

SEVERITY_ORDER = {"high": 0, "medium": 1, "low": 2}
LISTING_PLATFORM_LABELS = {
    "landwatch": "LandWatch",
    "land_com": "Land.com",
    "redfin": "Redfin",
    "zillow": "Zillow",
}

MODULE_SHORT_LABELS = {
    "energy_solar": "solar",
    "energy_wind": "wind",
    "energy_battery": "battery storage",
    "agriculture_general": "agriculture",
    "residential_light_development": "residential / light development",
    "industrial_storage": "industrial / storage",
    "recreational_rural_hold": "rural hold",
}


def deterministic_uuid(*parts: str) -> str:
    return str(uuid5(NAMESPACE_URL, "|".join(parts)))


def bounded(value: float) -> float:
    return max(0.0, min(1.0, round(value, 3)))


def _workspace_alice_root(workspace_root: str | Path) -> Path:
    alice_root = Path(workspace_root) / "alice"
    alice_root.mkdir(parents=True, exist_ok=True)
    return alice_root


def _load_optional_json(path: Path) -> dict[str, Any] | None:
    return load_json(path) if path.exists() else None


def load_parcel_memo(path: str | Path) -> dict[str, Any]:
    return load_json(Path(path))


def load_report_summary(path: str | Path) -> dict[str, Any]:
    return load_json(Path(path))


def _field_value(field: dict[str, Any] | None) -> Any:
    if not isinstance(field, dict):
        return None
    return field.get("value")


def _field_status(field: dict[str, Any] | None) -> str:
    if not isinstance(field, dict):
        return "missing"
    return field.get("status") or "missing"


def _field_scope(field: dict[str, Any] | None) -> str:
    if not isinstance(field, dict):
        return "unresolved"
    return field.get("evidence_scope") or "unresolved"


def _field_confidence(field: dict[str, Any] | None) -> float:
    if not isinstance(field, dict):
        return 0.0
    raw = field.get("confidence")
    return bounded(float(raw)) if isinstance(raw, (int, float)) else 0.0


def _field_citation_ids(field: dict[str, Any] | None) -> list[str]:
    if not isinstance(field, dict):
        return []
    citation_ids = field.get("citation_ids") or []
    return sorted({citation_id for citation_id in citation_ids if citation_id})


def _citation_index(parcel_memo: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {
        citation["citation_id"]: citation
        for citation in parcel_memo.get("citations", [])
    }


def _source_name_from_id(source_id: str) -> str:
    descriptor = source_by_id().get(source_id)
    if descriptor is None:
        return source_id
    return descriptor.get("name") or source_id


def _source_names_from_ids(source_ids: list[str]) -> list[str]:
    seen: set[str] = set()
    names: list[str] = []
    for source_id in source_ids:
        if not source_id or source_id in seen:
            continue
        seen.add(source_id)
        names.append(_source_name_from_id(source_id))
    return names


def _source_ids_from_citation_ids(citation_ids: list[str], citation_index: dict[str, dict[str, Any]]) -> list[str]:
    source_ids: list[str] = []
    for citation_id in citation_ids:
        citation = citation_index.get(citation_id)
        if citation is None:
            continue
        source_id = citation.get("source_id")
        if source_id:
            source_ids.append(source_id)
    return sorted({source_id for source_id in source_ids})


def _source_names_from_citation_ids(citation_ids: list[str], citation_index: dict[str, dict[str, Any]]) -> list[str]:
    names: list[str] = []
    for source_id in _source_ids_from_citation_ids(citation_ids, citation_index):
        names.append(_source_name_from_id(source_id))
    return names


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


def _format_confidence(score: float) -> str:
    return f"{round(score * 100):.0f}%"


def _format_coordinates(value: dict[str, Any] | None) -> str | None:
    if not isinstance(value, dict):
        return None
    latitude = value.get("latitude")
    longitude = value.get("longitude")
    if not isinstance(latitude, (int, float)) or not isinstance(longitude, (int, float)):
        return None
    return f"{float(latitude):.6f}, {float(longitude):.6f}"


def _format_address(value: dict[str, Any] | str | None) -> str | None:
    if isinstance(value, dict):
        full_address = value.get("full_address")
        if isinstance(full_address, str) and full_address.strip():
            return full_address.strip()
        parts = [
            value.get("street_line1"),
            value.get("city"),
            value.get("county_name"),
            value.get("state_code"),
            value.get("postal_code"),
        ]
        compact = ", ".join(str(part).strip() for part in parts if part)
        return compact or None
    if isinstance(value, str) and value.strip():
        return value.strip()
    return None


def _format_value(value: Any) -> str | None:
    if value is None:
        return None
    if isinstance(value, bool):
        return "Yes" if value else "No"
    if isinstance(value, (int, float)):
        currency = _format_currency(value)
        if currency is not None and float(value) >= 1000:
            return currency
        return _format_number(value)
    if isinstance(value, dict):
        address_value = _format_address(value)
        if address_value is not None:
            return address_value
        coordinates = _format_coordinates(value)
        if coordinates is not None:
            return coordinates
        return json.dumps(value, sort_keys=True)
    if isinstance(value, list):
        return "; ".join(_format_value(item) or str(item) for item in value if item is not None) or None
    string_value = str(value).strip()
    return string_value or None


def _display_confirmation_level(level: str) -> str:
    return level.replace("_", " ")


def _display_confirmation_basis(basis: str | None) -> str:
    return {
        "text_corroborated": "text-corroborated",
        "local_record_corroborated": "local-record corroborated",
        "geometry_confirmed": "geometry-confirmed",
        "unknown": "unknown",
        None: "unknown",
    }[basis]


def _display_candidate_status(status: str) -> str:
    return status.replace("_", " ")


def _display_listing_platform(platform: str) -> str:
    return LISTING_PLATFORM_LABELS.get(platform, platform.replace("_", " "))


def _scope_label(scope: str) -> str:
    return SCOPE_LABELS.get(scope, scope.replace("_", " ").title())


def _ensure_sentence(text: str | None) -> str:
    sentence = (text or "").strip()
    if not sentence:
        return ""
    if sentence.endswith((".", "!", "?")):
        return sentence
    return f"{sentence}."


def _with_sources(text: str, citation_ids: list[str], citation_index: dict[str, dict[str, Any]]) -> str:
    source_names = _source_names_from_citation_ids(citation_ids, citation_index)
    if not source_names:
        return _ensure_sentence(text)
    source_text = "; ".join(source_names[:3])
    if len(source_names) > 3:
        source_text += f"; +{len(source_names) - 3} more"
    return f"{_ensure_sentence(text)} Sources: {source_text}."


def _narrative_from_field(
    label: str,
    field: dict[str, Any] | None,
    *,
    citation_index: dict[str, dict[str, Any]],
    missing_sentence: str,
    prefer_raw_string: bool = True,
    numeric_suffix: str | None = None,
) -> str:
    if _field_status(field) in {"missing", "not_applicable"} or _field_value(field) is None:
        return f"Unresolved: {_ensure_sentence(missing_sentence)}"

    scope = _field_scope(field)
    value = _field_value(field)
    if prefer_raw_string and isinstance(value, str):
        body = value
    else:
        formatted_value = _format_value(value) or "an unresolved value"
        if numeric_suffix:
            formatted_value = f"{formatted_value} {numeric_suffix}".strip()
        if scope == "listing_derived":
            body = f"The listing reports {label.lower()} as {formatted_value}"
        elif scope == "county_level":
            body = f"County-level context indicates {label.lower()} as {formatted_value}"
        elif scope == "geography_only":
            body = f"Geography-linked screening indicates {label.lower()} as {formatted_value}"
        elif scope == "inferred":
            body = f"Carried-forward context suggests {label.lower()} as {formatted_value}"
        else:
            body = f"{label} is {formatted_value}"
    return f"{_scope_label(scope)}: {_with_sources(body, _field_citation_ids(field), citation_index)}"


def _evidence_quality_label(overall_confidence: float, overall_completeness: float) -> str:
    if overall_confidence >= 0.65 and overall_completeness >= 0.45:
        return "strong_footing"
    if overall_confidence >= 0.45 and overall_completeness >= 0.25:
        return "usable_but_partial"
    if overall_confidence >= 0.3 or overall_completeness >= 0.15:
        return "directional_only"
    return "thin_screening_only"


def _subject_snapshot(
    parcel_memo: dict[str, Any],
    coverage_assessment: dict[str, Any] | None,
    jurisdiction_context: dict[str, Any] | None,
) -> dict[str, Any]:
    subject = parcel_memo["subject"]
    request_context = parcel_memo["request_context"]
    jurisdiction = parcel_memo["jurisdiction"]
    parcel_identity = parcel_memo["parcel_identity"]
    city_field = jurisdiction.get("city_or_unincorporated")
    city_or_place = None
    if isinstance(city_field, dict) and _field_status(city_field) == "confirmed":
        city_or_place = _field_value(city_field)
    return {
        "primary_subject_label": subject["primary_subject_label"],
        "request_mode": request_context["request_mode"],
        "research_unit_type": subject["research_unit_type"],
        "resolution_status": subject["resolution_status"],
        "county_name": jurisdiction["county"]["name"],
        "state_name": jurisdiction["state"]["name"],
        "state_code": _state_code_from_memo(parcel_memo, jurisdiction_context),
        "city_or_place": city_or_place,
        "parcel_count": subject["parcel_count"],
        "candidate_set_status": parcel_identity["candidate_set_status"],
        "overall_confirmation_level": parcel_identity["overall_confirmation_level"],
        "overall_confirmation_basis": parcel_identity.get("overall_confirmation_basis", "unknown"),
        "canonical_address": _format_address(_field_value(subject.get("canonical_address"))),
        "coordinates": _format_coordinates(_field_value(subject.get("coordinates"))),
    }


def _state_code_from_memo(parcel_memo: dict[str, Any], jurisdiction_context: dict[str, Any] | None) -> str | None:
    if jurisdiction_context and jurisdiction_context.get("state_code"):
        return jurisdiction_context["state_code"]
    subject_state_code = parcel_memo["subject"].get("canonical_address", {}).get("value", {}).get("state_code")
    if isinstance(subject_state_code, str) and subject_state_code:
        return subject_state_code
    raw_inputs = parcel_memo["subject"].get("raw_inputs", [])
    for raw_input in raw_inputs:
        raw_value = raw_input.get("raw_value")
        if isinstance(raw_value, str) and ", TX" in raw_value:
            return "TX"
        if isinstance(raw_value, str) and ", CA" in raw_value:
            return "CA"
    return None


def _subject_fragment(
    parcel_memo: dict[str, Any],
    parcel_candidates: dict[str, Any] | None,
    jurisdiction_context: dict[str, Any] | None,
) -> str:
    parcel_identity = parcel_memo["parcel_identity"]
    subject = parcel_memo["subject"]
    county_name = parcel_memo["jurisdiction"]["county"]["name"]
    state_code = _state_code_from_memo(parcel_memo, jurisdiction_context) or ""
    parcels = parcel_identity.get("parcels", [])
    primary_candidate = None
    primary_id = parcel_identity.get("primary_candidate_id")
    for candidate in parcels:
        if candidate.get("candidate_id") == primary_id:
            primary_candidate = candidate
            break
    if primary_candidate is None and parcels:
        primary_candidate = parcels[0]

    if parcel_identity["candidate_set_status"] == "multiple_competing_candidates":
        city_name = None
        city_field = parcel_memo["jurisdiction"].get("city_or_unincorporated")
        if isinstance(city_field, dict):
            city_name = _field_value(city_field)
        location = ", ".join(
            part for part in [city_name, county_name, state_code] if part
        ) or subject["primary_subject_label"]
        return f"Competing parcel candidates near {location}"

    if primary_candidate is not None:
        apn = _field_value(primary_candidate.get("apn"))
        if isinstance(apn, str) and parcel_identity["overall_confirmation_level"] == "parcel_confirmed":
            suffix = f", {state_code}" if state_code else ""
            return f"Parcel-confirmed APN {apn} in {county_name}{suffix}"
        address = _format_address(_field_value(primary_candidate.get("assessor_site_address")))
        if address:
            return address

    canonical_address = _format_address(_field_value(subject.get("canonical_address")))
    if canonical_address:
        return canonical_address
    return subject["primary_subject_label"]


def _title(
    parcel_memo: dict[str, Any],
    parcel_candidates: dict[str, Any] | None,
    jurisdiction_context: dict[str, Any] | None,
) -> str:
    return f"Alice Deep Research Memo: {_subject_fragment(parcel_memo, parcel_candidates, jurisdiction_context)}"


def _identity_finding(parcel_memo: dict[str, Any], citation_index: dict[str, dict[str, Any]]) -> dict[str, Any]:
    parcel_identity = parcel_memo["parcel_identity"]
    parcels = parcel_identity.get("parcels", [])
    primary_candidate = None
    for candidate in parcels:
        if candidate.get("candidate_id") == parcel_identity.get("primary_candidate_id"):
            primary_candidate = candidate
            break
    if primary_candidate is None and parcels:
        primary_candidate = parcels[0]

    if primary_candidate is None:
        summary = "Unresolved: No viable parcel candidate is currently active in the structured memo."
        return {
            "finding_id": "parcel_identity_status",
            "section": "parcel_identity",
            "label": "Parcel identity status",
            "summary": summary,
            "evidence_scope": "unresolved",
            "confidence": 0.0,
            "citation_ids": [],
            "supporting_source_ids": [],
        }

    apn_field = primary_candidate.get("apn")
    address_field = primary_candidate.get("assessor_site_address")
    apn_value = _field_value(apn_field)
    address_value = _format_address(_field_value(address_field))
    citation_ids = sorted(set(_field_citation_ids(apn_field) + _field_citation_ids(address_field)))
    scope = _field_scope(apn_field) if _field_value(apn_field) is not None else _field_scope(address_field)
    confidence = max(_field_confidence(apn_field), _field_confidence(address_field))

    if parcel_identity["candidate_set_status"] == "multiple_competing_candidates":
        alternate_apns = [
            _field_value(candidate.get("apn"))
            for candidate in parcels
            if candidate.get("candidate_id") != primary_candidate.get("candidate_id")
            and _field_value(candidate.get("apn")) is not None
        ]
        alternate_text = ""
        if alternate_apns:
            alternate_text = f" Competing APN clues remain active: {', '.join(str(apn) for apn in alternate_apns)}."
        body = (
            f"Local public-record clues strengthen primary candidate {apn_value or primary_candidate['candidate_id']}, "
            f"but parcel identity is still provisional because multiple candidates remain active.{alternate_text}"
        )
        scope = "parcel_candidate"
    elif parcel_identity["overall_confirmation_level"] == "parcel_confirmed":
        body = f"County public records corroborate active parcel candidate {apn_value or primary_candidate['candidate_id']}"
        if address_value:
            body += f" at or near {address_value}"
    elif parcel_identity["overall_confirmation_level"] == "candidate_corroborated":
        body = f"One candidate is locally corroborated around {apn_value or address_value or primary_candidate['candidate_id']}, but parcel confirmation is not final"
        scope = "parcel_candidate"
    else:
        body = f"The active subject still relies on a weak parcel candidate around {apn_value or address_value or primary_candidate['candidate_id']}"
        scope = "parcel_candidate"

    return {
        "finding_id": "parcel_identity_status",
        "section": "parcel_identity",
        "label": "Parcel identity status",
        "summary": f"{_scope_label(scope)}: {_with_sources(body, citation_ids, citation_index)}",
        "evidence_scope": scope,
        "confidence": bounded(confidence),
        "citation_ids": citation_ids,
        "supporting_source_ids": _source_ids_from_citation_ids(citation_ids, citation_index),
    }


def _planning_finding(parcel_memo: dict[str, Any], citation_index: dict[str, dict[str, Any]]) -> dict[str, Any] | None:
    planning = parcel_memo["planning_and_land_use"]
    zoning = planning["zoning_designation"]
    summary_field = planning["development_constraints_summary"]
    if _field_status(zoning) not in {"missing", "not_applicable"} and _field_value(zoning) is not None:
        summary = _narrative_from_field(
            "Zoning designation",
            zoning,
            citation_index=citation_index,
            missing_sentence="No parcel-specific zoning designation was verified.",
            prefer_raw_string=False,
        )
        citation_ids = _field_citation_ids(zoning)
        confidence = _field_confidence(zoning)
        scope = _field_scope(zoning)
    elif _field_status(summary_field) not in {"missing", "not_applicable"} and _field_value(summary_field) is not None:
        summary = _narrative_from_field(
            "Planning summary",
            summary_field,
            citation_index=citation_index,
            missing_sentence="Local planning footing is still thin.",
        )
        citation_ids = _field_citation_ids(summary_field)
        confidence = _field_confidence(summary_field)
        scope = _field_scope(summary_field)
    else:
        return None

    return {
        "finding_id": "planning_context",
        "section": "planning_and_land_use",
        "label": "Planning and land use",
        "summary": summary,
        "evidence_scope": scope,
        "confidence": bounded(confidence),
        "citation_ids": citation_ids,
        "supporting_source_ids": _source_ids_from_citation_ids(citation_ids, citation_index),
    }


def _environmental_finding(parcel_memo: dict[str, Any], citation_index: dict[str, dict[str, Any]]) -> dict[str, Any] | None:
    environmental = parcel_memo["environmental_constraints"]
    summary_field = environmental["environmental_summary"]
    if _field_status(summary_field) not in {"missing", "not_applicable"} and _field_value(summary_field) is not None:
        summary = _narrative_from_field(
            "Environmental summary",
            summary_field,
            citation_index=citation_index,
            missing_sentence="No defensible environmental summary is available.",
        )
        citation_ids = _field_citation_ids(summary_field)
        confidence = _field_confidence(summary_field)
        scope = _field_scope(summary_field)
    else:
        flood = environmental["flood_zone"]
        if _field_status(flood) in {"missing", "not_applicable"} or _field_value(flood) is None:
            return None
        summary = _narrative_from_field(
            "Flood screening",
            flood,
            citation_index=citation_index,
            missing_sentence="Flood screening remains unresolved.",
        )
        citation_ids = _field_citation_ids(flood)
        confidence = _field_confidence(flood)
        scope = _field_scope(flood)

    return {
        "finding_id": "environmental_baseline",
        "section": "environmental_constraints",
        "label": "Environmental baseline",
        "summary": summary,
        "evidence_scope": scope,
        "confidence": bounded(confidence),
        "citation_ids": citation_ids,
        "supporting_source_ids": _source_ids_from_citation_ids(citation_ids, citation_index),
    }


def _water_finding(parcel_memo: dict[str, Any], citation_index: dict[str, dict[str, Any]]) -> dict[str, Any] | None:
    water = parcel_memo["water_and_agriculture"]
    summary_field = water["water_and_ag_summary"]
    if _field_status(summary_field) not in {"missing", "not_applicable"} and _field_value(summary_field) is not None:
        field = summary_field
    else:
        field = water["soil_productivity_signal"]
        if _field_status(field) in {"missing", "not_applicable"} or _field_value(field) is None:
            return None

    summary = _narrative_from_field(
        "Water and agriculture",
        field,
        citation_index=citation_index,
        missing_sentence="Water and agriculture context remains under-resolved.",
    )
    citation_ids = _field_citation_ids(field)
    return {
        "finding_id": "water_ag_context",
        "section": "water_and_agriculture",
        "label": "Water and agriculture",
        "summary": summary,
        "evidence_scope": _field_scope(field),
        "confidence": bounded(_field_confidence(field)),
        "citation_ids": citation_ids,
        "supporting_source_ids": _source_ids_from_citation_ids(citation_ids, citation_index),
    }


def _infrastructure_finding(parcel_memo: dict[str, Any], citation_index: dict[str, dict[str, Any]]) -> dict[str, Any] | None:
    infrastructure = parcel_memo["infrastructure_and_utilities"]
    summary_field = infrastructure["infrastructure_summary"]
    if _field_status(summary_field) not in {"missing", "not_applicable"} and _field_value(summary_field) is not None:
        field = summary_field
        label = "Infrastructure summary"
        missing_sentence = "Infrastructure footing is still thin."
    else:
        field = infrastructure["legal_or_physical_access_signal"]
        if _field_status(field) in {"missing", "not_applicable"} or _field_value(field) is None:
            return None
        label = "Access signal"
        missing_sentence = "Access remains unresolved."
    summary = _narrative_from_field(
        label,
        field,
        citation_index=citation_index,
        missing_sentence=missing_sentence,
    )
    citation_ids = _field_citation_ids(field)
    return {
        "finding_id": "infrastructure_context",
        "section": "infrastructure_and_utilities",
        "label": "Infrastructure and utilities",
        "summary": summary,
        "evidence_scope": _field_scope(field),
        "confidence": bounded(_field_confidence(field)),
        "citation_ids": citation_ids,
        "supporting_source_ids": _source_ids_from_citation_ids(citation_ids, citation_index),
    }


def _market_finding(parcel_memo: dict[str, Any], citation_index: dict[str, dict[str, Any]]) -> dict[str, Any] | None:
    listing = parcel_memo["listing"]
    market = parcel_memo["market_signals"]
    asking_price = _field_value(listing["asking_price"])
    acreage = _field_value(listing["listed_acreage"])
    ask_per_acre = _field_value(market["ask_price_per_acre"])
    platform = _field_value(listing["platform"]) or "listing"
    platform_label = _display_listing_platform(platform)
    citation_ids = sorted(
        set(
            _field_citation_ids(listing["asking_price"])
            + _field_citation_ids(listing["listed_acreage"])
            + _field_citation_ids(market["ask_price_per_acre"])
            + _field_citation_ids(listing["listing_text_summary"])
        )
    )

    if isinstance(asking_price, (int, float)) and isinstance(acreage, (int, float)):
        body = (
            f"The {platform_label} listing asks {_format_currency(float(asking_price))} "
            f"for {_format_number(float(acreage))} acres"
        )
        if isinstance(ask_per_acre, (int, float)):
            body += f", or about {_format_currency(float(ask_per_acre))} per acre"
        summary = f"Listing-derived: {_with_sources(body, citation_ids, citation_index)}"
        confidence = max(
            _field_confidence(listing["asking_price"]),
            _field_confidence(listing["listed_acreage"]),
            _field_confidence(market["ask_price_per_acre"]),
        )
    else:
        listing_text = listing["listing_text_summary"]
        if _field_status(listing_text) in {"missing", "not_applicable"} or _field_value(listing_text) is None:
            return None
        summary = _narrative_from_field(
            "Listing text",
            listing_text,
            citation_index=citation_index,
            missing_sentence="Listing context remains thin.",
        )
        confidence = _field_confidence(listing_text)
        citation_ids = _field_citation_ids(listing_text)

    return {
        "finding_id": "listing_market_context",
        "section": "market_signals",
        "label": "Listing and market context",
        "summary": summary,
        "evidence_scope": "listing_derived",
        "confidence": bounded(confidence),
        "citation_ids": citation_ids,
        "supporting_source_ids": _source_ids_from_citation_ids(citation_ids, citation_index),
    }


def _module_findings(parcel_memo: dict[str, Any], citation_index: dict[str, dict[str, Any]]) -> list[dict[str, Any]]:
    findings: list[dict[str, Any]] = []
    for module in parcel_memo.get("use_case_modules", []):
        if module["module_status"] == "insufficient_data" and module["fit_assessment"] == "unknown":
            continue
        scope = module["evidence_scope_summary"]["dominant_scope"]
        fit_label = module["fit_assessment"].replace("_", " ")
        status_label = module["module_status"].replace("_", " ")
        body = (
            f"{module['module_label']} is {fit_label} at screening grade ({status_label}). "
            f"{module['summary']}"
        )
        summary = (
            f"{_scope_label(scope)}: "
            f"{_with_sources(body, module['citation_ids'], citation_index)}"
        )
        findings.append(
            {
                "finding_id": f"use_case_{module['module_id']}",
                "section": "use_case_modules",
                "label": module["module_label"],
                "summary": summary,
                "evidence_scope": scope,
                "confidence": bounded(float(module["confidence"])),
                "citation_ids": module["citation_ids"],
                "supporting_source_ids": _source_ids_from_citation_ids(module["citation_ids"], citation_index),
            }
        )
    return findings[:2]


def _top_findings(parcel_memo: dict[str, Any], citation_index: dict[str, dict[str, Any]]) -> list[dict[str, Any]]:
    findings: list[dict[str, Any]] = []
    identity = _identity_finding(parcel_memo, citation_index)
    if identity is not None:
        findings.append(identity)
    findings.extend(_module_findings(parcel_memo, citation_index))
    for builder in (
        _planning_finding,
        _environmental_finding,
        _water_finding,
        _infrastructure_finding,
        _market_finding,
    ):
        finding = builder(parcel_memo, citation_index)
        if finding is not None:
            findings.append(finding)
    return findings[:6]


def _warnings(
    parcel_memo: dict[str, Any],
    coverage_assessment: dict[str, Any] | None,
    jurisdiction_context: dict[str, Any] | None,
) -> list[dict[str, Any]]:
    warnings: list[dict[str, Any]] = []
    parcel_identity = parcel_memo["parcel_identity"]
    request_mode = parcel_memo["request_context"]["request_mode"]
    if parcel_identity["candidate_set_status"] == "multiple_competing_candidates":
        warnings.append(
            {
                "warning_type": "competing_candidates",
                "severity": "high",
                "message": "Multiple parcel candidates remain active, so parcel-specific conclusions should stay provisional.",
            }
        )
    elif parcel_identity["overall_confirmation_level"] != "parcel_confirmed":
        warnings.append(
            {
                "warning_type": "parcel_identity_unconfirmed",
                "severity": "high",
                "message": "Parcel identity is not yet parcel-confirmed and remains vulnerable to mis-attachment.",
            }
        )

    if coverage_assessment is not None:
        if coverage_assessment.get("county_registry_mode") == "generated_fallback":
            warnings.append(
                {
                    "warning_type": "fallback_county",
                    "severity": "medium",
                    "message": "The effective county uses a generated fallback registry entry rather than a curated override.",
                }
            )
        if coverage_assessment.get("source_summary", {}).get("only_federal_baseline_available"):
            warnings.append(
                {
                    "warning_type": "federal_baseline_only",
                    "severity": "high",
                    "message": "Only federal baseline plus listing context is currently available for this memo.",
                }
            )
        if coverage_assessment.get("effective_coverage_tier") == "minimal":
            warnings.append(
                {
                    "warning_type": "thin_local_coverage",
                    "severity": "medium",
                    "message": "Local county footing is thin, so the memo is screening-grade rather than locally complete.",
                }
            )
        if "zoning" in coverage_assessment.get("missing_surfaces", []):
            warnings.append(
                {
                    "warning_type": "missing_zoning",
                    "severity": "medium",
                    "message": "Parcel-specific zoning remains unresolved in the current evidence set.",
                }
            )

    environmental_summary = parcel_memo["environmental_constraints"]["environmental_summary"]
    if _field_scope(environmental_summary) == "geography_only":
        warnings.append(
            {
                "warning_type": "geography_only_findings",
                "severity": "medium",
                "message": "Key environmental signals are point- or geography-linked rather than parcel-boundary confirmed.",
            }
        )

    if request_mode == "follow_up":
        warnings.append(
            {
                "warning_type": "follow_up_inherited_context",
                "severity": "low",
                "message": "This memo inherits subject context from prior Alice session state before adding new Step 7 evidence.",
            }
        )

    market_summary = parcel_memo["market_signals"]["market_summary"]
    if parcel_memo["listing"]["listing_present"] and _field_scope(market_summary) == "listing_derived":
        warnings.append(
            {
                "warning_type": "listing_dominant_context",
                "severity": "low",
                "message": "Market context remains dominated by seller-facing listing information rather than broader comps.",
            }
        )

    deduped: list[dict[str, Any]] = []
    seen: set[tuple[str, str]] = set()
    for warning in warnings:
        key = (warning["warning_type"], warning["message"])
        if key in seen:
            continue
        seen.add(key)
        deduped.append(warning)
    deduped.sort(key=lambda item: (SEVERITY_ORDER[item["severity"]], item["warning_type"]))
    return deduped


def _top_risks(parcel_memo: dict[str, Any]) -> list[dict[str, Any]]:
    risks = list(parcel_memo.get("risks", []))
    risks.sort(key=lambda item: (not item.get("blocking", False), SEVERITY_ORDER.get(item["severity"], 99), item["label"]))
    return [
        {
            "risk_id": risk["risk_id"],
            "label": risk["label"],
            "severity": risk["severity"],
            "category": risk["category"],
            "summary": _ensure_sentence(risk["description"]),
            "blocking": bool(risk.get("blocking")),
            "citation_ids": sorted({citation_id for citation_id in risk.get("citation_ids", []) if citation_id}),
        }
        for risk in risks[:4]
    ]


def _top_unknowns(parcel_memo: dict[str, Any]) -> list[dict[str, Any]]:
    unknowns = list(parcel_memo.get("unknowns", []))
    unknowns.sort(key=lambda item: (not item.get("blocking", False), item["question"]))
    return [
        {
            "unknown_id": unknown["unknown_id"],
            "question": unknown["question"],
            "why_it_matters": unknown["why_it_matters"],
            "blocking": bool(unknown.get("blocking")),
            "recommended_next_source": unknown.get("recommended_next_source"),
        }
        for unknown in unknowns[:4]
    ]


def _next_actions(parcel_memo: dict[str, Any]) -> list[dict[str, Any]]:
    actions = sorted(parcel_memo.get("next_actions", []), key=lambda item: item.get("priority", 999))
    return [
        {
            "priority": int(action["priority"]),
            "action": action["action"],
            "reason": action["reason"],
        }
        for action in actions[:5]
    ]


def _source_groups(parcel_memo: dict[str, Any]) -> list[dict[str, Any]]:
    grouped: dict[str, dict[str, Any]] = defaultdict(lambda: {"source_ids": set(), "source_names": set()})
    for citation in parcel_memo.get("citations", []):
        category = citation["source_category"]
        bucket = grouped[category]
        bucket["source_ids"].add(citation["source_id"])
        bucket["source_names"].add(citation["source_name"])

    groups: list[dict[str, Any]] = []
    for category in SOURCE_CATEGORY_ORDER:
        bucket = grouped.get(category)
        if not bucket:
            continue
        groups.append(
            {
                "group_id": f"source_group_{category}",
                "label": SOURCE_GROUP_LABELS[category],
                "source_category": category,
                "authority_level": "official" if category in {"county", "city_local", "state", "federal"} else "secondary",
                "source_count": len(bucket["source_ids"]),
                "source_ids": sorted(bucket["source_ids"]),
                "source_names": sorted(bucket["source_names"]),
                "notes": SOURCE_GROUP_NOTES[category],
            }
        )
    return groups


def _one_line_conclusion(
    parcel_memo: dict[str, Any],
    coverage_assessment: dict[str, Any] | None,
) -> str:
    parcel_identity = parcel_memo["parcel_identity"]
    county_name = parcel_memo["jurisdiction"]["county"]["name"]
    overall_confirmation = parcel_identity["overall_confirmation_level"]
    candidate_set_status = parcel_identity["candidate_set_status"]
    only_federal = bool(
        coverage_assessment
        and coverage_assessment.get("source_summary", {}).get("only_federal_baseline_available")
    )
    missing_surfaces = set((coverage_assessment or {}).get("missing_surfaces", []))
    request_mode = parcel_memo["request_context"]["request_mode"]
    modules = [
        module
        for module in parcel_memo.get("use_case_modules", [])
        if not (module["module_status"] == "insufficient_data" and module["fit_assessment"] == "unknown")
    ]

    def module_label(module: dict[str, Any]) -> str:
        return MODULE_SHORT_LABELS.get(module["module_id"], module["module_label"].replace(" Screening", "").lower())

    def module_screen_phrase(module: dict[str, Any]) -> str:
        return f"{module_label(module)} screening"

    def module_fit_phrase(module: dict[str, Any]) -> str:
        article = "an" if module["fit_assessment"].startswith(("a", "e", "i", "o", "u")) else "a"
        return f"{article} {module['fit_assessment'].replace('_', ' ')} {module_label(module)} screen"

    if candidate_set_status == "multiple_competing_candidates":
        if modules:
            top_module = modules[0]
            weak_modules = [module for module in modules if module["fit_assessment"] == "weak"]
            if weak_modules:
                return (
                    f"{county_name} screening keeps {module_fit_phrase(top_module)}, while {module_screen_phrase(weak_modules[0])} stays weak "
                    "because competing parcel candidates and local compatibility questions remain active."
                )
        return (
            f"Local Kern County clues strengthen one parcel candidate, but competing candidates remain active, "
            "so parcel-specific conclusions should stay provisional."
        )
    if only_federal:
        if modules:
            return (
                f"{module_screen_phrase(modules[0]).capitalize()} stays directional because parcel identity is still weak and only federal baseline plus listing context are available in {county_name}."
            )
        return (
            f"Only federal baseline plus listing context is available for {county_name}, "
            "so this memo is useful for screening but not parcel-level conclusions."
        )
    if overall_confirmation == "parcel_confirmed" and request_mode == "follow_up":
        if modules:
            module_labels = ", ".join(module_fit_phrase(module) for module in modules[:2])
            if "zoning" in missing_surfaces or any(module["module_id"] == "agriculture_general" for module in modules):
                return (
                    f"The follow-up stays anchored to a parcel-confirmed {county_name} subject and now shows {module_labels}, "
                    "but zoning and water-related diligence still remain incomplete."
                )
        if "zoning" in missing_surfaces:
            return (
                f"The follow-up reuses a parcel-confirmed {county_name} subject and adds usable baseline screening, "
                "but zoning still remains unresolved."
            )
        return (
            f"The follow-up stays anchored to a parcel-confirmed {county_name} subject and materially improves the evidence footing."
        )
    if overall_confirmation == "parcel_confirmed":
        if modules:
            positive_modules = [module for module in modules if module["fit_assessment"] in {"favorable", "mixed"}]
            if positive_modules:
                if "zoning" in missing_surfaces:
                    return (
                        f"One parcel-confirmed {county_name} candidate supports {module_fit_phrase(positive_modules[0])}, "
                        "but zoning and boundary-based environmental checks remain incomplete."
                    )
                return (
                    f"One parcel-confirmed {county_name} candidate supports {module_fit_phrase(positive_modules[0])}."
                )
        if "zoning" in missing_surfaces:
            return (
                f"County public records support one parcel-confirmed {county_name} candidate, "
                "but zoning and boundary-based environmental checks remain incomplete."
            )
        return f"County public records support one parcel-confirmed {county_name} candidate."
    if overall_confirmation == "candidate_corroborated":
        if modules:
            return (
                f"One {county_name} candidate supports {module_fit_phrase(modules[0])}, "
                "but parcel identity is still not stable enough for full parcel-level conclusions."
            )
        return (
            f"One {county_name} candidate is locally corroborated, but parcel identity is still not stable enough "
            "for full parcel-level conclusions."
        )
    if modules:
        return (
            f"The active subject in {county_name} only supports {module_fit_phrase(modules[0])}, "
            "so Alice should treat the current memo as directional rather than parcel-confirmed."
        )
    return (
        f"The active subject in {county_name} remains only weakly resolved, so Alice should treat the current memo "
        "as directional rather than parcel-confirmed."
    )


def _executive_summary(
    parcel_memo: dict[str, Any],
    report_summary: dict[str, Any],
) -> list[str]:
    evidence_panel = report_summary["evidence_quality_panel"]
    warnings = report_summary["warnings"]
    bullets: list[str] = []

    if report_summary["top_findings"]:
        bullets.append(report_summary["top_findings"][0]["summary"])
    if len(report_summary["top_findings"]) > 1:
        bullets.append(report_summary["top_findings"][1]["summary"])

    coverage_bullet = (
        f"Coverage: {evidence_panel['coverage_tier']} tier, "
        f"{evidence_panel['evidence_quality_label'].replace('_', ' ')}, "
        f"confidence {_format_confidence(evidence_panel['overall_confidence'])}, "
        f"completeness {_format_confidence(evidence_panel['overall_completeness'])}."
    )
    bullets.append(coverage_bullet)

    if warnings:
        bullets.append(f"Warning: {warnings[0]['message']}")
    elif report_summary["top_risks"]:
        bullets.append(f"Primary risk: {report_summary['top_risks'][0]['summary']}")

    if report_summary["next_actions"]:
        next_action = report_summary["next_actions"][0]
        bullets.append(f"Next action: {next_action['reason']}")

    deduped: list[str] = []
    seen: set[str] = set()
    for bullet in bullets:
        if bullet in seen:
            continue
        seen.add(bullet)
        deduped.append(_ensure_sentence(bullet))
    return deduped[:5]


def build_report_summary(
    parcel_memo: dict[str, Any],
    *,
    parcel_candidates: dict[str, Any] | None = None,
    research_assembly_summary: dict[str, Any] | None = None,
    coverage_assessment: dict[str, Any] | None = None,
    jurisdiction_context: dict[str, Any] | None = None,
    render_mode: str = "generic_summary",
    generated_at: str | None = None,
) -> dict[str, Any]:
    citation_index = _citation_index(parcel_memo)
    summary_generated_at = generated_at or now_iso()
    subject_snapshot = _subject_snapshot(parcel_memo, coverage_assessment, jurisdiction_context)
    overall_confidence = bounded(parcel_memo["scores"]["overall_confidence"])
    overall_completeness = bounded(parcel_memo["scores"]["overall_completeness"])
    evidence_panel = {
        "coverage_tier": (
            coverage_assessment.get("effective_coverage_tier")
            if coverage_assessment is not None
            else parcel_memo["scores"]["coverage_tier"]
        ),
        "local_footing_status": coverage_assessment.get("local_footing_status") if coverage_assessment else None,
        "suitability_for_deep_research": coverage_assessment.get("suitability_for_deep_research") if coverage_assessment else None,
        "overall_confidence": overall_confidence,
        "overall_completeness": overall_completeness,
        "evidence_quality_label": _evidence_quality_label(overall_confidence, overall_completeness),
        "citation_count": len(parcel_memo.get("citations", [])),
        "risk_count": len(parcel_memo.get("risks", [])),
        "unknown_count": len(parcel_memo.get("unknowns", [])),
        "official_source_count": (
            coverage_assessment.get("source_summary", {}).get("official_source_count", 0)
            if coverage_assessment is not None
            else len(
                [
                    citation
                    for citation in parcel_memo.get("citations", [])
                    if citation["source_category"] in {"county", "city_local", "state", "federal"}
                ]
            )
        ),
        "listing_source_count": (
            len(coverage_assessment.get("source_summary", {}).get("listing_platform_source_ids", []))
            if coverage_assessment is not None
            else len(
                [
                    citation
                    for citation in parcel_memo.get("citations", [])
                    if citation["source_category"] == "listing_platform"
                ]
            )
        ),
        "only_federal_baseline_available": bool(
            coverage_assessment
            and coverage_assessment.get("source_summary", {}).get("only_federal_baseline_available")
        ),
        "notes": list((coverage_assessment or {}).get("notes", [])[:3]),
    }
    title = _title(parcel_memo, parcel_candidates, jurisdiction_context)
    top_findings = _top_findings(parcel_memo, citation_index)
    warnings = _warnings(parcel_memo, coverage_assessment, jurisdiction_context)
    summary: dict[str, Any] = {
        "schema_version": REPORT_SUMMARY_VERSION,
        "report_summary_id": deterministic_uuid(parcel_memo["object_id"], render_mode, "report_summary"),
        "request_id": parcel_memo["request_context"]["request_id"],
        "land_research_object_id": parcel_memo["object_id"],
        "parcel_candidates_id": (
            parcel_candidates.get("parcel_candidates_id")
            if parcel_candidates is not None
            else None
        ),
        "render_mode": render_mode,
        "generated_at": summary_generated_at,
        "title": title,
        "one_line_conclusion": _ensure_sentence(_one_line_conclusion(parcel_memo, coverage_assessment)),
        "subject_snapshot": subject_snapshot,
        "evidence_quality_panel": evidence_panel,
        "top_findings": top_findings,
        "top_risks": _top_risks(parcel_memo),
        "top_unknowns": _top_unknowns(parcel_memo),
        "next_actions": _next_actions(parcel_memo),
        "source_groups": _source_groups(parcel_memo),
        "warnings": warnings,
        "channel_hints": {
            "canonical_markdown_artifact_ref": "alice/report.md",
            "slack_artifact_ref": "alice/report_slack.txt",
            "email_artifact_ref": "alice/report_email.txt",
            "recommended_email_subject": title,
            "recommended_slack_title": title,
            "recommended_slack_bullet_limit": 6,
            "primary_delivery_channel": parcel_memo["request_context"].get("channel"),
        },
    }
    summary["executive_summary"] = _executive_summary(parcel_memo, summary)
    if research_assembly_summary is not None and research_assembly_summary.get("citation_count") is not None:
        summary["evidence_quality_panel"]["citation_count"] = int(research_assembly_summary["citation_count"])
    return summary


def _summary_line_list(items: list[str]) -> list[str]:
    return [f"- {item}" for item in items]


def _bulletize_lines(lines: list[str]) -> list[str]:
    bulletized: list[str] = []
    seen: set[str] = set()
    for line in lines:
        normalized = line.strip()
        if normalized and normalized in seen:
            continue
        if normalized:
            seen.add(normalized)
        if not line:
            bulletized.append(line)
        elif line.startswith("- "):
            bulletized.append(line)
        else:
            bulletized.append(f"- {line}")
    return bulletized


def _render_subject_snapshot(report_summary: dict[str, Any]) -> list[str]:
    snapshot = report_summary["subject_snapshot"]
    lines = [
        f"- Subject: {snapshot['primary_subject_label']}",
        f"- Mode: {snapshot['request_mode']}",
        f"- Research unit: {snapshot['research_unit_type'].replace('_', ' ')}",
        f"- Resolution: {snapshot['resolution_status'].replace('_', ' ')}",
        (
            f"- Jurisdiction: {', '.join(part for part in [snapshot.get('city_or_place'), snapshot.get('county_name'), snapshot.get('state_code')] if part)}"
            if any([snapshot.get("city_or_place"), snapshot.get("county_name"), snapshot.get("state_code")])
            else "- Jurisdiction: unresolved"
        ),
        f"- Parcel count: {snapshot['parcel_count']}",
        f"- Candidate status: {_display_candidate_status(snapshot['candidate_set_status'])}",
        f"- Confirmation level: {_display_confirmation_level(snapshot['overall_confirmation_level'])}",
    ]
    if snapshot.get("overall_confirmation_basis") and snapshot["overall_confirmation_basis"] != "unknown":
        lines.append(
            f"- Confirmation basis: {_display_confirmation_basis(snapshot['overall_confirmation_basis'])}"
        )
    if snapshot.get("canonical_address"):
        lines.append(f"- Address hint: {snapshot['canonical_address']}")
    if snapshot.get("coordinates"):
        lines.append(f"- Coordinates hint: {snapshot['coordinates']}")
    return lines


def _render_evidence_panel(report_summary: dict[str, Any]) -> list[str]:
    panel = report_summary["evidence_quality_panel"]
    lines = [
        f"- Coverage tier: {panel['coverage_tier']}",
        f"- Local footing: {panel['local_footing_status'] or 'not stated'}",
        f"- Deep research readiness: {panel['suitability_for_deep_research'] or 'not stated'}",
        f"- Overall confidence: {_format_confidence(panel['overall_confidence'])}",
        f"- Overall completeness: {_format_confidence(panel['overall_completeness'])}",
        f"- Evidence quality label: {panel['evidence_quality_label'].replace('_', ' ')}",
        f"- Source mix: {panel['official_source_count']} official source(s), {panel['listing_source_count']} listing source(s), {panel['citation_count']} citation(s)",
    ]
    if panel["only_federal_baseline_available"]:
        lines.append("- Coverage note: Only federal baseline plus listing context is currently available.")
    for note in panel.get("notes", [])[:3]:
        lines.append(f"- Note: {note}")
    return lines


def _render_jurisdiction_lines(
    parcel_memo: dict[str, Any],
    report_summary: dict[str, Any],
) -> list[str]:
    jurisdiction = parcel_memo["jurisdiction"]
    lines = [
        f"- County: {jurisdiction['county']['name']} ({jurisdiction['county']['fips']})",
        f"- State: {jurisdiction['state']['name']} ({jurisdiction['state']['fips']})",
        _narrative_from_field(
            "City or place",
            jurisdiction["city_or_unincorporated"],
            citation_index=_citation_index(parcel_memo),
            missing_sentence="City or unincorporated status remains unresolved.",
        ),
    ]
    lines.append(f"- Coverage tier in effect: {report_summary['evidence_quality_panel']['coverage_tier']}")
    lines.append(f"- {parcel_memo['parcel_identity']['boundary_notes']}")
    return _bulletize_lines(lines)


def _candidate_lines(parcel_memo: dict[str, Any]) -> list[str]:
    parcel_identity = parcel_memo["parcel_identity"]
    lines = [
        f"- Candidate set status: {_display_candidate_status(parcel_identity['candidate_set_status'])}",
        f"- Candidate strategy: {parcel_identity['candidate_strategy'].replace('_', ' ')}",
        f"- Overall confirmation: {_display_confirmation_level(parcel_identity['overall_confirmation_level'])}",
    ]
    if parcel_identity.get("overall_confirmation_basis") and parcel_identity["overall_confirmation_basis"] != "unknown":
        lines.append(
            f"- Overall confirmation basis: {_display_confirmation_basis(parcel_identity['overall_confirmation_basis'])}"
        )
    for parcel in parcel_identity.get("parcels", []):
        apn = _field_value(parcel.get("apn"))
        address = _format_address(_field_value(parcel.get("assessor_site_address")))
        label = apn or address or parcel["candidate_id"]
        support_names = _source_names_from_ids(parcel.get("supporting_source_ids", []))
        contradiction_names = _source_names_from_ids(parcel.get("contradicting_source_ids", []))
        body = (
            f"{label}: {parcel['candidate_strength']} / {_display_confirmation_level(parcel['confirmation_level'])}; "
            f"geometry {parcel['geometry_status'].replace('_', ' ')}"
        )
        if parcel.get("confirmation_basis") and parcel["confirmation_basis"] != "unknown":
            body += f"; basis {_display_confirmation_basis(parcel['confirmation_basis'])}"
        if support_names:
            body += f"; supported by {', '.join(support_names)}"
        if contradiction_names:
            body += f"; contradicted by {', '.join(contradiction_names)}"
        lines.append(f"- {body}.")
    for conflict in parcel_identity.get("identity_conflicts", []):
        lines.append(f"- Conflict: {conflict['description']}")
    return lines


def _render_compact_field(
    label: str,
    field: dict[str, Any] | None,
    *,
    citation_index: dict[str, dict[str, Any]],
) -> str:
    if _field_status(field) in {"missing", "not_applicable"}:
        notes = field.get("notes") if isinstance(field, dict) else None
        return f"- {label}: Unresolved: {_ensure_sentence(notes or 'No supporting evidence is attached yet.')}"
    value_text = (
        _format_value(_field_value(field))
        or (field.get("notes") if isinstance(field, dict) else None)
    )
    if value_text is None:
        value_text = field.get("notes") if isinstance(field, dict) else "No supporting value is attached."
    scope = _field_scope(field)
    return (
        f"- {label}: {_scope_label(scope)}: "
        f"{_with_sources(str(value_text), _field_citation_ids(field), citation_index)}"
    )


def _render_use_case_modules(parcel_memo: dict[str, Any]) -> list[str]:
    citation_index = _citation_index(parcel_memo)
    lines: list[str] = []
    modules = parcel_memo.get("use_case_modules", [])
    if not modules:
        return ["- No thesis-aware screening modules were populated."]

    for module in modules:
        fit_label = module["fit_assessment"].replace("_", " ")
        status_label = module["module_status"].replace("_", " ")
        scope_summary = module["evidence_scope_summary"]
        supporting_scopes = ", ".join(
            scope.replace("_", " ") for scope in scope_summary.get("supporting_scopes", [])
        )
        lines.extend(
            [
                f"### {module['module_label']}",
                f"- Fit assessment: {fit_label}",
                f"- Module status: {status_label}",
                f"- Confidence: {_format_confidence(float(module['confidence']))}",
                (
                    f"- Evidence footing: dominant {scope_summary['dominant_scope'].replace('_', ' ')}; "
                    f"supporting {supporting_scopes}."
                    if supporting_scopes
                    else f"- Evidence footing: dominant {scope_summary['dominant_scope'].replace('_', ' ')}."
                ),
                f"- Summary: {_with_sources(module['summary'], module['citation_ids'], citation_index)}",
            ]
        )
        for signal in module.get("supporting_signals", [])[:3]:
            lines.append(f"- Supporting signal: {signal}")
        for blocker in module.get("blocking_flags", [])[:3]:
            lines.append(f"- Blocking flag: {blocker}")
        for unknown in module.get("key_unknowns", [])[:3]:
            lines.append(f"- Unknown: {unknown}")
        for requirement in module.get("economics_inputs_required", [])[:3]:
            lines.append(f"- Economics input still needed: {requirement}")
        if scope_summary.get("notes"):
            lines.append(f"- Scope note: {scope_summary['notes']}")
        lines.append("")
    return lines[:-1] if lines and lines[-1] == "" else lines


def _render_directional_economics(parcel_memo: dict[str, Any]) -> list[str]:
    citation_index = _citation_index(parcel_memo)
    economics = parcel_memo["directional_economics"]
    modules_by_id = {
        module["module_id"]: module["module_label"]
        for module in parcel_memo.get("use_case_modules", [])
    }
    lines = [
        f"- Status: {economics['status'].replace('_', ' ')}",
        f"- Basis: {economics['basis']}",
    ]
    if economics.get("active_module_ids"):
        labels = [modules_by_id.get(module_id, module_id) for module_id in economics["active_module_ids"]]
        lines.append(f"- Active modules: {', '.join(labels)}")
    for assumption in economics.get("assumptions", []):
        lines.append(
            f"- Assumption: {assumption['name']} = {assumption['value']} ({assumption['source'].replace('_', ' ')})"
        )
    for key, field in economics.get("carry_costs", {}).items():
        label = key.replace("_", " ").title()
        lines.append(_render_compact_field(label, field, citation_index=citation_index))
    for key, field in economics.get("improvement_capex_proxies", {}).items():
        label = key.replace("_", " ").title()
        lines.append(_render_compact_field(label, field, citation_index=citation_index))
    for scenario in economics.get("revenue_or_exit_cases", []):
        lines.append(
            f"- Scenario case: {scenario['scenario_name']} for `{scenario['module_id']}`. "
            f"{scenario['description']} Inputs: {', '.join(scenario['key_inputs'])}."
        )
        if scenario.get("notes"):
            lines.append(f"- Scenario note: {scenario['notes']}")
    for summary in economics.get("scenario_summary", []):
        lines.append(f"- Summary: {summary}")
    for limitation in economics.get("limitations", []):
        lines.append(f"- Limitation: {limitation}")
    return lines


def _section_lines(parcel_memo: dict[str, Any], section_name: str) -> list[str]:
    citation_index = _citation_index(parcel_memo)
    if section_name == "planning_and_land_use":
        planning = parcel_memo["planning_and_land_use"]
        lines = [
            _narrative_from_field(
                "Zoning designation",
                planning["zoning_designation"],
                citation_index=citation_index,
                missing_sentence="No parcel-specific zoning designation was verified in the current evidence set.",
                prefer_raw_string=False,
            ),
            _narrative_from_field(
                "Planning summary",
                planning["development_constraints_summary"],
                citation_index=citation_index,
                missing_sentence="No strong planning summary was recovered beyond page-access signals.",
            ),
        ]
        for limit in planning.get("interpretation_limits", [])[:2]:
            lines.append(f"- Interpretation limit: {limit}")
        return _bulletize_lines(lines)

    if section_name == "environmental_constraints":
        environmental = parcel_memo["environmental_constraints"]
        lines = [
            _narrative_from_field(
                "Environmental summary",
                environmental["environmental_summary"],
                citation_index=citation_index,
                missing_sentence="No defensible parcel-specific environmental summary was recovered.",
            )
        ]
        if _field_status(environmental["environmental_summary"]) in {"missing", "not_applicable"}:
            for field_name, label, missing_sentence in (
                ("flood_zone", "Flood screening", "Flood screening remains unresolved."),
                ("wetlands_signal", "Wetlands screening", "Wetlands screening remains unresolved."),
                ("elevation_signal", "Elevation signal", "Elevation remains unresolved."),
                ("soil_constraints_signal", "Soil constraints", "Soil constraints remain unresolved."),
            ):
                lines.append(
                    _narrative_from_field(
                        label,
                        environmental[field_name],
                        citation_index=citation_index,
                        missing_sentence=missing_sentence,
                    )
                )
        for note in environmental.get("regulatory_notes", [])[:1]:
            lines.append(f"- Regulatory note: {note}")
        return _bulletize_lines(lines)

    if section_name == "water_and_agriculture":
        water = parcel_memo["water_and_agriculture"]
        lines = [
            _narrative_from_field(
                "Water and agriculture summary",
                water["water_and_ag_summary"],
                citation_index=citation_index,
                missing_sentence="Water and agriculture remain under-resolved in the current Alice evidence set.",
            ),
            _narrative_from_field(
                "Soil productivity signal",
                water["soil_productivity_signal"],
                citation_index=citation_index,
                missing_sentence="No soil productivity signal was recovered.",
            ),
        ]
        return _bulletize_lines(lines)

    if section_name == "infrastructure_and_utilities":
        infrastructure = parcel_memo["infrastructure_and_utilities"]
        lines = [
            _narrative_from_field(
                "Infrastructure summary",
                infrastructure["infrastructure_summary"],
                citation_index=citation_index,
                missing_sentence="No defensible infrastructure summary was recovered.",
            ),
            _narrative_from_field(
                "Access signal",
                infrastructure["legal_or_physical_access_signal"],
                citation_index=citation_index,
                missing_sentence="Legal or physical access remains unresolved.",
            ),
        ]
        return _bulletize_lines(lines)

    if section_name == "market_signals":
        listing = parcel_memo["listing"]
        market = parcel_memo["market_signals"]
        lines = [
            _narrative_from_field(
                "Market summary",
                market["market_summary"],
                citation_index=citation_index,
                missing_sentence="No ask-per-acre or broader market summary is currently available.",
            ),
            _narrative_from_field(
                "Listing text",
                listing["listing_text_summary"],
                citation_index=citation_index,
                missing_sentence="No listing text summary is currently available.",
            ),
        ]
        asking_price = _field_value(listing["asking_price"])
        acreage = _field_value(listing["listed_acreage"])
        if isinstance(asking_price, (int, float)) and isinstance(acreage, (int, float)):
            lines.append(
                f"- Listing-derived: Asking price {_format_currency(float(asking_price))}; listed acreage {_format_number(float(acreage))}."
            )
        return _bulletize_lines(lines)

    return []


def _sources_section(parcel_memo: dict[str, Any], report_summary: dict[str, Any]) -> list[str]:
    citation_index = _citation_index(parcel_memo)
    lines: list[str] = []
    for group in report_summary["source_groups"]:
        lines.append(f"### {group['label']}")
        for source_id in group["source_ids"]:
            matching_citations = [
                citation
                for citation in parcel_memo.get("citations", [])
                if citation["source_id"] == source_id
            ]
            if not matching_citations:
                lines.append(f"- {source_id}")
                continue
            citation = matching_citations[0]
            lines.append(
                "- "
                f"{citation['source_name']} (`{citation['source_id']}`): "
                f"{citation['url']} "
                f"(retrieved {citation['retrieved_at']}, authority rank {citation['authority_rank']})."
            )
        lines.append("")
    return lines


def render_markdown_memo(
    parcel_memo: dict[str, Any],
    report_summary: dict[str, Any],
    *,
    parcel_candidates: dict[str, Any] | None = None,
    coverage_assessment: dict[str, Any] | None = None,
    jurisdiction_context: dict[str, Any] | None = None,
) -> str:
    lines: list[str] = [
        f"# {report_summary['title']}",
        "",
        f"_Generated {report_summary['generated_at']} from structured Alice artifact `{parcel_memo['object_id']}`._",
        "",
        "## Advisory Boundary",
        "",
        "This memo is a public-data-first Alice v0 research rendering. It is not legal advice, brokerage advice, appraisal, entitlement confirmation, engineering review, or guaranteed feasibility analysis. The structured `alice_land_research` object remains the system of record; this markdown memo is a derived human-readable view.",
        "",
        "## Executive Summary",
        "",
    ]
    lines.extend(_summary_line_list(report_summary["executive_summary"]))
    lines.extend(
        [
            "",
            "## Subject Snapshot",
            "",
        ]
    )
    lines.extend(_render_subject_snapshot(report_summary))
    lines.extend(
        [
            "",
            "## Evidence Quality / Coverage Panel",
            "",
        ]
    )
    lines.extend(_render_evidence_panel(report_summary))
    if report_summary["warnings"]:
        lines.extend(["", "### Warnings", ""])
        for warning in report_summary["warnings"]:
            lines.append(f"- {warning['severity'].title()}: {warning['message']}")

    lines.extend(["", "## Parcel Identity and Jurisdiction", ""])
    lines.append(f"- {report_summary['top_findings'][0]['summary']}")
    lines.extend(_render_jurisdiction_lines(parcel_memo, report_summary))

    lines.extend(["", "## Public-Data Findings", "", "### Planning and Land Use", ""])
    lines.extend(_section_lines(parcel_memo, "planning_and_land_use"))
    lines.extend(["", "### Environmental Constraints", ""])
    lines.extend(_section_lines(parcel_memo, "environmental_constraints"))
    lines.extend(["", "### Water and Agriculture", ""])
    lines.extend(_section_lines(parcel_memo, "water_and_agriculture"))
    lines.extend(["", "### Infrastructure and Utilities", ""])
    lines.extend(_section_lines(parcel_memo, "infrastructure_and_utilities"))
    lines.extend(["", "### Market and Listing Context", ""])
    lines.extend(_section_lines(parcel_memo, "market_signals"))

    lines.extend(["", "## Use-Case Screening", ""])
    lines.extend(_render_use_case_modules(parcel_memo))

    lines.extend(["", "## Directional Economics", ""])
    lines.extend(_render_directional_economics(parcel_memo))

    lines.extend(["", "## Parcel Candidate Analysis", ""])
    lines.extend(_candidate_lines(parcel_memo))

    lines.extend(["", "## Risks", ""])
    for risk in report_summary["top_risks"]:
        blocker_text = " Blocking." if risk["blocking"] else ""
        lines.append(
            f"- {risk['label']} ({risk['severity']}, {risk['category']}): {risk['summary']}{blocker_text}"
        )

    lines.extend(["", "## Unknowns", ""])
    if report_summary["top_unknowns"]:
        for unknown in report_summary["top_unknowns"]:
            blocker_text = " Blocking." if unknown["blocking"] else ""
            recommendation = (
                f" Recommended next source: `{unknown['recommended_next_source']}`."
                if unknown.get("recommended_next_source")
                else ""
            )
            lines.append(
                f"- {unknown['question']} Why it matters: {unknown['why_it_matters']}{blocker_text}{recommendation}"
            )
    else:
        lines.append("- No first-class unknowns were emitted in the current research object.")

    lines.extend(["", "## Recommended Next Actions", ""])
    for action in report_summary["next_actions"]:
        lines.append(f"- P{action['priority']}: `{action['action']}` because {action['reason']}")

    lines.extend(["", "## Sources and Citations", ""])
    lines.extend(_sources_section(parcel_memo, report_summary))

    lines.extend(["## Notes", ""])
    for note in parcel_memo["scores"].get("score_notes", [])[:3]:
        lines.append(f"- {note}")
    if jurisdiction_context is not None and jurisdiction_context.get("unresolved_questions"):
        for question in jurisdiction_context["unresolved_questions"][:2]:
            lines.append(f"- Jurisdiction follow-up: {question}")
    return "\n".join(lines).rstrip() + "\n"


def render_slack_summary(report_summary: dict[str, Any]) -> str:
    positives = [finding["summary"] for finding in report_summary["top_findings"][:2]]
    module_findings = [
        finding["summary"]
        for finding in report_summary["top_findings"]
        if finding["section"] == "use_case_modules"
    ]
    blockers = [warning["message"] for warning in report_summary["warnings"][:2]]
    if not blockers:
        blockers = [risk["summary"] for risk in report_summary["top_risks"][:2]]
    unknowns = [unknown["question"] for unknown in report_summary["top_unknowns"][:2]]
    panel = report_summary["evidence_quality_panel"]
    subject = report_summary["subject_snapshot"]

    lines = [
        report_summary["channel_hints"]["recommended_slack_title"],
        f"Conclusion: {report_summary['one_line_conclusion']}",
        (
            "Subject: "
            f"{subject['primary_subject_label']} "
            f"({_display_confirmation_level(subject['overall_confirmation_level'])}; "
            f"{_display_candidate_status(subject['candidate_set_status'])}"
            + (
                f"; {_display_confirmation_basis(subject.get('overall_confirmation_basis'))}"
                if subject.get("overall_confirmation_basis") not in {None, 'unknown'}
                else ""
            )
            + ")"
        ),
        (
            "Evidence: "
            f"{panel['coverage_tier']} tier, {panel['evidence_quality_label'].replace('_', ' ')}, "
            f"confidence {_format_confidence(panel['overall_confidence'])}, "
            f"completeness {_format_confidence(panel['overall_completeness'])}"
        ),
    ]
    if module_findings:
        lines.append("Use-case screens:")
        for item in module_findings[:2]:
            lines.append(f"- {item}")
    lines.append("Top positives:")
    for item in positives:
        lines.append(f"- {item}")
    lines.append("Top blockers / unknowns:")
    for item in blockers + unknowns:
        lines.append(f"- {item}")
    if report_summary["next_actions"]:
        next_action = report_summary["next_actions"][0]
        lines.append(f"Recommended next action: {next_action['reason']}")
    lines.append(f"Full memo: {report_summary['channel_hints']['canonical_markdown_artifact_ref']}")
    return "\n".join(lines).rstrip() + "\n"


def render_email_summary(report_summary: dict[str, Any]) -> str:
    subject_snapshot = report_summary["subject_snapshot"]
    panel = report_summary["evidence_quality_panel"]
    module_findings = [
        finding["summary"]
        for finding in report_summary["top_findings"]
        if finding["section"] == "use_case_modules"
    ]
    lines = [
        f"Subject: {report_summary['channel_hints']['recommended_email_subject']}",
        "",
        "Alice prepared a public-data-first land research summary from the current structured Alice artifacts.",
        "",
        "Executive summary",
    ]
    lines.extend(_summary_line_list(report_summary["executive_summary"]))
    lines.extend(
        [
            "",
            "Subject snapshot",
            f"- {subject_snapshot['primary_subject_label']}",
            f"- Jurisdiction: {', '.join(part for part in [subject_snapshot.get('city_or_place'), subject_snapshot.get('county_name'), subject_snapshot.get('state_code')] if part)}",
            (
                f"- Parcel status: {_display_confirmation_level(subject_snapshot['overall_confirmation_level'])}; "
                f"{_display_candidate_status(subject_snapshot['candidate_set_status'])}"
                + (
                    f"; {_display_confirmation_basis(subject_snapshot.get('overall_confirmation_basis'))}"
                    if subject_snapshot.get("overall_confirmation_basis") not in {None, 'unknown'}
                    else ""
                )
            ),
            (
                f"- Evidence footing: {panel['coverage_tier']} tier, "
                f"{panel['evidence_quality_label'].replace('_', ' ')}, "
                f"confidence {_format_confidence(panel['overall_confidence'])}, "
                f"completeness {_format_confidence(panel['overall_completeness'])}"
            ),
            "",
            "Key findings",
        ]
    )
    for finding in report_summary["top_findings"][:4]:
        lines.append(f"- {finding['summary']}")
    if module_findings:
        lines.extend(["", "Use-case screens"])
        for finding in module_findings[:3]:
            lines.append(f"- {finding}")
    lines.extend(["", "Major risks / unknowns"])
    for risk in report_summary["top_risks"][:3]:
        lines.append(f"- Risk: {risk['summary']}")
    for unknown in report_summary["top_unknowns"][:3]:
        lines.append(f"- Unknown: {unknown['question']}")
    lines.extend(["", "Recommended next actions"])
    for action in report_summary["next_actions"][:3]:
        lines.append(f"- P{action['priority']}: {action['reason']}")
    lines.extend(
        [
            "",
            f"Full markdown memo: {report_summary['channel_hints']['canonical_markdown_artifact_ref']}",
        ]
    )
    return "\n".join(lines).rstrip() + "\n"


def render_workspace_report(
    workspace_root: str | Path,
    *,
    render_mode: str = "generic_summary",
    generated_at: str | None = None,
    write_outputs: bool = True,
) -> dict[str, Any]:
    alice_root = _workspace_alice_root(workspace_root)
    parcel_memo = load_parcel_memo(alice_root / "parcel_memo.json")
    parcel_candidates = _load_optional_json(alice_root / "parcel_candidates.json")
    research_assembly_summary = _load_optional_json(alice_root / "research_assembly_summary.json")
    coverage_assessment = _load_optional_json(alice_root / "coverage_assessment.json")
    jurisdiction_context = _load_optional_json(alice_root / "jurisdiction_context.json")

    summary = build_report_summary(
        parcel_memo,
        parcel_candidates=parcel_candidates,
        research_assembly_summary=research_assembly_summary,
        coverage_assessment=coverage_assessment,
        jurisdiction_context=jurisdiction_context,
        render_mode=render_mode,
        generated_at=generated_at,
    )
    markdown = render_markdown_memo(
        parcel_memo,
        summary,
        parcel_candidates=parcel_candidates,
        coverage_assessment=coverage_assessment,
        jurisdiction_context=jurisdiction_context,
    )
    slack_text = render_slack_summary(summary)
    email_text = render_email_summary(summary)

    if write_outputs:
        (alice_root / "report_summary.json").write_text(
            json.dumps(summary, indent=2),
            encoding="utf-8",
        )
        (alice_root / "report.md").write_text(markdown, encoding="utf-8")
        (alice_root / "report_slack.txt").write_text(slack_text, encoding="utf-8")
        (alice_root / "report_email.txt").write_text(email_text, encoding="utf-8")

    return {
        "report_summary": summary,
        "report_markdown": markdown,
        "report_slack": slack_text,
        "report_email": email_text,
    }
