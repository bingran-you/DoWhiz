#!/usr/bin/env python3

"""Step 7 parcel candidate synthesis for Alice AI."""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any
from uuid import NAMESPACE_URL, uuid5

from alice_registry import load_json, source_by_id
from alice_subject_resolution import dedupe_strings, normalize_apn, now_iso


PARCEL_CANDIDATES_VERSION = "alice.parcel_candidates.v1"
SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
SCHEMAS_ROOT = SKILL_ROOT / "schemas"
PARCEL_CANDIDATES_SCHEMA_PATH = SCHEMAS_ROOT / "parcel_candidates.schema.json"

APN_FIELD_PREFIX = "parcel_identity.apn_hint"
ADDRESS_FIELDS = {"parcel_identity.site_address_hint", "subject.address"}
ACREAGE_FIELDS = {"parcel_identity.acreage_hint", "listing.listed_acreage"}
COUNTY_PARCEL_ID_FIELDS = {"parcel_identity.county_parcel_id_hint"}
OWNER_FIELDS = {"parcel_identity.owner_name_hint"}
LEGAL_DESCRIPTION_FIELDS = {"parcel_identity.legal_description_fragment"}
COUNTY_NAME_FIELDS = {"parcel_identity.county_name_hint"}
COORDINATE_FIELDS = {"subject.coordinates"}
PARCEL_GROUP_FIELDS = {"parcel_identity.multiple_parcel_hint"}


def deterministic_uuid(*parts: str) -> str:
    return str(uuid5(NAMESPACE_URL, "|".join(parts)))


def bounded(value: float) -> float:
    return max(0.0, min(1.0, round(value, 3)))


def _workspace_alice_root(workspace_root: str | Path) -> Path:
    alice_root = Path(workspace_root) / "alice"
    alice_root.mkdir(parents=True, exist_ok=True)
    return alice_root


