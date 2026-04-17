#!/usr/bin/env python3

"""Step 6 live retrieval foundation for Alice AI."""

from __future__ import annotations

import json
import hashlib
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from uuid import NAMESPACE_URL, uuid5

from alice_registry import load_json, source_by_id
from alice_subject_resolution import now_iso


SOURCE_FETCH_LOG_VERSION = "alice.source_fetch_log.v1"
FETCHER_VERSION = "alice.fetch.v0"
SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
SCHEMAS_ROOT = SKILL_ROOT / "schemas"
SOURCE_FETCH_LOG_SCHEMA_PATH = SCHEMAS_ROOT / "source_fetch_log.schema.json"

FEMA_FLOOD_QUERY_URL = "https://hazards.fema.gov/arcgis/rest/services/public/NFHL/MapServer/28/query"
NWI_WETLANDS_QUERY_URL = (
    "https://fwspublicservices.wim.usgs.gov/wetlandsmapservice/rest/services/Wetlands/MapServer/0/query"
)
USGS_ELEVATION_QUERY_URL = (
    "https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer/getSamples"
)
USDA_SDA_SOAP_URL = "https://sdmdataaccess.nrcs.usda.gov/Tabular/SDMTabularService.asmx"

DEFAULT_HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
        "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36"
    ),
    "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    "Accept-Language": "en-US,en;q=0.9",
    "Cache-Control": "no-cache",
    "Pragma": "no-cache",
}

POINT_QUERY_SOURCE_IDS = {
    "federal_fema_nfhl",
    "federal_usfws_nwi",
    "federal_usgs_3dep_elevation",
    "federal_usda_soil_data_access",
}
LISTING_SOURCE_PREFIX = "listing_platform_"

BLOCKER_TO_ERROR = {
    "jurisdiction_unresolved": "missing_prerequisite",
    "county_required": "missing_prerequisite",
    "subject_resolution_required": "missing_prerequisite",
    "local_source_gap": "unsupported_source",
    "recommendation_flow_deferred": "unsupported_source",
    "mixed_batch_jurisdiction": "missing_prerequisite",
    "city_specific_context_missing": "missing_prerequisite",
}


@dataclass
class SourceRequest:
    fixture_key: str
    url: str
    method: str = "GET"
    headers: dict[str, str] | None = None
    data: bytes | None = None


@dataclass
class SourceResponse:
    status: int | None
    headers: dict[str, str]
    body: bytes
    final_url: str
    content_type: str | None


class LiveTransport:
    def __init__(self, timeout_seconds: int = 30) -> None:
        self.timeout_seconds = timeout_seconds

    def request(self, source_request: SourceRequest) -> SourceResponse:
        headers = dict(DEFAULT_HEADERS)
        headers.update(source_request.headers or {})
        request = urllib.request.Request(
            source_request.url,
            data=source_request.data,
            headers=headers,
            method=source_request.method,
        )
        with urllib.request.urlopen(request, timeout=self.timeout_seconds) as response:
            body = response.read()
            return SourceResponse(
                status=getattr(response, "status", None),
                headers=dict(response.headers.items()),
                body=body,
                final_url=response.geturl(),
                content_type=response.headers.get("Content-Type"),
            )


class FixtureTransport:
    def __init__(self, manifest_path: str | Path) -> None:
        self.manifest_path = Path(manifest_path)
        self.manifest = load_json(self.manifest_path)
        self.responses = self.manifest.get("responses", {})

    def request(self, source_request: SourceRequest) -> SourceResponse:
        entry = self.responses.get(source_request.fixture_key)
        if entry is None:
            raise KeyError(
                f"Fixture response for {source_request.fixture_key} was not found in {self.manifest_path}"
            )
        body = b""
        if entry.get("body_path"):
            body = (self.manifest_path.parent / entry["body_path"]).read_bytes()
        headers = dict(entry.get("headers", {}))
        if entry.get("content_type") and "Content-Type" not in headers and "content-type" not in headers:
            headers["Content-Type"] = entry["content_type"]
        return SourceResponse(
            status=entry.get("status"),
            headers=headers,
            body=body,
            final_url=entry.get("final_url", source_request.url),
            content_type=entry.get("content_type"),
        )


def deterministic_uuid(*parts: str) -> str:
    return str(uuid5(NAMESPACE_URL, "|".join(parts)))


def _generated_at(override: str | None = None) -> str:
    return override or now_iso()


def _safe_json_dump(payload: Any) -> bytes:
    return json.dumps(payload, indent=2, sort_keys=True).encode("utf-8")


