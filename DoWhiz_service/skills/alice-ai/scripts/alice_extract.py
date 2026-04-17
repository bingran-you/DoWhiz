#!/usr/bin/env python3

"""Step 7 evidence extraction for Alice AI."""

from __future__ import annotations

import html
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Any
from uuid import NAMESPACE_URL, uuid5

from alice_fetch import load_source_fetch_log
from alice_registry import load_json, source_by_id
from alice_subject_resolution import now_iso


EXTRACTED_EVIDENCE_VERSION = "alice.extracted_evidence.v1"
EXTRACTOR_VERSION = "alice.extract.v2"
SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
SCHEMAS_ROOT = SKILL_ROOT / "schemas"
EXTRACTED_EVIDENCE_SCHEMA_PATH = SCHEMAS_ROOT / "extracted_evidence.schema.json"

TAG_RE = re.compile(r"<[^>]+>")
SCRIPT_RE = re.compile(r"<script[^>]*>.*?</script>", re.I | re.S)
STYLE_RE = re.compile(r"<style[^>]*>.*?</style>", re.I | re.S)
TITLE_RE = re.compile(r"<title[^>]*>(.*?)</title>", re.I | re.S)
CANONICAL_RE = re.compile(
    r'<link[^>]+rel=["\']canonical["\'][^>]+href=["\']([^"\']+)["\']',
    re.I,
)
OG_URL_RE = re.compile(
    r'<meta[^>]+property=["\']og:url["\'][^>]+content=["\']([^"\']+)["\']',
    re.I,
)
META_NAME_RE = re.compile(
    r'<meta[^>]+name=["\']([^"\']+)["\'][^>]+content=["\']([^"\']+)["\']',
    re.I,
)
META_PROPERTY_RE = re.compile(
    r'<meta[^>]+property=["\']([^"\']+)["\'][^>]+content=["\']([^"\']+)["\']',
    re.I,
)
JSON_LD_RE = re.compile(
    r'<script[^>]+type=["\']application/ld\+json["\'][^>]*>(.*?)</script>',
    re.I | re.S,
)
PRICE_RE = re.compile(
    r'(?:(?:price|askingPrice|listPrice)["\']?\s*[:=]\s*["\']?\$?)(\d[\d,]*(?:\.\d+)?)',
    re.I,
)
PRICE_TEXT_RE = re.compile(r"\$([0-9][0-9,]{2,}(?:\.\d+)?)")
ACREAGE_RE = re.compile(r"(\d+(?:\.\d+)?)\s*(?:acre|acres|ac)\b", re.I)
APN_RE = re.compile(
    r"(?:\bAPN\b|\bAssessor'?s?\s+Parcel(?:\s+Number)?\b|\bParcel\s+Number\b)\s*(?:[:#-]|\bis\b)?\s*([A-Z0-9-]{4,})",
    re.I,
)
COUNTY_PARCEL_ID_RE = re.compile(
    r"(?:\bProperty\s*ID\b|\bParcel\s*ID\b|\bAccount\s*(?:No\.?|Number)\b)\s*(?:[:#-]|\bis\b)?\s*([A-Z0-9-]{4,})",
    re.I,
)
COUNTY_NAME_RE = re.compile(r"\b([A-Z][A-Za-z' -]{1,40}? County)\b")
MULTI_PARCEL_RE = re.compile(
    r"\b(?:two|three|multiple|\d+)\s+(?:adjacent\s+)?parcels?\b|\bmultiple\s+APNs?\b|\bAPNs\b\s*(?:[:#-]).*(?:,| and )",
    re.I,
)
LATITUDE_RE = re.compile(r'"latitude"\s*:\s*(-?\d+(?:\.\d+)?)', re.I)
LONGITUDE_RE = re.compile(r'"longitude"\s*:\s*(-?\d+(?:\.\d+)?)', re.I)
FIELD_LABELS = [
    "APN",
    "Parcel ID",
    "Parcel Number",
    "Property ID",
    "Account Number",
    "Owner Name",
    "Owner",
    "Situs Address",
    "Site Address",
    "Property Address",
    "Address",
    "Acreage",
    "Gross Acres",
    "Acres",
    "Legal Description",
    "Legal Desc",
    "Zoning",
    "General Plan",
]

FALSE_POSITIVE_IDENTIFIER_TOKENS = {
    "address",
    "dataset",
    "description",
    "identifier",
    "information",
    "listed",
    "listing",
    "marketed",
    "near",
    "owner",
    "parcel",
    "price",
    "property",
    "public",
    "records",
    "search",
}

ADDRESS_HINT_WORDS = {
    "address",
    "avenue",
    "ave",
    "blvd",
    "boulevard",
    "circle",
    "county road",
    "court",
    "cr",
    "drive",
    "dr",
    "highway",
    "hwy",
    "lane",
    "ln",
    "lot",
    "parkway",
    "pkwy",
    "place",
    "pl",
    "rd",
    "road",
    "route",
    "st",
    "street",
    "trail",
    "way",
}

OWNER_FALSE_POSITIVE_PHRASES = {
    "parcel id",
    "property address",
    "property id",
    "search by",
}