def _safe_key(value: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")
    return slug or "unknown"


def _field_matches(item: dict[str, Any], prefixes: set[str]) -> bool:
    return any(item["field_name"] == prefix or item["field_name"].startswith(f"{prefix}_") for prefix in prefixes)


def _normalize_address_value(value: Any) -> dict[str, Any] | None:
    if isinstance(value, dict) and value.get("full_address"):
        return {
            "full_address": value["full_address"],
            "street_line1": value.get("street_line1"),
            "street_line2": value.get("street_line2"),
            "city": value.get("city"),
            "county_name": value.get("county_name"),
            "state_code": value.get("state_code"),
            "postal_code": value.get("postal_code"),
            "country_code": value.get("country_code") or "US",
        }
    if isinstance(value, str) and value.strip():
        cleaned = re.sub(r"\s+", " ", value).strip()
        return {
            "full_address": cleaned,
            "street_line1": cleaned,
            "street_line2": None,
            "city": None,
            "county_name": None,
            "state_code": None,
            "postal_code": None,
            "country_code": "US",
        }
    return None


def _address_key(value: dict[str, Any] | None) -> str | None:
    if value is None:
        return None
    full_address = value.get("full_address")
    if not full_address:
        return None
    county_name = value.get("county_name") or ""
    state_code = value.get("state_code") or ""
    return _safe_key(f"{full_address}|{county_name}|{state_code}")


def _source_category(source_id: str) -> str:
    if source_id in {"subject_resolution", "session_state"}:
        return "inferred"
    descriptor = source_by_id().get(source_id)
    if descriptor is None:
        return "unknown"
    return descriptor["category"]


def _source_weight(scope: str, clue_type: str, source_id: str) -> float:
    category = _source_category(source_id)
    if clue_type == "apn":
        if category in {"county", "city_local"}:
            return 0.5
        if category == "listing_platform":
            return 0.32
        if source_id == "subject_resolution":
            return 0.28
    if clue_type == "county_parcel_id":
        return 0.24 if category in {"county", "city_local"} else 0.1
    if clue_type == "address":
        if category in {"county", "city_local"}:
            return 0.22
        if category == "listing_platform":
            return 0.14
        if source_id == "subject_resolution":
            return 0.18
    if clue_type == "acreage":
        if category in {"county", "city_local"}:
            return 0.12
        if category == "listing_platform":
            return 0.06
    if clue_type == "owner_name":
        return 0.12 if category in {"county", "city_local"} else 0.03
    if clue_type == "legal_description":
        return 0.1
    if clue_type == "coordinates":
        return 0.08
    if clue_type == "parcel_group_hint":
        return 0.05
    return 0.04 if scope != "unresolved" else 0.0


def _base_candidate(*, candidate_id: str, county_fips: str | None, state_fips: str | None) -> dict[str, Any]:
    return {
        "candidate_id": candidate_id,
        "county_fips": county_fips,
        "state_fips": state_fips,
        "apn_raw": None,
        "apn_normalized": None,
        "apn_variants": [],
        "parcel_address": None,
        "acreage_claims": [],
        "source_clues": [],
        "supporting_source_ids": [],
        "contradicting_source_ids": [],
        "candidate_strength": "none",
        "confirmation_level": "unresolved",
        "confirmation_basis": "unknown",
        "geometry_status": "none",
        "notes": [],
    }


def _clue_from_item(
    item: dict[str, Any],
    *,
    county_fips: str | None,
    state_fips: str | None,
) -> dict[str, Any] | None:
    field_name = item["field_name"]
    clue_type = item["clue_type"]
    raw_value = item["value"]
    normalized_value: str | float | bool | None = None
    notes = list(item.get("notes", []))
    raw_scalar: str | int | float | bool | None

    if clue_type == "apn" and isinstance(raw_value, str):
        apn = normalize_apn(raw_value, county_fips=county_fips, state_fips=state_fips)["normalized"]
        normalized_value = apn["apn_normalized"]
        notes.append("APN normalized conservatively for candidate matching.")
        raw_scalar = raw_value
    elif clue_type == "address":
        address = _normalize_address_value(raw_value)
        normalized_value = address["full_address"] if address else None
        raw_scalar = address["full_address"] if address else None
    elif clue_type == "acreage" and isinstance(raw_value, (int, float)):
        normalized_value = round(float(raw_value), 4)
        raw_scalar = round(float(raw_value), 4)
    elif clue_type == "coordinates" and isinstance(raw_value, dict):
        lat = raw_value.get("latitude")
        lon = raw_value.get("longitude")
        if isinstance(lat, (int, float)) and isinstance(lon, (int, float)):
            normalized_value = f"{lat:.6f},{lon:.6f}"
            raw_scalar = normalized_value
        else:
            raw_scalar = None
    elif isinstance(raw_value, (str, int, float, bool)):
        normalized_value = raw_value
        raw_scalar = raw_value
    else:
        raw_scalar = json.dumps(raw_value, sort_keys=True) if raw_value is not None else None

    return {
        "clue_id": item["item_id"],
        "clue_type": clue_type,
        "raw_value": raw_scalar,
        "normalized_value": normalized_value,
        "source_id": item["source_id"],
        "source_item_id": item["item_id"],
        "evidence_scope": item["evidence_scope"],
        "confidence": bounded(float(item["confidence"])),
        "weight": bounded(_source_weight(item["evidence_scope"], clue_type, item["source_id"])) if _source_weight(item["evidence_scope"], clue_type, item["source_id"]) >= 0 else _source_weight(item["evidence_scope"], clue_type, item["source_id"]),
        "citation_ids": list(item.get("citation_ids", [])),
        "notes": dedupe_strings(notes),
    }


def _subject_resolution_seed_clues(
    subject_resolution: dict[str, Any],
    jurisdiction_context: dict[str, Any],
) -> list[dict[str, Any]]:
    county_fips = jurisdiction_context.get("county_fips")
    state_fips = jurisdiction_context.get("state_fips")
    clues: list[dict[str, Any]] = []
    for subject in subject_resolution.get("active_subjects", []):
        identifiers = subject.get("identifiers", {})
        if identifiers.get("apn_raw"):
            apn = normalize_apn(
                identifiers["apn_raw"],
                county_fips=identifiers.get("county_fips") or county_fips,
                state_fips=identifiers.get("state_fips") or state_fips,
            )["normalized"]
            clues.append(
                {
                    "clue_id": f"subject_resolution:{subject['subject_id']}:apn",
                    "clue_type": "apn",
                    "raw_value": identifiers["apn_raw"],
                    "normalized_value": apn["apn_normalized"],
                    "source_id": "subject_resolution",
                    "source_item_id": None,
                    "evidence_scope": "inferred",
                    "confidence": 0.78 if subject["resolution_status"] == "resolved" else 0.52,
                    "weight": 0.36 if subject["resolution_status"] == "resolved" else 0.22,
                    "citation_ids": [],
                    "notes": [
                        "APN carried from subject resolution or prior session state rather than a Step 7 fetched local record."
                    ],
                }
            )
        if identifiers.get("address_text"):
            address = _normalize_address_value(identifiers["address_text"])
            clues.append(
                {
                    "clue_id": f"subject_resolution:{subject['subject_id']}:address",
                    "clue_type": "address",
                    "raw_value": identifiers["address_text"],
                    "normalized_value": address["full_address"] if address else identifiers["address_text"],
                    "source_id": "subject_resolution",
                    "source_item_id": None,
                    "evidence_scope": "inferred",
                    "confidence": 0.72 if subject["resolution_status"] == "resolved" else 0.46,
                    "weight": 0.18,
                    "citation_ids": [],
                    "notes": [
                        "Address carried from subject resolution and still subject to official parcel-record confirmation."
                    ],
                }
            )
        if identifiers.get("coordinates"):
            coordinates = identifiers["coordinates"]
            if coordinates is not None:
                clues.append(
                    {
                        "clue_id": f"subject_resolution:{subject['subject_id']}:coordinates",
                        "clue_type": "coordinates",
                        "raw_value": json.dumps(coordinates, sort_keys=True),
                        "normalized_value": f"{coordinates['latitude']:.6f},{coordinates['longitude']:.6f}",
                        "source_id": "subject_resolution",
                        "source_item_id": None,
                        "evidence_scope": "inferred",
                        "confidence": 0.64,
                        "weight": 0.08,
                        "citation_ids": [],
                        "notes": [
                            "Coordinates carried from subject resolution remain point-based until parcel geometry is confirmed."
                        ],
                    }
                )
    for candidate in subject_resolution.get("parcel_candidates", []):
        if candidate.get("apn_raw"):
            apn = normalize_apn(
                candidate["apn_raw"],
                county_fips=candidate.get("county_fips") or county_fips,
                state_fips=candidate.get("state_fips") or state_fips,
            )["normalized"]
            clues.append(
                {
                    "clue_id": f"subject_resolution:{candidate['candidate_id']}:seed_apn",
                    "clue_type": "apn",
                    "raw_value": candidate["apn_raw"],
                    "normalized_value": apn["apn_normalized"],
                    "source_id": "subject_resolution",
                    "source_item_id": None,
                    "evidence_scope": "inferred",
                    "confidence": bounded(float(candidate.get("confidence", 0.4))),
                    "weight": 0.22,
                    "citation_ids": [],
                    "notes": list(candidate.get("notes", [])) or ["Seed candidate came from subject resolution."],
                }
            )
    return clues


def _candidate_bucket_key(clue: dict[str, Any], county_fips: str | None) -> str | None:
    if clue["clue_type"] == "apn" and isinstance(clue.get("normalized_value"), str):
        return f"apn:{county_fips or 'unknown'}:{clue['normalized_value']}"
    if clue["clue_type"] == "address" and isinstance(clue.get("normalized_value"), str):
        return f"address:{county_fips or 'unknown'}:{_safe_key(clue['normalized_value'])}"
    return None


def _register_candidate(
    candidates_by_key: dict[str, dict[str, Any]],
    key: str,
    *,
    county_fips: str | None,
    state_fips: str | None,
    preferred_candidate_id: str | None = None,
) -> dict[str, Any]:
    if key in candidates_by_key:
        return candidates_by_key[key]
    candidate_id = preferred_candidate_id or f"cand-{_safe_key(key)}"
    candidate = _base_candidate(
        candidate_id=candidate_id,
        county_fips=county_fips,
        state_fips=state_fips,
    )
    candidates_by_key[key] = candidate
    return candidate


def _attach_clue(candidate: dict[str, Any], clue: dict[str, Any]) -> None:
    if clue["clue_id"] not in {existing["clue_id"] for existing in candidate["source_clues"]}:
        candidate["source_clues"].append(clue)
    candidate["supporting_source_ids"] = dedupe_strings(
        [*candidate["supporting_source_ids"], clue["source_id"]]
    )

    if clue["clue_type"] == "apn" and isinstance(clue.get("raw_value"), str) and candidate["apn_raw"] is None:
        candidate["apn_raw"] = clue["raw_value"]
    if clue["clue_type"] == "apn" and isinstance(clue.get("normalized_value"), str):
        candidate["apn_normalized"] = candidate["apn_normalized"] or clue["normalized_value"]
        candidate["apn_variants"] = dedupe_strings(
            [*candidate["apn_variants"], clue["normalized_value"]]
        )
        if isinstance(clue.get("raw_value"), str):
            candidate["apn_variants"] = dedupe_strings(
                [*candidate["apn_variants"], clue["raw_value"]]
            )
    if clue["clue_type"] == "address":
        address = _normalize_address_value(clue["raw_value"])
        if candidate["parcel_address"] is None and address is not None:
            candidate["parcel_address"] = address
    if clue["clue_type"] == "acreage":
        acreage_value = None
        if isinstance(clue.get("normalized_value"), (int, float)):
            acreage_value = float(clue["normalized_value"])
        elif isinstance(clue.get("raw_value"), (int, float)):
            acreage_value = float(clue["raw_value"])
        candidate["acreage_claims"].append(
            {
                "value_acres": acreage_value,
                "status": "estimated",
                "confidence": clue["confidence"],
                "evidence_scope": clue["evidence_scope"],
                "source_id": clue["source_id"],
                "source_item_id": clue["source_item_id"],
                "citation_ids": list(clue.get("citation_ids", [])),
                "notes": list(clue["notes"]),
            }
        )
    if clue["clue_type"] == "coordinates":
        candidate["geometry_status"] = "point_only"
    if clue["clue_type"] == "owner_name":
        candidate["notes"] = dedupe_strings([*candidate["notes"], f"Owner clue: {clue['raw_value']}"])
    if clue["clue_type"] == "legal_description":
        candidate["notes"] = dedupe_strings([*candidate["notes"], "Legal-description fragment recovered from public evidence."])


def _candidate_source_ids(candidate: dict[str, Any]) -> set[str]:
    return {
        clue["source_id"]
        for clue in candidate.get("source_clues", [])
        if clue.get("source_id")
    } or set(candidate.get("supporting_source_ids", []))


def _geometry_rank(status: str) -> int:
    return {
        "boundary_confirmed": 3,
        "map_page_hint": 2,
        "point_only": 1,
        "none": 0,
    }[status]


def _candidate_keys_for_object(
    candidates_by_key: dict[str, dict[str, Any]],
    target: dict[str, Any],
) -> list[str]:
    return [
        key
        for key, candidate in candidates_by_key.items()
        if candidate is target
    ]


def _merge_candidate_into(target: dict[str, Any], other: dict[str, Any]) -> None:
    for clue in other.get("source_clues", []):
        _attach_clue(target, clue)

    if target["apn_raw"] is None and other.get("apn_raw") is not None:
        target["apn_raw"] = other["apn_raw"]
    if target["apn_normalized"] is None and other.get("apn_normalized") is not None:
        target["apn_normalized"] = other["apn_normalized"]
    target["apn_variants"] = dedupe_strings([*target["apn_variants"], *other.get("apn_variants", [])])

    if target["parcel_address"] is None and other.get("parcel_address") is not None:
        target["parcel_address"] = other["parcel_address"]

    existing_acreage_keys = {
        (
            claim.get("source_id"),
            claim.get("source_item_id"),
            claim.get("value_acres"),
        )
        for claim in target.get("acreage_claims", [])
    }
    for claim in other.get("acreage_claims", []):
        acreage_key = (
            claim.get("source_id"),
            claim.get("source_item_id"),
            claim.get("value_acres"),
        )
        if acreage_key not in existing_acreage_keys:
            target["acreage_claims"].append(claim)
            existing_acreage_keys.add(acreage_key)

    target["supporting_source_ids"] = dedupe_strings(
        [*target["supporting_source_ids"], *other.get("supporting_source_ids", [])]
    )
    target["contradicting_source_ids"] = dedupe_strings(
        [*target["contradicting_source_ids"], *other.get("contradicting_source_ids", [])]
    )
    target["notes"] = dedupe_strings(
        [
            *target["notes"],
            *other.get("notes", []),
            f"Merged same-source parcel clues from {other['candidate_id']} into {target['candidate_id']}.",
        ]
    )
    if _geometry_rank(other["geometry_status"]) > _geometry_rank(target["geometry_status"]):
        target["geometry_status"] = other["geometry_status"]


def _merge_source_context_candidates(candidates_by_key: dict[str, dict[str, Any]]) -> None:
    changed = True
    while changed:
        changed = False
        source_ids = sorted(
            {
                source_id
                for candidate in candidates_by_key.values()
                for source_id in _candidate_source_ids(candidate)
            }
        )
        for source_id in source_ids:
            source_candidates = [
                candidate
                for candidate in candidates_by_key.values()
                if source_id in _candidate_source_ids(candidate)
            ]
            apn_candidates = [candidate for candidate in source_candidates if candidate.get("apn_normalized")]
            address_only_candidates = [
                candidate
                for candidate in source_candidates
                if candidate.get("parcel_address") is not None
                and not candidate.get("apn_normalized")
            ]
            parcel_id_candidates = [
                candidate
                for candidate in source_candidates
                if any(clue["clue_type"] == "county_parcel_id" for clue in candidate.get("source_clues", []))
            ]

            merge_target = None
            merge_other = None
            if len(apn_candidates) == 1:
                candidate_sources = _candidate_source_ids(apn_candidates[0])
                compatible_addresses = [
                    candidate
                    for candidate in address_only_candidates
                    if _candidate_source_ids(candidate) <= candidate_sources
                ]
                if len(compatible_addresses) == 1:
                    merge_target = apn_candidates[0]
                    merge_other = compatible_addresses[0]
            if merge_target is None and len(parcel_id_candidates) == 1:
                candidate_sources = _candidate_source_ids(parcel_id_candidates[0])
                compatible_addresses = [
                    candidate
                    for candidate in address_only_candidates
                    if _candidate_source_ids(candidate) <= candidate_sources
                ]
                if len(compatible_addresses) == 1:
                    merge_target = parcel_id_candidates[0]
                    merge_other = compatible_addresses[0]

            if merge_target is None or merge_other is None or merge_target is merge_other:
                continue

            _merge_candidate_into(merge_target, merge_other)
            for key in _candidate_keys_for_object(candidates_by_key, merge_other):
                del candidates_by_key[key]
            changed = True
            break


def _prune_uninformative_candidates(candidates_by_key: dict[str, dict[str, Any]]) -> None:
    if len(candidates_by_key) <= 1:
        return
    for key, candidate in list(candidates_by_key.items()):
        if candidate.get("source_clues"):
            continue
        if candidate.get("apn_normalized"):
            continue
        del candidates_by_key[key]


def _score_candidate(candidate: dict[str, Any]) -> tuple[str, str]:
    total = 0.0
    source_categories = {_source_category(clue["source_id"]) for clue in candidate["source_clues"]}
    clue_types = {clue["clue_type"] for clue in candidate["source_clues"]}
    for clue in candidate["source_clues"]:
        total += float(clue["weight"]) * float(clue["confidence"])

    apn_sources = {
        clue["source_id"]
        for clue in candidate["source_clues"]
        if clue["clue_type"] == "apn" and clue.get("normalized_value")
    }
    if len(apn_sources) >= 2:
        total += 0.22
    address_sources = {
        clue["source_id"]
        for clue in candidate["source_clues"]
        if clue["clue_type"] == "address" and clue.get("normalized_value")
    }
    if len(address_sources) >= 2:
        total += 0.12
    if "county_parcel_id" in clue_types:
        total += 0.08
    if "owner_name" in clue_types:
        total += 0.05
    if "listing_platform" in source_categories and source_categories <= {"listing_platform"}:
        total = min(total, 0.55)
    if "subject_resolution" in candidate["supporting_source_ids"] and "parcel_confirmed" not in {clue["evidence_scope"] for clue in candidate["source_clues"]}:
        total += 0.06

    score = bounded(total)
    if score >= 0.85:
        strength = "strong"
    elif score >= 0.6:
        strength = "moderate"
    elif score > 0:
        strength = "weak"
    else:
        strength = "none"

    official_local_apn = any(
        clue["clue_type"] == "apn" and _source_category(clue["source_id"]) in {"county", "city_local"}
        for clue in candidate["source_clues"]
    )
    corroborating_address = any(
        clue["clue_type"] == "address" and clue["evidence_scope"] in {"parcel_candidate", "inferred"}
        for clue in candidate["source_clues"]
    )
    has_parcel_id = any(clue["clue_type"] == "county_parcel_id" for clue in candidate["source_clues"])

    if official_local_apn and (corroborating_address or has_parcel_id) and score >= 0.78:
        confirmation = "parcel_confirmed"
    elif score >= 0.62 and official_local_apn:
        confirmation = "candidate_corroborated"
    elif candidate["apn_normalized"] or candidate["parcel_address"] is not None:
        confirmation = "candidate_unconfirmed"
    elif any(clue["evidence_scope"] == "listing_derived" for clue in candidate["source_clues"]):
        confirmation = "listing_hint_only"
    elif candidate["geometry_status"] == "point_only":
        confirmation = "geography_only"
    else:
        confirmation = "unresolved"

    return strength, confirmation


def _confirmation_basis(candidate: dict[str, Any]) -> str:
    if candidate["geometry_status"] == "boundary_confirmed":
        return "geometry_confirmed"

    local_record_categories = {"county", "city_local"}
    local_apn_sources = {
        clue["source_id"]
        for clue in candidate["source_clues"]
        if clue["clue_type"] == "apn"
        and clue.get("normalized_value")
        and _source_category(clue["source_id"]) in local_record_categories
    }
    local_address_present = any(
        clue["clue_type"] == "address"
        and clue.get("normalized_value")
        and _source_category(clue["source_id"]) in local_record_categories
        for clue in candidate["source_clues"]
    )
    local_parcel_id_present = any(
        clue["clue_type"] == "county_parcel_id"
        and _source_category(clue["source_id"]) in local_record_categories
        for clue in candidate["source_clues"]
    )
    if local_apn_sources and (local_address_present or local_parcel_id_present or len(local_apn_sources) >= 2):
        return "local_record_corroborated"

    text_source_ids: set[str] = set()
    text_clue_types: set[str] = set()
    for clue in candidate["source_clues"]:
        if clue["clue_type"] not in {"apn", "address", "county_parcel_id", "legal_description"}:
            continue
        if clue.get("normalized_value") in {None, ""} and clue.get("raw_value") in {None, ""}:
            continue
        text_source_ids.add(clue["source_id"])
        text_clue_types.add(clue["clue_type"])
    if len(text_source_ids) >= 2 and ({"apn", "county_parcel_id"} & text_clue_types):
        return "text_corroborated"

    return "unknown"


def _assign_contradictions(candidates: list[dict[str, Any]]) -> None:
    apn_to_sources = {
        candidate["candidate_id"]: set(candidate["supporting_source_ids"])
        for candidate in candidates
        if candidate["apn_normalized"]
    }
    if len(apn_to_sources) <= 1:
        return
    all_source_ids = {
        source_id
        for source_ids in apn_to_sources.values()
        for source_id in source_ids
    }
    for candidate in candidates:
        candidate["contradicting_source_ids"] = sorted(
            all_source_ids - set(candidate["supporting_source_ids"])
        )


def build_parcel_candidates(
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
    jurisdiction_context: dict[str, Any],
    source_fetch_log: dict[str, Any],
    extracted_evidence: dict[str, Any],
    *,
    workspace_root: str | Path,
    generated_at: str | None = None,
) -> dict[str, Any]:
    county_fips = jurisdiction_context.get("county_fips")
    state_fips = jurisdiction_context.get("state_fips")
    all_clues = _subject_resolution_seed_clues(subject_resolution, jurisdiction_context)
    all_clues.extend(
        clue
        for item in extracted_evidence.get("items", [])
        if (clue := _clue_from_item(item, county_fips=county_fips, state_fips=state_fips)) is not None
        and (
            item["field_name"].startswith(APN_FIELD_PREFIX)
            or item["field_name"] in ADDRESS_FIELDS
            or item["field_name"] in ACREAGE_FIELDS
            or item["field_name"] in COUNTY_PARCEL_ID_FIELDS
            or item["field_name"] in OWNER_FIELDS
            or item["field_name"] in LEGAL_DESCRIPTION_FIELDS
            or item["field_name"] in COUNTY_NAME_FIELDS
            or item["field_name"] in COORDINATE_FIELDS
            or item["field_name"] in PARCEL_GROUP_FIELDS
        )
    )

    candidates_by_key: dict[str, dict[str, Any]] = {}
    notes: list[str] = []
    parcel_group_hint_present = any(clue["clue_type"] == "parcel_group_hint" for clue in all_clues)

    for candidate_seed in subject_resolution.get("parcel_candidates", []):
        if candidate_seed.get("apn_normalized"):
            key = f"apn:{candidate_seed.get('county_fips') or county_fips or 'unknown'}:{candidate_seed['apn_normalized']}"
        elif candidate_seed.get("address_text"):
            key = f"address:{candidate_seed.get('county_fips') or county_fips or 'unknown'}:{_safe_key(candidate_seed['address_text'])}"
        else:
            key = f"seed:{candidate_seed['candidate_id']}"
        candidate = _register_candidate(
            candidates_by_key,
            key,
            county_fips=candidate_seed.get("county_fips") or county_fips,
            state_fips=candidate_seed.get("state_fips") or state_fips,
            preferred_candidate_id=candidate_seed["candidate_id"],
        )
        candidate["notes"] = dedupe_strings([*candidate["notes"], *candidate_seed.get("notes", [])])
        if candidate_seed.get("apn_raw"):
            candidate["apn_raw"] = candidate_seed["apn_raw"]
        if candidate_seed.get("apn_normalized"):
            candidate["apn_normalized"] = candidate_seed["apn_normalized"]
            candidate["apn_variants"] = dedupe_strings(
                [*candidate["apn_variants"], candidate_seed["apn_normalized"]]
            )
        if candidate_seed.get("address_text") and candidate["parcel_address"] is None:
            candidate["parcel_address"] = _normalize_address_value(candidate_seed["address_text"])

    for clue in all_clues:
        key = _candidate_bucket_key(clue, county_fips)
        if key is None:
            if len(candidates_by_key) == 1:
                candidate = next(iter(candidates_by_key.values()))
                _attach_clue(candidate, clue)
            continue
        candidate = _register_candidate(candidates_by_key, key, county_fips=county_fips, state_fips=state_fips)
        _attach_clue(candidate, clue)

    _merge_source_context_candidates(candidates_by_key)
    _prune_uninformative_candidates(candidates_by_key)

    if not candidates_by_key and subject_resolution.get("active_subjects"):
        fallback_subject = subject_resolution["active_subjects"][0]
        key = f"seed:{fallback_subject['subject_id']}"
        candidate = _register_candidate(
            candidates_by_key,
            key,
            county_fips=county_fips,
            state_fips=state_fips,
            preferred_candidate_id=f"cand-{fallback_subject['subject_id']}",
        )
        candidate["notes"].append("No structured parcel clue was strong enough to build an APN or address bucket, so this remains a weak subject-level candidate.")

    candidates = list(candidates_by_key.values())
    _assign_contradictions(candidates)

    for candidate in candidates:
        strength, confirmation = _score_candidate(candidate)
        candidate["candidate_strength"] = strength
        candidate["confirmation_level"] = confirmation
        if candidate["geometry_status"] == "none" and any(
            clue["clue_type"] == "coordinates" for clue in candidate["source_clues"]
        ):
            candidate["geometry_status"] = "point_only"
        if candidate["geometry_status"] == "none" and any(
            "interactive_mapping" in clue["source_id"] or "gis" in clue["source_id"]
            for clue in candidate["source_clues"]
        ):
            candidate["geometry_status"] = "map_page_hint"
        candidate["acreage_claims"] = [
            claim
            for claim in candidate["acreage_claims"]
            if claim["value_acres"] is not None
        ]
        if candidate["confirmation_level"] == "parcel_confirmed":
            candidate["notes"] = dedupe_strings([*candidate["notes"], "Current clues support one conservatively parcel-confirmed candidate."])
        elif candidate["candidate_strength"] == "weak":
            candidate["notes"] = dedupe_strings([*candidate["notes"], "Current clues remain weak and should not be treated as parcel confirmation."])

    if len(candidates) > 1 and any(candidate.get("apn_normalized") for candidate in candidates):
        candidates = [
            candidate
            for candidate in candidates
            if not (
                candidate["candidate_id"].startswith("cand-subject-")
                and not candidate.get("apn_normalized")
                and candidate["candidate_strength"] in {"none", "weak"}
            )
        ]

    distinct_apns = {
        candidate["apn_normalized"]
        for candidate in candidates
        if candidate.get("apn_normalized")
    }
    if len(candidates) > 1 and len(distinct_apns) > 1:
        for candidate in candidates:
            if candidate["confirmation_level"] == "parcel_confirmed":
                candidate["confirmation_level"] = "candidate_corroborated"
                candidate["notes"] = dedupe_strings(
                    [
                        *candidate["notes"],
                        "Confirmation was downgraded because competing APN-backed candidates remain active.",
                    ]
                )

    for candidate in candidates:
        candidate["confirmation_basis"] = _confirmation_basis(candidate)

    candidates.sort(
        key=lambda candidate: (
            {"strong": 0, "moderate": 1, "weak": 2, "none": 3}[candidate["candidate_strength"]],
            candidate["candidate_id"],
        )
    )

    primary_candidate_id = candidates[0]["candidate_id"] if candidates else None
    primary_confirmation = candidates[0]["confirmation_level"] if candidates else "unresolved"

    if not candidates:
        candidate_set_status = "no_viable_candidate"
        candidate_strategy = "none"
        notes.append("No viable parcel candidate could be built from the current subject and fetched evidence.")
    elif parcel_group_hint_present and len(candidates) > 1:
        candidate_set_status = "parcel_group_case"
        candidate_strategy = "clue_cluster"
        notes.append("Evidence suggests the subject may involve multiple parcels or a grouped offering.")
    elif len(candidates) > 1:
        candidate_set_status = "multiple_competing_candidates"
        candidate_strategy = "clue_cluster"
        notes.append("Competing APN or address clues remain unresolved, so Alice preserved a candidate set.")
    elif candidates[0]["candidate_strength"] == "strong":
        candidate_set_status = "single_strong_candidate"
        if any(_source_category(source_id) in {"county", "city_local"} for source_id in candidates[0]["supporting_source_ids"]) and any(
            _source_category(source_id) == "listing_platform" for source_id in candidates[0]["supporting_source_ids"]
        ):
            candidate_strategy = "apn_corroboration"
        elif "subject_resolution" in candidates[0]["supporting_source_ids"]:
            candidate_strategy = "follow_up_inherited"
        else:
            candidate_strategy = "local_record_corroboration"
        notes.append("One candidate currently stands above the rest based on corroborated parcel clues.")
    else:
        candidate_set_status = "single_weak_candidate"
        if all(_source_category(source_id) == "listing_platform" for source_id in candidates[0]["supporting_source_ids"]):
            candidate_strategy = "listing_only"
        else:
            candidate_strategy = "address_apn_merge"
        notes.append("Only a weak single candidate is available; parcel-specific conclusions should remain conservative.")

    artifact = {
        "schema_version": PARCEL_CANDIDATES_VERSION,
        "parcel_candidates_id": deterministic_uuid(
            "alice-parcel-candidates",
            request["request_id"],
            source_fetch_log["fetch_log_id"],
        ),
        "request_id": request["request_id"],
        "subject_resolution_id": subject_resolution["resolution_id"],
        "jurisdiction_context_id": jurisdiction_context["jurisdiction_context_id"],
        "source_fetch_log_id": source_fetch_log["fetch_log_id"],
        "generated_at": generated_at or now_iso(),
        "candidate_set_status": candidate_set_status,
        "candidate_strategy": candidate_strategy,
        "primary_candidate_id": primary_candidate_id,
        "candidate_count": len(candidates),
        "candidates": candidates,
        "notes": dedupe_strings(notes),
    }

    alice_root = _workspace_alice_root(workspace_root)
    (alice_root / "parcel_candidates.json").write_text(json.dumps(artifact, indent=2), encoding="utf-8")
    return artifact


def summarize_parcel_candidates(artifact: dict[str, Any]) -> dict[str, Any]:
    return {
        "parcel_candidates_id": artifact["parcel_candidates_id"],
        "candidate_set_status": artifact["candidate_set_status"],
        "candidate_strategy": artifact["candidate_strategy"],
        "primary_candidate_id": artifact["primary_candidate_id"],
        "candidate_count": artifact["candidate_count"],
        "top_candidate_strength": artifact["candidates"][0]["candidate_strength"] if artifact["candidates"] else "none",
        "top_confirmation_level": artifact["candidates"][0]["confirmation_level"] if artifact["candidates"] else "unresolved",
        "top_confirmation_basis": artifact["candidates"][0]["confirmation_basis"] if artifact["candidates"] else "unknown",
    }


def load_parcel_candidates(path: str | Path) -> dict[str, Any]:
    return load_json(Path(path))