def _primary_coordinates(subject_resolution: dict[str, Any]) -> dict[str, float] | None:
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


def _primary_listing_subjects(subject_resolution: dict[str, Any], platform: str) -> list[dict[str, Any]]:
    matching = []
    for subject in subject_resolution.get("active_subjects", []):
        identifiers = subject.get("identifiers", {})
        if identifiers.get("listing_platform") != platform:
            continue
        if identifiers.get("canonical_url") is None:
            continue
        matching.append(subject)
    return matching


def _source_descriptor(source_id: str) -> dict[str, Any]:
    descriptor = source_by_id().get(source_id)
    if descriptor is None:
        raise KeyError(f"Unknown source_id: {source_id}")
    return descriptor


def _workspace_alice_root(workspace_root: str | Path) -> Path:
    alice_root = Path(workspace_root) / "alice"
    alice_root.mkdir(parents=True, exist_ok=True)
    return alice_root


def _raw_evidence_root(workspace_root: str | Path, source_id: str) -> Path:
    raw_root = _workspace_alice_root(workspace_root) / "raw_evidence" / source_id
    raw_root.mkdir(parents=True, exist_ok=True)
    return raw_root


def _artifact_path(
    workspace_root: str | Path,
    source_id: str,
    entry_id: str,
    suffix: str,
) -> tuple[Path, str]:
    relative_path = f"alice/raw_evidence/{source_id}/{entry_id}{suffix}"
    absolute_path = Path(workspace_root) / relative_path
    absolute_path.parent.mkdir(parents=True, exist_ok=True)
    return absolute_path, relative_path


def _sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _content_type_to_suffix(content_type: str | None) -> str:
    if content_type is None:
        return ".txt"
    lowered = content_type.lower()
    if "html" in lowered:
        return ".html"
    if "json" in lowered:
        return ".json"
    if "xml" in lowered:
        return ".xml"
    return ".txt"


def _write_artifact(
    workspace_root: str | Path,
    source_id: str,
    entry_id: str,
    artifact_type: str,
    body: bytes,
    content_type: str | None,
    *,
    suffix_override: str | None = None,
) -> dict[str, Any]:
    suffix = suffix_override or _content_type_to_suffix(content_type)
    path, relative_path = _artifact_path(workspace_root, source_id, entry_id, suffix)
    path.write_bytes(body)
    return {
        "artifact_type": artifact_type,
        "path": relative_path,
        "content_type": content_type,
        "sha256": _sha256_bytes(body),
        "byte_count": len(body),
    }


def _write_headers_artifact(
    workspace_root: str | Path,
    source_id: str,
    entry_id: str,
    headers: dict[str, str],
) -> dict[str, Any]:
    payload = _safe_json_dump(headers)
    return _write_artifact(
        workspace_root,
        source_id,
        entry_id,
        "response_headers",
        payload,
        "application/json",
        suffix_override=".headers.json",
    )


def _body_preview(data: bytes | None, limit: int = 300) -> str | None:
    if not data:
        return None
    preview = data.decode("utf-8", "ignore").strip()
    if not preview:
        return None
    return preview[:limit]


def _http_error_type(source_id: str, status_code: int | None) -> str:
    if status_code == 403 and source_id.startswith(LISTING_SOURCE_PREFIX):
        return "anti_bot_block"
    if status_code is None:
        return "network_error"
    return "http_error"


def _request_descriptor(
    *,
    request_kind: str,
    method: str | None,
    target_url: str | None,
    subject_ids: list[str],
    county_fips: str | None,
    state_fips: str | None,
    coordinates: dict[str, float] | None,
    query_params: dict[str, Any] | None = None,
    body_preview: str | None = None,
    notes: list[str] | None = None,
) -> dict[str, Any]:
    return {
        "request_kind": request_kind,
        "method": method,
        "target_url": target_url,
        "subject_ids": sorted(set(subject_ids)),
        "county_fips": county_fips,
        "state_fips": state_fips,
        "coordinates": coordinates,
        "query_params": query_params or {},
        "body_preview": body_preview,
        "notes": notes or [],
    }


