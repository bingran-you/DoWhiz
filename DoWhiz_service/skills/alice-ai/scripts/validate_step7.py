#!/usr/bin/env python3

"""Validate Alice Step 7 parcel candidates and refined research assembly artifacts."""

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
from alice_jurisdiction import build_jurisdiction_context, load_subject_resolution
from alice_parcel_candidates import (
    PARCEL_CANDIDATES_SCHEMA_PATH,
    build_parcel_candidates,
    summarize_parcel_candidates,
)
from alice_registry import load_json
from alice_source_plan import build_source_plan
from alice_subject_resolution import load_request, load_session_state


SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
EXAMPLES_ROOT = SKILL_ROOT / "examples"
FIXTURES_ROOT = SKILL_ROOT / "fixtures"
WORKSPACE_EXAMPLES_ROOT = EXAMPLES_ROOT / "workspaces"
CHUNK_SIZE = 200


SCENARIOS = [
    {
        "name": "strong_curated_hudspeth",
        "request_path": EXAMPLES_ROOT / "request.listing_url_inferred_curated.example.json",
        "subject_resolution_path": EXAMPLES_ROOT / "subject_resolution.listing_url_inferred_curated.example.json",
        "session_state_path": None,
        "fixture_manifest": FIXTURES_ROOT / "step7" / "strong_curated_hudspeth" / "manifest.json",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_strong_curated_hudspeth",
        "timestamp": "2026-04-17T01:05:00Z",
        "expected_successes": {
            "listing_platform_landwatch",
            "county_tx_hudspeth_cad_property_search",
            "county_tx_hudspeth_cad_public_information",
        },
        "expected_candidate_set_status": "single_strong_candidate",
        "expected_confirmation_level": "parcel_confirmed",
    },
    {
        "name": "wind_hudspeth",
        "request_path": EXAMPLES_ROOT / "request.listing_url_wind_hudspeth.example.json",
        "subject_resolution_path": EXAMPLES_ROOT / "subject_resolution.listing_url_inferred_curated.example.json",
        "session_state_path": None,
        "fixture_manifest": FIXTURES_ROOT / "step7" / "strong_curated_hudspeth" / "manifest.json",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_wind_hudspeth",
        "timestamp": "2026-04-17T01:07:00Z",
        "expected_successes": {
            "listing_platform_landwatch",
            "county_tx_hudspeth_cad_property_search",
            "county_tx_hudspeth_cad_public_information",
        },
        "expected_candidate_set_status": "single_strong_candidate",
        "expected_confirmation_level": "parcel_confirmed",
    },
    {
        "name": "fallback_weak_autauga",
        "request_path": EXAMPLES_ROOT / "request.listing_url_fallback.example.json",
        "subject_resolution_path": EXAMPLES_ROOT / "subject_resolution.listing_url_fallback.example.json",
        "session_state_path": None,
        "fixture_manifest": FIXTURES_ROOT / "step6" / "listing_url_fallback" / "manifest.json",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_fallback_weak_autauga",
        "timestamp": "2026-04-17T01:10:00Z",
        "expected_successes": {
            "listing_platform_land_com",
            "federal_fcc_bdc_national",
        },
        "expected_candidate_set_status": "single_weak_candidate",
        "expected_confirmation_level": "candidate_unconfirmed",
    },
    {
        "name": "competing_kern",
        "request_path": EXAMPLES_ROOT / "request.listing_url_competing_kern.example.json",
        "subject_resolution_path": EXAMPLES_ROOT / "subject_resolution.listing_url_competing_kern.example.json",
        "session_state_path": None,
        "fixture_manifest": FIXTURES_ROOT / "step7" / "competing_kern" / "manifest.json",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_competing_kern",
        "timestamp": "2026-04-17T01:15:00Z",
        "expected_successes": {
            "listing_platform_land_com",
            "county_ca_kern_parcelquest_property_search",
            "county_ca_kern_interactive_mapping",
            "local_ca_bakersfield_planning",
        },
        "expected_candidate_set_status": "multiple_competing_candidates",
        "expected_confirmation_level": "candidate_corroborated",
    },
    {
        "name": "follow_up_strengthened",
        "request_path": EXAMPLES_ROOT / "request.follow_up.example.json",
        "subject_resolution_path": EXAMPLES_ROOT / "subject_resolution.resolved_follow_up.example.json",
        "session_state_path": EXAMPLES_ROOT / "session_state.follow_up.example.json",
        "fixture_manifest": FIXTURES_ROOT / "step7" / "follow_up_strengthened" / "manifest.json",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_follow_up_strengthened",
        "timestamp": "2026-04-17T01:20:00Z",
        "expected_successes": {
            "county_tx_hudspeth_cad_property_search",
            "county_tx_hudspeth_cad_public_information",
            "federal_fema_nfhl",
            "federal_usfws_nwi",
            "federal_usgs_3dep_elevation",
            "federal_usda_soil_data_access",
        },
        "expected_candidate_set_status": "single_strong_candidate",
        "expected_confirmation_level": "parcel_confirmed",
    },
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write-examples",
        action="store_true",
        help="Write deterministic Step 7 example workspaces into examples/workspaces/.",
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


def _load_scenario_inputs(scenario: dict[str, object]) -> tuple[dict, dict, dict | None, dict, dict, dict]:
    request = load_request(scenario["request_path"])
    subject_resolution = load_subject_resolution(scenario["subject_resolution_path"])
    session_state = (
        load_session_state(scenario["session_state_path"])
        if scenario["session_state_path"] is not None
        else None
    )

    if scenario["name"] == "follow_up_strengthened":
        request = copy.deepcopy(request)
        request["request_id"] = subject_resolution["request_id"]
        request["subjects"][0]["subject_id"] = "active_subject_1"
        request["follow_up_context"]["carry_forward_subject_ids"] = ["active_subject_1"]

    jurisdiction_context = build_jurisdiction_context(request, subject_resolution, session_state)
    coverage_assessment = build_coverage_assessment(request, subject_resolution, jurisdiction_context)
    source_plan = build_source_plan(request, subject_resolution, jurisdiction_context, coverage_assessment)
    return request, subject_resolution, session_state, jurisdiction_context, coverage_assessment, source_plan


def _copy_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def _copy_inputs_to_workspace(
    workspace_dir: Path,
    request: dict,
    subject_resolution: dict,
    jurisdiction_context: dict,
    coverage_assessment: dict,
    source_plan: dict,
    session_state: dict | None,
) -> None:
    alice_root = workspace_dir / "alice"
    _copy_json(alice_root / "request_normalized.json", request)
    _copy_json(alice_root / "subject_resolution.json", subject_resolution)
    _copy_json(alice_root / "jurisdiction_context.json", jurisdiction_context)
    _copy_json(alice_root / "coverage_assessment.json", coverage_assessment)
    _copy_json(alice_root / "source_plan.json", source_plan)
    if session_state is not None:
        _copy_json(alice_root / "session_state.json", session_state)


def _validate_workspace_refs(workspace_dir: Path, fetch_log: dict) -> None:
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
        f"Generated artifact {actual_path} drifted from committed example {expected_path}. Re-run validate_step7.py --write-examples.",
    )