def deterministic_uuid(*parts: str) -> str:
    return str(uuid5(NAMESPACE_URL, "|".join(parts)))


def _workspace_alice_root(workspace_root: str | Path) -> Path:
    return Path(workspace_root) / "alice"


def _citation_id(entry: dict[str, Any]) -> str:
    return f"c_{entry['entry_id'].split('-')[0]}"


def _field_item_id(source_id: str, entry_id: str, field_name: str) -> str:
    safe_field_name = re.sub(r"[^a-z0-9_:.-]", "_", field_name.lower()).replace(".", "_")
    safe_field_name = safe_field_name.replace("-", "_")
    return f"{source_id}:{entry_id.split('-')[0]}:{safe_field_name}"


def _infer_value_type(value: Any) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, (int, float)):
        return "number"
    if isinstance(value, dict):
        return "object"
    if isinstance(value, list):
        return "array"
    return "string"


def _item(
    *,
    entry: dict[str, Any],
    field_name: str,
    section_hint: str,
    value: Any,
    status: str,
    confidence: float,
    evidence_scope: str,
    clue_type: str = "other",
    extraction_status: str = "parsed",
    observed_at: str | None = None,
    evidence_ref: str | None = None,
    notes: list[str] | None = None,
) -> dict[str, Any]:
    return {
        "item_id": _field_item_id(entry["source_id"], entry["entry_id"], field_name),
        "fetch_entry_id": entry["entry_id"],
        "source_id": entry["source_id"],
        "subject_ids": entry["subject_ids"],
        "capability": entry["capability"],
        "section_hint": section_hint,
        "field_name": field_name,
        "extraction_status": extraction_status,
        "value": value,
        "value_type": _infer_value_type(value),
        "status": status,
        "confidence": confidence,
        "evidence_scope": evidence_scope,
        "clue_type": clue_type,
        "citation_ids": [_citation_id(entry)],
        "observed_at": observed_at,
        "evidence_ref": evidence_ref,
        "notes": notes or [],
    }


def _artifact_bytes(workspace_root: str | Path, artifact_ref: dict[str, Any]) -> bytes:
    return (Path(workspace_root) / artifact_ref["path"]).read_bytes()


def _raw_artifacts(entry: dict[str, Any], artifact_type: str) -> list[dict[str, Any]]:
    return [artifact for artifact in entry["artifact_refs"] if artifact["artifact_type"] == artifact_type]


def _primary_body_artifact(entry: dict[str, Any]) -> dict[str, Any] | None:
    for preferred in ("raw_json", "raw_html", "raw_xml", "raw_text"):
        matches = _raw_artifacts(entry, preferred)
        if matches:
            return matches[0]
    return None


def _clean_text(raw_html: str) -> str:
    without_script = SCRIPT_RE.sub(" ", raw_html)
    without_style = STYLE_RE.sub(" ", without_script)
    without_tags = TAG_RE.sub(" ", without_style)
    collapsed = re.sub(r"\s+", " ", html.unescape(without_tags))
    return collapsed.strip()


def _extract_title(raw_html: str) -> str | None:
    match = TITLE_RE.search(raw_html)
    if match:
        return html.unescape(match.group(1)).strip()
    return None


def _extract_meta_map(raw_html: str) -> dict[str, str]:
    meta: dict[str, str] = {}
    for name, content in META_NAME_RE.findall(raw_html):
        meta[name.lower()] = html.unescape(content).strip()
    for name, content in META_PROPERTY_RE.findall(raw_html):
        meta[name.lower()] = html.unescape(content).strip()
    return meta


def _extract_json_ld(raw_html: str) -> list[dict[str, Any]]:
    objects: list[dict[str, Any]] = []
    for block in JSON_LD_RE.findall(raw_html):
        block = html.unescape(block).strip()
        if not block:
            continue
        try:
            payload = json.loads(block)
        except json.JSONDecodeError:
            continue
        if isinstance(payload, list):
            objects.extend(obj for obj in payload if isinstance(obj, dict))
        elif isinstance(payload, dict):
            objects.append(payload)
    return objects


def _extract_canonical(raw_html: str, meta_map: dict[str, str]) -> str | None:
    match = CANONICAL_RE.search(raw_html)
    if match:
        return html.unescape(match.group(1)).strip()
    match = OG_URL_RE.search(raw_html)
    if match:
        return html.unescape(match.group(1)).strip()
    return meta_map.get("og:url")


def _extract_price(raw_html: str, meta_map: dict[str, str]) -> float | None:
    for key in ("product:price:amount", "og:price:amount"):
        if key in meta_map:
            try:
                return float(meta_map[key].replace(",", "").replace("$", ""))
            except ValueError:
                continue
    match = PRICE_RE.search(raw_html)
    if match:
        return float(match.group(1).replace(",", ""))
    match = PRICE_TEXT_RE.search(_clean_text(raw_html))
    if match:
        return float(match.group(1).replace(",", ""))
    return None


def _extract_acreage(raw_html: str, title: str | None, description: str | None) -> float | None:
    search_text = " ".join(part for part in [title, description, raw_html] if part)
    match = ACREAGE_RE.search(search_text)
    if match:
        return float(match.group(1))
    return None