def _base_entry(
    *,
    entry_id: str,
    source_id: str | None,
    source_name: str | None,
    source_category: str | None,
    interface_type: str | None,
    capability: str,
    requested_capabilities: list[str],
    source_capabilities: list[str],
    subject_ids: list[str],
) -> dict[str, Any]:
    return {
        "entry_id": entry_id,
        "source_id": source_id,
        "source_name": source_name,
        "source_category": source_category,
        "interface_type": interface_type,
        "capability": capability,
        "requested_capabilities": sorted(set(requested_capabilities)),
        "source_capabilities": sorted(set(source_capabilities)),
        "subject_ids": sorted(set(subject_ids)),
        "retrieval_status": "blocked",
        "execution_mode": "not_executed",
        "attempted_at": None,
        "completed_at": None,
        "request_descriptor": _request_descriptor(
            request_kind="plan_step_block",
            method=None,
            target_url=None,
            subject_ids=subject_ids,
            county_fips=None,
            state_fips=None,
            coordinates=None,
        ),
        "artifact_refs": [],
        "extracted_field_refs": [],
        "parser_version": None,
        "http_status": None,
        "error_type": "none",
        "error_message": None,
        "notes": [],
    }


def _blocked_or_deferred_plan_entries(source_plan: dict[str, Any]) -> list[dict[str, Any]]:
    entries: list[dict[str, Any]] = []
    for bucket_name, retrieval_status in (("blocked_steps", "blocked"), ("deferred_steps", "deferred")):
        for index, step in enumerate(source_plan.get(bucket_name, []), start=1):
            entry_id = deterministic_uuid(
                "alice-fetch-plan-step",
                source_plan["source_plan_id"],
                bucket_name,
                str(index),
                step["capability"],
                ",".join(step["subject_ids"]),
            )
            entry = _base_entry(
                entry_id=entry_id,
                source_id=None,
                source_name=None,
                source_category=None,
                interface_type=None,
                capability=step["capability"],
                requested_capabilities=[step["capability"]],
                source_capabilities=[],
                subject_ids=step["subject_ids"],
            )
            entry["retrieval_status"] = retrieval_status
            entry["error_type"] = BLOCKER_TO_ERROR.get(step["blocker_type"], "missing_prerequisite")
            entry["error_message"] = step["reason"]
            entry["notes"] = [step["reason"]]
            entry["request_descriptor"] = _request_descriptor(
                request_kind="plan_step_block",
                method=None,
                target_url=None,
                subject_ids=step["subject_ids"],
                county_fips=None,
                state_fips=None,
                coordinates=None,
                query_params={"dependency": step.get("dependency")},
                notes=[f"{step['blocker_type']}: {step['reason']}"],
            )
            entries.append(entry)
    return entries


def _execution_items(
    source_plan: dict[str, Any],
    subject_resolution: dict[str, Any],
) -> list[dict[str, Any]]:
    aggregated: dict[str, dict[str, Any]] = {}
    execution_items: list[dict[str, Any]] = []

    for group in source_plan.get("planned_source_groups", []):
        if group["group_status"] != "ready":
            continue
        for source_id in group["source_ids"]:
            descriptor = _source_descriptor(source_id)
            if source_id.startswith(LISTING_SOURCE_PREFIX):
                platform = source_id.removeprefix(LISTING_SOURCE_PREFIX)
                subjects = _primary_listing_subjects(subject_resolution, platform)
                for subject in subjects:
                    key = f"{source_id}::{subject['subject_id']}"
                    execution_item = aggregated.get(key)
                    if execution_item is None:
                        execution_item = {
                            "source_id": source_id,
                            "descriptor": descriptor,
                            "subject_ids": [subject["subject_id"]],
                            "requested_capabilities": [],
                            "priority_order": group["priority_order"],
                            "target_url": subject["identifiers"]["canonical_url"],
                            "listing_subject": subject,
                        }
                        aggregated[key] = execution_item
                        execution_items.append(execution_item)
                    execution_item["requested_capabilities"].append(group["capability"])
                    execution_item["priority_order"] = min(
                        execution_item["priority_order"],
                        group["priority_order"],
                    )
                continue

            key = source_id
            execution_item = aggregated.get(key)
            if execution_item is None:
                execution_item = {
                    "source_id": source_id,
                    "descriptor": descriptor,
                    "subject_ids": list(group["subject_ids"]),
                    "requested_capabilities": [],
                    "priority_order": group["priority_order"],
                    "target_url": descriptor["base_url"],
                }
                aggregated[key] = execution_item
                execution_items.append(execution_item)
            execution_item["requested_capabilities"].append(group["capability"])
            execution_item["priority_order"] = min(
                execution_item["priority_order"],
                group["priority_order"],
            )
            execution_item["subject_ids"] = sorted(
                set(execution_item["subject_ids"]) | set(group["subject_ids"])
            )

    execution_items.sort(key=lambda item: (item["priority_order"], item["source_id"]))
    return execution_items


