#!/usr/bin/env python3

"""Step 9 universal land research assembly for Alice AI."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any
from uuid import NAMESPACE_URL, uuid5

from alice_parcel_candidates import build_parcel_candidates, load_parcel_candidates
from alice_extract import load_extracted_evidence
from alice_fetch import load_source_fetch_log
from alice_registry import load_json, source_by_id
from alice_subject_resolution import now_iso
from alice_use_case_modules import evaluate_use_case_modules


SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
SCHEMAS_ROOT = SKILL_ROOT / "schemas"
LAND_RESEARCH_SCHEMA_PATH = SCHEMAS_ROOT / "alice_land_research.schema.json"

def deterministic_uuid(*parts: str) -> str:
    return str(uuid5(NAMESPACE_URL, "|".join(parts)))


def bounded(value: float) -> float:
    return max(0.0, min(1.0, round(value, 3)))


def sorted_unique_strings(values: list[str]) -> list[str]:
    return sorted({value for value in values if value})


def evidence_field(
    value: Any = None,
    *,
    status: str = "missing",
    confidence: float = 0.0,
    evidence_scope: str = "unresolved",
    supporting_item_ids: list[str] | None = None,
    linked_candidate_ids: list[str] | None = None,
    citation_ids: list[str] | None = None,
    as_of: str | None = None,
    notes: str | None = None,
) -> dict[str, Any]:
    return {
        "value": value,
        "status": status,
        "confidence": bounded(confidence),
        "evidence_scope": evidence_scope,
        "supporting_item_ids": sorted_unique_strings(supporting_item_ids or []),
        "linked_candidate_ids": sorted_unique_strings(linked_candidate_ids or []),
        "citation_ids": citation_ids or [],
        "as_of": as_of,
        "notes": notes,
    }


def address_field(
    value: dict[str, Any] | None = None,
    *,
    status: str = "missing",
    confidence: float = 0.0,
    evidence_scope: str = "unresolved",
    supporting_item_ids: list[str] | None = None,
    linked_candidate_ids: list[str] | None = None,
    citation_ids: list[str] | None = None,
    as_of: str | None = None,
    notes: str | None = None,
) -> dict[str, Any]:
    return evidence_field(
        value,
        status=status,
        confidence=confidence,
        evidence_scope=evidence_scope,
        supporting_item_ids=supporting_item_ids,
        linked_candidate_ids=linked_candidate_ids,
        citation_ids=citation_ids,
        as_of=as_of,
        notes=notes,
    )


def coordinates_field(
    value: dict[str, Any] | None = None,
    *,
    status: str = "missing",
    confidence: float = 0.0,
    evidence_scope: str = "unresolved",
    supporting_item_ids: list[str] | None = None,
    linked_candidate_ids: list[str] | None = None,
    citation_ids: list[str] | None = None,
    as_of: str | None = None,
    notes: str | None = None,
) -> dict[str, Any]:
    return evidence_field(
        value,
        status=status,
        confidence=confidence,
        evidence_scope=evidence_scope,
        supporting_item_ids=supporting_item_ids,
        linked_candidate_ids=linked_candidate_ids,
        citation_ids=citation_ids,
        as_of=as_of,
        notes=notes,
    )


def signal_item(
    label: str,
    *,
    status: str = "estimated",
    confidence: float = 0.5,
    evidence_scope: str = "unresolved",
    supporting_item_ids: list[str] | None = None,
    linked_candidate_ids: list[str] | None = None,
    citation_ids: list[str] | None = None,
    as_of: str | None = None,
    notes: str | None = None,
) -> dict[str, Any]:
    return {
        "label": label,
        "status": status,
        "confidence": bounded(confidence),
        "evidence_scope": evidence_scope,
        "supporting_item_ids": sorted_unique_strings(supporting_item_ids or []),
        "linked_candidate_ids": sorted_unique_strings(linked_candidate_ids or []),
        "citation_ids": citation_ids or [],
        "as_of": as_of,
        "notes": notes,
    }


def _workspace_alice_root(workspace_root: str | Path) -> Path:
    alice_root = Path(workspace_root) / "alice"
    alice_root.mkdir(parents=True, exist_ok=True)
    return alice_root


def _items_by_field(extracted_evidence: dict[str, Any]) -> dict[str, list[dict[str, Any]]]:
    grouped: dict[str, list[dict[str, Any]]] = {}
    for item in extracted_evidence.get("items", []):
        grouped.setdefault(item["field_name"], []).append(item)
    for values in grouped.values():
        values.sort(key=lambda item: item["confidence"], reverse=True)
    return grouped


def _items_by_id(extracted_evidence: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {
        item["item_id"]: item
        for item in extracted_evidence.get("items", [])
    }


def _candidate_item_links(parcel_candidates: dict[str, Any] | None) -> dict[str, list[str]]:
    if not parcel_candidates:
        return {}
    linked: dict[str, list[str]] = {}
    for candidate in parcel_candidates.get("candidates", []):
        candidate_id = candidate["candidate_id"]
        for clue in candidate.get("source_clues", []):
            source_item_id = clue.get("source_item_id")
            if not source_item_id:
                continue
            linked.setdefault(source_item_id, []).append(candidate_id)
    return {
        item_id: sorted_unique_strings(candidate_ids)
        for item_id, candidate_ids in linked.items()
    }


def _best_item(
    grouped_items: dict[str, list[dict[str, Any]]],
    field_name: str,
    *,
    source_id: str | None = None,
) -> dict[str, Any] | None:
    candidates = grouped_items.get(field_name, [])
    if source_id is not None:
        candidates = [item for item in candidates if item["source_id"] == source_id]
    return candidates[0] if candidates else None


def _all_items(grouped_items: dict[str, list[dict[str, Any]]], field_name: str) -> list[dict[str, Any]]:
    return grouped_items.get(field_name, [])


def _linked_candidate_ids_for_items(
    items: list[dict[str, Any]],
    candidate_links: dict[str, list[str]] | None,
) -> list[str]:
    if not candidate_links:
        return []
    candidate_ids: list[str] = []
    for item in items:
        candidate_ids.extend(candidate_links.get(item["item_id"], []))
    return sorted_unique_strings(candidate_ids)


def _supporting_item_ids(items: list[dict[str, Any]]) -> list[str]:
    return sorted_unique_strings([item["item_id"] for item in items if item.get("item_id")])


def _field_from_item(
    item: dict[str, Any] | None,
    *,
    default_note: str | None = None,
    candidate_links: dict[str, list[str]] | None = None,
    evidence_scope: str | None = None,
) -> dict[str, Any]:
    if item is None:
        return evidence_field(notes=default_note)
    return evidence_field(
        item["value"],
        status=item["status"],
        confidence=item["confidence"],
        evidence_scope=evidence_scope or item.get("evidence_scope", "inferred"),
        supporting_item_ids=[item["item_id"]] if item.get("item_id") else [],
        linked_candidate_ids=_linked_candidate_ids_for_items([item], candidate_links) if item.get("item_id") else [],
        citation_ids=item["citation_ids"],
        as_of=item["observed_at"],
        notes="; ".join(item["notes"]) if item["notes"] else default_note,
    )


def _address_from_item(
    item: dict[str, Any] | None,
    *,
    default_note: str | None = None,
    candidate_links: dict[str, list[str]] | None = None,
    evidence_scope: str | None = None,
) -> dict[str, Any]:
    if item is None:
        return address_field(notes=default_note)
    return address_field(
        item["value"],
        status=item["status"],
        confidence=item["confidence"],
        evidence_scope=evidence_scope or item.get("evidence_scope", "inferred"),
        supporting_item_ids=[item["item_id"]] if item.get("item_id") else [],
        linked_candidate_ids=_linked_candidate_ids_for_items([item], candidate_links) if item.get("item_id") else [],
        citation_ids=item["citation_ids"],
        as_of=item["observed_at"],
        notes="; ".join(item["notes"]) if item["notes"] else default_note,
    )


def _coordinates_from_item(
    item: dict[str, Any] | None,
    *,
    default_note: str | None = None,
    candidate_links: dict[str, list[str]] | None = None,
    evidence_scope: str | None = None,
) -> dict[str, Any]:
    if item is None:
        return coordinates_field(notes=default_note)
    return coordinates_field(
        item["value"],
        status=item["status"],
        confidence=item["confidence"],
        evidence_scope=evidence_scope or item.get("evidence_scope", "inferred"),
        supporting_item_ids=[item["item_id"]] if item.get("item_id") else [],
        linked_candidate_ids=_linked_candidate_ids_for_items([item], candidate_links) if item.get("item_id") else [],
        citation_ids=item["citation_ids"],
        as_of=item["observed_at"],
        notes="; ".join(item["notes"]) if item["notes"] else default_note,
    )


def _subject_address_from_resolution(subject_resolution: dict[str, Any]) -> dict[str, Any] | None:
    for subject in subject_resolution.get("active_subjects", []):
        identifiers = subject.get("identifiers", {})
        address_text = identifiers.get("address_text")
        if not address_text:
            continue
        street = address_text.split(",", 1)[0].strip()
        return {
            "full_address": address_text,
            "street_line1": street or None,
            "street_line2": None,
            "city": identifiers.get("city_name"),
            "county_name": identifiers.get("county_name"),
            "state_code": identifiers.get("state_code"),
            "postal_code": identifiers.get("postal_code"),
            "country_code": "US",
        }
    return None


def _subject_coordinates_from_resolution(subject_resolution: dict[str, Any]) -> dict[str, Any] | None:
    for subject in subject_resolution.get("active_subjects", []):
        coordinates = subject.get("identifiers", {}).get("coordinates")
        if coordinates is not None:
            return coordinates
    centroid = subject_resolution.get("geography_context", {}).get("centroid_coordinates")
    if centroid is not None:
        return centroid
    for candidate in subject_resolution.get("parcel_candidates", []):
        coordinates = candidate.get("coordinates")
        if coordinates is not None:
            return coordinates
    return None


def _candidate_lookup(parcel_candidates: dict[str, Any] | None) -> dict[str, dict[str, Any]]:
    if not parcel_candidates:
        return {}
    return {
        candidate["candidate_id"]: candidate
        for candidate in parcel_candidates.get("candidates", [])
    }


def _primary_candidate(parcel_candidates: dict[str, Any] | None) -> dict[str, Any] | None:
    if not parcel_candidates:
        return None
    primary_candidate_id = parcel_candidates.get("primary_candidate_id")
    for candidate in parcel_candidates.get("candidates", []):
        if candidate["candidate_id"] == primary_candidate_id:
            return candidate
    return parcel_candidates.get("candidates", [None])[0]


def _confirmation_scope(confirmation_level: str | None) -> str:
    return {
        "parcel_confirmed": "parcel_confirmed",
        "candidate_corroborated": "parcel_candidate",
        "candidate_unconfirmed": "parcel_candidate",
        "listing_hint_only": "listing_derived",
        "geography_only": "geography_only",
        "unresolved": "unresolved",
        None: "unresolved",
    }[confirmation_level]


def _item_by_candidate_clue_type(
    candidate: dict[str, Any] | None,
    clue_type: str,
    items_by_id: dict[str, dict[str, Any]],
) -> dict[str, Any] | None:
    if candidate is None:
        return None
    matching = []
    for clue in candidate.get("source_clues", []):
        if clue["clue_type"] != clue_type or not clue.get("source_item_id"):
            continue
        item = items_by_id.get(clue["source_item_id"])
        if item is not None:
            matching.append(item)
    if not matching:
        return None
    matching.sort(key=lambda item: item["confidence"], reverse=True)
    return matching[0]


def _raw_input_refs(subject_resolution: dict[str, Any]) -> list[dict[str, str]]:
    return [
        {
            "input_kind": raw_input["input_kind"],
            "raw_value": raw_input["raw_value"],
        }
        for raw_input in subject_resolution.get("raw_inputs", [])
    ]


def _request_context(request: dict[str, Any]) -> dict[str, Any]:
    thesis = request["thesis"]
    return {
        "request_id": request["request_id"],
        "thread_id": request["thread_id"],
        "request_mode": request["request_mode"],
        "channel": request["channel"],
        "user_thesis_summary": thesis.get("summary"),
        "use_case_hypotheses": thesis.get("use_case_hypotheses", []),
        "user_constraints": {
            "target_geographies": thesis.get("target_geographies", []),
            "filters": thesis.get("filters", {}),
            "budget": thesis.get("budget", {}),
            "hold_period": thesis.get("hold_period", {}),
            "return_preferences": thesis.get("return_preferences", {}),
        },
        "batch_parent_id": None,
    }


def _subject_context(
    subject_resolution: dict[str, Any],
    grouped_items: dict[str, list[dict[str, Any]]],
    parcel_candidates: dict[str, Any] | None,
    candidate_links: dict[str, list[str]],
) -> dict[str, Any]:
    address_item = _best_item(grouped_items, "subject.address")
    coordinates_item = _best_item(grouped_items, "subject.coordinates")
    if address_item is None:
        address_value = _subject_address_from_resolution(subject_resolution)
        address_field_value = address_field(
            address_value,
            status="estimated" if address_value else "missing",
            confidence=0.6 if address_value else 0.0,
            evidence_scope="inferred" if address_value else "unresolved",
            notes="Carried forward from subject resolution and not yet reconfirmed in Step 7."
            if address_value
            else None,
        )
    else:
        address_field_value = _address_from_item(address_item, candidate_links=candidate_links)

    if coordinates_item is None:
        coordinates_value = _subject_coordinates_from_resolution(subject_resolution)
        coordinates_field_value = coordinates_field(
            coordinates_value,
            status="estimated" if coordinates_value else "missing",
            confidence=0.65 if coordinates_value else 0.0,
            evidence_scope="inferred" if coordinates_value else "unresolved",
            notes="Carried forward from subject resolution and not yet reconfirmed in Step 7."
            if coordinates_value
            else None,
        )
    else:
        coordinates_field_value = _coordinates_from_item(coordinates_item, candidate_links=candidate_links)

    if subject_resolution["subject_kind"] == "single_parcel":
        parcel_count = 1
    elif subject_resolution["subject_kind"] == "parcel_group":
        parcel_count = (
            max(2, int(parcel_candidates.get("candidate_count", 0)))
            if parcel_candidates is not None
            else max(2, len(subject_resolution.get("active_subjects", [])))
        )
    else:
        parcel_count = (
            parcel_candidates.get("candidate_count")
            if parcel_candidates is not None
            else max(
                0,
                len(subject_resolution.get("parcel_candidates", []))
                if subject_resolution["subject_kind"] == "unresolved_candidate_set"
                else max(1, len(subject_resolution.get("active_subjects", []))),
            )
        )

    return {
        "research_unit_type": subject_resolution["subject_kind"],
        "resolution_status": subject_resolution["resolution_status"],
        "primary_subject_label": subject_resolution["primary_subject_label"],
        "raw_inputs": _raw_input_refs(subject_resolution),
        "canonical_address": address_field_value,
        "coordinates": coordinates_field_value,
        "parcel_count": parcel_count,
    }


def _listing_metadata(
    subject_resolution: dict[str, Any],
    fetch_log: dict[str, Any],
    grouped_items: dict[str, list[dict[str, Any]]],
    candidate_links: dict[str, list[str]],
) -> dict[str, Any]:
    listing_context = subject_resolution.get("listing_context", {})
    listing_present = bool(listing_context.get("has_listing_inputs"))
    platform = None
    listing_id = None
    canonical_url = None
    for subject in subject_resolution.get("active_subjects", []):
        identifiers = subject.get("identifiers", {})
        if identifiers.get("listing_platform"):
            platform = identifiers.get("listing_platform")
            listing_id = identifiers.get("listing_id")
            canonical_url = identifiers.get("canonical_url")
            break

    listing_fetch_time = None
    for entry in fetch_log["fetch_entries"]:
        if entry["source_id"] and entry["source_id"].startswith("listing_platform_") and entry["completed_at"]:
            listing_fetch_time = entry["completed_at"]
            break

    return {
        "listing_present": listing_present,
        "platform": evidence_field(
            platform,
            status="confirmed" if platform else "missing",
            confidence=0.95 if platform else 0.0,
            evidence_scope="listing_derived" if platform else "unresolved",
            notes=None if platform else "No listing platform is active for this subject.",
        ),
        "canonical_url": _best_item(grouped_items, "listing.canonical_url")["value"]
        if _best_item(grouped_items, "listing.canonical_url")
        else canonical_url,
        "listing_id": listing_id,
        "status": "active" if listing_present else "unknown",
        "asking_price": _field_from_item(
            _best_item(grouped_items, "listing.asking_price"),
            default_note="Listing asking price was not recovered in Step 7.",
            candidate_links=candidate_links,
        ),
        "listed_acreage": _field_from_item(
            _best_item(grouped_items, "listing.listed_acreage"),
            default_note="Listing acreage was not recovered in Step 7.",
            candidate_links=candidate_links,
        ),
        "days_on_market": evidence_field(
            notes="Days on market are not yet extracted in Step 7."
        ),
        "listing_agent_name": evidence_field(
            notes="Listing agent extraction is deferred beyond Step 7."
        ),
        "listing_brokerage": evidence_field(
            notes="Listing brokerage extraction is deferred beyond Step 7."
        ),
        "listing_text_summary": _field_from_item(
            _best_item(grouped_items, "listing.description_text") or _best_item(grouped_items, "listing.title"),
            default_note="Listing descriptive text was not recovered in Step 7.",
            candidate_links=candidate_links,
        ),
        "listing_snapshot_at": listing_fetch_time,
    }


def _source_urls(fetch_log: dict[str, Any], subject_resolution: dict[str, Any]) -> list[dict[str, Any]]:
    urls: dict[tuple[str, str], dict[str, Any]] = {}
    for entry in fetch_log.get("fetch_entries", []):
        source_id = entry.get("source_id")
        target_url = entry.get("request_descriptor", {}).get("target_url")
        if source_id is None or target_url is None:
            continue
        urls[(source_id, target_url)] = {
            "label": entry["source_name"] or source_id,
            "url": target_url,
            "source_id": source_id,
        }
    for raw_input in subject_resolution.get("raw_inputs", []):
        if raw_input["input_kind"] != "listing_url":
            continue
        urls.setdefault(
            ("subject_input_listing_url", raw_input["raw_value"]),
            {
                "label": "User-supplied listing URL",
                "url": raw_input["raw_value"],
                "source_id": "subject_input_listing_url",
            },
        )
    return sorted(
        urls.values(),
        key=lambda item: (item["source_id"], item["url"]),
    )


def _jurisdiction(
    jurisdiction_context: dict[str, Any],
    coverage_assessment: dict[str, Any],
) -> dict[str, Any]:
    state = None
    if jurisdiction_context["state"]["resolution_status"] == "resolved":
        state = {
            "name": jurisdiction_context["state"]["name"],
            "fips": jurisdiction_context["state"]["fips"],
        }
    county = None
    if jurisdiction_context["county"]["resolution_status"] == "resolved":
        county = {
            "name": jurisdiction_context["county"]["name"],
            "fips": jurisdiction_context["county"]["fips"],
        }

    city_name = jurisdiction_context["city_or_place"]["name"]
    if city_name:
        city_field = evidence_field(
            city_name,
            status="confirmed",
            confidence=0.82,
            evidence_scope="county_level",
            notes=None,
        )
    elif county is not None:
        city_field = evidence_field(
            f"County-level context only; city or unincorporated place not yet confirmed for {county['name']}.",
            status="estimated",
            confidence=0.46,
            evidence_scope="county_level",
            notes="County context is known but city/incorporated status is still incomplete.",
        )
    else:
        city_field = evidence_field(notes="Jurisdiction remains unresolved.")

    federal_relevance = []
    if "environmental_baseline" in coverage_assessment.get("critical_surfaces", []):
        federal_relevance.extend(
            [
                "FEMA flood screening",
                "USFWS wetlands screening",
                "USGS terrain screening",
                "USDA soils screening",
            ]
        )
    return {
        "state": state,
        "county": county,
        "city_or_unincorporated": city_field,
        "special_districts": [],
        "federal_relevance": federal_relevance,
        "coverage_tier": coverage_assessment.get("effective_coverage_tier"),
    }


def _candidate_clue_field(
    candidate: dict[str, Any] | None,
    clue_type: str,
    items_by_id: dict[str, dict[str, Any]],
    *,
    default_note: str,
) -> dict[str, Any]:
    item = _item_by_candidate_clue_type(candidate, clue_type, items_by_id)
    if item is not None:
        return _field_from_item(
            item,
            default_note=default_note,
            candidate_links={item["item_id"]: [candidate["candidate_id"]]} if candidate else {},
            evidence_scope=_confirmation_scope(candidate["confirmation_level"]) if candidate else None,
        )
    if candidate is None:
        return evidence_field(notes=default_note)
    if clue_type == "apn" and candidate.get("apn_raw"):
        return evidence_field(
            candidate["apn_raw"],
            status="confirmed" if candidate["confirmation_level"] == "parcel_confirmed" else "estimated",
            confidence=0.9 if candidate["confirmation_level"] == "parcel_confirmed" else 0.62,
            evidence_scope=_confirmation_scope(candidate["confirmation_level"]),
            linked_candidate_ids=[candidate["candidate_id"]],
            notes="Candidate APN synthesized from parcel-candidate clues rather than a dedicated assessor export.",
        )
    if clue_type == "address" and candidate.get("parcel_address"):
        return address_field(
            candidate["parcel_address"],
            status="confirmed" if candidate["confirmation_level"] == "parcel_confirmed" else "estimated",
            confidence=0.86 if candidate["confirmation_level"] == "parcel_confirmed" else 0.64,
            evidence_scope=_confirmation_scope(candidate["confirmation_level"]),
            linked_candidate_ids=[candidate["candidate_id"]],
            notes="Candidate address remains subject to official parcel-record reconciliation.",
        )
    return evidence_field(
        notes=default_note,
        linked_candidate_ids=[candidate["candidate_id"]] if candidate else [],
        evidence_scope=_confirmation_scope(candidate["confirmation_level"]) if candidate else "unresolved",
    )


def _candidate_address_field(
    candidate: dict[str, Any] | None,
    items_by_id: dict[str, dict[str, Any]],
    *,
    default_note: str,
) -> dict[str, Any]:
    item = _item_by_candidate_clue_type(candidate, "address", items_by_id)
    if item is not None:
        return _address_from_item(
            item,
            default_note=default_note,
            candidate_links={item["item_id"]: [candidate["candidate_id"]]} if candidate else {},
            evidence_scope=_confirmation_scope(candidate["confirmation_level"]) if candidate else None,
        )
    if candidate is not None and candidate.get("parcel_address"):
        return address_field(
            candidate["parcel_address"],
            status="confirmed" if candidate["confirmation_level"] == "parcel_confirmed" else "estimated",
            confidence=0.86 if candidate["confirmation_level"] == "parcel_confirmed" else 0.64,
            evidence_scope=_confirmation_scope(candidate["confirmation_level"]),
            linked_candidate_ids=[candidate["candidate_id"]],
            notes="Candidate address was synthesized from corroborating parcel clues.",
        )
    return address_field(notes=default_note)


def _candidate_acreage_field(
    candidate: dict[str, Any] | None,
    *,
    default_note: str,
) -> dict[str, Any]:
    if candidate is None or not candidate.get("acreage_claims"):
        return evidence_field(notes=default_note)
    claims = sorted(
        candidate["acreage_claims"],
        key=lambda claim: (claim["evidence_scope"] != "parcel_candidate", -claim["confidence"]),
    )
    claim = claims[0]
    return evidence_field(
        claim["value_acres"],
        status=claim["status"],
        confidence=claim["confidence"],
        evidence_scope=claim["evidence_scope"],
        citation_ids=claim["citation_ids"],
        supporting_item_ids=[claim["source_item_id"]] if claim["source_item_id"] else [],
        linked_candidate_ids=[candidate["candidate_id"]],
        notes="; ".join(claim["notes"]) if claim["notes"] else default_note,
    )


def _parcel_record_from_candidate(
    candidate: dict[str, Any],
    items_by_id: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    county_parcel_item = _item_by_candidate_clue_type(candidate, "county_parcel_id", items_by_id)
    owner_item = _item_by_candidate_clue_type(candidate, "owner_name", items_by_id)
    geometry_source_id = next(
        (
            clue["source_id"]
            for clue in candidate.get("source_clues", [])
            if clue["clue_type"] == "coordinates" or "interactive_mapping" in clue["source_id"] or "gis" in clue["source_id"]
        ),
        None,
    )
    geometry_confidence = {
        "boundary_confirmed": 0.95,
        "map_page_hint": 0.45,
        "point_only": 0.28,
        "none": 0.0,
    }[candidate["geometry_status"]]

    return {
        "candidate_id": candidate["candidate_id"],
        "candidate_strength": candidate["candidate_strength"],
        "confirmation_level": candidate["confirmation_level"],
        "confirmation_basis": candidate.get("confirmation_basis", "unknown"),
        "supporting_source_ids": sorted_unique_strings(candidate["supporting_source_ids"]),
        "contradicting_source_ids": sorted_unique_strings(candidate["contradicting_source_ids"]),
        "geometry_status": candidate["geometry_status"],
        "apn": _candidate_clue_field(
            candidate,
            "apn",
            items_by_id,
            default_note="No APN clue is currently attached to this candidate.",
        ),
        "county_parcel_id": _field_from_item(
            county_parcel_item,
            default_note="County parcel ID has not been recovered for this candidate.",
            candidate_links={county_parcel_item["item_id"]: [candidate["candidate_id"]]} if county_parcel_item else {},
            evidence_scope=_confirmation_scope(candidate["confirmation_level"]),
        ),
        "assessor_site_address": _candidate_address_field(
            candidate,
            items_by_id,
            default_note="No assessor or site address was recovered for this candidate.",
        ),
        "assessor_acreage": _candidate_acreage_field(
            candidate,
            default_note="No acreage claim has been recovered for this candidate.",
        ),
        "geometry_source_id": geometry_source_id,
        "geometry_confidence": geometry_confidence,
        "owner_name_public": _field_from_item(
            owner_item,
            default_note="Owner name extraction remains unresolved for this candidate.",
            candidate_links={owner_item["item_id"]: [candidate["candidate_id"]]} if owner_item else {},
            evidence_scope=_confirmation_scope(candidate["confirmation_level"]),
        ),
        "tax_status_summary": evidence_field(
            notes="Tax-roll extraction is not yet implemented beyond high-level parcel-candidate clues.",
            evidence_scope=_confirmation_scope(candidate["confirmation_level"]),
            linked_candidate_ids=[candidate["candidate_id"]],
        ),
    }


def _parcel_identity(
    subject_resolution: dict[str, Any],
    grouped_items: dict[str, list[dict[str, Any]]],
    items_by_id: dict[str, dict[str, Any]],
    parcel_candidates: dict[str, Any],
    candidate_links: dict[str, list[str]],
) -> dict[str, Any]:
    candidates = parcel_candidates.get("candidates", [])
    primary_candidate = _primary_candidate(parcel_candidates)
    parcel_records = [
        _parcel_record_from_candidate(candidate, items_by_id)
        for candidate in candidates
    ]

    conflict_items = []
    listing_acreage = _best_item(grouped_items, "listing.listed_acreage")
    if primary_candidate is not None and listing_acreage is not None:
        official_acreage = None
        if primary_candidate.get("acreage_claims"):
            official_like_claims = [
                claim
                for claim in primary_candidate["acreage_claims"]
                if claim["evidence_scope"] in {"parcel_candidate", "parcel_confirmed"}
            ]
            if official_like_claims:
                official_acreage = official_like_claims[0]["value_acres"]
        listing_value = listing_acreage["value"]
        if isinstance(listing_value, (int, float)) and isinstance(official_acreage, (int, float)) and abs(float(listing_value) - float(official_acreage)) > 0.01:
            conflict_items.append(
                {
                    "field_name": "acreage",
                    "description": (
                        f"Listing acreage {listing_value} differs from candidate acreage {official_acreage}."
                    ),
                    "supporting_item_ids": [listing_acreage["item_id"]],
                    "linked_candidate_ids": [primary_candidate["candidate_id"]],
                    "citation_ids": listing_acreage["citation_ids"],
                }
            )
    if parcel_candidates.get("candidate_set_status") == "multiple_competing_candidates":
        conflict_items.append(
            {
                "field_name": "parcel_identity",
                "description": "Multiple competing parcel candidates remain active and were not collapsed into one parcel.",
                "supporting_item_ids": _supporting_item_ids(
                    [
                        item
                        for candidate in candidates
                        for clue in candidate.get("source_clues", [])
                        if clue.get("source_item_id") and (item := items_by_id.get(clue["source_item_id"])) is not None
                    ]
                ),
                "linked_candidate_ids": sorted_unique_strings([candidate["candidate_id"] for candidate in candidates]),
                "citation_ids": sorted_unique_strings(
                    [
                        citation_id
                        for candidate in candidates
                        for clue in candidate.get("source_clues", [])
                        if clue.get("source_item_id") and (item := items_by_id.get(clue["source_item_id"])) is not None
                        for citation_id in item["citation_ids"]
                    ]
                ),
            }
        )

    coordinates_item = _best_item(grouped_items, "subject.coordinates")
    if coordinates_item is None:
        coordinates_value = _subject_coordinates_from_resolution(subject_resolution)
        access_point = coordinates_field(
            coordinates_value,
            status="estimated" if coordinates_value else "missing",
            confidence=0.64 if coordinates_value else 0.0,
            evidence_scope="geography_only" if coordinates_value else "unresolved",
            linked_candidate_ids=[primary_candidate["candidate_id"]] if primary_candidate else [],
            notes="Access-point estimate is still a point hint rather than parcel geometry."
            if coordinates_value
            else None,
        )
    else:
        access_point = _coordinates_from_item(
            coordinates_item,
            default_note="Access-point estimate is still a point hint rather than parcel geometry.",
            candidate_links=candidate_links,
            evidence_scope="geography_only",
        )

    if primary_candidate is not None and primary_candidate["confirmation_level"] == "parcel_confirmed":
        basis = primary_candidate.get("confirmation_basis", "unknown").replace("_", "-")
        boundary_notes = (
            "Current evidence supports one conservatively parcel-confirmed candidate "
            f"on a {basis} basis, but Step 7 still does not include boundary-geometry retrieval."
        )
    elif primary_candidate is not None:
        basis = primary_candidate.get("confirmation_basis", "unknown")
        if basis == "unknown":
            boundary_notes = "Current parcel identity remains candidate-based; point locations and local pages should not be treated as final parcel-boundary confirmation."
        else:
            boundary_notes = (
                "Current parcel identity remains candidate-based; the leading candidate is "
                f"{basis.replace('_', '-')} rather than geometry-confirmed, so point locations and local pages "
                "should not be treated as final parcel-boundary confirmation."
            )
    else:
        boundary_notes = "Parcel identity remains unresolved."

    return {
        "candidate_set_status": parcel_candidates["candidate_set_status"],
        "candidate_strategy": parcel_candidates["candidate_strategy"],
        "primary_candidate_id": parcel_candidates["primary_candidate_id"],
        "active_candidate_ids": [candidate["candidate_id"] for candidate in candidates],
        "overall_confirmation_level": primary_candidate["confirmation_level"] if primary_candidate else "unresolved",
        "overall_confirmation_basis": primary_candidate.get("confirmation_basis", "unknown") if primary_candidate else "unknown",
        "parcels": parcel_records,
        "access_point_estimate": access_point,
        "boundary_notes": boundary_notes,
        "identity_conflicts": conflict_items,
    }


def _planning_and_land_use(
    grouped_items: dict[str, list[dict[str, Any]]],
    source_plan: dict[str, Any],
    candidate_links: dict[str, list[str]],
) -> dict[str, Any]:
    planning_titles = _all_items(grouped_items, "page.title")
    zoning_item = _best_item(grouped_items, "planning.zoning_reference")
    planning_title_texts = [
        item["value"]
        for item in planning_titles
        if item["section_hint"] == "planning_and_land_use"
    ]
    development_summary = None
    if planning_title_texts:
        development_summary = _field_from_item(
            {
                "value": (
                    "Local planning-related pages were reachable, but Step 7 did not yet retrieve a parcel-specific zoning designation. "
                    + "Observed planning surfaces: "
                    + "; ".join(planning_title_texts)
                    + "."
                ),
                "status": "estimated",
                "confidence": 0.58,
                "evidence_scope": "county_level",
                "citation_ids": sorted_unique_strings(
                    [
                        citation_id
                        for item in planning_titles
                        if item["section_hint"] == "planning_and_land_use"
                        for citation_id in item["citation_ids"]
                    ]
                ),
                "observed_at": planning_titles[0]["observed_at"],
                "notes": ["Page access does not equal parcel-specific zoning confirmation."],
            }
        )
    else:
        development_summary = evidence_field(
            notes="No local planning or zoning source was successfully retrieved in Step 7."
        )

    interpretation_limits = [
        "Step 7 does not yet interpret local zoning text as legal advice or final entitlement feasibility."
    ]
    if "zoning" in source_plan.get("missing_capabilities", []):
        interpretation_limits.append(
            "No strong zoning source was available in the current Step 5 plan, so zoning remains unresolved."
        )
    return {
        "zoning_designation": _field_from_item(
            zoning_item,
            default_note="Parcel-specific zoning designation has not been verified in Step 7.",
            candidate_links=candidate_links,
        ),
        "zoning_description": evidence_field(
            notes="Zoning description remains unresolved without a parcel-specific zoning record."
        ),
        "future_land_use_designation": evidence_field(
            notes="Future land-use designation is not yet extracted in Step 7."
        ),
        "plan_area": evidence_field(
            value=planning_title_texts[0] if planning_title_texts else None,
            status="estimated" if planning_title_texts else "missing",
            confidence=0.35 if planning_title_texts else 0.0,
            evidence_scope="county_level" if planning_title_texts else "unresolved",
            supporting_item_ids=[planning_titles[0]["item_id"]] if planning_title_texts else [],
            linked_candidate_ids=_linked_candidate_ids_for_items([planning_titles[0]], candidate_links) if planning_title_texts else [],
            citation_ids=planning_titles[0]["citation_ids"] if planning_title_texts else [],
            as_of=planning_titles[0]["observed_at"] if planning_title_texts else None,
            notes="Derived from a reachable planning surface title, not a parcel-specific planning map."
            if planning_title_texts
            else None,
        ),
        "overlay_districts": [],
        "minimum_lot_size": evidence_field(
            notes="Minimum lot size is not yet extracted in Step 7."
        ),
        "setback_signals": [],
        "subdivision_signals": [],
        "permitted_use_signals": [],
        "conditional_use_signals": [],
        "development_constraints_summary": development_summary,
        "interpretation_limits": interpretation_limits,
    }


def _environmental_constraints(
    grouped_items: dict[str, list[dict[str, Any]]],
    candidate_links: dict[str, list[str]],
) -> dict[str, Any]:
    flood_item = _best_item(grouped_items, "environmental.flood_zone")
    wetlands_item = _best_item(grouped_items, "environmental.wetlands_signal")
    elevation_item = _best_item(grouped_items, "environmental.elevation_meters")
    soil_item = _best_item(grouped_items, "environmental.soil_constraints_signal")

    slope_field = evidence_field(
        notes="Slope remains unresolved without a parcel-specific terrain interpretation."
    )
    if soil_item is not None and isinstance(soil_item["value"], str):
        slope_field = evidence_field(
            soil_item["value"],
            status="estimated",
            confidence=0.56,
            evidence_scope="geography_only",
            supporting_item_ids=[soil_item["item_id"]],
            citation_ids=soil_item["citation_ids"],
            as_of=soil_item["observed_at"],
            notes="Slope clue is embedded in the NRCS soil summary rather than derived from parcel geometry.",
        )

    if elevation_item is not None and isinstance(elevation_item["value"], (int, float)):
        elevation_field = evidence_field(
            f"USGS 3DEP point elevation sample: {elevation_item['value']} meters.",
            status="confirmed",
            confidence=elevation_item["confidence"],
            evidence_scope="geography_only",
            supporting_item_ids=[elevation_item["item_id"]],
            citation_ids=elevation_item["citation_ids"],
            as_of=elevation_item["observed_at"],
            notes="Point elevation sample only; not a full topographic profile.",
        )
    else:
        elevation_field = evidence_field(
            notes="Elevation could not be sampled in Step 7."
        )

    summary_bits = []
    for item in [flood_item, wetlands_item, soil_item]:
        if item is not None and isinstance(item["value"], str):
            summary_bits.append(item["value"])
    if elevation_item is not None and isinstance(elevation_item["value"], (int, float)):
        summary_bits.append(f"USGS point elevation sampled at {elevation_item['value']} meters.")
    return {
        "flood_zone": _field_from_item(
            flood_item,
            default_note="Flood screening was not executed or remained unsupported in Step 7.",
            candidate_links=candidate_links,
            evidence_scope="geography_only",
        ),
        "wetlands_signal": _field_from_item(
            wetlands_item,
            default_note="Wetlands screening was not executed or remained unsupported in Step 7.",
            candidate_links=candidate_links,
            evidence_scope="geography_only",
        ),
        "slope_signal": slope_field,
        "elevation_signal": elevation_field,
        "soil_constraints_signal": _field_from_item(
            soil_item,
            default_note="Soil constraints screening was not executed or remained unsupported in Step 7.",
            candidate_links=candidate_links,
            evidence_scope="geography_only",
        ),
        "fire_risk_signal": evidence_field(
            notes="Fire-risk screening is outside the Step 7 universal baseline."
        ),
        "habitat_or_conservation_signal": evidence_field(
            notes="Habitat or conservation overlays are not yet broadly supported in Step 7."
        ),
        "superfund_or_contamination_signal": evidence_field(
            notes="Environmental contamination screening remains unsupported in Step 7."
        ),
        "environmental_summary": evidence_field(
            " ".join(summary_bits) if summary_bits else None,
            status="estimated" if summary_bits else "missing",
            confidence=0.62 if summary_bits else 0.0,
            evidence_scope="geography_only" if summary_bits else "unresolved",
            supporting_item_ids=_supporting_item_ids(
                [
                    item
                    for item in [flood_item, wetlands_item, soil_item, elevation_item]
                    if item is not None
                ]
            ),
            citation_ids=sorted_unique_strings(
                [
                    citation_id
                    for item in [flood_item, wetlands_item, soil_item, elevation_item]
                    if item is not None
                    for citation_id in item["citation_ids"]
                ]
            ),
            as_of=next(
                (
                    item["observed_at"]
                    for item in [flood_item, wetlands_item, soil_item, elevation_item]
                    if item is not None
                ),
                None,
            ),
            notes="Step 7 environmental findings are still point-based or partial unless parcel geometry has been confirmed."
            if summary_bits
            else None,
        ),
        "regulatory_notes": [
            "Flood, wetlands, soils, and elevation are screened directionally in Step 7 and should not be treated as final parcel-boundary determinations."
        ],
    }


def _water_and_agriculture(
    grouped_items: dict[str, list[dict[str, Any]]],
    candidate_links: dict[str, list[str]],
) -> dict[str, Any]:
    soil_productivity_item = _best_item(grouped_items, "water_ag.soil_productivity_signal")
    return {
        "surface_water_proximity": evidence_field(
            notes="Surface-water proximity is not yet calculated in Step 7."
        ),
        "groundwater_or_well_signal": evidence_field(
            notes="Groundwater or well screening is not yet implemented in Step 7."
        ),
        "irrigation_district_signal": evidence_field(
            notes="Irrigation district coverage is not yet implemented in Step 7."
        ),
        "water_rights_status": evidence_field(
            notes="Water-rights status remains unresolved and Step 7 provides no legal interpretation."
        ),
        "soil_productivity_signal": _field_from_item(
            soil_productivity_item,
            default_note="Soil productivity screening was not recovered in Step 7.",
            candidate_links=candidate_links,
            evidence_scope="geography_only",
        ),
        "cropland_or_pasture_signal": evidence_field(
            notes="Cropland or pasture classification is not yet implemented in Step 7."
        ),
        "ag_exemption_signal": evidence_field(
            notes="Ag exemption status is not yet implemented in Step 7."
        ),
        "water_and_ag_summary": _field_from_item(
            soil_productivity_item,
            default_note="Water and agriculture remain under-resolved in Step 7.",
            candidate_links=candidate_links,
            evidence_scope="geography_only",
        ),
    }


def _infrastructure_and_utilities(
    grouped_items: dict[str, list[dict[str, Any]]],
    source_plan: dict[str, Any],
    candidate_links: dict[str, list[str]],
) -> dict[str, Any]:
    listing_description = _best_item(grouped_items, "listing.description_text")
    utility_pages = [
        item
        for item in _all_items(grouped_items, "page.title")
        if item["section_hint"] == "infrastructure_and_utilities"
    ]
    access_signal = evidence_field(
        notes="Legal or physical access remains unresolved in Step 7."
    )
    if listing_description is not None and isinstance(listing_description["value"], str):
        lowered = listing_description["value"].lower()
        if "access" in lowered or "road" in lowered:
            access_signal = evidence_field(
                "Listing text mentions road or access context, but legal access remains unconfirmed.",
                status="estimated",
                confidence=0.52,
                evidence_scope="listing_derived",
                supporting_item_ids=[listing_description["item_id"]],
                linked_candidate_ids=_linked_candidate_ids_for_items([listing_description], candidate_links),
                citation_ids=listing_description["citation_ids"],
                as_of=listing_description["observed_at"],
                notes="Seller-facing access language is directional only.",
            )

    page_based_summary = None
    if utility_pages:
        page_based_summary = evidence_field(
            "Utility or transmission-related public entrypoints were reachable: "
            + "; ".join(item["value"] for item in utility_pages),
            status="estimated",
            confidence=0.5,
            evidence_scope=utility_pages[0].get("evidence_scope", "county_level"),
            supporting_item_ids=_supporting_item_ids(utility_pages),
            linked_candidate_ids=_linked_candidate_ids_for_items(utility_pages, candidate_links),
            citation_ids=sorted_unique_strings(
                [
                    citation
                    for item in utility_pages
                    for citation in item["citation_ids"]
                ]
            ),
            as_of=utility_pages[0]["observed_at"],
            notes="Page access does not confirm parcel serviceability or interconnection feasibility.",
        )
    else:
        page_based_summary = evidence_field(
            notes="No utility or transmission source returned a parcel-specific service finding in Step 7."
        )

    return {
        "legal_or_physical_access_signal": access_signal,
        "road_frontage_signal": evidence_field(
            notes="Road frontage remains unresolved without parcel geometry or county road context."
        ),
        "power_service_signal": evidence_field(
            notes="Power service availability is not yet confirmed in Step 7."
        ),
        "utility_territory_signal": evidence_field(
            notes="Utility territory mapping is not yet parcel-specific in Step 7."
        ),
        "substation_proximity_signal": evidence_field(
            notes="Substation proximity is not yet calculated in Step 7."
        ),
        "transmission_proximity_signal": evidence_field(
            notes="Transmission proximity is not yet calculated in Step 7."
        ),
        "broadband_signal": evidence_field(
            notes="Broadband dataset execution is not yet point-specific in Step 7."
        ),
        "water_service_signal": evidence_field(
            notes="Municipal or private water service remains unresolved in Step 7."
        ),
        "wastewater_or_septic_signal": evidence_field(
            notes="Wastewater or septic feasibility remains unresolved in Step 7."
        ),
        "rail_or_highway_logistics_signal": evidence_field(
            notes="Rail or highway logistics screening is not yet implemented in Step 7."
        ),
        "infrastructure_summary": page_based_summary,
    }


def _market_signals(
    grouped_items: dict[str, list[dict[str, Any]]],
    listing: dict[str, Any],
) -> dict[str, Any]:
    ask_price = listing["asking_price"]["value"]
    acreage = listing["listed_acreage"]["value"]
    ask_price_per_acre = None
    if isinstance(ask_price, (int, float)) and isinstance(acreage, (int, float)) and acreage > 0:
        ask_price_per_acre = round(float(ask_price) / float(acreage), 2)
    market_summary = None
    if ask_price_per_acre is not None:
        market_summary = evidence_field(
            f"Listing ask implies roughly ${ask_price_per_acre:,.2f} per acre.",
            status="confirmed",
            confidence=min(listing["asking_price"]["confidence"], listing["listed_acreage"]["confidence"]),
            evidence_scope="listing_derived",
            supporting_item_ids=sorted_unique_strings(
                [
                    *listing["asking_price"]["supporting_item_ids"],
                    *listing["listed_acreage"]["supporting_item_ids"],
                ]
            ),
            linked_candidate_ids=sorted_unique_strings(
                [
                    *listing["asking_price"]["linked_candidate_ids"],
                    *listing["listed_acreage"]["linked_candidate_ids"],
                ]
            ),
            citation_ids=sorted_unique_strings(
                [
                    *listing["asking_price"]["citation_ids"],
                    *listing["listed_acreage"]["citation_ids"],
                ]
            ),
            as_of=listing["listing_snapshot_at"],
            notes="Listing economics remain seller-facing until broader market comps are added.",
        )
    else:
        market_summary = evidence_field(
            notes="Listing ask or acreage was insufficient to compute a directional ask-per-acre signal."
        )
    return {
        "ask_price_per_acre": evidence_field(
            ask_price_per_acre,
            status="confirmed" if ask_price_per_acre is not None else "missing",
            confidence=market_summary["confidence"] if ask_price_per_acre is not None else 0.0,
            evidence_scope=market_summary["evidence_scope"] if ask_price_per_acre is not None else "unresolved",
            supporting_item_ids=market_summary["supporting_item_ids"] if ask_price_per_acre is not None else [],
            linked_candidate_ids=market_summary["linked_candidate_ids"] if ask_price_per_acre is not None else [],
            citation_ids=market_summary["citation_ids"] if ask_price_per_acre is not None else [],
            as_of=market_summary["as_of"] if ask_price_per_acre is not None else None,
            notes=market_summary["notes"] if ask_price_per_acre is not None else "Missing ask price or acreage.",
        ),
        "nearby_listing_context": [],
        "county_market_context": evidence_field(
            notes="County-wide market context is outside the Step 7 universal baseline."
        ),
        "tax_burden_signal": evidence_field(
            notes="Tax burden remains unresolved until parcel-specific tax roll extraction is added."
        ),
        "liquidity_signal": evidence_field(
            notes="Liquidity signal remains unresolved until broader market evidence is added."
        ),
        "market_summary": market_summary,
    }


def _risks(
    subject_resolution: dict[str, Any],
    source_plan: dict[str, Any],
    grouped_items: dict[str, list[dict[str, Any]]],
    parcel_candidates: dict[str, Any],
) -> list[dict[str, Any]]:
    risks = []
    if parcel_candidates.get("candidate_set_status") in {
        "single_weak_candidate",
        "multiple_competing_candidates",
        "no_viable_candidate",
    } or subject_resolution["resolution_status"] != "resolved":
        citation_ids = []
        listing_title = _best_item(grouped_items, "listing.title")
        if listing_title is not None:
            citation_ids = listing_title["citation_ids"]
        risks.append(
            {
                "risk_id": "parcel_identity_unconfirmed",
                "label": "Parcel Identity Unconfirmed",
                "severity": "high",
                "category": "identity",
                "description": "The active subject is not yet anchored to one official parcel record.",
                "impact": "Later zoning, tax, and environmental conclusions may attach to the wrong parcel.",
                "mitigation_or_next_check": "Confirm APN or county parcel ID against an official county or appraisal-district source.",
                "blocking": True,
                "citation_ids": citation_ids,
            }
        )
    if "zoning" in source_plan.get("missing_capabilities", []):
        risks.append(
            {
                "risk_id": "zoning_unresolved",
                "label": "Zoning Still Unresolved",
                "severity": "medium",
                "category": "zoning",
                "description": "No parcel-specific zoning source was confirmed in Step 7.",
                "impact": "Development and use-case feasibility remain constrained by local planning unknowns.",
                "mitigation_or_next_check": "Run a direct local zoning verification step before treating use assumptions as reliable.",
                "blocking": False,
                "citation_ids": [],
            }
        )
    if not _best_item(grouped_items, "environmental.flood_zone"):
        risks.append(
            {
                "risk_id": "flood_screen_incomplete",
                "label": "Flood Screen Incomplete",
                "severity": "medium",
                "category": "environmental",
                "description": "Flood screening was not completed or lacked a defensible point/parcel geometry.",
                "impact": "Environmental feasibility may change materially after parcel-specific flood review.",
                "mitigation_or_next_check": "Add coordinates or parcel geometry and rerun federal flood screening.",
                "blocking": subject_resolution["resolution_status"] != "resolved",
                "citation_ids": [],
            }
        )
    return risks


def _unknowns(
    subject_resolution: dict[str, Any],
    source_plan: dict[str, Any],
    fetch_log: dict[str, Any],
    parcel_candidates: dict[str, Any],
) -> list[dict[str, Any]]:
    unknowns = []
    if parcel_candidates.get("candidate_set_status") in {"multiple_competing_candidates", "single_weak_candidate"}:
        unknowns.append(
            {
                "unknown_id": "parcel_candidate_strengthening",
                "question": "Which candidate should be treated as the authoritative parcel target before local conclusions are hardened?",
                "why_it_matters": "Weak or competing candidate identity can attach local zoning, tax, or acreage findings to the wrong parcel.",
                "recommended_next_source": "official_county_parcel_source",
                "blocking": True,
            }
        )
    for index, question in enumerate(subject_resolution.get("unresolved_questions", []), start=1):
        unknowns.append(
            {
                "unknown_id": f"subject_resolution_q_{index}",
                "question": question,
                "why_it_matters": "Parcel-level diligence should not overclaim beyond the stabilized subject boundary.",
                "recommended_next_source": "official_county_parcel_source",
                "blocking": True,
            }
        )
    for capability in source_plan.get("missing_capabilities", []):
        unknowns.append(
            {
                "unknown_id": f"missing_{capability}",
                "question": f"What authoritative source will resolve the missing {capability} surface?",
                "why_it_matters": "A missing critical diligence surface lowers completeness and may change conclusions materially.",
                "recommended_next_source": capability,
                "blocking": capability in {"parcel_identity", "parcel_geometry", "zoning"},
            }
        )
    for entry in fetch_log.get("fetch_entries", []):
        if entry["retrieval_status"] not in {"failed", "blocked", "unsupported"}:
            continue
        if entry["source_id"] is None:
            continue
        unknowns.append(
            {
                "unknown_id": f"fetch_{entry['entry_id'].split('-')[0]}",
                "question": f"Why was {entry['source_name'] or entry['source_id']} not successfully executed?",
                "why_it_matters": "Unsuccessful retrieval leaves part of the diligence surface unverified.",
                "recommended_next_source": entry["source_id"],
                "blocking": entry["capability"] in {"parcel_identity", "parcel_geometry", "zoning"},
            }
        )
    # dedupe by unknown_id
    deduped = {}
    for unknown in unknowns:
        deduped[unknown["unknown_id"]] = unknown
    return sorted(deduped.values(), key=lambda item: item["unknown_id"])


def _next_actions(
    subject_resolution: dict[str, Any],
    source_plan: dict[str, Any],
    fetch_log: dict[str, Any],
    parcel_candidates: dict[str, Any],
) -> list[dict[str, Any]]:
    actions = []
    priority = 1
    if parcel_candidates.get("candidate_set_status") == "multiple_competing_candidates":
        actions.append(
            {
                "priority": priority,
                "action": "reconcile_competing_parcel_candidates",
                "reason": "Multiple competing parcel candidates remain active; confirm the authoritative APN or parcel record before relying on local findings.",
            }
        )
        priority += 1
    elif parcel_candidates.get("candidate_set_status") == "single_weak_candidate":
        actions.append(
            {
                "priority": priority,
                "action": "strengthen_primary_parcel_candidate",
                "reason": "Current parcel identity remains weak and needs official local corroboration.",
            }
        )
        priority += 1
    for action in subject_resolution.get("next_resolution_actions", []):
        actions.append(
            {
                "priority": priority,
                "action": action["action"],
                "reason": action["rationale"],
            }
        )
        priority += 1
    for step in source_plan.get("blocked_steps", []):
        actions.append(
            {
                "priority": priority,
                "action": f"resolve_{step['capability']}",
                "reason": step["reason"],
            }
        )
        priority += 1
    for entry in fetch_log.get("fetch_entries", []):
        if entry["retrieval_status"] not in {"failed", "unsupported"} or entry["source_id"] is None:
            continue
        actions.append(
            {
                "priority": priority,
                "action": f"retry_or_replace_{entry['source_id']}",
                "reason": entry["error_message"] or "Source execution did not succeed.",
            }
        )
        priority += 1
    return actions[:8]


def _merge_unknowns(
    base_unknowns: list[dict[str, Any]],
    extra_unknowns: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    merged: list[dict[str, Any]] = []
    seen: set[tuple[str, str]] = set()
    for unknown in base_unknowns + extra_unknowns:
        key = (unknown["question"], unknown.get("recommended_next_source") or "")
        if key in seen:
            continue
        seen.add(key)
        merged.append(unknown)
    return merged[:10]


def _merge_next_actions(
    base_actions: list[dict[str, Any]],
    extra_actions: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    merged: list[dict[str, Any]] = []
    seen: set[str] = set()
    for action in sorted(base_actions + extra_actions, key=lambda item: (item["priority"], item["action"])):
        if action["action"] in seen:
            continue
        seen.add(action["action"])
        merged.append(action)
    return merged[:10]


def _citation_objects(fetch_log: dict[str, Any]) -> list[dict[str, Any]]:
    citations = []
    for entry in fetch_log.get("fetch_entries", []):
        if entry["retrieval_status"] not in {"success", "partial"}:
            continue
        if entry["source_id"] is None:
            continue
        descriptor = source_by_id().get(entry["source_id"])
        if descriptor is None:
            continue
        primary_artifact = next(
            (
                artifact
                for artifact in entry["artifact_refs"]
                if artifact["artifact_type"] in {"raw_html", "raw_json", "raw_xml", "raw_text"}
            ),
            None,
        )
        citations.append(
            {
                "citation_id": f"c_{entry['entry_id'].split('-')[0]}",
                "source_id": entry["source_id"],
                "source_name": entry["source_name"] or descriptor["name"],
                "source_category": descriptor["category"],
                "interface_type": descriptor["interface_type"],
                "url": entry["request_descriptor"]["target_url"] or descriptor["base_url"],
                "retrieved_at": entry["completed_at"] or fetch_log["generated_at"],
                "published_or_effective_at": None,
                "authority_rank": descriptor["authority_rank"],
                "content_hash": primary_artifact["sha256"] if primary_artifact else None,
                "notes": "; ".join(entry["notes"]) if entry["notes"] else None,
            }
        )
    return sorted(citations, key=lambda item: item["citation_id"])


def _iter_status_confidence(obj: Any):
    if isinstance(obj, dict):
        if {"status", "confidence"} <= set(obj.keys()) and (
            "value" in obj or "label" in obj
        ):
            yield obj["status"], obj["confidence"]
            return
        for value in obj.values():
            yield from _iter_status_confidence(value)
        return
    if isinstance(obj, list):
        for value in obj:
            yield from _iter_status_confidence(value)


def _section_scores(section_obj: Any) -> tuple[float, float]:
    observations = list(_iter_status_confidence(section_obj))
    if not observations:
        return 0.0, 0.0
    available = [confidence for status, confidence in observations if status not in {"missing", "not_applicable"}]
    completeness = len(available) / len(observations)
    confidence = sum(available) / len(available) if available else 0.0
    return bounded(confidence), bounded(completeness)


def _scores(
    subject: dict[str, Any],
    parcel_identity: dict[str, Any],
    planning: dict[str, Any],
    environmental: dict[str, Any],
    water_ag: dict[str, Any],
    infrastructure: dict[str, Any],
    market: dict[str, Any],
    economics: dict[str, Any],
    coverage_assessment: dict[str, Any],
) -> dict[str, Any]:
    identity_confidence, identity_completeness = _section_scores({"subject": subject, "parcel_identity": parcel_identity})
    planning_confidence, planning_completeness = _section_scores(planning)
    environmental_confidence, environmental_completeness = _section_scores(environmental)
    water_confidence, water_completeness = _section_scores(water_ag)
    infrastructure_confidence, infrastructure_completeness = _section_scores(infrastructure)
    market_confidence, market_completeness = _section_scores(market)
    if economics["status"] == "available":
        economics_confidence = 0.58
        economics_completeness = 0.45
    elif economics["status"] == "limited":
        economics_confidence = 0.38
        economics_completeness = 0.28
    else:
        economics_confidence = 0.1
        economics_completeness = 0.05

    section_confidence = {
        "identity": identity_confidence,
        "planning": planning_confidence,
        "environmental": environmental_confidence,
        "water_and_agriculture": water_confidence,
        "infrastructure": infrastructure_confidence,
        "market": market_confidence,
        "economics": bounded(economics_confidence),
    }
    section_completeness = {
        "identity": identity_completeness,
        "planning": planning_completeness,
        "environmental": environmental_completeness,
        "water_and_agriculture": water_completeness,
        "infrastructure": infrastructure_completeness,
        "market": market_completeness,
        "economics": bounded(economics_completeness),
    }
    overall_confidence = bounded(sum(section_confidence.values()) / len(section_confidence))
    overall_completeness = bounded(sum(section_completeness.values()) / len(section_completeness))

    score_notes = [
        "Confidence reflects support quality for populated claims; completeness reflects how much of the diligence surface was actually covered.",
        "Step 9 use-case modules and directional economics remain screening-grade rather than underwriting-grade.",
    ]
    if coverage_assessment.get("source_summary", {}).get("only_federal_baseline_available"):
        score_notes.append("Only federal baseline sources were effectively available for local diligence context.")
    return {
        "overall_confidence": overall_confidence,
        "overall_completeness": overall_completeness,
        "section_confidence": section_confidence,
        "section_completeness": section_completeness,
        "coverage_tier": coverage_assessment.get("effective_coverage_tier"),
        "score_notes": score_notes,
    }


def assemble_land_research(
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
    jurisdiction_context: dict[str, Any],
    coverage_assessment: dict[str, Any],
    source_plan: dict[str, Any],
    fetch_log: dict[str, Any],
    extracted_evidence: dict[str, Any],
    *,
    workspace_root: str | Path,
    parcel_candidates: dict[str, Any] | None = None,
    generated_at: str | None = None,
) -> dict[str, Any]:
    parcel_candidates = parcel_candidates or build_parcel_candidates(
        request,
        subject_resolution,
        jurisdiction_context,
        fetch_log,
        extracted_evidence,
        workspace_root=workspace_root,
        generated_at=generated_at,
    )
    grouped_items = _items_by_field(extracted_evidence)
    items_by_id = _items_by_id(extracted_evidence)
    candidate_links = _candidate_item_links(parcel_candidates)
    request_context = _request_context(request)
    subject = _subject_context(subject_resolution, grouped_items, parcel_candidates, candidate_links)
    listing = _listing_metadata(subject_resolution, fetch_log, grouped_items, candidate_links)
    source_urls = _source_urls(fetch_log, subject_resolution)
    jurisdiction = _jurisdiction(jurisdiction_context, coverage_assessment)
    parcel_identity = _parcel_identity(subject_resolution, grouped_items, items_by_id, parcel_candidates, candidate_links)
    planning = _planning_and_land_use(grouped_items, source_plan, candidate_links)
    environmental = _environmental_constraints(grouped_items, candidate_links)
    water_ag = _water_and_agriculture(grouped_items, candidate_links)
    infrastructure = _infrastructure_and_utilities(grouped_items, source_plan, candidate_links)
    market = _market_signals(grouped_items, listing)
    citations = _citation_objects(fetch_log)
    base_parcel_memo = {
        "schema_version": "alice.land_research.v1",
        "object_id": deterministic_uuid(
            "alice-land-research",
            request["request_id"],
            fetch_log["fetch_log_id"],
        ),
        "report_type": {
            "recommendation": "recommendation_candidate",
            "deep_research": "deep_research",
            "follow_up": "deep_research",
            "batch_compare": "batch_compare_item",
        }[request["request_mode"]],
        "created_at": generated_at or now_iso(),
        "request_context": request_context,
        "subject": subject,
        "listing": listing,
        "source_urls": source_urls,
        "jurisdiction": jurisdiction,
        "parcel_identity": parcel_identity,
        "planning_and_land_use": planning,
        "environmental_constraints": environmental,
        "water_and_agriculture": water_ag,
        "infrastructure_and_utilities": infrastructure,
        "market_signals": market,
        "citations": citations,
    }
    use_case_results = evaluate_use_case_modules(
        request,
        base_parcel_memo,
        parcel_candidates=parcel_candidates,
        coverage_assessment=coverage_assessment,
        jurisdiction_context=jurisdiction_context,
    )
    use_case_modules = use_case_results["use_case_modules"]
    directional_economics = use_case_results["directional_economics"]
    risks = _risks(subject_resolution, source_plan, grouped_items, parcel_candidates)
    unknowns = _merge_unknowns(
        _unknowns(subject_resolution, source_plan, fetch_log, parcel_candidates),
        use_case_results["derived_unknowns"],
    )
    next_actions = _merge_next_actions(
        _next_actions(subject_resolution, source_plan, fetch_log, parcel_candidates),
        use_case_results["derived_next_actions"],
    )
    scores = _scores(
        subject,
        parcel_identity,
        planning,
        environmental,
        water_ag,
        infrastructure,
        market,
        directional_economics,
        coverage_assessment,
    )

    parcel_memo = {
        **base_parcel_memo,
        "use_case_modules": use_case_modules,
        "directional_economics": directional_economics,
        "risks": risks,
        "unknowns": unknowns,
        "next_actions": next_actions,
        "scores": scores,
    }

    output_path = _workspace_alice_root(workspace_root) / "parcel_memo.json"
    output_path.write_text(json.dumps(parcel_memo, indent=2), encoding="utf-8")
    summary_path = _workspace_alice_root(workspace_root) / "research_assembly_summary.json"
    summary_path.write_text(json.dumps(summarize_land_research(parcel_memo), indent=2), encoding="utf-8")
    return parcel_memo


def summarize_land_research(parcel_memo: dict[str, Any]) -> dict[str, Any]:
    return {
        "object_id": parcel_memo["object_id"],
        "report_type": parcel_memo["report_type"],
        "resolution_status": parcel_memo["subject"]["resolution_status"],
        "candidate_set_status": parcel_memo["parcel_identity"]["candidate_set_status"],
        "overall_confirmation_level": parcel_memo["parcel_identity"]["overall_confirmation_level"],
        "overall_confirmation_basis": parcel_memo["parcel_identity"]["overall_confirmation_basis"],
        "coverage_tier": parcel_memo["scores"]["coverage_tier"],
        "overall_confidence": parcel_memo["scores"]["overall_confidence"],
        "overall_completeness": parcel_memo["scores"]["overall_completeness"],
        "citation_count": len(parcel_memo["citations"]),
        "risk_count": len(parcel_memo["risks"]),
        "unknown_count": len(parcel_memo["unknowns"]),
    }


def load_land_research(path: str | Path) -> dict[str, Any]:
    return load_json(Path(path))
