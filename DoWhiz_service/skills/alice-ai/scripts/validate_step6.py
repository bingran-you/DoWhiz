#!/usr/bin/env python3

"""Validate Alice Step 6 fetch, extraction, and research assembly artifacts."""

from __future__ import annotations

import argparse
import copy
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

from alice_assemble_research import LAND_RESEARCH_SCHEMA_PATH, assemble_land_research
from alice_coverage_assessment import build_coverage_assessment
from alice_extract import EXTRACTED_EVIDENCE_SCHEMA_PATH, extract_evidence
from alice_fetch import (
    SOURCE_FETCH_LOG_SCHEMA_PATH,
    FixtureTransport,
    execute_source_plan,
    summarize_fetch_log,
)
from alice_jurisdiction import build_jurisdiction_context
from alice_registry import load_json
from alice_source_plan import build_source_plan
from alice_subject_resolution import load_request, load_session_state


SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
EXAMPLES_ROOT = SKILL_ROOT / "examples"
FIXTURES_ROOT = SKILL_ROOT / "fixtures" / "step6"
WORKSPACE_EXAMPLES_ROOT = EXAMPLES_ROOT / "workspaces"
CHUNK_SIZE = 200


SCENARIOS = [
    {
        "name": "listing_url_inferred_curated",
        "request_path": EXAMPLES_ROOT / "request.listing_url_inferred_curated.example.json",
        "subject_resolution_path": EXAMPLES_ROOT / "subject_resolution.listing_url_inferred_curated.example.json",
        "jurisdiction_context_path": EXAMPLES_ROOT / "jurisdiction_context.listing_url_inferred_curated.example.json",
        "coverage_assessment_path": EXAMPLES_ROOT / "coverage_assessment.listing_url_inferred_curated.example.json",
        "source_plan_path": EXAMPLES_ROOT / "source_plan.listing_url_inferred_curated.example.json",
        "session_state_path": None,
        "fixture_manifest": FIXTURES_ROOT / "listing_url_inferred_curated" / "manifest.json",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step6_listing_url_inferred_curated",
        "timestamp": "2026-04-16T23:15:00Z",
        "expected": {
            "successes": {"listing_platform_landwatch", "county_tx_hudspeth_cad_property_search"},
            "failures": {"federal_epa_ejscreen"},
            "blocked_min": 4,
        },
    },
    {
        "name": "listing_url_fallback",
        "request_path": EXAMPLES_ROOT / "request.listing_url_fallback.example.json",
        "subject_resolution_path": EXAMPLES_ROOT / "subject_resolution.listing_url_fallback.example.json",
        "jurisdiction_context_path": EXAMPLES_ROOT / "jurisdiction_context.listing_url_fallback.example.json",
        "coverage_assessment_path": EXAMPLES_ROOT / "coverage_assessment.listing_url_fallback.example.json",
        "source_plan_path": EXAMPLES_ROOT / "source_plan.listing_url_fallback.example.json",
        "session_state_path": None,
        "fixture_manifest": FIXTURES_ROOT / "listing_url_fallback" / "manifest.json",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step6_listing_url_fallback",
        "timestamp": "2026-04-16T23:20:00Z",
        "expected": {
            "successes": {"listing_platform_land_com", "federal_fcc_bdc_national"},
            "failures": {"federal_epa_ejscreen"},
            "blocked_min": 8,
        },
    },
    {
        "name": "apn_unresolved",
        "request_path": EXAMPLES_ROOT / "request.apn_unresolved.example.json",
        "subject_resolution_path": EXAMPLES_ROOT / "subject_resolution.apn_unresolved.example.json",
        "jurisdiction_context_path": EXAMPLES_ROOT / "jurisdiction_context.apn_unresolved.example.json",
        "coverage_assessment_path": EXAMPLES_ROOT / "coverage_assessment.apn_unresolved.example.json",
        "source_plan_path": EXAMPLES_ROOT / "source_plan.apn_unresolved.example.json",
        "session_state_path": None,
        "fixture_manifest": FIXTURES_ROOT / "apn_unresolved" / "manifest.json",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step6_apn_unresolved",
        "timestamp": "2026-04-16T23:25:00Z",
        "expected": {
            "successes": set(),
            "failures": set(),
            "blocked_min": 8,
        },
    },
    {
        "name": "follow_up_resolved",
        "request_path": EXAMPLES_ROOT / "request.follow_up.example.json",
        "subject_resolution_path": EXAMPLES_ROOT / "subject_resolution.resolved_follow_up.example.json",
        "jurisdiction_context_path": None,
        "coverage_assessment_path": None,
        "source_plan_path": None,
        "session_state_path": EXAMPLES_ROOT / "session_state.follow_up.example.json",
        "fixture_manifest": FIXTURES_ROOT / "follow_up_resolved" / "manifest.json",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step6_follow_up_resolved",
        "timestamp": "2026-04-16T23:30:00Z",
        "expected": {
            "successes": {
                "federal_fema_nfhl",
                "federal_usfws_nwi",
                "federal_usgs_3dep_elevation",
                "federal_usda_soil_data_access",
            },
            "failures": {"federal_epa_ejscreen"},
            "blocked_min": 1,
        },
    },
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write-examples",
        action="store_true",
        help="Write deterministic Step 6 example workspaces into examples/workspaces/.",
    )
    return parser.parse_args()


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