def _soap_envelope(query: str) -> bytes:
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<soap:Envelope xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" '
        'xmlns:xsd="http://www.w3.org/2001/XMLSchema" '
        'xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">\n'
        "  <soap:Body>\n"
        '    <RunQuery xmlns="http://SDMDataAccess.nrcs.usda.gov/Tabular/SDMTabularService.asmx">\n'
        f"      <Query>{query}</Query>\n"
        "    </RunQuery>\n"
        "  </soap:Body>\n"
        "</soap:Envelope>"
    ).encode("utf-8")


def _request_transport(
    transport: LiveTransport | FixtureTransport,
    source_request: SourceRequest,
) -> SourceResponse:
    return transport.request(source_request)


def _execute_generic_page(
    *,
    transport: LiveTransport | FixtureTransport,
    workspace_root: str | Path,
    entry: dict[str, Any],
    request_key: str,
    target_url: str,
    request_kind: str,
    county_fips: str | None,
    state_fips: str | None,
    coordinates: dict[str, float] | None,
    timestamp: str,
) -> dict[str, Any]:
    request = SourceRequest(
        fixture_key=request_key,
        url=target_url,
        method="GET",
    )
    entry["execution_mode"] = "fixture_replay" if isinstance(transport, FixtureTransport) else "live_http"
    entry["attempted_at"] = timestamp
    entry["request_descriptor"] = _request_descriptor(
        request_kind=request_kind,
        method="GET",
        target_url=target_url,
        subject_ids=entry["subject_ids"],
        county_fips=county_fips,
        state_fips=state_fips,
        coordinates=coordinates,
    )
    try:
        response = _request_transport(transport, request)
        entry["completed_at"] = timestamp
        entry["http_status"] = response.status
        entry["artifact_refs"] = [
            _write_artifact(
                workspace_root,
                entry["source_id"],
                entry["entry_id"],
                "raw_html" if "html" in (response.content_type or "").lower() else "raw_text",
                response.body,
                response.content_type,
            ),
            _write_headers_artifact(
                workspace_root,
                entry["source_id"],
                entry["entry_id"],
                response.headers,
            ),
        ]
        if response.status is not None and response.status >= 400:
            entry["retrieval_status"] = "failed"
            entry["error_type"] = _http_error_type(entry["source_id"], response.status)
            entry["error_message"] = f"HTTP status {response.status} returned from {response.final_url}"
            entry["notes"].append(entry["error_message"])
        else:
            entry["retrieval_status"] = "success"
            entry["error_type"] = "none"
            entry["error_message"] = None
            entry["notes"].append(f"Fetched {response.final_url} successfully.")
        return entry
    except urllib.error.HTTPError as exc:
        body = exc.read()
        entry["completed_at"] = timestamp
        entry["http_status"] = exc.code
        entry["artifact_refs"] = [
            _write_artifact(
                workspace_root,
                entry["source_id"],
                entry["entry_id"],
                "raw_html" if "html" in (exc.headers.get("Content-Type", "")).lower() else "raw_text",
                body,
                exc.headers.get("Content-Type"),
            ),
            _write_headers_artifact(
                workspace_root,
                entry["source_id"],
                entry["entry_id"],
                dict(exc.headers.items()),
            ),
        ]
        entry["retrieval_status"] = "failed"
        entry["error_type"] = _http_error_type(entry["source_id"], exc.code)
        entry["error_message"] = str(exc)
        entry["notes"].append(f"HTTP error while fetching {target_url}: {exc}")
        return entry
    except KeyError as exc:
        entry["completed_at"] = timestamp
        entry["retrieval_status"] = "unsupported"
        entry["error_type"] = "unsupported_source"
        entry["error_message"] = str(exc)
        entry["notes"].append(str(exc))
        return entry
    except Exception as exc:  # pragma: no cover - defensive network fallback
        entry["completed_at"] = timestamp
        entry["retrieval_status"] = "failed"
        entry["error_type"] = "network_error"
        entry["error_message"] = str(exc)
        entry["notes"].append(f"Network error while fetching {target_url}: {exc}")
        return entry