def _extract_coordinates(raw_html: str, json_ld_objects: list[dict[str, Any]]) -> dict[str, float] | None:
    for obj in json_ld_objects:
        geo = obj.get("geo")
        if isinstance(geo, dict) and "latitude" in geo and "longitude" in geo:
            try:
                return {
                    "latitude": round(float(geo["latitude"]), 6),
                    "longitude": round(float(geo["longitude"]), 6),
                }
            except (TypeError, ValueError):
                pass
    lat_match = LATITUDE_RE.search(raw_html)
    lon_match = LONGITUDE_RE.search(raw_html)
    if lat_match and lon_match:
        return {
            "latitude": round(float(lat_match.group(1)), 6),
            "longitude": round(float(lon_match.group(1)), 6),
        }
    return None


def _extract_address(meta_map: dict[str, str], json_ld_objects: list[dict[str, Any]]) -> dict[str, Any] | None:
    for obj in json_ld_objects:
        address = obj.get("address")
        if isinstance(address, dict):
            street = address.get("streetAddress")
            city = address.get("addressLocality")
            state = address.get("addressRegion")
            postal_code = address.get("postalCode")
            country = address.get("addressCountry", "US")
            parts = [street, city, state, postal_code]
            full_address = ", ".join(part for part in parts if part)
            if full_address:
                return {
                    "full_address": full_address,
                    "street_line1": street,
                    "street_line2": None,
                    "city": city,
                    "county_name": None,
                    "state_code": state,
                    "postal_code": postal_code,
                    "country_code": country if isinstance(country, str) else "US",
                }
    for key in ("og:street-address", "place:location:address"):
        if key in meta_map:
            return {
                "full_address": meta_map[key],
                "street_line1": meta_map[key],
                "street_line2": None,
                "city": None,
                "county_name": None,
                "state_code": None,
                "postal_code": None,
                "country_code": "US",
            }
    return None


def _extract_description(meta_map: dict[str, str], json_ld_objects: list[dict[str, Any]]) -> str | None:
    for key in ("description", "og:description"):
        if key in meta_map:
            return meta_map[key]
    for obj in json_ld_objects:
        description = obj.get("description")
        if isinstance(description, str) and description.strip():
            return description.strip()
    return None


def _extract_apn(raw_html: str) -> str | None:
    match = APN_RE.search(raw_html)
    if match and _looks_like_apn(match.group(1)):
        return match.group(1).strip()
    return None


def _extract_apns(raw_html: str) -> list[str]:
    seen: list[str] = []
    for match in APN_RE.findall(raw_html):
        value = _clean_capture(match)
        if value and _looks_like_apn(value) and value not in seen:
            seen.append(value)
    return seen


def _clean_capture(value: str | None) -> str | None:
    if value is None:
        return None
    cleaned = re.sub(r"\s+", " ", html.unescape(value)).strip(" :,-")
    return cleaned or None


def _looks_like_apn(value: str | None) -> bool:
    cleaned = _clean_capture(value)
    if cleaned is None:
        return False
    lowered = cleaned.lower()
    if lowered in FALSE_POSITIVE_IDENTIFIER_TOKENS:
        return False
    if cleaned.isalpha():
        return False
    alnum = re.sub(r"[^A-Z0-9]", "", cleaned.upper())
    if len(alnum) < 5:
        return False
    return bool(re.search(r"\d", cleaned))


def _looks_like_county_parcel_id(value: str | None) -> bool:
    cleaned = _clean_capture(value)
    if cleaned is None:
        return False
    lowered = cleaned.lower()
    if lowered in FALSE_POSITIVE_IDENTIFIER_TOKENS:
        return False
    alnum = re.sub(r"[^A-Z0-9]", "", cleaned.upper())
    if len(alnum) < 4:
        return False
    return bool(re.search(r"\d", cleaned))


def _looks_like_address(value: str | None) -> bool:
    cleaned = _clean_capture(value)
    if cleaned is None:
        return False
    lowered = cleaned.lower()
    if lowered.startswith("or "):
        return False
    if any(phrase in lowered for phrase in OWNER_FALSE_POSITIVE_PHRASES):
        return False
    if any(word in lowered for word in ADDRESS_HINT_WORDS):
        return True
    if "," in cleaned and re.search(r"\b[A-Z]{2}\b", cleaned):
        return True
    return bool(re.search(r"\d", cleaned))


def _looks_like_owner_name(value: str | None) -> bool:
    cleaned = _clean_capture(value)
    if cleaned is None:
        return False
    lowered = cleaned.lower()
    if any(phrase in lowered for phrase in OWNER_FALSE_POSITIVE_PHRASES):
        return False
    if "property" in lowered or "parcel" in lowered:
        return False
    return len(cleaned.split()) >= 2 or any(
        suffix in cleaned.upper()
        for suffix in (" LLC", " LP", " INC", " LTD", " CO", " COMPANY", " TRUST")
    )


