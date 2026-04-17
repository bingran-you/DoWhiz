#!/usr/bin/env python3

"""Lightweight subject normalization and resolution helpers for Alice AI."""

from __future__ import annotations

import json
import re
from datetime import UTC, datetime
from functools import lru_cache
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, unquote, urlsplit, urlunsplit
from uuid import NAMESPACE_URL, uuid5

from alice_registry import get_county_row, county_rows, load_json


SUBJECT_RESOLUTION_VERSION = "alice.subject_resolution.v1"
SESSION_STATE_VERSION = "alice.session_state.v1"
REQUEST_MODES = {"recommendation", "deep_research", "batch_compare", "follow_up"}
LISTING_PLATFORMS = {
    "redfin",
    "zillow",
    "landwatch",
    "land_com",
    "loopnet",
    "broker_site",
    "county_listing",
    "unknown",
}
PLATFORM_LABELS = {
    "redfin": "Redfin",
    "zillow": "Zillow",
    "landwatch": "LandWatch",
    "land_com": "Land.com",
    "loopnet": "LoopNet",
    "broker_site": "Broker Site",
    "county_listing": "County Listing",
    "unknown": "Unknown Listing",
}

SCHEMA_ROOT = Path(__file__).resolve().parent.parent / "schemas"
SUBJECT_RESOLUTION_SCHEMA_PATH = SCHEMA_ROOT / "subject_resolution.schema.json"
SESSION_STATE_SCHEMA_PATH = SCHEMA_ROOT / "alice_session_state.schema.json"
REQUEST_SCHEMA_PATH = SCHEMA_ROOT / "alice_request.schema.json"


def now_iso() -> str:
    return datetime.now(UTC).isoformat().replace("+00:00", "Z")


def dedupe_strings(values: list[str]) -> list[str]:
    seen = set()
    deduped = []
    for value in values:
        if not value:
            continue
        if value in seen:
            continue
        deduped.append(value)
        seen.add(value)
    return deduped


def normalize_name_key(value: str) -> str:
    return re.sub(r"[^a-z0-9]", "", value.lower())


def title_from_slug(value: str) -> str:
    words = [token for token in re.split(r"[-_]+", value) if token]
    if not words:
        return value
    titled = []
    for word in words:
        if word.lower() in {"of", "and", "the", "in", "near"}:
            titled.append(word.lower())
        elif word.lower() == "st":
            titled.append("St.")
        else:
            titled.append(word.capitalize())
    return " ".join(titled)


def empty_resolution_hints() -> dict[str, Any]:
    return {
        "state_fips": None,
        "state_code": None,
        "state_name": None,
        "county_fips": None,
        "county_name": None,
        "city_name": None,
        "source_url": None,
    }


def empty_identifier_bundle() -> dict[str, Any]:
    return {
        "listing_platform": None,
        "canonical_url": None,
        "listing_id": None,
        "address_like_slug": None,
        "address_text": None,
        "address_parse_status": None,
        "apn_raw": None,
        "apn_normalized": None,
        "apn_matching_variants": [],
        "coordinates": None,
        "county_fips": None,
        "county_name": None,
        "state_fips": None,
        "state_code": None,
        "state_name": None,
        "city_name": None,
        "postal_code": None,
        "subject_ref_key": None,
    }


def empty_state_refs(request: dict[str, Any]) -> dict[str, Any]:
    follow_up = request.get("follow_up_context", {})
    return {
        "conversation_state_ref": request.get("conversation_state_ref"),
        "previous_session_state_ref": follow_up.get("previous_session_state_ref"),
        "previous_resolution_ref": follow_up.get("previous_resolution_ref"),
        "previous_report_ref": follow_up.get("previous_report_ref"),
    }


@lru_cache(maxsize=1)
def state_maps() -> dict[str, dict[str, str]]:
    states: dict[str, dict[str, str]] = {}
    for row in county_rows().values():
        states[row["state_code"]] = {
            "state_code": row["state_code"],
            "state_fips": row["state_fips"],
            "state_name": row["state_name"],
        }
    return states


@lru_cache(maxsize=1)
def state_name_map() -> dict[str, dict[str, str]]:
    mapping: dict[str, dict[str, str]] = {}
    for state in state_maps().values():
        mapping[normalize_name_key(state["state_name"])] = state
    return mapping


@lru_cache(maxsize=1)
def county_name_index() -> dict[tuple[str, str], dict[str, Any]]:
    index: dict[tuple[str, str], dict[str, Any]] = {}
    for row in county_rows().values():
        index[(row["state_code"], normalize_name_key(row["county_name"]))] = row
    return index


def resolve_state_hint(*values: str | None) -> dict[str, str] | None:
    for value in values:
        if not value:
            continue
        raw = value.strip()
        if not raw:
            continue
        if re.fullmatch(r"[A-Z]{2}", raw) and raw in state_maps():
            return state_maps()[raw]
        if re.fullmatch(r"[0-9]{2}", raw):
            for state in state_maps().values():
                if state["state_fips"] == raw:
                    return state
        mapped = state_name_map().get(normalize_name_key(raw))
        if mapped is not None:
            return mapped
    return None


def resolve_county_hint(state_code: str | None, county_name: str | None) -> dict[str, Any] | None:
    if not state_code or not county_name:
        return None
    return county_name_index().get((state_code, normalize_name_key(county_name)))


def merge_resolution_hints(base: dict[str, Any], overrides: dict[str, Any]) -> dict[str, Any]:
    merged = dict(base)
    for key, value in overrides.items():
        if value in (None, "", []):
            continue
        merged[key] = value
    if merged.get("county_fips"):
        county = get_county_row(merged["county_fips"])
        merged["county_name"] = county["county_name"]
        merged["state_fips"] = county["state_fips"]
        merged["state_code"] = county["state_code"]
        merged["state_name"] = county["state_name"]
    else:
        state = resolve_state_hint(merged.get("state_code"), merged.get("state_fips"), merged.get("state_name"))
        if state is not None:
            merged["state_code"] = state["state_code"]
            merged["state_fips"] = state["state_fips"]
            merged["state_name"] = state["state_name"]
            county = resolve_county_hint(state["state_code"], merged.get("county_name"))
            if county is not None:
                merged["county_fips"] = county["county_fips"]
                merged["county_name"] = county["county_name"]
    return merged