def _load_scenario_inputs(scenario: dict[str, Any]) -> tuple[dict, dict, dict | None, dict, dict, dict]:
    request = load_request(scenario["request_path"])
    subject_resolution = load_json(scenario["subject_resolution_path"])
    session_state = (
        load_session_state(scenario["session_state_path"])
        if scenario["session_state_path"] is not None
        else None
    )

    if scenario["name"] == "follow_up_resolved":
        request = copy.deepcopy(request)
        request["request_id"] = subject_resolution["request_id"]
        request["subjects"][0]["subject_id"] = "active_subject_1"
        request["follow_up_context"]["carry_forward_subject_ids"] = ["active_subject_1"]
        jurisdiction_context = build_jurisdiction_context(request, subject_resolution, session_state)
        coverage_assessment = build_coverage_assessment(request, subject_resolution, jurisdiction_context)
        source_plan = build_source_plan(request, subject_resolution, jurisdiction_context, coverage_assessment)
    else:
        jurisdiction_context = load_json(scenario["jurisdiction_context_path"])
        coverage_assessment = load_json(scenario["coverage_assessment_path"])
        source_plan = load_json(scenario["source_plan_path"])
    return request, subject_resolution, session_state, jurisdiction_context, coverage_assessment, source_plan


def _copy_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def _copy_inputs_to_workspace(
    workspace_dir: Path,
    request: dict[str, Any],
    subject_resolution: dict[str, Any],
    jurisdiction_context: dict[str, Any],
    coverage_assessment: dict[str, Any],
    source_plan: dict[str, Any],
    session_state: dict[str, Any] | None,
) -> None:
    alice_root = workspace_dir / "alice"
    _copy_json(alice_root / "request_normalized.json", request)
    _copy_json(alice_root / "subject_resolution.json", subject_resolution)
    _copy_json(alice_root / "jurisdiction_context.json", jurisdiction_context)
    _copy_json(alice_root / "coverage_assessment.json", coverage_assessment)
    _copy_json(alice_root / "source_plan.json", source_plan)
    if session_state is not None:
        _copy_json(alice_root / "session_state.json", session_state)


def _validate_workspace_refs(workspace_dir: Path, fetch_log: dict[str, Any]) -> None:
    for entry in fetch_log["fetch_entries"]:
        for artifact in entry["artifact_refs"]:
            assert_true(
                (workspace_dir / artifact["path"]).exists(),
                f"Artifact ref {artifact['path']} is missing in {workspace_dir}",
            )


def _compare_committed_json(expected_path: Path, actual_path: Path) -> None:
    expected = json.loads(expected_path.read_text(encoding="utf-8"))
    actual = json.loads(actual_path.read_text(encoding="utf-8"))
    assert_true(
        expected == actual,
        f"Generated artifact {actual_path} drifted from committed example {expected_path}. Re-run validate_step6.py --write-examples.",
    )


def _run_pipeline_for_scenario(scenario: dict[str, Any], *, workspace_dir: Path) -> tuple[dict, dict, dict]:
    request, subject_resolution, session_state, jurisdiction_context, coverage_assessment, source_plan = _load_scenario_inputs(scenario)
    _copy_inputs_to_workspace(
        workspace_dir,
        request,
        subject_resolution,
        jurisdiction_context,
        coverage_assessment,
        source_plan,
        session_state,
    )
    fetch_log = execute_source_plan(
        request,
        subject_resolution,
        jurisdiction_context,
        source_plan,
        workspace_root=workspace_dir,
        transport=FixtureTransport(scenario["fixture_manifest"]),
        generated_at=scenario["timestamp"],
    )
    extracted = extract_evidence(
        fetch_log,
        workspace_root=workspace_dir,
        request_id=request["request_id"],
        subject_resolution_id=subject_resolution["resolution_id"],
        generated_at=scenario["timestamp"],
    )
    parcel_memo = assemble_land_research(
        request,
        subject_resolution,
        jurisdiction_context,
        coverage_assessment,
        source_plan,
        fetch_log,
        extracted,
        workspace_root=workspace_dir,
        generated_at=scenario["timestamp"],
    )
    return fetch_log, extracted, parcel_memo