def _extract_labeled_fragment(clean_text: str, labels: list[str]) -> str | None:
    label_pattern = "|".join(re.escape(label) for label in labels)
    stop_labels = [label for label in FIELD_LABELS if label not in labels]
    stop_pattern = "|".join(re.escape(label) for label in stop_labels)
    pattern = re.compile(
        rf"(?:{label_pattern})\s*(?:[:#-])\s*(.+?)(?=(?:{stop_pattern})\s*(?:[:#-])|$)",
        re.I,
    )
    match = pattern.search(clean_text)
    if not match:
        return None
    return _clean_capture(match.group(1))


def _extract_county_parcel_id(clean_text: str) -> str | None:
    match = COUNTY_PARCEL_ID_RE.search(clean_text)
    if match:
        value = _clean_capture(match.group(1))
        if _looks_like_county_parcel_id(value):
            return value
    return None


def _extract_county_name(clean_text: str) -> str | None:
    matches = COUNTY_NAME_RE.findall(clean_text)
    if matches:
        return _clean_capture(matches[-1])
    return None


def _extract_owner_name(clean_text: str) -> str | None:
    value = _extract_labeled_fragment(clean_text, ["Owner Name", "Owner"])
    return value if _looks_like_owner_name(value) else None


def _extract_labeled_address(clean_text: str) -> str | None:
    value = _extract_labeled_fragment(
        clean_text,
        ["Situs Address", "Site Address", "Property Address", "Address"],
    )
    return value if _looks_like_address(value) else None


def _extract_legal_description(clean_text: str) -> str | None:
    return _extract_labeled_fragment(clean_text, ["Legal Description", "Legal Desc"])


def _extract_zoning_reference(clean_text: str) -> str | None:
    return _extract_labeled_fragment(clean_text, ["Zoning", "General Plan"])


def _extract_multi_parcel_hint(clean_text: str) -> str | None:
    match = MULTI_PARCEL_RE.search(clean_text)
    if not match:
        return None
    return _clean_capture(match.group(0))


def _page_context_scope(entry: dict[str, Any]) -> str:
    if entry["source_category"] in {"county", "city_local"}:
        return "county_level"
    if entry["source_category"] == "listing_platform":
        return "listing_derived"
    if entry["source_category"] in {"state", "federal"}:
        return "geography_only"
    return "inferred"


def _local_clue_scope(entry: dict[str, Any]) -> str:
    if entry["source_category"] in {"county", "city_local"}:
        return "parcel_candidate"
    if entry["source_category"] == "listing_platform":
        return "listing_derived"
    if entry["source_category"] == "state":
        return "county_level"
    return "geography_only"