def merge_normalized_values(base: dict[str, Any], overrides: dict[str, Any]) -> dict[str, Any]:
    merged = dict(base)
    for key, value in overrides.items():
        if value in (None, "", []):
            continue
        merged[key] = value
    return merged


def normalize_canonical_url(raw_url: str) -> tuple[str, Any]:
    parsed = urlsplit(raw_url.strip())
    if not parsed.scheme:
        parsed = urlsplit(f"https://{raw_url.strip()}")
    path = parsed.path or "/"
    if path != "/" and path.endswith("/"):
        path = path.rstrip("/")
    canonical = urlunsplit((parsed.scheme.lower(), parsed.netloc.lower(), path, "", ""))
    return canonical, parsed


def classify_listing_platform(netloc: str) -> str:
    host = netloc.lower()
    if "redfin." in host:
        return "redfin"
    if "zillow." in host:
        return "zillow"
    if "landwatch." in host:
        return "landwatch"
    if "land.com" in host:
        return "land_com"
    if "loopnet." in host:
        return "loopnet"
    if any(keyword in host for keyword in ["county", "assessor", "appraisal", "tax"]):
        return "county_listing"
    if host:
        return "broker_site"
    return "unknown"


def parse_coordinates_pair(latitude: Any, longitude: Any) -> dict[str, float] | None:
    try:
        lat = float(latitude)
        lon = float(longitude)
    except (TypeError, ValueError):
        return None
    if not (-90 <= lat <= 90 and -180 <= lon <= 180):
        return None
    return {
        "latitude": round(lat, 6),
        "longitude": round(lon, 6),
    }


def extract_coordinates_from_text(raw_text: str) -> dict[str, float] | None:
    matches = re.findall(r"[-+]?\d+(?:\.\d+)?", raw_text)
    if len(matches) < 2:
        return None
    return parse_coordinates_pair(matches[0], matches[1])


def infer_state_from_segments(segments: list[str]) -> dict[str, str] | None:
    for segment in segments:
        raw = unquote(segment)
        if re.fullmatch(r"[A-Z]{2}", raw):
            state = resolve_state_hint(raw)
            if state is not None:
                return state
    for segment in segments:
        state = resolve_state_hint(unquote(segment))
        if state is not None:
            return state
        for token in re.split(r"[-_]", unquote(segment)):
            if re.fullmatch(r"[A-Z]{2}", token):
                state = resolve_state_hint(token)
                if state is not None:
                    return state
            elif len(token) > 2:
                state = resolve_state_hint(token)
                if state is not None:
                    return state
    return None


def infer_county_text_from_segments(segments: list[str]) -> str | None:
    county_suffixes = {"county", "parish", "borough", "municipio", "district", "city", "island"}
    stop_tokens = {"in", "near", "at", "of", "for", "property", "land", "acre", "acres"}
    for segment in segments:
        raw = unquote(segment)
        normalized = raw.lower()
        tokens = [token for token in normalized.split("-") if token]
        for index, token in enumerate(tokens):
            if token not in county_suffixes and not (
                token == "area" and index > 0 and tokens[index - 1] == "census"
            ):
                continue
            trailing_tokens = tokens[index + 1 :]
            if trailing_tokens and resolve_state_hint(trailing_tokens[0]) is None:
                continue
            start = 0
            for reverse_index in range(index - 1, -1, -1):
                if tokens[reverse_index] in stop_tokens or re.fullmatch(r"\d+", tokens[reverse_index]):
                    start = reverse_index + 1
                    break
            candidate_tokens = tokens[start : index + 1]
            if not candidate_tokens:
                continue
            candidate = "-".join(candidate_tokens)
            if candidate:
                return title_from_slug(candidate)
    return None


def normalize_listing_url(raw_url: str) -> dict[str, Any]:
    notes: list[str] = []
    normalized: dict[str, Any] = {}
    hints = empty_resolution_hints()

    canonical_url, parsed = normalize_canonical_url(raw_url)
    platform = classify_listing_platform(parsed.netloc)
    segments = [segment for segment in parsed.path.split("/") if segment]
    query = parse_qs(parsed.query)

    normalized["platform"] = platform
    normalized["canonical_url"] = canonical_url
    hints["source_url"] = canonical_url

    if parsed.query or parsed.fragment:
        notes.append("Stripped query parameters and fragments from the canonical URL.")

    state = infer_state_from_segments(segments)
    county_text = infer_county_text_from_segments(segments)
    if state is not None:
        normalized["state_code_hint"] = state["state_code"]
        hints["state_code"] = state["state_code"]
        hints["state_fips"] = state["state_fips"]
        hints["state_name"] = state["state_name"]
    if county_text is not None:
        normalized["county_text_hint"] = county_text
        hints["county_name"] = county_text
        if state is not None:
            county = resolve_county_hint(state["state_code"], county_text)
            if county is not None:
                hints["county_fips"] = county["county_fips"]
                hints["county_name"] = county["county_name"]

    if platform == "redfin":
        if "home" in segments:
            home_index = segments.index("home")
            if home_index > 0:
                normalized["address_like_slug"] = unquote(segments[home_index - 1])
            if home_index + 1 < len(segments):
                match = re.search(r"(\d+)", segments[home_index + 1])
                if match is not None:
                    normalized["listing_id"] = match.group(1)
    elif platform == "zillow":
        if segments:
            tail = segments[-1]
            match = re.search(r"(\d+)_zpid", tail)
            if match is not None:
                normalized["listing_id"] = match.group(1)
            if len(segments) >= 2:
                normalized["address_like_slug"] = unquote(segments[-2])
            elif "homedetails" in segments and len(segments) >= 2:
                normalized["address_like_slug"] = unquote(segments[1])
    elif platform == "landwatch":
        listing_id = None
        address_slug = None
        for idx, segment in enumerate(segments):
            if segment.lower() in {"id", "listing"} and idx + 1 < len(segments):
                if re.fullmatch(r"\d+", segments[idx + 1]):
                    listing_id = segments[idx + 1]
                    if idx > 0:
                        address_slug = unquote(segments[idx - 1])
                    break
        if listing_id is None and segments and re.fullmatch(r"\d+", segments[-1]):
            listing_id = segments[-1]
            if len(segments) >= 2:
                address_slug = unquote(segments[-2])
        if listing_id is not None:
            normalized["listing_id"] = listing_id
        if address_slug:
            normalized["address_like_slug"] = address_slug
    elif platform == "land_com":
        if segments and re.fullmatch(r"\d+", segments[-1]):
            normalized["listing_id"] = segments[-1]
            if len(segments) >= 2:
                normalized["address_like_slug"] = unquote(segments[-2])
        elif len(segments) >= 3 and re.fullmatch(r"\d+", segments[-2]):
            normalized["listing_id"] = segments[-2]
            normalized["address_like_slug"] = unquote(segments[-3])

    coordinates = None
    lat_values = query.get("lat") or query.get("latitude")
    lon_values = query.get("lng") or query.get("lon") or query.get("longitude") or query.get("long")
    if lat_values and lon_values:
        coordinates = parse_coordinates_pair(lat_values[0], lon_values[0])
        if coordinates is not None:
            normalized["coordinates"] = coordinates
            normalized["coordinate_source"] = "url_query"
            notes.append("Captured coordinate hints from URL query parameters.")

    if "listing_id" not in normalized:
        notes.append("Listing ID was not reliably extractable from the URL path.")
    if "address_like_slug" not in normalized and platform in {"redfin", "zillow", "landwatch", "land_com"}:
        notes.append("Address-like slug was not reliably extractable from the URL path.")

    status = "normalized" if platform in LISTING_PLATFORMS and "canonical_url" in normalized else "best_effort"
    return {
        "normalized": normalized,
        "resolution_hints": hints,
        "normalization_status": status,
        "notes": notes,
    }


