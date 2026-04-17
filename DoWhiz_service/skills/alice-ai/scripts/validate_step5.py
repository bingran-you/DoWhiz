#!/usr/bin/env python3

"""Validate Alice Step 5 jurisdiction, coverage, and source-planning artifacts."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path

from alice_coverage_assessment import (
    COVERAGE_ASSESSMENT_SCHEMA_PATH,
    build_coverage_assessment,
    summarize_coverage_assessment,
)
from alice_jurisdiction import (
    JURISDICTION_CONTEXT_SCHEMA_PATH,
    build_jurisdiction_context,
    summarize_jurisdiction_context,
)
from alice_source_plan import SOURCE_PLAN_SCHEMA_PATH, build_source_plan, summarize_source_plan
from alice_subject_resolution import load_request, load_session_state


EXAMPLES_ROOT = Path(__file__).resolve().parent.parent / "examples"
CHUNK_SIZE = 200

SCENARIOS = [
    {
        "name": "listing_url_inferred_curated",
        "request": "request.listing_url_inferred_curated.example.json",
        "subject_resolution": "subject_resolution.listing_url_inferred_curated.example.json",
        "session_state": None,
        "expected_jurisdiction": {
            "geography_status": "county_inferred",
            "county_fips": "48229",
            "effective_county_registry_mode": "curated_override",
        },
        "expected_coverage": {
            "effective_coverage_tier": "partial",
            "suitability_for_deep_research": "partially_ready",
        },
        "expected_plan": {
            "plan_status": "partially_blocked",
            "blocked_step_count": 1,
        },
    },
    {
        "name": "listing_url_fallback",
        "request": "request.listing_url_fallback.example.json",
        "subject_resolution": "subject_resolution.listing_url_fallback.example.json",
        "session_state": None,
        "expected_jurisdiction": {
            "geography_status": "county_inferred",
            "county_fips": "01001",
            "effective_county_registry_mode": "generated_fallback",
        },
        "expected_coverage": {
            "effective_coverage_tier": "minimal",
            "local_footing_status": "federal_baseline_only",
        },
        "expected_plan": {
            "plan_status": "partially_blocked",
            "blocked_step_count": 5,
        },
    },
    {
        "name": "apn_unresolved",
        "request": "request.apn_unresolved.example.json",
        "subject_resolution": "subject_resolution.apn_unresolved.example.json",
        "session_state": None,
        "expected_jurisdiction": {
            "geography_status": "unresolved",
            "county_fips": None,
            "effective_county_registry_mode": "none",
        },
        "expected_coverage": {
            "effective_coverage_tier": None,
            "suitability_for_deep_research": "not_ready",
        },
        "expected_plan": {
            "plan_status": "blocked",
            "blocked_step_count": 8,
        },
    },
    {
        "name": "address",
        "request": "request.address.example.json",
        "subject_resolution": "subject_resolution.address.example.json",
        "session_state": None,
        "expected_jurisdiction": {
            "geography_status": "city_state_partial",
            "county_fips": None,
            "effective_county_registry_mode": "none",
        },
        "expected_coverage": {
            "effective_coverage_tier": None,
            "suitability_for_deep_research": "not_ready",
        },
        "expected_plan": {
            "plan_status": "partially_blocked",
            "blocked_step_count": 4,
        },
    },
    {
        "name": "coordinates",
        "request": "request.coordinates.example.json",
        "subject_resolution": "subject_resolution.coordinates.example.json",
        "session_state": None,
        "expected_jurisdiction": {
            "geography_status": "geography_only",
            "county_fips": None,
            "effective_county_registry_mode": "none",
        },
        "expected_coverage": {
            "effective_coverage_tier": None,
            "suitability_for_deep_research": "not_ready",
        },
        "expected_plan": {
            "plan_status": "partially_blocked",
            "blocked_step_count": 7,
        },
    },
    {
        "name": "follow_up",
        "request": "request.follow_up.example.json",
        "subject_resolution": "subject_resolution.follow_up.example.json",
        "session_state": "session_state.follow_up.example.json",
        "expected_jurisdiction": {
            "geography_status": "county_resolved",
            "county_fips": "48229",
            "effective_county_registry_mode": "curated_override",
        },
        "expected_coverage": {
            "effective_coverage_tier": "partial",
            "suitability_for_deep_research": "partially_ready",
        },
        "expected_plan": {
            "plan_status": "partially_blocked",
            "blocked_step_count": 1,
        },
    },
    {
        "name": "batch_compare",
        "request": "request.batch_compare.example.json",
        "subject_resolution": "subject_resolution.batch_compare.example.json",
        "session_state": None,
        "expected_jurisdiction": {
            "geography_status": "mixed_subjects",
            "county_fips": None,
            "effective_county_registry_mode": "none",
        },
        "expected_coverage": {
            "effective_coverage_tier": None,
            "suitability_for_deep_research": "not_ready",
        },
        "expected_plan": {
            "plan_status": "partially_blocked",
            "deferred_step_count": 1,
            "blocked_step_count": 2,
        },
    },
]


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


def validate_static_examples() -> tuple[list[Path], list[Path], list[Path]]:
    jurisdiction_paths = sorted(EXAMPLES_ROOT.glob("jurisdiction_context*.example.json"))
    coverage_paths = sorted(EXAMPLES_ROOT.glob("coverage_assessment*.example.json"))
    source_plan_paths = sorted(EXAMPLES_ROOT.glob("source_plan*.example.json"))

    assert_true(jurisdiction_paths, "No jurisdiction context examples were found.")
    assert_true(coverage_paths, "No coverage assessment examples were found.")
    assert_true(source_plan_paths, "No source plan examples were found.")

    run_ajv(JURISDICTION_CONTEXT_SCHEMA_PATH, jurisdiction_paths, "jurisdiction context examples")
    run_ajv(COVERAGE_ASSESSMENT_SCHEMA_PATH, coverage_paths, "coverage assessment examples")
    run_ajv(SOURCE_PLAN_SCHEMA_PATH, source_plan_paths, "source plan examples")
    return jurisdiction_paths, coverage_paths, source_plan_paths


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def _example_path(prefix: str, name: str) -> Path:
    return EXAMPLES_ROOT / f"{prefix}.{name}.example.json"


def validate_generated_step5_artifacts() -> None:
    with tempfile.TemporaryDirectory(prefix="alice-step5-generated-") as temp_dir:
        temp_root = Path(temp_dir)
        generated_jurisdiction_paths: list[Path] = []
        generated_coverage_paths: list[Path] = []
        generated_source_plan_paths: list[Path] = []

        for scenario in SCENARIOS:
            request = load_request(_example_path("request", scenario["name"]))
            subject_resolution = load_json(_example_path("subject_resolution", scenario["name"]))
            session_state = (
                load_session_state(EXAMPLES_ROOT / scenario["session_state"])
                if scenario["session_state"] is not None
                else None
            )

            jurisdiction = build_jurisdiction_context(request, subject_resolution, session_state)
            coverage = build_coverage_assessment(request, subject_resolution, jurisdiction)
            source_plan = build_source_plan(request, subject_resolution, jurisdiction, coverage)

            jurisdiction_summary = summarize_jurisdiction_context(jurisdiction)
            for key, expected_value in scenario["expected_jurisdiction"].items():
                assert_true(
                    jurisdiction_summary[key] == expected_value,
                    f"{scenario['name']} jurisdiction {key} mismatch: expected {expected_value}, got {jurisdiction_summary[key]}",
                )

            coverage_summary = summarize_coverage_assessment(coverage)
            for key, expected_value in scenario["expected_coverage"].items():
                assert_true(
                    coverage_summary[key] == expected_value,
                    f"{scenario['name']} coverage {key} mismatch: expected {expected_value}, got {coverage_summary[key]}",
                )

            source_plan_summary = summarize_source_plan(source_plan)
            for key, expected_value in scenario["expected_plan"].items():
                assert_true(
                    source_plan_summary[key] == expected_value,
                    f"{scenario['name']} source plan {key} mismatch: expected {expected_value}, got {source_plan_summary[key]}",
                )

            jurisdiction_path = temp_root / f"generated_jurisdiction_context.{scenario['name']}.json"
            jurisdiction_path.write_text(json.dumps(jurisdiction, indent=2), encoding="utf-8")
            generated_jurisdiction_paths.append(jurisdiction_path)

            coverage_path = temp_root / f"generated_coverage_assessment.{scenario['name']}.json"
            coverage_path.write_text(json.dumps(coverage, indent=2), encoding="utf-8")
            generated_coverage_paths.append(coverage_path)

            source_plan_path = temp_root / f"generated_source_plan.{scenario['name']}.json"
            source_plan_path.write_text(json.dumps(source_plan, indent=2), encoding="utf-8")
            generated_source_plan_paths.append(source_plan_path)

        run_ajv(
            JURISDICTION_CONTEXT_SCHEMA_PATH,
            generated_jurisdiction_paths,
            "generated jurisdiction context artifacts",
        )
        run_ajv(
            COVERAGE_ASSESSMENT_SCHEMA_PATH,
            generated_coverage_paths,
            "generated coverage assessment artifacts",
        )
        run_ajv(
            SOURCE_PLAN_SCHEMA_PATH,
            generated_source_plan_paths,
            "generated source plan artifacts",
        )


def smoke_test_example_coverage(
    jurisdiction_paths: list[Path],
    coverage_paths: list[Path],
    source_plan_paths: list[Path],
) -> None:
    geography_statuses = {load_json(path)["geography_status"] for path in jurisdiction_paths}
    assert_true(
        {
            "county_inferred",
            "county_resolved",
            "city_state_partial",
            "geography_only",
            "mixed_subjects",
            "unresolved",
        }
        <= geography_statuses,
        "Jurisdiction examples do not cover the expected geography-status cases.",
    )

    readiness_values = {load_json(path)["suitability_for_deep_research"] for path in coverage_paths}
    assert_true(
        {"partially_ready", "not_ready"} <= readiness_values,
        "Coverage examples do not cover both partially_ready and not_ready cases.",
    )

    plan_statuses = {load_json(path)["plan_status"] for path in source_plan_paths}
    assert_true(
        {"partially_blocked", "blocked"} <= plan_statuses,
        "Source plan examples do not cover both partially_blocked and blocked cases.",
    )


def main() -> int:
    jurisdiction_paths, coverage_paths, source_plan_paths = validate_static_examples()
    smoke_test_example_coverage(jurisdiction_paths, coverage_paths, source_plan_paths)
    validate_generated_step5_artifacts()

    print(
        "Alice Step 5 validation passed: "
        f"{len(jurisdiction_paths)} jurisdiction examples, "
        f"{len(coverage_paths)} coverage examples, "
        f"{len(source_plan_paths)} source plan examples."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