def _execute_arcgis_point_query(
    *,
    transport: LiveTransport | FixtureTransport,
    workspace_root: str | Path,
    entry: dict[str, Any],
    request_key: str,
    target_url: str,
    query_params: dict[str, Any],
    county_fips: str | None,
    state_fips: str | None,
    coordinates: dict[str, float] | None,
    timestamp: str,
) -> dict[str, Any]:
    if coordinates is None:
        entry["retrieval_status"] = "blocked"
        entry["error_type"] = "missing_prerequisite"
        entry["error_message"] = "Point geometry is required for this source."
        entry["notes"].append("Skipped because no coordinates were available for a point query.")
        entry["request_descriptor"] = _request_descriptor(
            request_kind="arcgis_query",
            method="GET",
            target_url=target_url,
            subject_ids=entry["subject_ids"],
            county_fips=county_fips,
            state_fips=state_fips,
            coordinates=None,
            query_params=query_params,
            notes=["Point geometry missing; query not executed."],
        )
        return entry

    encoded_url = f"{target_url}?{urllib.parse.urlencode(query_params)}"
    request = SourceRequest(
        fixture_key=request_key,
        url=encoded_url,
        method="GET",
    )
    entry["execution_mode"] = "fixture_replay" if isinstance(transport, FixtureTransport) else "live_gis_query"
    entry["attempted_at"] = timestamp
    entry["request_descriptor"] = _request_descriptor(
        request_kind="arcgis_query",
        method="GET",
        target_url=target_url,
        subject_ids=entry["subject_ids"],
        county_fips=county_fips,
        state_fips=state_fips,
        coordinates=coordinates,
        query_params=query_params,
    )
    try:
        response = _request_transport(transport, request)
        entry["completed_at"] = timestamp
        entry["http_status"] = response.status
        entry["artifact_refs"] = [
            _write_artifact(
                workspace_root,
                entry["source_id"],
                entry["entry_id"],
                "raw_json",
                response.body,
                response.content_type or "application/json",
            ),
            _write_headers_artifact(
                workspace_root,
                entry["source_id"],
                entry["entry_id"],
                response.headers,
            ),
        ]
        if response.status is not None and response.status >= 400:
            entry["retrieval_status"] = "failed"
            entry["error_type"] = "http_error"
            entry["error_message"] = f"HTTP status {response.status} returned from {response.final_url}"
            entry["notes"].append(entry["error_message"])
        else:
            entry["retrieval_status"] = "success"
            entry["error_type"] = "none"
            entry["notes"].append("ArcGIS point query completed.")
        return entry
    except urllib.error.HTTPError as exc:
        body = exc.read()
        entry["completed_at"] = timestamp
        entry["http_status"] = exc.code
        entry["artifact_refs"] = [
            _write_artifact(
                workspace_root,
                entry["source_id"],
                entry["entry_id"],
                "raw_json",
                body,
                exc.headers.get("Content-Type") or "application/json",
            ),
            _write_headers_artifact(
                workspace_root,
                entry["source_id"],
                entry["entry_id"],
                dict(exc.headers.items()),
            ),
        ]
        entry["retrieval_status"] = "failed"
        entry["error_type"] = "http_error"
        entry["error_message"] = str(exc)
        entry["notes"].append(f"ArcGIS point query failed: {exc}")
        return entry
    except KeyError as exc:
        entry["completed_at"] = timestamp
        entry["retrieval_status"] = "unsupported"
        entry["error_type"] = "unsupported_source"
        entry["error_message"] = str(exc)
        entry["notes"].append(str(exc))
        return entry
    except Exception as exc:  # pragma: no cover - defensive network fallback
        entry["completed_at"] = timestamp
        entry["retrieval_status"] = "failed"
        entry["error_type"] = "network_error"
        entry["error_message"] = str(exc)
        entry["notes"].append(f"ArcGIS point query network error: {exc}")
        return entry


def _extract_mukey_from_soap(xml_bytes: bytes) -> str | None:
    text = xml_bytes.decode("utf-8", "ignore")
    start_tag = "<mukey>"
    end_tag = "</mukey>"
    if start_tag not in text or end_tag not in text:
        return None
    return text.split(start_tag, 1)[1].split(end_tag, 1)[0].strip() or None