def normalize_apn(raw_apn: str, *, county_fips: str | None = None, state_fips: str | None = None) -> dict[str, Any]:
    cleaned = re.sub(r"\s+", " ", raw_apn.strip()).upper()
    parts = [part for part in re.split(r"[^A-Z0-9]+", cleaned) if part]
    normalized = {
        "apn_raw": raw_apn,
        "apn_normalized": "".join(parts) if parts else cleaned,
    }
    notes: list[str] = ["Preserved the raw APN exactly as supplied."]
    hints = empty_resolution_hints()

    county = None
    if county_fips:
        county = get_county_row(county_fips)
        hints["county_fips"] = county["county_fips"]
        hints["county_name"] = county["county_name"]
        hints["state_fips"] = county["state_fips"]
        hints["state_code"] = county["state_code"]
        hints["state_name"] = county["state_name"]
    elif state_fips:
        state = resolve_state_hint(state_fips)
        if state is not None:
            hints["state_fips"] = state["state_fips"]
            hints["state_code"] = state["state_code"]
            hints["state_name"] = state["state_name"]

    variants = [normalized["apn_normalized"]]
    hyphenated = "-".join(parts)
    digits_only = "".join(character for character in normalized["apn_normalized"] if character.isdigit())
    strategy = "generic_alphanumeric"
    if county is not None:
        if county["state_fips"] == "06":
            strategy = "state_ca_numeric_hyphenated"
        elif county["state_fips"] == "48":
            strategy = "state_tx_alphanumeric_hyphenated"
        else:
            strategy = "county_hint_alphanumeric"
        notes.append("Used county context to generate conservative matching variants.")
    elif state_fips:
        strategy = "county_hint_alphanumeric"
        notes.append("Used state context to generate conservative matching variants.")
    else:
        notes.append("No county context was available, so only conservative generic normalization was applied.")

    for variant in [hyphenated, digits_only]:
        if variant and variant not in variants:
            variants.append(variant)

    normalized["apn_strategy"] = strategy
    normalized["apn_matching_variants"] = variants

    if any(character.isalpha() for character in normalized["apn_normalized"]):
        notes.append("The normalized APN retains letters; this is acceptable for matching but not proof of parcel identity.")

    return {
        "normalized": normalized,
        "resolution_hints": hints,
        "normalization_status": "normalized",
        "notes": notes,
    }


def normalize_address(raw_address: str) -> dict[str, Any]:
    collapsed = re.sub(r"\s+", " ", raw_address.strip())
    collapsed = re.sub(r"\s*,\s*", ", ", collapsed)
    notes: list[str] = []
    hints = empty_resolution_hints()

    normalized: dict[str, Any] = {
        "address_text": collapsed,
        "address_parse_status": "freeform",
    }

    parts = [part.strip() for part in collapsed.split(",") if part.strip()]
    if len(parts) >= 2:
        city = parts[-2]
        normalized["city_hint"] = city
        hints["city_name"] = city

    tail = parts[-1] if parts else collapsed
    state_match = re.search(r"\b([A-Z]{2})\b", tail)
    zip_match = re.search(r"\b(\d{5}(?:-\d{4})?)\b", tail)

    if state_match is not None:
        state = resolve_state_hint(state_match.group(1))
        if state is not None:
            normalized["state_code_hint"] = state["state_code"]
            hints["state_code"] = state["state_code"]
            hints["state_fips"] = state["state_fips"]
            hints["state_name"] = state["state_name"]

    if zip_match is not None:
        normalized["postal_code_hint"] = zip_match.group(1)

    if len(parts) >= 3 and normalized.get("state_code_hint"):
        normalized["address_parse_status"] = "parsed_components"
        notes.append("Parsed a street, city, state, and optional postal code from the address text.")
    elif len(parts) >= 2 and normalized.get("state_code_hint"):
        normalized["address_parse_status"] = "city_state_only"
        notes.append("Parsed city and state hints, but the address still needs geocoding.")
    elif normalized.get("state_code_hint"):
        normalized["address_parse_status"] = "state_only"
        notes.append("Parsed only state-level context from the address text.")
    else:
        notes.append("Address remains a freeform string until geocoding is available.")

    return {
        "normalized": normalized,
        "resolution_hints": hints,
        "normalization_status": "normalized",
        "notes": notes,
    }