def _workspace_json_paths(workspace_dir: Path) -> tuple[Path, Path, Path, Path, Path]:
    alice_root = workspace_dir / "alice"
    return (
        alice_root / "source_fetch_log.json",
        alice_root / "extracted_evidence.json",
        alice_root / "parcel_candidates.json",
        alice_root / "parcel_memo.json",
        alice_root / "research_assembly_summary.json",
    )


def _run_pipeline_for_scenario(
    scenario: dict[str, object],
    *,
    workspace_dir: Path,
) -> tuple[dict, dict, dict, dict]:
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
    parcel_candidates = build_parcel_candidates(
        request,
        subject_resolution,
        jurisdiction_context,
        fetch_log,
        extracted,
        workspace_root=workspace_dir,
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
        parcel_candidates=parcel_candidates,
        generated_at=scenario["timestamp"],
    )
    return fetch_log, extracted, parcel_candidates, parcel_memo


def _check_expectations(
    scenario: dict[str, object],
    fetch_log: dict,
    parcel_candidates: dict,
    parcel_memo: dict,
) -> None:
    summary = summarize_fetch_log(fetch_log)
    successful_source_ids = set(summary["successful_source_ids"])
    assert_true(
        set(scenario["expected_successes"]) <= successful_source_ids,
        f"{scenario['name']} missing expected successes: {sorted(set(scenario['expected_successes']) - successful_source_ids)}",
    )

    candidate_summary = summarize_parcel_candidates(parcel_candidates)
    assert_true(
        candidate_summary["candidate_set_status"] == scenario["expected_candidate_set_status"],
        (
            f"{scenario['name']} expected candidate_set_status "
            f"{scenario['expected_candidate_set_status']} but saw {candidate_summary['candidate_set_status']}"
        ),
    )
    assert_true(
        candidate_summary["top_confirmation_level"] == scenario["expected_confirmation_level"],
        (
            f"{scenario['name']} expected top confirmation "
            f"{scenario['expected_confirmation_level']} but saw {candidate_summary['top_confirmation_level']}"
        ),
    )
    if candidate_summary["top_confirmation_level"] == "parcel_confirmed":
        assert_true(
            candidate_summary["top_confirmation_basis"] in {
                "text_corroborated",
                "local_record_corroborated",
                "geometry_confirmed",
            },
            f"{scenario['name']} must expose a non-unknown confirmation basis for parcel_confirmed candidates.",
        )
    assert_true(
        parcel_memo["parcel_identity"]["candidate_set_status"] == scenario["expected_candidate_set_status"],
        f"{scenario['name']} parcel memo candidate set status drifted from parcel_candidates.json",
    )
    assert_true(
        parcel_memo["parcel_identity"]["overall_confirmation_level"] == scenario["expected_confirmation_level"],
        f"{scenario['name']} parcel memo overall confirmation drifted from parcel_candidates.json",
    )
    assert_true(
        parcel_memo["parcel_identity"]["overall_confirmation_basis"] == candidate_summary["top_confirmation_basis"],
        f"{scenario['name']} parcel memo overall confirmation basis drifted from parcel_candidates.json",
    )
    assert_true(
        all(parcel["confirmation_basis"] != "geometry_confirmed" for parcel in parcel_memo["parcel_identity"]["parcels"]),
        f"{scenario['name']} should not overclaim geometry_confirmed before a geometry-confirmation step exists.",
    )

    if scenario["name"] == "strong_curated_hudspeth":
        primary = parcel_memo["parcel_identity"]["parcels"][0]
        assert_true(
            primary["apn"]["evidence_scope"] == "parcel_confirmed",
            "Strong curated Hudspeth case should mark APN as parcel_confirmed.",
        )
        assert_true(
            primary["confirmation_basis"] == "local_record_corroborated",
            "Strong curated Hudspeth case should stay local_record_corroborated rather than geometry_confirmed.",
        )
        assert_true(
            "county_tx_hudspeth_cad_property_search" in primary["supporting_source_ids"],
            "Strong curated Hudspeth case should include county parcel-search support.",
        )
    elif scenario["name"] == "wind_hudspeth":
        primary = parcel_memo["parcel_identity"]["parcels"][0]
        assert_true(
            primary["confirmation_basis"] == "local_record_corroborated",
            "Wind Hudspeth case should preserve local-record parcel confirmation semantics.",
        )
    elif scenario["name"] == "fallback_weak_autauga":
        assert_true(
            parcel_memo["parcel_identity"]["access_point_estimate"]["evidence_scope"] == "geography_only",
            "Fallback Autauga case should keep listing-derived coordinates as geography_only.",
        )
        assert_true(
            parcel_memo["parcel_identity"]["parcels"][0]["apn"]["status"] == "missing",
            "Fallback Autauga case should not invent an APN.",
        )
    elif scenario["name"] == "competing_kern":
        assert_true(
            parcel_candidates["candidate_count"] >= 2,
            "Competing Kern case should preserve at least two candidates.",
        )
        assert_true(
            len(parcel_memo["parcel_identity"]["identity_conflicts"]) >= 1,
            "Competing Kern case should preserve identity conflicts.",
        )
        assert_true(
            parcel_memo["planning_and_land_use"]["zoning_designation"]["evidence_scope"] == "parcel_candidate",
            "Competing Kern zoning should stay parcel_candidate-scoped.",
        )
    elif scenario["name"] == "follow_up_strengthened":
        assert_true(
            parcel_memo["subject"]["research_unit_type"] == "single_parcel",
            "Follow-up strengthening case should remain a single_parcel research unit.",
        )
        assert_true(
            parcel_memo["subject"]["parcel_count"] == 1,
            "Follow-up strengthening case should preserve parcel_count=1.",
        )
        assert_true(
            parcel_memo["environmental_constraints"]["flood_zone"]["evidence_scope"] == "geography_only",
            "Follow-up strengthening case should keep FEMA point screening geography_only.",
        )