def _execute_usda_soil_query(
    *,
    transport: LiveTransport | FixtureTransport,
    workspace_root: str | Path,
    entry: dict[str, Any],
    county_fips: str | None,
    state_fips: str | None,
    coordinates: dict[str, float] | None,
    timestamp: str,
) -> dict[str, Any]:
    if coordinates is None:
        entry["retrieval_status"] = "blocked"
        entry["error_type"] = "missing_prerequisite"
        entry["error_message"] = "Point geometry is required for USDA SDA soil lookup."
        entry["notes"].append("Skipped USDA soil lookup because no coordinates were available.")
        entry["request_descriptor"] = _request_descriptor(
            request_kind="soap_query",
            method="POST",
            target_url=USDA_SDA_SOAP_URL,
            subject_ids=entry["subject_ids"],
            county_fips=county_fips,
            state_fips=state_fips,
            coordinates=None,
            notes=["Point geometry missing; SOAP query not executed."],
        )
        return entry

    point_wkt = f"POINT({coordinates['longitude']} {coordinates['latitude']})"
    mukey_query = f"SELECT * FROM SDA_Get_Mukey_from_intersection_with_WktWgs84('{point_wkt}')"
    mukey_body = _soap_envelope(mukey_query)
    mukey_request = SourceRequest(
        fixture_key=f"{entry['source_id']}.mukey_lookup",
        url=USDA_SDA_SOAP_URL,
        method="POST",
        headers={
            "Content-Type": "text/xml; charset=utf-8",
            "SOAPAction": '"http://SDMDataAccess.nrcs.usda.gov/Tabular/SDMTabularService.asmx/RunQuery"',
        },
        data=mukey_body,
    )

    entry["execution_mode"] = "fixture_replay" if isinstance(transport, FixtureTransport) else "live_soap"
    entry["attempted_at"] = timestamp
    entry["request_descriptor"] = _request_descriptor(
        request_kind="soap_query",
        method="POST",
        target_url=USDA_SDA_SOAP_URL,
        subject_ids=entry["subject_ids"],
        county_fips=county_fips,
        state_fips=state_fips,
        coordinates=coordinates,
        body_preview=_body_preview(mukey_body),
        notes=["SOAP step 1 finds the mukey intersecting the point geometry."],
    )

    artifact_refs: list[dict[str, Any]] = []
    try:
        mukey_response = _request_transport(transport, mukey_request)
        artifact_refs.append(
            _write_artifact(
                workspace_root,
                entry["source_id"],
                entry["entry_id"],
                "raw_xml",
                mukey_response.body,
                mukey_response.content_type or "text/xml",
                suffix_override="__mukey.xml",
            )
        )
        artifact_refs.append(
            _write_headers_artifact(
                workspace_root,
                entry["source_id"],
                entry["entry_id"],
                mukey_response.headers,
            )
        )
        if mukey_response.status is not None and mukey_response.status >= 400:
            entry["completed_at"] = timestamp
            entry["http_status"] = mukey_response.status
            entry["artifact_refs"] = artifact_refs
            entry["retrieval_status"] = "failed"
            entry["error_type"] = "http_error"
            entry["error_message"] = f"HTTP status {mukey_response.status} returned from {mukey_response.final_url}"
            entry["notes"].append(entry["error_message"])
            return entry
        mukey = _extract_mukey_from_soap(mukey_response.body)
        if mukey is None:
            entry["completed_at"] = timestamp
            entry["http_status"] = mukey_response.status
            entry["artifact_refs"] = artifact_refs
            entry["retrieval_status"] = "partial"
            entry["error_type"] = "empty_result"
            entry["error_message"] = "USDA SDA returned no mukey for the point query."
            entry["notes"].append("SOAP lookup succeeded but returned no mukey for the sampled point.")
            return entry

        detail_query = (
            "SELECT mukey, muname, slopegraddcp, drclassdcd, hydgrpdcd, flodfreqdcd, "
            f"niccdcd, awmmfpwwta, aws0100wta, hydclprs FROM muaggatt WHERE mukey = '{mukey}'"
        )
        detail_body = _soap_envelope(detail_query)
        detail_request = SourceRequest(
            fixture_key=f"{entry['source_id']}.muaggatt_lookup",
            url=USDA_SDA_SOAP_URL,
            method="POST",
            headers={
                "Content-Type": "text/xml; charset=utf-8",
                "SOAPAction": '"http://SDMDataAccess.nrcs.usda.gov/Tabular/SDMTabularService.asmx/RunQuery"',
            },
            data=detail_body,
        )
        detail_response = _request_transport(transport, detail_request)
        artifact_refs.append(
            _write_artifact(
                workspace_root,
                entry["source_id"],
                entry["entry_id"],
                "raw_xml",
                detail_response.body,
                detail_response.content_type or "text/xml",
                suffix_override="__muaggatt.xml",
            )
        )
        entry["completed_at"] = timestamp
        entry["http_status"] = detail_response.status
        entry["artifact_refs"] = artifact_refs
        if detail_response.status is not None and detail_response.status >= 400:
            entry["retrieval_status"] = "failed"
            entry["error_type"] = "http_error"
            entry["error_message"] = f"HTTP status {detail_response.status} returned from {detail_response.final_url}"
            entry["notes"].append(entry["error_message"])
        else:
            entry["retrieval_status"] = "success"
            entry["error_type"] = "none"
            entry["notes"].append(f"USDA SDA soil lookup returned mukey {mukey}.")
        return entry
    except urllib.error.HTTPError as exc:
        body = exc.read()
        artifact_refs.append(
            _write_artifact(
                workspace_root,
                entry["source_id"],
                entry["entry_id"],
                "raw_xml",
                body,
                exc.headers.get("Content-Type") or "text/xml",
                suffix_override="__error.xml",
            )
        )
        entry["completed_at"] = timestamp
        entry["http_status"] = exc.code
        entry["artifact_refs"] = artifact_refs
        entry["retrieval_status"] = "failed"
        entry["error_type"] = "http_error"
        entry["error_message"] = str(exc)
        entry["notes"].append(f"USDA SDA SOAP query failed: {exc}")
        return entry
    except KeyError as exc:
        entry["completed_at"] = timestamp
        entry["artifact_refs"] = artifact_refs
        entry["retrieval_status"] = "unsupported"
        entry["error_type"] = "unsupported_source"
        entry["error_message"] = str(exc)
        entry["notes"].append(str(exc))
        return entry
    except Exception as exc:  # pragma: no cover - defensive network fallback
        entry["completed_at"] = timestamp
        entry["artifact_refs"] = artifact_refs
        entry["retrieval_status"] = "failed"
        entry["error_type"] = "network_error"
        entry["error_message"] = str(exc)
        entry["notes"].append(f"USDA SDA network error: {exc}")
        return entry