def normalize_coordinates(raw_value: str) -> dict[str, Any]:
    coordinates = extract_coordinates_from_text(raw_value)
    hints = empty_resolution_hints()
    if coordinates is None:
        return {
            "normalized": {},
            "resolution_hints": hints,
            "normalization_status": "invalid",
            "notes": [
                "Could not parse a valid latitude/longitude pair from the supplied coordinates."
            ],
        }
    return {
        "normalized": {
            "coordinates": coordinates,
            "coordinate_source": "user_provided",
        },
        "resolution_hints": hints,
        "normalization_status": "normalized",
        "notes": [
            "Validated the coordinate pair, but did not infer parcel boundaries or county overlays in Step 4."
        ],
    }


def normalize_subject_reference(raw_value: str) -> dict[str, Any]:
    return {
        "normalized": {
            "subject_ref_key": raw_value.strip(),
        },
        "resolution_hints": empty_resolution_hints(),
        "normalization_status": "normalized",
        "notes": [
            "Subject reference depends on prior Alice session state and is not parcel resolution by itself."
        ],
    }


def normalize_thesis_input(raw_value: str) -> dict[str, Any]:
    return {
        "normalized": {},
        "resolution_hints": empty_resolution_hints(),
        "normalization_status": "normalized",
        "notes": [
            "This is a thesis-only input used to prepare recommendation search setup rather than parcel confirmation."
        ],
    }


def normalize_subject_input(subject: dict[str, Any], index: int) -> tuple[dict[str, Any], dict[str, Any]]:
    subject_id = subject.get("subject_id") or f"subject-{index + 1}"
    input_kind = subject["input_kind"]
    raw_value = subject["raw_value"]
    existing_normalized = subject.get("normalized", {})
    existing_hints = subject.get("resolution_hints", {})

    county_fips = existing_hints.get("county_fips")
    state_fips = existing_hints.get("state_fips")

    if input_kind == "listing_url":
        normalized_result = normalize_listing_url(raw_value)
    elif input_kind == "apn":
        normalized_result = normalize_apn(raw_value, county_fips=county_fips, state_fips=state_fips)
    elif input_kind == "address":
        normalized_result = normalize_address(raw_value)
    elif input_kind == "coordinates":
        normalized_result = normalize_coordinates(raw_value)
    elif input_kind == "subject_ref":
        normalized_result = normalize_subject_reference(raw_value)
    elif input_kind == "natural_language_intent":
        normalized_result = normalize_thesis_input(raw_value)
    else:
        raise ValueError(f"Unsupported input_kind for Step 4 subject resolution: {input_kind}")

    merged_normalized = merge_normalized_values(normalized_result["normalized"], existing_normalized)
    merged_hints = merge_resolution_hints(normalized_result["resolution_hints"], existing_hints)

    if merged_normalized.get("state_code_hint") and not merged_hints.get("state_code"):
        merged_hints = merge_resolution_hints(
            merged_hints,
            {
                "state_code": merged_normalized.get("state_code_hint"),
                "county_name": merged_normalized.get("county_text_hint"),
                "city_name": merged_normalized.get("city_hint"),
            },
        )

    if input_kind == "listing_url":
        merged_hints["source_url"] = merged_normalized.get("canonical_url", raw_value)

    raw_input = {
        "subject_id": subject_id,
        "input_kind": input_kind,
        "raw_value": raw_value,
    }
    normalized_input = {
        "subject_id": subject_id,
        "input_kind": input_kind,
        "raw_value": raw_value,
        "normalization_status": normalized_result["normalization_status"],
        "normalized": merged_normalized,
        "resolution_hints": merged_hints,
        "normalization_notes": normalized_result["notes"],
    }
    if existing_normalized:
        normalized_input["normalization_notes"].append(
            "Request-provided normalized fields were preserved where available."
        )
    if existing_hints:
        normalized_input["normalization_notes"].append(
            "Request-provided resolution hints were merged into the normalized subject."
        )
    normalized_input["normalization_notes"] = dedupe_strings(normalized_input["normalization_notes"])
    return raw_input, normalized_input


def make_action(action: str, priority: str, subject_id: str, rationale: str) -> dict[str, Any]:
    return {
        "action": action,
        "priority": priority,
        "subject_ids": [subject_id],
        "rationale": rationale,
    }


def format_display_label(input_kind: str, normalized_input: dict[str, Any], request: dict[str, Any]) -> str:
    normalized = normalized_input["normalized"]
    hints = normalized_input["resolution_hints"]
    if input_kind == "natural_language_intent":
        return request["thesis"]["summary"] or "Recommendation thesis request"
    if input_kind == "listing_url":
        platform = PLATFORM_LABELS.get(normalized.get("platform", "unknown"), "Listing")
        listing_id = normalized.get("listing_id")
        address_slug = normalized.get("address_like_slug")
        state_code = hints.get("state_code") or normalized.get("state_code_hint")
        if listing_id and address_slug and state_code:
            return f"{platform} listing {listing_id} near {title_from_slug(address_slug)} ({state_code})"
        if listing_id:
            return f"{platform} listing {listing_id}"
        return f"{platform} listing"
    if input_kind == "apn":
        if hints.get("county_name"):
            return f"{hints['county_name']} APN {normalized.get('apn_raw') or normalized.get('apn_normalized')}"
        return f"APN {normalized.get('apn_raw') or normalized.get('apn_normalized')}"
    if input_kind == "address":
        return normalized.get("address_text") or "Address subject"
    if input_kind == "coordinates":
        coordinates = normalized.get("coordinates")
        if coordinates is not None:
            return f"Point at {coordinates['latitude']}, {coordinates['longitude']}"
        return "Coordinate subject"
    if input_kind == "subject_ref":
        return f"Follow-up reference: {normalized.get('subject_ref_key') or normalized_input['raw_value']}"
    return normalized_input["raw_value"]