def write_examples() -> None:
    for scenario in SCENARIOS:
        workspace_dir = scenario["workspace_dir"]
        if workspace_dir.exists():
            shutil.rmtree(workspace_dir)
        workspace_dir.mkdir(parents=True, exist_ok=True)
        _run_pipeline_for_scenario(scenario, workspace_dir=workspace_dir)

    strong_single_parcel = (
        WORKSPACE_EXAMPLES_ROOT
        / "step7_follow_up_strengthened"
        / "alice"
        / "parcel_memo.json"
    )
    shutil.copyfile(strong_single_parcel, EXAMPLES_ROOT / "land_research.single_parcel.example.json")


def validate_generated_and_committed_examples() -> None:
    with tempfile.TemporaryDirectory(prefix="alice-step7-validation-") as temp_dir:
        temp_root = Path(temp_dir)
        fetch_paths: list[Path] = []
        extract_paths: list[Path] = []
        candidate_paths: list[Path] = []
        memo_paths: list[Path] = []

        for scenario in SCENARIOS:
            generated_workspace = temp_root / scenario["name"]
            generated_workspace.mkdir(parents=True, exist_ok=True)
            fetch_log, _, parcel_candidates, parcel_memo = _run_pipeline_for_scenario(
                scenario,
                workspace_dir=generated_workspace,
            )
            _check_expectations(scenario, fetch_log, parcel_candidates, parcel_memo)
            _validate_workspace_refs(generated_workspace, fetch_log)

            fetch_path, extract_path, candidate_path, memo_path, summary_path = _workspace_json_paths(generated_workspace)
            fetch_paths.append(fetch_path)
            extract_paths.append(extract_path)
            candidate_paths.append(candidate_path)
            memo_paths.append(memo_path)

            committed_fetch, committed_extract, committed_candidates, committed_memo, committed_summary = _workspace_json_paths(
                scenario["workspace_dir"]
            )
            assert_true(
                all(path.exists() for path in [committed_fetch, committed_extract, committed_candidates, committed_memo, committed_summary]),
                (
                    f"Committed Step 7 example workspace for {scenario['name']} is missing. "
                    "Run validate_step7.py --write-examples first."
                ),
            )
            _compare_committed_json(committed_fetch, fetch_path)
            _compare_committed_json(committed_extract, extract_path)
            _compare_committed_json(committed_candidates, candidate_path)
            _compare_committed_json(committed_memo, memo_path)
            _compare_committed_json(committed_summary, summary_path)
            _validate_workspace_refs(scenario["workspace_dir"], load_json(committed_fetch))

        run_ajv(SOURCE_FETCH_LOG_SCHEMA_PATH, fetch_paths, "Step 7 source fetch logs")
        run_ajv(EXTRACTED_EVIDENCE_SCHEMA_PATH, extract_paths, "Step 7 extracted evidence artifacts")
        run_ajv(PARCEL_CANDIDATES_SCHEMA_PATH, candidate_paths, "Step 7 parcel candidate artifacts")
        run_ajv(LAND_RESEARCH_SCHEMA_PATH, memo_paths, "Step 7 land research artifacts")


def main() -> int:
    args = parse_args()
    if args.write_examples:
        write_examples()

    validate_generated_and_committed_examples()
    print(
        "Alice Step 7 validation passed: "
        f"{len(SCENARIOS)} scenario workspaces, "
        "schema-valid fetch logs, extracted evidence, parcel candidates, and parcel memos."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