def _execute_item(
    *,
    item: dict[str, Any],
    transport: LiveTransport | FixtureTransport,
    workspace_root: str | Path,
    subject_resolution: dict[str, Any],
    jurisdiction_context: dict[str, Any],
    generated_at: str | None = None,
) -> dict[str, Any]:
    descriptor = item["descriptor"]
    source_id = item["source_id"]
    requested_capabilities = sorted(set(item["requested_capabilities"]))
    capability = requested_capabilities[0]
    entry_id = deterministic_uuid(
        "alice-fetch-entry",
        source_id,
        ",".join(item["subject_ids"]),
        item["target_url"],
    )
    entry = _base_entry(
        entry_id=entry_id,
        source_id=source_id,
        source_name=descriptor["name"],
        source_category=descriptor["category"],
        interface_type=descriptor["interface_type"],
        capability=capability,
        requested_capabilities=requested_capabilities,
        source_capabilities=descriptor["capabilities"],
        subject_ids=item["subject_ids"],
    )
    county_fips = jurisdiction_context.get("county_fips")
    state_fips = jurisdiction_context.get("state_fips")
    coordinates = _primary_coordinates(subject_resolution)
    timestamp = _generated_at(generated_at)

    if source_id.startswith(LISTING_SOURCE_PREFIX):
        return _execute_generic_page(
            transport=transport,
            workspace_root=workspace_root,
            entry=entry,
            request_key=f"{source_id}.page",
            target_url=item["target_url"],
            request_kind="listing_page_fetch",
            county_fips=county_fips,
            state_fips=state_fips,
            coordinates=coordinates,
            timestamp=timestamp,
        )

    if source_id == "federal_fema_nfhl":
        query_params = {
            "f": "json",
            "geometry": json.dumps(
                {
                    "x": coordinates["longitude"] if coordinates else None,
                    "y": coordinates["latitude"] if coordinates else None,
                    "spatialReference": {"wkid": 4326},
                }
            ),
            "geometryType": "esriGeometryPoint",
            "spatialRel": "esriSpatialRelIntersects",
            "returnGeometry": "false",
            "outFields": "FLD_ZONE,ZONE_SUBTY,SFHA_TF,STATIC_BFE,V_DATUM",
        }
        return _execute_arcgis_point_query(
            transport=transport,
            workspace_root=workspace_root,
            entry=entry,
            request_key=f"{source_id}.point_query",
            target_url=FEMA_FLOOD_QUERY_URL,
            query_params=query_params,
            county_fips=county_fips,
            state_fips=state_fips,
            coordinates=coordinates,
            timestamp=timestamp,
        )

    if source_id == "federal_usfws_nwi":
        query_params = {
            "f": "json",
            "geometry": json.dumps(
                {
                    "x": coordinates["longitude"] if coordinates else None,
                    "y": coordinates["latitude"] if coordinates else None,
                    "spatialReference": {"wkid": 4326},
                }
            ),
            "geometryType": "esriGeometryPoint",
            "spatialRel": "esriSpatialRelIntersects",
            "returnGeometry": "false",
            "outFields": (
                "Wetlands.ATTRIBUTE,Wetlands.WETLAND_TYPE,Wetlands.ACRES,"
                "NWI_Wetland_Codes.SYSTEM_NAME,NWI_Wetland_Codes.CLASS_NAME"
            ),
        }
        return _execute_arcgis_point_query(
            transport=transport,
            workspace_root=workspace_root,
            entry=entry,
            request_key=f"{source_id}.point_query",
            target_url=NWI_WETLANDS_QUERY_URL,
            query_params=query_params,
            county_fips=county_fips,
            state_fips=state_fips,
            coordinates=coordinates,
            timestamp=timestamp,
        )

    if source_id == "federal_usgs_3dep_elevation":
        query_params = {
            "geometry": json.dumps(
                {
                    "x": coordinates["longitude"] if coordinates else None,
                    "y": coordinates["latitude"] if coordinates else None,
                    "spatialReference": {"wkid": 4326},
                }
            ),
            "geometryType": "esriGeometryPoint",
            "returnFirstValueOnly": "true",
            "f": "pjson",
        }
        return _execute_arcgis_point_query(
            transport=transport,
            workspace_root=workspace_root,
            entry=entry,
            request_key=f"{source_id}.point_query",
            target_url=USGS_ELEVATION_QUERY_URL,
            query_params=query_params,
            county_fips=county_fips,
            state_fips=state_fips,
            coordinates=coordinates,
            timestamp=timestamp,
        )

    if source_id == "federal_usda_soil_data_access":
        return _execute_usda_soil_query(
            transport=transport,
            workspace_root=workspace_root,
            entry=entry,
            county_fips=county_fips,
            state_fips=state_fips,
            coordinates=coordinates,
            timestamp=timestamp,
        )

    if descriptor["interface_type"] in {"web_only", "downloadable_dataset"}:
        request_kind = (
            "dataset_landing_page_fetch"
            if descriptor["interface_type"] == "downloadable_dataset"
            else "web_page_access"
        )
        return _execute_generic_page(
            transport=transport,
            workspace_root=workspace_root,
            entry=entry,
            request_key=f"{source_id}.page",
            target_url=item["target_url"],
            request_kind=request_kind,
            county_fips=county_fips,
            state_fips=state_fips,
            coordinates=coordinates,
            timestamp=timestamp,
        )

    entry["retrieval_status"] = "unsupported"
    entry["error_type"] = "unsupported_source"
    entry["error_message"] = f"Step 6 does not yet support source interface type {descriptor['interface_type']} for {source_id}."
    entry["notes"].append(entry["error_message"])
    return entry