def build_identifiers(normalized_input: dict[str, Any]) -> dict[str, Any]:
    identifiers = empty_identifier_bundle()
    normalized = normalized_input["normalized"]
    hints = normalized_input["resolution_hints"]

    identifiers["listing_platform"] = normalized.get("platform")
    identifiers["canonical_url"] = normalized.get("canonical_url")
    identifiers["listing_id"] = normalized.get("listing_id")
    identifiers["address_like_slug"] = normalized.get("address_like_slug")
    identifiers["address_text"] = normalized.get("address_text")
    identifiers["address_parse_status"] = normalized.get("address_parse_status")
    identifiers["apn_raw"] = normalized.get("apn_raw")
    identifiers["apn_normalized"] = normalized.get("apn_normalized")
    identifiers["apn_matching_variants"] = normalized.get("apn_matching_variants", [])
    identifiers["coordinates"] = normalized.get("coordinates")
    identifiers["county_fips"] = hints.get("county_fips")
    identifiers["county_name"] = hints.get("county_name")
    identifiers["state_fips"] = hints.get("state_fips")
    identifiers["state_code"] = hints.get("state_code") or normalized.get("state_code_hint")
    identifiers["state_name"] = hints.get("state_name")
    identifiers["city_name"] = hints.get("city_name") or normalized.get("city_hint")
    identifiers["postal_code"] = normalized.get("postal_code_hint")
    identifiers["subject_ref_key"] = normalized.get("subject_ref_key")
    return identifiers