def _check_expectations(scenario: dict[str, Any], fetch_log: dict[str, Any], parcel_memo: dict[str, Any]) -> None:
    summary = summarize_fetch_log(fetch_log)
    successful_source_ids = set(summary["successful_source_ids"])
    expected = scenario["expected"]
    assert_true(
        expected["successes"] <= successful_source_ids,
        f"{scenario['name']} missing expected successes: {sorted(expected['successes'] - successful_source_ids)}",
    )
    failed_source_ids = {
        entry["source_id"]
        for entry in fetch_log["fetch_entries"]
        if entry["retrieval_status"] == "failed" and entry["source_id"] is not None
    }
    assert_true(
        expected["failures"] <= failed_source_ids,
        f"{scenario['name']} missing expected failed sources: {sorted(expected['failures'] - failed_source_ids)}",
    )
    blocked_count = summary["counts"].get("blocked", 0)
    assert_true(
        blocked_count >= expected["blocked_min"],
        f"{scenario['name']} expected at least {expected['blocked_min']} blocked entries, found {blocked_count}",
    )
    assert_true(
        parcel_memo["schema_version"] == "alice.land_research.v1",
        f"{scenario['name']} did not produce a land research object.",
    )
    if scenario["name"] == "apn_unresolved":
        assert_true(
            parcel_memo["subject"]["resolution_status"] == "unresolved",
            "APN unresolved scenario should remain unresolved in parcel memo.",
        )
        assert_true(
            parcel_memo["jurisdiction"]["state"] is None,
            "APN unresolved scenario should keep jurisdiction state unresolved.",
        )


def _workspace_json_paths(workspace_dir: Path) -> tuple[Path, Path, Path]:
    alice_root = workspace_dir / "alice"
    return (
        alice_root / "source_fetch_log.json",
        alice_root / "extracted_evidence.json",
        alice_root / "parcel_memo.json",
    )


def write_examples() -> None:
    for scenario in SCENARIOS:
        workspace_dir = scenario["workspace_dir"]
        if workspace_dir.exists():
            shutil.rmtree(workspace_dir)
        workspace_dir.mkdir(parents=True, exist_ok=True)
        _run_pipeline_for_scenario(scenario, workspace_dir=workspace_dir)


def validate_generated_and_committed_examples() -> None:
    with tempfile.TemporaryDirectory(prefix="alice-step6-validation-") as temp_dir:
        temp_root = Path(temp_dir)
        fetch_paths: list[Path] = []
        extract_paths: list[Path] = []
        memo_paths: list[Path] = []
        for scenario in SCENARIOS:
            generated_workspace = temp_root / scenario["name"]
            generated_workspace.mkdir(parents=True, exist_ok=True)
            fetch_log, _, parcel_memo = _run_pipeline_for_scenario(
                scenario,
                workspace_dir=generated_workspace,
            )
            _check_expectations(scenario, fetch_log, parcel_memo)
            _validate_workspace_refs(generated_workspace, fetch_log)

            fetch_path, extract_path, memo_path = _workspace_json_paths(generated_workspace)
            fetch_paths.append(fetch_path)
            extract_paths.append(extract_path)
            memo_paths.append(memo_path)

            committed_fetch, committed_extract, committed_memo = _workspace_json_paths(
                scenario["workspace_dir"]
            )
            assert_true(
                committed_fetch.exists() and committed_extract.exists() and committed_memo.exists(),
                (
                    f"Committed Step 6 example workspace for {scenario['name']} is missing. "
                    "Run validate_step6.py --write-examples first."
                ),
            )
            _compare_committed_json(committed_fetch, fetch_path)
            _compare_committed_json(committed_extract, extract_path)
            _compare_committed_json(committed_memo, memo_path)
            _validate_workspace_refs(scenario["workspace_dir"], load_json(committed_fetch))

        run_ajv(SOURCE_FETCH_LOG_SCHEMA_PATH, fetch_paths, "Step 6 source fetch logs")
        run_ajv(EXTRACTED_EVIDENCE_SCHEMA_PATH, extract_paths, "Step 6 extracted evidence artifacts")
        run_ajv(LAND_RESEARCH_SCHEMA_PATH, memo_paths, "Step 6 land research artifacts")


def main() -> int:
    args = parse_args()
    if args.write_examples:
        write_examples()

    validate_generated_and_committed_examples()
    print(
        "Alice Step 6 validation passed: "
        f"{len(SCENARIOS)} scenario workspaces, "
        "schema-valid fetch logs, extracted evidence, and parcel memos."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
