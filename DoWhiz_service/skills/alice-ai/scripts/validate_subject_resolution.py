#!/usr/bin/env python3

"""Validate Alice Step 4 request, subject resolution, and session-state examples."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path

from alice_subject_resolution import (
    REQUEST_SCHEMA_PATH,
    SESSION_STATE_SCHEMA_PATH,
    SUBJECT_RESOLUTION_SCHEMA_PATH,
    build_subject_resolution,
    load_request,
    normalize_apn,
    normalize_listing_url,
)


EXAMPLES_ROOT = Path(__file__).resolve().parent.parent / "examples"
CHUNK_SIZE = 200
FOLLOW_UP_REQUEST_EXAMPLE = EXAMPLES_ROOT / "request.follow_up.example.json"
FOLLOW_UP_SESSION_EXAMPLE = EXAMPLES_ROOT / "session_state.follow_up.example.json"
RESOLVED_FOLLOW_UP_EXAMPLE = EXAMPLES_ROOT / "subject_resolution.resolved_follow_up.example.json"


def assert_true(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def require_npx() -> str:
    npx = shutil.which("npx")
    if npx is None:
        raise SystemExit("npx is required for schema validation but was not found in PATH.")
    return npx


def run_ajv(schema_path: Path, json_paths: list[Path], label: str) -> None:
    if not json_paths:
        return
    npx = require_npx()
    for start in range(0, len(json_paths), CHUNK_SIZE):
        chunk = json_paths[start : start + CHUNK_SIZE]
        command = [
            npx,
            "--yes",
            "-p",
            "ajv-cli",
            "-p",
            "ajv-formats",
            "ajv",
            "validate",
            "--spec=draft2020",
            "-c",
            "ajv-formats",
            "-s",
            str(schema_path.resolve()),
        ]
        for path in chunk:
            command.extend(["-d", str(path.resolve())])
        result = subprocess.run(command, check=False, capture_output=True, text=True)
        if result.returncode != 0:
            raise SystemExit(
                f"Schema validation failed for {label}:\n{result.stdout}\n{result.stderr}"
            )


def validate_request_examples() -> list[Path]:
    request_paths = sorted(EXAMPLES_ROOT.glob("request*.example.json"))
    assert_true(bool(request_paths), "No Alice request examples were found.")
    run_ajv(REQUEST_SCHEMA_PATH, request_paths, "request examples")
    return request_paths


def validate_subject_resolution_examples() -> list[Path]:
    resolution_paths = sorted(EXAMPLES_ROOT.glob("subject_resolution*.example.json"))
    assert_true(bool(resolution_paths), "No Alice subject resolution examples were found.")
    run_ajv(SUBJECT_RESOLUTION_SCHEMA_PATH, resolution_paths, "subject resolution examples")
    return resolution_paths


def validate_session_state_examples() -> list[Path]:
    session_paths = sorted(EXAMPLES_ROOT.glob("session_state*.example.json"))
    assert_true(bool(session_paths), "No Alice session state examples were found.")
    run_ajv(SESSION_STATE_SCHEMA_PATH, session_paths, "session state examples")
    return session_paths


def validate_generated_resolution_from_requests(request_paths: list[Path]) -> None:
    with tempfile.TemporaryDirectory(prefix="alice-generated-subject-resolution-") as temp_dir:
        temp_root = Path(temp_dir)
        generated_paths: list[Path] = []
        for request_path in request_paths:
            request = load_request(request_path)
            resolution = build_subject_resolution(request)
            output_path = temp_root / request_path.name.replace("request", "generated_subject_resolution")
            output_path.write_text(json.dumps(resolution, indent=2), encoding="utf-8")
            generated_paths.append(output_path)
        run_ajv(SUBJECT_RESOLUTION_SCHEMA_PATH, generated_paths, "generated subject resolution artifacts")


def smoke_test_listing_url_patterns() -> None:
    cases = [
        (
            "https://www.redfin.com/TX/Sierra-Blanca/12345-Ranch-Rd-79851/home/123456789?utm_source=alice",
            "redfin",
            "123456789",
            None,
        ),
        (
            "https://www.zillow.com/homedetails/12345-County-Road-12-Sierra-Blanca-TX-79851/207654321_zpid/?view=public",
            "zillow",
            "207654321",
            None,
        ),
        (
            "https://www.landwatch.com/texas-land-for-sale/hudspeth-county/property/80-acres-near-sierra-blanca/id/12345678",
            "landwatch",
            "12345678",
            "Hudspeth County",
        ),
        (
            "https://www.land.com/property/80-acres-in-Hudspeth-County-Texas/87654321/",
            "land_com",
            "87654321",
            "Hudspeth County",
        ),
    ]
    for raw_url, expected_platform, expected_listing_id, expected_county in cases:
        normalized = normalize_listing_url(raw_url)
        assert_true(
            normalized["normalized"].get("platform") == expected_platform,
            f"Expected platform {expected_platform} for {raw_url}",
        )
        assert_true(
            normalized["normalized"].get("listing_id") == expected_listing_id,
            f"Expected listing ID {expected_listing_id} for {raw_url}",
        )
        assert_true(
            "?" not in normalized["normalized"]["canonical_url"],
            f"Expected canonical URL query stripping for {raw_url}",
        )
        if expected_county is not None:
            assert_true(
                normalized["normalized"].get("county_text_hint") == expected_county,
                f"Expected county hint {expected_county} for {raw_url}",
            )


def smoke_test_apn_normalization() -> None:
    normalized = normalize_apn("H123-456-7890-0000", county_fips="48229")
    fields = normalized["normalized"]
    assert_true(fields["apn_raw"] == "H123-456-7890-0000", "APN normalization must preserve the raw APN.")
    assert_true(
        fields["apn_normalized"] == "H12345678900000",
        "APN normalization should emit the conservative canonical key.",
    )
    assert_true(
        fields["apn_strategy"] == "state_tx_alphanumeric_hyphenated",
        "Texas county-scoped APN normalization should use the Texas strategy.",
    )
    assert_true(
        {"H12345678900000", "H123-456-7890-0000", "12345678900000"} <= set(fields["apn_matching_variants"]),
        "APN matching variants are missing expected canonical forms.",
    )


def smoke_test_resolution_coverage(resolution_paths: list[Path]) -> None:
    statuses = set()
    request_modes = set()
    has_batch = False
    for path in resolution_paths:
        payload = json.loads(path.read_text())
        statuses.add(payload["resolution_status"])
        request_modes.add(payload["request_mode"])
        if payload["subject_kind"] == "batch_subject_set":
            has_batch = True
    assert_true({"resolved", "partially_resolved", "unresolved"} <= statuses, "Resolution examples do not cover all target statuses.")
    assert_true(has_batch, "Resolution examples do not include a batch subject set case.")
    assert_true("recommendation" in request_modes, "Resolution examples do not include a recommendation-mode case.")
    assert_true("follow_up" in request_modes, "Resolution examples do not include a follow-up case.")


def smoke_test_follow_up_state_contract() -> None:
    assert_true(FOLLOW_UP_REQUEST_EXAMPLE.exists(), "Follow-up request example is missing.")
    assert_true(FOLLOW_UP_SESSION_EXAMPLE.exists(), "Follow-up session-state example is missing.")
    assert_true(RESOLVED_FOLLOW_UP_EXAMPLE.exists(), "Resolved follow-up subject-resolution example is missing.")

    request = json.loads(FOLLOW_UP_REQUEST_EXAMPLE.read_text())
    session_state = json.loads(FOLLOW_UP_SESSION_EXAMPLE.read_text())
    resolved_follow_up = json.loads(RESOLVED_FOLLOW_UP_EXAMPLE.read_text())

    active_subject_ids = {subject["subject_id"] for subject in session_state["active_subjects"]}
    carry_forward_ids = set(request["follow_up_context"].get("carry_forward_subject_ids", []))
    assert_true(
        carry_forward_ids <= active_subject_ids,
        "Follow-up carry_forward_subject_ids do not map to the example session state's active subjects.",
    )
    assert_true(
        resolved_follow_up["resolution_status"] == "resolved",
        "Resolved follow-up example must demonstrate a fully attached prior subject.",
    )
    assert_true(
        resolved_follow_up["active_subjects"][0]["subject_id"] in active_subject_ids,
        "Resolved follow-up example does not point at a subject present in the example session state.",
    )


def main() -> int:
    request_paths = validate_request_examples()
    resolution_paths = validate_subject_resolution_examples()
    validate_session_state_examples()
    validate_generated_resolution_from_requests(request_paths)
    smoke_test_listing_url_patterns()
    smoke_test_apn_normalization()
    smoke_test_resolution_coverage(resolution_paths)
    smoke_test_follow_up_state_contract()

    print(
        "Alice subject resolution validation passed: "
        f"{len(request_paths)} request examples, {len(resolution_paths)} subject resolution examples."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