def build_subject_resolution_state(
    normalized_input: dict[str, Any],
    request: dict[str, Any],
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    input_kind = normalized_input["input_kind"]
    subject_id = normalized_input["subject_id"]
    normalized = normalized_input["normalized"]
    hints = normalized_input["resolution_hints"]
    identifiers = build_identifiers(normalized_input)
    display_label = format_display_label(input_kind, normalized_input, request)

    ambiguity_flags: list[str] = []
    unresolved_questions: list[str] = []
    next_actions: list[dict[str, Any]] = []
    parcel_candidates: list[dict[str, Any]] = []
    resolution_level = "geography_only"
    resolution_status = "unresolved"
    active_subject_kind = "geography_area"

    if input_kind == "natural_language_intent":
        active_subject_kind = "thesis"
        resolution_level = "thesis_only"
        resolution_status = "partially_resolved"
    elif input_kind == "listing_url":
        active_subject_kind = "listing"
        resolution_level = "listing_identified"
        resolution_status = "partially_resolved"
        ambiguity_flags.append("listing_to_parcel_unconfirmed")
        unresolved_questions.append(
            "Which parcel or APN on this listing should be treated as the authoritative parcel target?"
        )
        next_actions.append(
            make_action(
                "collect_additional_identifiers",
                "high",
                subject_id,
                "Listing URL parsing alone does not anchor the listing to a confirmed parcel.",
            )
        )
        if normalized.get("platform") in {"unknown", "broker_site"}:
            ambiguity_flags.append("listing_platform_best_effort")
        if not identifiers["state_code"]:
            ambiguity_flags.append("state_missing")
            unresolved_questions.append("What state is the listing parcel actually in?")
            next_actions.append(
                make_action(
                    "confirm_state_context",
                    "high",
                    subject_id,
                    "Listing URL parsing did not produce a reliable state hint.",
                )
            )
        if not identifiers["county_fips"]:
            ambiguity_flags.append("listing_metadata_incomplete")
            unresolved_questions.append("Which county or APN should be attached to this listing before deep research?")
        next_actions.append(
            make_action(
                "query_listing_page",
                "high",
                subject_id,
                "Listing-page retrieval is needed later to extract APN, address, acreage, and parcel clues.",
            )
        )
        candidate_id = f"cand-{subject_id}-1"
        candidate_label = f"Unconfirmed parcel from {display_label}"
        parcel_candidates.append(
            {
                "candidate_id": candidate_id,
                "source_subject_ids": [subject_id],
                "candidate_status": "hinted",
                "label": candidate_label,
                "apn_raw": None,
                "apn_normalized": None,
                "county_fips": identifiers["county_fips"],
                "county_name": identifiers["county_name"],
                "state_fips": identifiers["state_fips"],
                "state_code": identifiers["state_code"],
                "state_name": identifiers["state_name"],
                "address_text": title_from_slug(identifiers["address_like_slug"])
                if identifiers["address_like_slug"]
                else None,
                "coordinates": identifiers["coordinates"],
                "match_basis": dedupe_strings(
                    [
                        "listing_url_id" if identifiers["listing_id"] else "",
                        "listing_url_slug" if identifiers["address_like_slug"] else "",
                        "listing_url_coordinates" if identifiers["coordinates"] else "",
                    ]
                ),
                "confidence": 0.48 if identifiers["county_fips"] else 0.32,
                "notes": [
                    "Listing URL normalization does not confirm that the listing maps to one parcel.",
                ],
            }
        )
    elif input_kind == "apn":
        active_subject_kind = "parcel"
        resolution_level = "parcel_candidate_identified"
        resolution_status = "partially_resolved" if identifiers["county_fips"] else "unresolved"
        unresolved_questions.append("Can this APN be confirmed against official parcel records before research proceeds?")
        if not identifiers["county_fips"]:
            ambiguity_flags.append("apn_needs_county_context")
            unresolved_questions.append("What county or state should this APN be searched in?")
            next_actions.append(
                make_action(
                    "confirm_county_context",
                    "high",
                    subject_id,
                    "APN normalization does not reveal the correct county by itself.",
                )
            )
        next_actions.append(
            make_action(
                "query_parcel_by_apn",
                "high",
                subject_id,
                "Later parcel lookup should confirm the APN against county or appraisal-district sources.",
            )
        )
        if any(character.isalpha() for character in normalized.get("apn_normalized", "")):
            ambiguity_flags.append("apn_format_unusual")
        candidate_id = f"cand-{subject_id}-1"
        parcel_candidates.append(
            {
                "candidate_id": candidate_id,
                "source_subject_ids": [subject_id],
                "candidate_status": "probable" if identifiers["county_fips"] else "hinted",
                "label": display_label,
                "apn_raw": identifiers["apn_raw"],
                "apn_normalized": identifiers["apn_normalized"],
                "county_fips": identifiers["county_fips"],
                "county_name": identifiers["county_name"],
                "state_fips": identifiers["state_fips"],
                "state_code": identifiers["state_code"],
                "state_name": identifiers["state_name"],
                "address_text": None,
                "coordinates": None,
                "match_basis": ["user_supplied_apn"],
                "confidence": 0.82 if identifiers["county_fips"] else 0.45,
                "notes": [
                    "APN normalization is a matching aid only; official parcel lookup is still required.",
                ],
            }
        )
    elif input_kind == "address":
        active_subject_kind = "address_point"
        resolution_level = "address_identified"
        resolution_status = "partially_resolved" if normalized_input["normalization_status"] == "normalized" else "unresolved"
        ambiguity_flags.append("address_needs_geocoding")
        if identifiers["state_code"] is None:
            ambiguity_flags.append("state_missing")
            unresolved_questions.append("What state should this address be geocoded in?")
            next_actions.append(
                make_action(
                    "confirm_state_context",
                    "high",
                    subject_id,
                    "State context is needed before later geocoding and parcel lookup.",
                )
            )
        if normalized.get("address_parse_status") == "freeform":
            ambiguity_flags.append("address_too_broad")
        unresolved_questions.append("What parcel contains this address after geocoding?")
        next_actions.append(
            make_action(
                "geocode_address",
                "high",
                subject_id,
                "Later geocoding is required before parcel lookup can begin.",
            )
        )
        candidate_id = f"cand-{subject_id}-1"
        parcel_candidates.append(
            {
                "candidate_id": candidate_id,
                "source_subject_ids": [subject_id],
                "candidate_status": "hinted",
                "label": f"Parcel containing address {display_label}",
                "apn_raw": None,
                "apn_normalized": None,
                "county_fips": identifiers["county_fips"],
                "county_name": identifiers["county_name"],
                "state_fips": identifiers["state_fips"],
                "state_code": identifiers["state_code"],
                "state_name": identifiers["state_name"],
                "address_text": identifiers["address_text"],
                "coordinates": None,
                "match_basis": ["user_supplied_address"],
                "confidence": 0.56 if identifiers["state_code"] else 0.34,
                "notes": [
                    "Address normalization does not imply parcel identity until geocoding and parcel lookup happen later.",
                ],
            }
        )
    elif input_kind == "coordinates":
        active_subject_kind = "coordinate_point"
        resolution_level = "geography_only"
        resolution_status = "partially_resolved" if identifiers["coordinates"] is not None else "unresolved"
        ambiguity_flags.append("coordinates_not_parcel_confirmed")
        unresolved_questions.append("Which parcel, if any, contains this point?")
        next_actions.append(
            make_action(
                "intersect_coordinates_with_parcel_fabric",
                "high",
                subject_id,
                "Later parcel-fabric intersection is required before parcel-specific conclusions.",
            )
        )
        next_actions.append(
            make_action(
                "proceed_with_geography_only_research",
                "medium",
                subject_id,
                "Geography-only research can still proceed if parcel data remains unavailable.",
            )
        )
    elif input_kind == "subject_ref":
        active_subject_kind = "subject_reference"
        resolution_level = "subject_reference_only"
        resolution_status = "unresolved"
        ambiguity_flags.append("follow_up_reference_ambiguous")
        unresolved_questions.append("Which previously resolved subject should this follow-up attach to?")
        next_actions.append(
            make_action(
                "confirm_follow_up_subject_reference",
                "high",
                subject_id,
                "Follow-up references need session-state resolution before reuse.",
            )
        )

    active_subject = {
        "subject_id": subject_id,
        "input_kind": input_kind,
        "subject_kind": active_subject_kind,
        "resolution_level": resolution_level,
        "resolution_status": resolution_status,
        "display_label": display_label,
        "identifiers": identifiers,
        "parcel_candidate_ids": [candidate["candidate_id"] for candidate in parcel_candidates],
        "ambiguity_flags": dedupe_strings(ambiguity_flags),
        "unresolved_questions": dedupe_strings(unresolved_questions),
        "next_resolution_actions": next_actions,
    }
    return active_subject, parcel_candidates


def determine_subject_kind(request: dict[str, Any], active_subjects: list[dict[str, Any]]) -> str:
    if request["request_mode"] == "recommendation":
        return "thesis_request"
    if len(active_subjects) > 1:
        return "batch_subject_set"
    if not active_subjects:
        return "thesis_request"
    subject = active_subjects[0]
    if subject["resolution_level"] == "parcel_resolved":
        return "single_parcel"
    if subject["resolution_level"] == "parcel_group_resolved":
        return "parcel_group"
    if subject["resolution_level"] == "geography_only":
        return "geography_only"
    return "unresolved_candidate_set"


def determine_resolution_status(request: dict[str, Any], active_subjects: list[dict[str, Any]]) -> str:
    if not active_subjects:
        return "unresolved"
    statuses = {subject["resolution_status"] for subject in active_subjects}
    if request["request_mode"] == "batch_compare":
        if "unresolved" in statuses:
            return "unresolved"
        if any("listing_metadata_incomplete" in subject["ambiguity_flags"] for subject in active_subjects):
            return "unresolved"
    if statuses == {"resolved"}:
        return "resolved"
    if "partially_resolved" in statuses or "resolved" in statuses:
        return "partially_resolved"
    return "unresolved"


def build_listing_context(normalized_inputs: list[dict[str, Any]]) -> dict[str, Any]:
    listing_inputs = [item for item in normalized_inputs if item["input_kind"] == "listing_url"]
    return {
        "has_listing_inputs": bool(listing_inputs),
        "listing_subject_ids": [item["subject_id"] for item in listing_inputs],
        "platforms": dedupe_strings(
            [item["normalized"].get("platform") for item in listing_inputs if item["normalized"].get("platform")]
        ),
        "canonical_urls": dedupe_strings(
            [item["normalized"].get("canonical_url") for item in listing_inputs if item["normalized"].get("canonical_url")]
        ),
        "extracted_listing_ids": dedupe_strings(
            [item["normalized"].get("listing_id") for item in listing_inputs if item["normalized"].get("listing_id")]
        ),
        "address_like_slugs": dedupe_strings(
            [item["normalized"].get("address_like_slug") for item in listing_inputs if item["normalized"].get("address_like_slug")]
        ),
        "listing_count": len(listing_inputs),
        "notes": dedupe_strings(
            [
                note
                for item in listing_inputs
                for note in item["normalization_notes"]
                if "listing" in note.lower() or "url" in note.lower()
            ]
        ),
    }


def build_geography_context(request: dict[str, Any], normalized_inputs: list[dict[str, Any]]) -> dict[str, Any]:
    state_records: dict[str, dict[str, str]] = {}
    county_records: dict[str, dict[str, str]] = {}
    cities: list[str] = []
    region_labels: list[str] = []
    coordinates: list[dict[str, float]] = []
    notes: list[str] = []

    for geography in request.get("thesis", {}).get("target_geographies", []):
        if geography["kind"] == "region":
            region_labels.append(geography["label"])
        if geography.get("state_code"):
            state = resolve_state_hint(geography.get("state_code"))
            if state is not None:
                state_records[state["state_code"]] = state
        if geography.get("county_fips"):
            county = get_county_row(geography["county_fips"])
            county_records[county["county_fips"]] = {
                "county_fips": county["county_fips"],
                "county_name": county["county_name"],
                "state_fips": county["state_fips"],
                "state_code": county["state_code"],
                "state_name": county["state_name"],
            }
            state_records[county["state_code"]] = {
                "state_fips": county["state_fips"],
                "state_code": county["state_code"],
                "state_name": county["state_name"],
            }
        if geography.get("county_name") and geography.get("state_code"):
            county = resolve_county_hint(geography["state_code"], geography["county_name"])
            if county is not None:
                county_records[county["county_fips"]] = {
                    "county_fips": county["county_fips"],
                    "county_name": county["county_name"],
                    "state_fips": county["state_fips"],
                    "state_code": county["state_code"],
                    "state_name": county["state_name"],
                }
        if geography["kind"] == "city" and geography["label"]:
            cities.append(geography["label"])

    for item in normalized_inputs:
        hints = item["resolution_hints"]
        if hints.get("state_code"):
            state_records[hints["state_code"]] = {
                "state_fips": hints["state_fips"],
                "state_code": hints["state_code"],
                "state_name": hints["state_name"],
            }
        if hints.get("county_fips"):
            county_records[hints["county_fips"]] = {
                "county_fips": hints["county_fips"],
                "county_name": hints["county_name"],
                "state_fips": hints["state_fips"],
                "state_code": hints["state_code"],
                "state_name": hints["state_name"],
            }
        if hints.get("city_name"):
            cities.append(hints["city_name"])
        if item["normalized"].get("coordinates") is not None:
            coordinates.append(item["normalized"]["coordinates"])

    unique_states = list(state_records.values())
    unique_counties = list(county_records.values())
    cities = dedupe_strings(cities)
    region_labels = dedupe_strings(region_labels)

    centroid = None
    if coordinates:
        centroid = {
            "latitude": round(sum(point["latitude"] for point in coordinates) / len(coordinates), 6),
            "longitude": round(sum(point["longitude"] for point in coordinates) / len(coordinates), 6),
        }

    if len(unique_counties) == 1 and len(unique_states) <= 1 and not coordinates:
        scope_status = "county_scoped"
    elif len(unique_counties) > 1 and len({county["state_code"] for county in unique_counties}) == 1:
        scope_status = "multi_county"
    elif coordinates and not unique_counties:
        scope_status = "point_scoped"
    elif region_labels and not unique_counties:
        scope_status = "region_scoped"
    elif len(unique_states) == 1 and not unique_counties:
        scope_status = "state_scoped"
    elif len(unique_states) > 1 or (unique_counties and coordinates):
        scope_status = "mixed"
    else:
        scope_status = "none"

    if scope_status == "county_scoped":
        notes.append("County context is available early, but this still does not imply parcel confirmation.")
    if scope_status == "point_scoped":
        notes.append("Coordinates support geography-only resolution until parcel-fabric intersection exists.")
    if region_labels:
        notes.append("Region labels are thesis constraints, not parcel identifiers.")

    return {
        "scope_status": scope_status,
        "states": unique_states,
        "counties": unique_counties,
        "cities": cities,
        "region_labels": region_labels,
        "centroid_coordinates": centroid,
        "notes": notes,
    }


def compute_scores(
    request: dict[str, Any],
    active_subjects: list[dict[str, Any]],
    geography_context: dict[str, Any],
    ambiguity_flags: list[str],
) -> dict[str, Any]:
    resolution_level_scores = {
        "thesis_only": 0.35,
        "geography_only": 0.45,
        "listing_identified": 0.64,
        "address_identified": 0.68,
        "parcel_candidate_identified": 0.82,
        "parcel_resolved": 0.96,
        "parcel_group_resolved": 0.9,
        "subject_reference_only": 0.4,
    }
    scope_scores = {
        "none": 0.1,
        "region_scoped": 0.45,
        "state_scoped": 0.62,
        "county_scoped": 0.84,
        "multi_county": 0.74,
        "point_scoped": 0.72,
        "mixed": 0.5,
    }
    status_scores = {
        "resolved": 0.94,
        "partially_resolved": 0.68,
        "unresolved": 0.32,
    }

    subject_specificity = 0.0
    if active_subjects:
        subject_specificity = round(
            sum(resolution_level_scores[subject["resolution_level"]] for subject in active_subjects)
            / len(active_subjects),
            2,
        )

    geography_specificity = round(scope_scores[geography_context["scope_status"]], 2)
    resolution_confidence = round(
        max(
            0.05,
            (
                sum(status_scores[subject["resolution_status"]] for subject in active_subjects) / max(len(active_subjects), 1)
                if active_subjects
                else 0.25
            )
            - min(0.25, len(ambiguity_flags) * 0.03),
        ),
        2,
    )

    batch_readiness = None
    if request["request_mode"] == "batch_compare":
        batch_penalty = min(0.3, len(ambiguity_flags) * 0.04)
        if "listing_metadata_incomplete" in ambiguity_flags:
            batch_penalty += 0.2
        batch_readiness = round(
            max(
                0.05,
                min(1.0, (subject_specificity + geography_specificity) / 2 - batch_penalty),
            ),
            2,
        )

    return {
        "resolution_confidence": resolution_confidence,
        "subject_specificity": subject_specificity,
        "geography_specificity": geography_specificity,
        "batch_readiness": batch_readiness,
    }


def build_subject_resolution(request: dict[str, Any]) -> dict[str, Any]:
    request_mode = request["request_mode"]
    if request_mode not in REQUEST_MODES:
        raise ValueError(f"Unsupported request_mode: {request_mode}")

    raw_inputs: list[dict[str, Any]] = []
    normalized_inputs: list[dict[str, Any]] = []

    subjects = request.get("subjects", [])
    if not subjects and request_mode == "recommendation":
        thesis_subject = {
            "subject_id": "subject-thesis-1",
            "input_kind": "natural_language_intent",
            "raw_value": request["raw_user_message"],
        }
        raw_input, normalized_input = normalize_subject_input(thesis_subject, 0)
        raw_inputs.append(raw_input)
        normalized_inputs.append(normalized_input)
    else:
        for index, subject in enumerate(subjects):
            raw_input, normalized_input = normalize_subject_input(subject, index)
            raw_inputs.append(raw_input)
            normalized_inputs.append(normalized_input)

    active_subjects: list[dict[str, Any]] = []
    parcel_candidates: list[dict[str, Any]] = []
    for normalized_input in normalized_inputs:
        active_subject, subject_candidates = build_subject_resolution_state(normalized_input, request)
        active_subjects.append(active_subject)
        parcel_candidates.extend(subject_candidates)

    listing_context = build_listing_context(normalized_inputs)
    geography_context = build_geography_context(request, normalized_inputs)

    ambiguity_flags = dedupe_strings(
        [
            flag
            for subject in active_subjects
            for flag in subject["ambiguity_flags"]
        ]
    )
    if request_mode == "batch_compare":
        if len(geography_context["states"]) > 1 or geography_context["scope_status"] == "mixed":
            ambiguity_flags.append("batch_mixed_geographies")
        if len({subject["resolution_level"] for subject in active_subjects}) > 1:
            ambiguity_flags.append("batch_mixed_resolution_levels")
        ambiguity_flags = dedupe_strings(ambiguity_flags)

    unresolved_questions = dedupe_strings(
        [
            question
            for subject in active_subjects
            for question in subject["unresolved_questions"]
        ]
    )

    next_actions: list[dict[str, Any]] = []
    seen_actions = set()
    for subject in active_subjects:
        for action in subject["next_resolution_actions"]:
            key = (
                action["action"],
                action["priority"],
                tuple(action["subject_ids"]),
                action["rationale"],
            )
            if key in seen_actions:
                continue
            next_actions.append(action)
            seen_actions.add(key)

    subject_kind = determine_subject_kind(request, active_subjects)
    resolution_status = determine_resolution_status(request, active_subjects)
    scores = compute_scores(request, active_subjects, geography_context, ambiguity_flags)

    primary_subject_label = (
        active_subjects[0]["display_label"]
        if active_subjects
        else request["thesis"]["summary"] or "Alice subject resolution"
    )

    citations_or_evidence_refs = dedupe_strings(
        listing_context["canonical_urls"]
        + [
            ref
            for ref in empty_state_refs(request).values()
            if isinstance(ref, str) and ref
        ]
    )

    if request_mode == "recommendation":
        batch_purpose = "thesis_search_setup"
    elif request_mode == "batch_compare":
        batch_purpose = "compare_candidates"
    elif request_mode == "follow_up":
        batch_purpose = "follow_up_refinement"
    else:
        batch_purpose = None

    resolution = {
        "schema_version": SUBJECT_RESOLUTION_VERSION,
        "resolution_id": str(uuid5(NAMESPACE_URL, f"alice-subject-resolution:{request['request_id']}")),
        "request_id": request["request_id"],
        "request_mode": request_mode,
        "batch_purpose": batch_purpose,
        "state_refs": empty_state_refs(request),
        "raw_inputs": raw_inputs,
        "normalized_inputs": normalized_inputs,
        "subject_kind": subject_kind,
        "resolution_status": resolution_status,
        "primary_subject_label": primary_subject_label,
        "active_subjects": active_subjects,
        "listing_context": listing_context,
        "parcel_candidates": parcel_candidates,
        "geography_context": geography_context,
        "ambiguity_flags": ambiguity_flags,
        "unresolved_questions": unresolved_questions,
        "next_resolution_actions": next_actions,
        "scores": scores,
        "citations_or_evidence_refs": citations_or_evidence_refs,
        "generated_at": now_iso(),
    }
    return resolution


def summarize_subject_resolution(resolution: dict[str, Any]) -> dict[str, Any]:
    return {
        "resolution_status": resolution["resolution_status"],
        "subject_kind": resolution["subject_kind"],
        "primary_subject_label": resolution["primary_subject_label"],
        "active_subject_count": len(resolution["active_subjects"]),
        "ambiguity_flags": resolution["ambiguity_flags"],
        "next_actions": [action["action"] for action in resolution["next_resolution_actions"]],
        "scores": resolution["scores"],
    }


def load_request(path: str | Path) -> dict[str, Any]:
    return load_json(Path(path))


def load_session_state(path: str | Path) -> dict[str, Any]:
    return load_json(Path(path))


def build_session_state_from_resolution(
    *,
    thread_id: str,
    active_request_mode: str,
    resolution: dict[str, Any],
    thesis_snapshot: dict[str, Any],
    artifact_refs: dict[str, Any],
    unresolved_questions: list[str],
    user_confirmed_assumptions: list[str],
    notes: list[str] | None = None,
) -> dict[str, Any]:
    return {
        "schema_version": SESSION_STATE_VERSION,
        "thread_id": thread_id,
        "updated_at": now_iso(),
        "active_request_mode": active_request_mode,
        "active_resolution_ref": artifact_refs["subject_resolution_ref"],
        "active_subject_set_kind": resolution["subject_kind"],
        "active_subjects": [
            {
                "subject_id": subject["subject_id"],
                "display_label": subject["display_label"],
                "subject_kind": resolution["subject_kind"],
                "resolution_level": subject["resolution_level"],
                "resolution_status": subject["resolution_status"],
                "parcel_candidate_id": subject["parcel_candidate_ids"][0] if subject["parcel_candidate_ids"] else None,
            }
            for subject in resolution["active_subjects"]
        ],
        "thesis_snapshot": thesis_snapshot,
        "artifact_refs": artifact_refs,
        "previous_resolution_refs": [],
        "unresolved_questions": unresolved_questions,
        "user_confirmed_assumptions": user_confirmed_assumptions,
        "notes": notes or [],
    }