def _extract_listing_items(entry: dict[str, Any], workspace_root: str | Path) -> list[dict[str, Any]]:
    artifact = _primary_body_artifact(entry)
    if artifact is None:
        return []
    raw_html = _artifact_bytes(workspace_root, artifact).decode("utf-8", "ignore")
    clean_text = _clean_text(raw_html)
    meta_map = _extract_meta_map(raw_html)
    json_ld_objects = _extract_json_ld(raw_html)
    title = _extract_title(raw_html)
    description = _extract_description(meta_map, json_ld_objects)
    canonical_url = _extract_canonical(raw_html, meta_map)
    price = _extract_price(raw_html, meta_map)
    acreage = _extract_acreage(raw_html, title, description)
    coordinates = _extract_coordinates(raw_html, json_ld_objects)
    address = _extract_address(meta_map, json_ld_objects)
    labeled_address = _extract_labeled_address(clean_text)
    county_name = _extract_county_name(" ".join(part for part in [title, description, clean_text] if part))
    legal_description = _extract_legal_description(clean_text)
    multi_parcel_hint = _extract_multi_parcel_hint(clean_text)
    apns = _extract_apns(raw_html)

    if address is None and labeled_address:
        address = {
            "full_address": labeled_address,
            "street_line1": labeled_address,
            "street_line2": None,
            "city": None,
            "county_name": county_name,
            "state_code": None,
            "postal_code": None,
            "country_code": "US",
        }
    elif address is not None and county_name and address.get("county_name") is None:
        address = dict(address)
        address["county_name"] = county_name

    items: list[dict[str, Any]] = []
    if title:
        items.append(
            _item(
                entry=entry,
                field_name="listing.title",
                section_hint="listing",
                value=title,
                status="confirmed",
                confidence=0.74,
                evidence_scope="listing_derived",
                clue_type="listing_identifier",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
    if canonical_url:
        items.append(
            _item(
                entry=entry,
                field_name="listing.canonical_url",
                section_hint="listing",
                value=canonical_url,
                status="confirmed",
                confidence=0.9,
                evidence_scope="listing_derived",
                clue_type="listing_identifier",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
    if price is not None:
        items.append(
            _item(
                entry=entry,
                field_name="listing.asking_price",
                section_hint="listing",
                value=price,
                status="confirmed",
                confidence=0.88,
                evidence_scope="listing_derived",
                clue_type="market_signal",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
    if acreage is not None:
        items.append(
            _item(
                entry=entry,
                field_name="listing.listed_acreage",
                section_hint="listing",
                value=acreage,
                status="confirmed",
                confidence=0.8,
                evidence_scope="listing_derived",
                clue_type="acreage",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
        items.append(
            _item(
                entry=entry,
                field_name="parcel_identity.acreage_hint",
                section_hint="parcel_identity",
                value=acreage,
                status="estimated",
                confidence=0.76,
                evidence_scope="listing_derived",
                clue_type="acreage",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
                notes=["Listing acreage is a seller-facing claim until corroborated by an official parcel source."],
            )
        )
    if description:
        items.append(
            _item(
                entry=entry,
                field_name="listing.description_text",
                section_hint="listing",
                value=description,
                status="confirmed",
                confidence=0.73,
                evidence_scope="listing_derived",
                clue_type="page_context",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
                notes=["Listing description is marketing text and does not confirm parcel identity."],
            )
        )
    if address:
        items.append(
            _item(
                entry=entry,
                field_name="subject.address",
                section_hint="subject",
                value=address,
                status="estimated",
                confidence=0.68,
                evidence_scope="listing_derived",
                clue_type="address",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
                notes=["Listing-derived address should be confirmed against official parcel or assessor records."],
            )
        )
        items.append(
            _item(
                entry=entry,
                field_name="parcel_identity.site_address_hint",
                section_hint="parcel_identity",
                value=address,
                status="estimated",
                confidence=0.68,
                evidence_scope="listing_derived",
                clue_type="address",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
                notes=["Listing-derived address is a parcel clue, not parcel confirmation by itself."],
            )
        )
    if coordinates is not None:
        items.append(
            _item(
                entry=entry,
                field_name="subject.coordinates",
                section_hint="subject",
                value=coordinates,
                status="estimated",
                confidence=0.7,
                evidence_scope="listing_derived",
                clue_type="coordinates",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
                notes=["Listing-derived coordinates are directional until parcel geometry is confirmed."],
            )
        )
    for index, apn in enumerate(apns, start=1):
        items.append(
            _item(
                entry=entry,
                field_name="parcel_identity.apn_hint" if index == 1 else f"parcel_identity.apn_hint_{index}",
                section_hint="parcel_identity",
                value=apn,
                status="estimated",
                confidence=0.7,
                evidence_scope="listing_derived",
                clue_type="apn",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
                notes=["APN clue came from a listing page and is not parcel confirmation by itself."],
            )
        )
    if county_name:
        items.append(
            _item(
                entry=entry,
                field_name="parcel_identity.county_name_hint",
                section_hint="parcel_identity",
                value=county_name,
                status="estimated",
                confidence=0.72,
                evidence_scope="listing_derived",
                clue_type="county_name",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
    if legal_description:
        items.append(
            _item(
                entry=entry,
                field_name="parcel_identity.legal_description_fragment",
                section_hint="parcel_identity",
                value=legal_description,
                status="estimated",
                confidence=0.62,
                evidence_scope="listing_derived",
                clue_type="legal_description",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
                notes=["Legal-description text from a listing page should be corroborated against an official public record."],
            )
        )
    if multi_parcel_hint:
        items.append(
            _item(
                entry=entry,
                field_name="parcel_identity.multiple_parcel_hint",
                section_hint="parcel_identity",
                value=multi_parcel_hint,
                status="estimated",
                confidence=0.66,
                evidence_scope="listing_derived",
                clue_type="parcel_group_hint",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
                notes=["Listing text suggests the offering may involve more than one parcel."],
            )
        )
    return items


def _extract_json_payload(entry: dict[str, Any], workspace_root: str | Path) -> dict[str, Any] | None:
    artifact = _primary_body_artifact(entry)
    if artifact is None:
        return None
    try:
        return json.loads(_artifact_bytes(workspace_root, artifact).decode("utf-8", "ignore"))
    except json.JSONDecodeError:
        return None


def _extract_fema_items(entry: dict[str, Any], workspace_root: str | Path) -> list[dict[str, Any]]:
    payload = _extract_json_payload(entry, workspace_root)
    artifact = _primary_body_artifact(entry)
    if payload is None or artifact is None:
        return []
    features = payload.get("features", [])
    if not features:
        return [
            _item(
                entry=entry,
                field_name="environmental.flood_zone",
                section_hint="environmental_constraints",
                value="No intersecting FEMA flood-hazard zone was returned for the sampled point.",
                status="confirmed",
                confidence=0.74,
                evidence_scope="geography_only",
                clue_type="environmental_signal",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
                notes=["This is a point-based flood screen, not a parcel-boundary determination."],
            )
        ]

    attributes = features[0].get("attributes", {})
    zone = attributes.get("FLD_ZONE")
    subtype = attributes.get("ZONE_SUBTY")
    sfha = attributes.get("SFHA_TF")
    bfe = attributes.get("STATIC_BFE")
    summary = f"FEMA sampled point returned flood zone {zone or 'unknown'}"
    if subtype:
        summary += f" ({subtype})"
    if sfha:
        summary += f"; SFHA={sfha}"
    if bfe not in (None, ""):
        summary += f"; static BFE={bfe}"
    summary += "."
    return [
        _item(
            entry=entry,
            field_name="environmental.flood_zone",
            section_hint="environmental_constraints",
            value=summary,
            status="confirmed",
            confidence=0.81,
            evidence_scope="geography_only",
            clue_type="environmental_signal",
            observed_at=entry["completed_at"],
            evidence_ref=artifact["path"],
            notes=["This is a point-based flood screen, not a parcel-boundary determination."],
        )
    ]


def _extract_nwi_items(entry: dict[str, Any], workspace_root: str | Path) -> list[dict[str, Any]]:
    payload = _extract_json_payload(entry, workspace_root)
    artifact = _primary_body_artifact(entry)
    if payload is None or artifact is None:
        return []
    features = payload.get("features", [])
    if not features:
        return [
            _item(
                entry=entry,
                field_name="environmental.wetlands_signal",
                section_hint="environmental_constraints",
                value="No mapped wetland polygon intersected the sampled point in the NWI service.",
                status="confirmed",
                confidence=0.73,
                evidence_scope="geography_only",
                clue_type="environmental_signal",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
                notes=["This is a point-based wetlands screen, not a parcel-boundary determination."],
            )
        ]

    attributes = features[0].get("attributes", {})
    wetland_type = attributes.get("Wetlands.WETLAND_TYPE") or attributes.get("WETLAND_TYPE")
    class_name = attributes.get("NWI_Wetland_Codes.CLASS_NAME") or attributes.get("CLASS_NAME")
    summary = "Sampled point intersected a mapped wetland polygon"
    if wetland_type:
        summary += f" ({wetland_type})"
    if class_name:
        summary += f" with class {class_name}"
    summary += "."
    return [
        _item(
            entry=entry,
            field_name="environmental.wetlands_signal",
            section_hint="environmental_constraints",
            value=summary,
            status="confirmed",
            confidence=0.79,
            evidence_scope="geography_only",
            clue_type="environmental_signal",
            observed_at=entry["completed_at"],
            evidence_ref=artifact["path"],
            notes=["This is a point-based wetlands screen, not a parcel-boundary determination."],
        )
    ]


def _extract_usgs_items(entry: dict[str, Any], workspace_root: str | Path) -> list[dict[str, Any]]:
    payload = _extract_json_payload(entry, workspace_root)
    artifact = _primary_body_artifact(entry)
    if payload is None or artifact is None:
        return []
    samples = payload.get("samples", [])
    if not samples:
        return []
    value = samples[0].get("value")
    try:
        elevation_meters = round(float(value), 2)
    except (TypeError, ValueError):
        return []
    return [
        _item(
            entry=entry,
            field_name="environmental.elevation_meters",
            section_hint="environmental_constraints",
            value=elevation_meters,
            status="confirmed",
            confidence=0.9,
            evidence_scope="geography_only",
            clue_type="environmental_signal",
            observed_at=entry["completed_at"],
            evidence_ref=artifact["path"],
            notes=["USGS 3DEP returned a point elevation sample."],
        )
    ]


def _parse_soap_table(xml_bytes: bytes) -> dict[str, str]:
    root = ET.fromstring(xml_bytes.decode("utf-8", "ignore"))
    table = root.find(".//{*}diffgram/{*}NewDataSet/{*}Table")
    if table is None:
        table = root.find(".//{*}Table")
    if table is None:
        return {}
    result: dict[str, str] = {}
    for child in list(table):
        tag_name = child.tag.split("}", 1)[-1]
        result[tag_name] = (child.text or "").strip()
    return result


def _artifact_by_suffix(entry: dict[str, Any], suffix: str) -> dict[str, Any] | None:
    for artifact in entry["artifact_refs"]:
        if artifact["path"].endswith(suffix):
            return artifact
    return None


def _extract_usda_items(entry: dict[str, Any], workspace_root: str | Path) -> list[dict[str, Any]]:
    mukey_artifact = _artifact_by_suffix(entry, "__mukey.xml")
    soil_artifact = _artifact_by_suffix(entry, "__muaggatt.xml")
    if mukey_artifact is None or soil_artifact is None:
        return []
    mukey_row = _parse_soap_table(_artifact_bytes(workspace_root, mukey_artifact))
    soil_row = _parse_soap_table(_artifact_bytes(workspace_root, soil_artifact))
    mukey = mukey_row.get("mukey")
    muname = soil_row.get("muname")
    slope = soil_row.get("slopegraddcp")
    drainage = soil_row.get("drclassdcd")
    hydrologic_group = soil_row.get("hydgrpdcd")
    flood_frequency = soil_row.get("flodfreqdcd")
    niccdcd = soil_row.get("niccdcd")
    aws0100wta = soil_row.get("aws0100wta")

    items: list[dict[str, Any]] = []
    if mukey:
        items.append(
            _item(
                entry=entry,
                field_name="soil.mukey",
                section_hint="environmental_constraints",
                value=mukey,
                status="confirmed",
                confidence=0.86,
                evidence_scope="geography_only",
                clue_type="environmental_signal",
                observed_at=entry["completed_at"],
                evidence_ref=mukey_artifact["path"],
            )
        )
    soil_constraints_parts = [part for part in [muname, f"slope {slope}%" if slope else None, drainage, f"hydrologic group {hydrologic_group}" if hydrologic_group else None] if part]
    if soil_constraints_parts:
        items.append(
            _item(
                entry=entry,
                field_name="environmental.soil_constraints_signal",
                section_hint="environmental_constraints",
                value="; ".join(soil_constraints_parts) + ".",
                status="confirmed",
                confidence=0.82,
                evidence_scope="geography_only",
                clue_type="environmental_signal",
                observed_at=entry["completed_at"],
                evidence_ref=soil_artifact["path"],
                notes=["NRCS soil data is directional and point-linked until parcel geometry is confirmed."],
            )
        )
    soil_productivity_parts = []
    if niccdcd:
        soil_productivity_parts.append(f"nonirrigated capability class {niccdcd}")
    if aws0100wta:
        soil_productivity_parts.append(f"available water storage to 100 cm {aws0100wta}")
    if flood_frequency:
        soil_productivity_parts.append(f"soil flood frequency {flood_frequency}")
    if soil_productivity_parts:
        items.append(
            _item(
                entry=entry,
                field_name="water_ag.soil_productivity_signal",
                section_hint="water_and_agriculture",
                value="NRCS soil attributes indicate " + ", ".join(soil_productivity_parts) + ".",
                status="confirmed",
                confidence=0.78,
                evidence_scope="geography_only",
                clue_type="environmental_signal",
                observed_at=entry["completed_at"],
                evidence_ref=soil_artifact["path"],
            )
        )
    return items


def _extract_page_items(entry: dict[str, Any], workspace_root: str | Path) -> list[dict[str, Any]]:
    artifact = _primary_body_artifact(entry)
    if artifact is None:
        return []
    raw_html = _artifact_bytes(workspace_root, artifact).decode("utf-8", "ignore")
    clean_text = _clean_text(raw_html)
    title = _extract_title(raw_html)
    meta_map = _extract_meta_map(raw_html)
    description = meta_map.get("description") or meta_map.get("og:description")
    text_excerpt = clean_text[:300] if clean_text else None
    apns = _extract_apns(raw_html)
    county_parcel_id = _extract_county_parcel_id(clean_text)
    labeled_address = _extract_labeled_address(clean_text)
    owner_name = _extract_owner_name(clean_text)
    legal_description = _extract_legal_description(clean_text)
    zoning_reference = _extract_zoning_reference(clean_text)
    acreage = _extract_acreage(clean_text, title, description)
    county_name = _extract_county_name(" ".join(part for part in [title, description, clean_text] if part))
    multi_parcel_hint = _extract_multi_parcel_hint(clean_text)

    section_hint = {
        "parcel_identity": "parcel_identity",
        "planning_docs": "planning_and_land_use",
        "zoning": "planning_and_land_use",
        "tax_roll": "parcel_identity",
        "utilities": "infrastructure_and_utilities",
        "transmission": "infrastructure_and_utilities",
        "market_context": "market_signals",
        "environmental_baseline": "environmental_constraints",
    }.get(entry["capability"], "planning_and_land_use")
    context_scope = _page_context_scope(entry)
    clue_scope = _local_clue_scope(entry)

    items: list[dict[str, Any]] = []
    if title:
        items.append(
            _item(
                entry=entry,
                field_name="page.title",
                section_hint=section_hint,
                value=title,
                status="confirmed",
                confidence=0.86,
                evidence_scope=context_scope,
                clue_type="page_context",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
    if description:
        items.append(
            _item(
                entry=entry,
                field_name="page.meta_description",
                section_hint=section_hint,
                value=description,
                status="confirmed",
                confidence=0.78,
                evidence_scope=context_scope,
                clue_type="page_context",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
    if text_excerpt:
        items.append(
            _item(
                entry=entry,
                field_name="page.text_excerpt",
                section_hint=section_hint,
                value=text_excerpt,
                status="estimated",
                confidence=0.6,
                evidence_scope=context_scope,
                clue_type="page_context",
                extraction_status="derived",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
    for index, apn in enumerate(apns, start=1):
        items.append(
            _item(
                entry=entry,
                field_name="parcel_identity.apn_hint" if index == 1 else f"parcel_identity.apn_hint_{index}",
                section_hint="parcel_identity",
                value=apn,
                status="estimated",
                confidence=0.79 if entry["source_category"] in {"county", "city_local"} else 0.68,
                evidence_scope=clue_scope,
                clue_type="apn",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
                notes=["APN text came from a fetched public page and should still be reconciled against competing clues before parcel confirmation."],
            )
        )
    if county_parcel_id:
        items.append(
            _item(
                entry=entry,
                field_name="parcel_identity.county_parcel_id_hint",
                section_hint="parcel_identity",
                value=county_parcel_id,
                status="estimated",
                confidence=0.8 if entry["source_category"] in {"county", "city_local"} else 0.65,
                evidence_scope=clue_scope,
                clue_type="county_parcel_id",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
    if labeled_address:
        address_value = {
            "full_address": labeled_address,
            "street_line1": labeled_address,
            "street_line2": None,
            "city": None,
            "county_name": county_name,
            "state_code": None,
            "postal_code": None,
            "country_code": "US"
        }
        items.append(
            _item(
                entry=entry,
                field_name="parcel_identity.site_address_hint",
                section_hint="parcel_identity",
                value=address_value,
                status="estimated",
                confidence=0.76 if entry["source_category"] in {"county", "city_local"} else 0.62,
                evidence_scope=clue_scope,
                clue_type="address",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
    if owner_name:
        items.append(
            _item(
                entry=entry,
                field_name="parcel_identity.owner_name_hint",
                section_hint="parcel_identity",
                value=owner_name,
                status="estimated",
                confidence=0.74 if entry["source_category"] in {"county", "city_local"} else 0.58,
                evidence_scope=clue_scope,
                clue_type="owner_name",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
    if acreage is not None:
        items.append(
            _item(
                entry=entry,
                field_name="parcel_identity.acreage_hint",
                section_hint="parcel_identity",
                value=acreage,
                status="estimated",
                confidence=0.75 if entry["source_category"] in {"county", "city_local"} else 0.6,
                evidence_scope=clue_scope,
                clue_type="acreage",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
    if legal_description:
        items.append(
            _item(
                entry=entry,
                field_name="parcel_identity.legal_description_fragment",
                section_hint="parcel_identity",
                value=legal_description,
                status="estimated",
                confidence=0.71 if entry["source_category"] in {"county", "city_local"} else 0.56,
                evidence_scope=clue_scope,
                clue_type="legal_description",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
    if county_name:
        items.append(
            _item(
                entry=entry,
                field_name="parcel_identity.county_name_hint",
                section_hint="parcel_identity",
                value=county_name,
                status="estimated",
                confidence=0.7,
                evidence_scope=context_scope,
                clue_type="county_name",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
    if zoning_reference:
        items.append(
            _item(
                entry=entry,
                field_name="planning.zoning_reference",
                section_hint="planning_and_land_use",
                value=zoning_reference,
                status="estimated",
                confidence=0.72 if entry["source_category"] in {"county", "city_local"} else 0.58,
                evidence_scope=clue_scope if entry["source_category"] in {"county", "city_local"} else context_scope,
                clue_type="zoning_reference",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
                notes=["Zoning text on a fetched public page is still directional until parcel identity is stabilized."],
            )
        )
    if multi_parcel_hint:
        items.append(
            _item(
                entry=entry,
                field_name="parcel_identity.multiple_parcel_hint",
                section_hint="parcel_identity",
                value=multi_parcel_hint,
                status="estimated",
                confidence=0.67,
                evidence_scope=clue_scope,
                clue_type="parcel_group_hint",
                observed_at=entry["completed_at"],
                evidence_ref=artifact["path"],
            )
        )
    return items


def _items_for_entry(entry: dict[str, Any], workspace_root: str | Path) -> list[dict[str, Any]]:
    if entry["retrieval_status"] not in {"success", "partial"}:
        return []
    if entry["source_id"] is None:
        return []
    if entry["source_id"].startswith("listing_platform_"):
        return _extract_listing_items(entry, workspace_root)
    if entry["source_id"] == "federal_fema_nfhl":
        return _extract_fema_items(entry, workspace_root)
    if entry["source_id"] == "federal_usfws_nwi":
        return _extract_nwi_items(entry, workspace_root)
    if entry["source_id"] == "federal_usgs_3dep_elevation":
        return _extract_usgs_items(entry, workspace_root)
    if entry["source_id"] == "federal_usda_soil_data_access":
        return _extract_usda_items(entry, workspace_root)
    return _extract_page_items(entry, workspace_root)


def extract_evidence(
    fetch_log: dict[str, Any],
    *,
    workspace_root: str | Path,
    request_id: str,
    subject_resolution_id: str,
    generated_at: str | None = None,
) -> dict[str, Any]:
    items: list[dict[str, Any]] = []
    for entry in fetch_log["fetch_entries"]:
        entry_items = _items_for_entry(entry, workspace_root)
        entry["parser_version"] = EXTRACTOR_VERSION if entry_items else entry["parser_version"]
        entry["extracted_field_refs"] = [item["item_id"] for item in entry_items]
        items.extend(entry_items)

    extracted = {
        "schema_version": EXTRACTED_EVIDENCE_VERSION,
        "extracted_evidence_id": deterministic_uuid(
            "alice-extracted-evidence",
            fetch_log["fetch_log_id"],
            request_id,
        ),
        "request_id": request_id,
        "subject_resolution_id": subject_resolution_id,
        "fetch_log_id": fetch_log["fetch_log_id"],
        "generated_at": generated_at or now_iso(),
        "items": items,
    }

    alice_root = _workspace_alice_root(workspace_root)
    (alice_root / "extracted_evidence.json").write_text(json.dumps(extracted, indent=2), encoding="utf-8")
    (alice_root / "source_fetch_log.json").write_text(json.dumps(fetch_log, indent=2), encoding="utf-8")
    return extracted


def load_extracted_evidence(path: str | Path) -> dict[str, Any]:
    return load_json(Path(path))