def execute_source_plan(
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
    jurisdiction_context: dict[str, Any],
    source_plan: dict[str, Any],
    *,
    workspace_root: str | Path,
    transport: LiveTransport | FixtureTransport | None = None,
    generated_at: str | None = None,
) -> dict[str, Any]:
    transport = transport or LiveTransport()
    fetch_entries = _blocked_or_deferred_plan_entries(source_plan)
    for item in _execution_items(source_plan, subject_resolution):
        fetch_entries.append(
            _execute_item(
                item=item,
                transport=transport,
                workspace_root=workspace_root,
                subject_resolution=subject_resolution,
                jurisdiction_context=jurisdiction_context,
                generated_at=generated_at,
            )
        )

    fetch_log = {
        "schema_version": SOURCE_FETCH_LOG_VERSION,
        "fetch_log_id": deterministic_uuid(
            "alice-source-fetch-log",
            request["request_id"],
            source_plan["source_plan_id"],
        ),
        "request_id": request["request_id"],
        "subject_resolution_id": subject_resolution["resolution_id"],
        "jurisdiction_context_id": jurisdiction_context["jurisdiction_context_id"],
        "source_plan_id": source_plan["source_plan_id"],
        "generated_at": _generated_at(generated_at),
        "fetch_entries": fetch_entries,
    }
    output_path = _workspace_alice_root(workspace_root) / "source_fetch_log.json"
    output_path.write_text(json.dumps(fetch_log, indent=2), encoding="utf-8")
    return fetch_log


def summarize_fetch_log(fetch_log: dict[str, Any]) -> dict[str, Any]:
    counts: dict[str, int] = {}
    for entry in fetch_log["fetch_entries"]:
        counts[entry["retrieval_status"]] = counts.get(entry["retrieval_status"], 0) + 1
    return {
        "fetch_log_id": fetch_log["fetch_log_id"],
        "entry_count": len(fetch_log["fetch_entries"]),
        "counts": counts,
        "successful_source_ids": sorted(
            {
                entry["source_id"]
                for entry in fetch_log["fetch_entries"]
                if entry["retrieval_status"] == "success" and entry["source_id"] is not None
            }
        ),
    }


def load_source_fetch_log(path: str | Path) -> dict[str, Any]:
    return load_json(Path(path))
