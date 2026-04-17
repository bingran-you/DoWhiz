#!/usr/bin/env python3

"""Run the unified Alice Phase 1 evaluation harness and Step 11 packaging checks."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
import time
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable

from alice_recommendation import EXHAUSTIVE_LANGUAGE
from alice_registry import (
    build_fallback_county_entry,
    county_rows,
    coverage_tier_rubric,
    iter_county_override_paths,
    iter_source_paths,
    load_all_sources,
    load_json,
)
from validate_registry import ACCESS_MODE_INTERFACE_COMPATIBILITY
from validate_step5 import SCENARIOS as STEP5_SCENARIOS
from validate_step6 import SCENARIOS as STEP6_SCENARIOS
from validate_step7 import SCENARIOS as STEP7_SCENARIOS, run_ajv
from validate_step8 import SCENARIOS as STEP8_SCENARIOS
from validate_step9 import SCENARIOS as STEP9_SCENARIOS
from validate_step10 import SCENARIOS as STEP10_SCENARIOS


SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
EXAMPLES_ROOT = SKILL_ROOT / "examples"
WORKSPACES_ROOT = EXAMPLES_ROOT / "workspaces"
SCHEMAS_ROOT = SKILL_ROOT / "schemas"
EVALUATION_SCHEMA_PATH = SCHEMAS_ROOT / "evaluation_report.schema.json"
EVALUATION_EXAMPLE_PATH = EXAMPLES_ROOT / "evaluation" / "phase1_evaluation_report.json"
EXAMPLE_GENERATED_AT = "2026-04-17T03:45:00Z"
EVIDENCE_SCOPE_MARKERS = [
    "Parcel-confirmed:",
    "Parcel-candidate:",
    "Listing-derived:",
    "County-level:",
    "Geography-only:",
    "Inferred:",
    "Unresolved:",
]


VALIDATOR_BUCKETS = [
    {
        "bucket_id": "registry_coverage",
        "label": "Registry / Coverage",
        "script": "validate_registry.py",
        "check_types": ["contract", "stabilization", "regression"],
        "case_labels": ["nationwide_county_index", "curated_overrides", "source_descriptors"],
        "notes": [
            "Validates the nationwide county backbone, curated override integrity, fallback generation, source descriptor integrity, access_mode coverage, and coverage-tier rubric alignment.",
        ],
    },
    {
        "bucket_id": "subject_resolution",
        "label": "Subject Resolution",
        "script": "validate_subject_resolution.py",
        "check_types": ["contract", "heuristic", "regression"],
        "case_labels": [],  # filled dynamically
        "notes": [
            "Validates request examples, subject-resolution examples, session-state examples, listing URL normalization, APN normalization, batch mode, and follow-up state attachment.",
        ],
    },
    {
        "bucket_id": "planning_source_plan",
        "label": "Planning / Source Plan",
        "script": "validate_step5.py",
        "check_types": ["contract", "heuristic", "integration", "regression"],
        "case_labels": [scenario["name"] for scenario in STEP5_SCENARIOS],
        "notes": [
            "Validates jurisdiction context, coverage assessment, and source planning before live retrieval.",
        ],
    },
    {
        "bucket_id": "retrieval_assembly",
        "label": "Retrieval / Extraction / Assembly",
        "script": "validate_step6.py",
        "check_types": ["contract", "integration", "regression"],
        "case_labels": [scenario["name"] for scenario in STEP6_SCENARIOS],
        "notes": [
            "Validates deterministic fixture-backed retrieval, raw evidence capture, extraction, and universal research assembly.",
        ],
    },
    {
        "bucket_id": "parcel_candidates",
        "label": "Parcel-Candidate Strengthening",
        "script": "validate_step7.py",
        "check_types": ["contract", "integration", "truthfulness", "regression"],
        "case_labels": [scenario["name"] for scenario in STEP7_SCENARIOS],
        "notes": [
            "Validates parcel-candidate synthesis, confirmation levels, confirmation_basis, local pilot retrieval depth, and conflict preservation.",
        ],
    },
    {
        "bucket_id": "report_rendering",
        "label": "Report Rendering",
        "script": "validate_step8.py",
        "check_types": ["integration", "truthfulness", "regression"],
        "case_labels": [scenario["name"] for scenario in STEP8_SCENARIOS],
        "notes": [
            "Validates markdown, Slack, and email rendering with evidence-scope-aware phrasing, unknown visibility, and conflict surfacing.",
        ],
    },
    {
        "bucket_id": "use_case_modules",
        "label": "Use-Case Modules / Directional Economics",
        "script": "validate_step9.py",
        "check_types": ["heuristic", "truthfulness", "integration", "regression"],
        "case_labels": [scenario["name"] for scenario in STEP9_SCENARIOS],
        "notes": [
            "Validates screening-grade use-case modules, directional economics guardrails, and thesis-aware rendering.",
        ],
    },
    {
        "bucket_id": "recommendation_shortlist",
        "label": "Recommendation / Shortlist",
        "script": "validate_step10.py",
        "check_types": ["integration", "truthfulness", "regression"],
        "case_labels": [scenario["name"] for scenario in STEP10_SCENARIOS],
        "notes": [
            "Validates candidate-universe capture, shortlist ranking, blocker visibility, unknown visibility, and non-exhaustive-market language.",
        ],
    },
]

STABILIZATION_BUCKET = {
    "bucket_id": "stabilization_semantics",
    "label": "Stabilization Semantics",
    "validator": "step11_internal_checks",
    "check_types": ["stabilization", "truthfulness", "regression"],
    "case_labels": [
        "access_mode_semantics",
        "confirmation_basis_semantics",
        "coverage_tier_rubric",
        "wind_module_exercised",
    ],
    "notes": [
        "Runs direct Step 11 checks for access_mode semantics, confirmation_basis semantics, coverage-tier rubric consistency, weak-data truthfulness, and wind scenario coverage.",
    ],
}

KNOWN_LIMITATIONS = [
    "Phase 1 recommendation outputs rank only across the candidate universe Alice actually observed; they are not exhaustive market scans.",
    "County and local retrieval remain intentionally narrow, with the strongest pilot footing in Hudspeth County, TX and Kern County, CA plus Bakersfield local planning context.",
    "Parcel confirmation in the committed scenarios is text-corroborated or local-record-corroborated; geometry-confirmed parcel fabric is not yet exercised.",
    "Directional economics remain screening-grade and explicitly avoid final underwriting, precise ROI, queue feasibility, or entitlement certainty.",
    "Most Step 6-10 evaluation is fixture-backed and deterministic by design, which is useful for regression safety but is not a substitute for broader live-source calibration.",
    "Report delivery is markdown, Slack-style text, and email-style text only; Phase 1 does not include polished UI, PDF export, or workflow productization.",
]

RECOMMENDED_NEXT_WORK = [
    "Expand county and local retrieval coverage beyond the pilot counties while preserving official-source-first provenance contracts.",
    "Add stronger parcel-fabric and geometry confirmation pathways so parcel_confirmed can graduate beyond text and local-record corroboration where justified.",
    "Broaden observed listing acquisition and open-discovery breadth without losing candidate-universe limitation notes.",
    "Calibrate use-case modules, evidence-weighting, and directional economics against a larger benchmark set and reviewer feedback.",
    "Layer in deeper underwriting, richer exports, and operational workflow surfaces only after evidence coverage and evaluation breadth improve.",
]

REPORT_WARNINGS = [
    "Step 11 reuses the committed Alice validators and mostly fixture-backed scenario workspaces; this harness is designed for regression safety and packaging clarity, not for broad live-web benchmarking.",
    "Later-step validators chain earlier steps internally, so bucket timings include some repeated upstream work by design.",
    "The committed example evaluation report is normalized to keep timing and git metadata deterministic for review.",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Optional path for a live evaluation report JSON artifact.",
    )
    parser.add_argument(
        "--write-example",
        action="store_true",
        help="Refresh the committed normalized evaluation example under examples/evaluation/.",
    )
    parser.add_argument(
        "--generated-at",
        default=None,
        help="Override the generated_at timestamp used in the emitted report.",
    )
    return parser.parse_args()


def assert_true(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def iso_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def _git_output(args: list[str]) -> str | None:
    result = subprocess.run(
        ["git", *args],
        cwd=SKILL_ROOT.parent.parent.parent,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return None
    value = result.stdout.strip()
    return value or None


def git_metadata() -> dict[str, Any]:
    branch = _git_output(["branch", "--show-current"])
    commit = _git_output(["rev-parse", "HEAD"])
    dirty_output = _git_output(["status", "--porcelain"])
    return {
        "branch": branch,
        "commit": commit,
        "dirty": bool(dirty_output),
    }


def _compact_text(text: str | None, *, limit: int = 500) -> str | None:
    if not text:
        return None
    collapsed = " ".join(line.strip() for line in text.splitlines() if line.strip())
    if not collapsed:
        return None
    if len(collapsed) <= limit:
        return collapsed
    return collapsed[: limit - 3] + "..."


def _last_nonempty_line(text: str | None) -> str | None:
    if not text:
        return None
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    if not lines:
        return None
    return lines[-1]


def _scenario_name_from_example(path: Path, prefix: str) -> str:
    suffix = ".example.json"
    name = path.name
    if name.startswith(f"{prefix}.") and name.endswith(suffix):
        return name[len(prefix) + 1 : -len(suffix)]
    return path.stem


def scenario_counts() -> dict[str, Any]:
    request_examples = sorted(EXAMPLES_ROOT.glob("request*.example.json"))
    subject_resolution_examples = sorted(EXAMPLES_ROOT.glob("subject_resolution*.example.json"))
    session_state_examples = sorted(EXAMPLES_ROOT.glob("session_state*.example.json"))
    counties = county_rows()
    source_paths = iter_source_paths()
    override_paths = iter_county_override_paths()
    access_mode_counts = Counter(source["access_mode"] for source in load_all_sources())

    VALIDATOR_BUCKETS[1]["case_labels"] = [
        _scenario_name_from_example(path, "request") for path in request_examples
    ]

    return {
        "registry": {
            "case_count": len(counties),
            "case_labels": ["nationwide_county_index", "curated_overrides", "source_descriptors"],
            "measurements": {
                "county_count": len(counties),
                "curated_override_count": len(override_paths),
                "source_count": len(source_paths),
                "machine_endpoint_count": access_mode_counts.get("machine_endpoint", 0),
                "landing_page_count": access_mode_counts.get("landing_page", 0),
                "viewer_count": access_mode_counts.get("viewer", 0),
                "mixed_count": access_mode_counts.get("mixed", 0),
            },
            "notes": [
                "Counts the nationwide county backbone plus the curated override and source descriptor layers.",
            ],
        },
        "subject_resolution": {
            "case_count": len(request_examples),
            "case_labels": [_scenario_name_from_example(path, "request") for path in request_examples],
            "measurements": {
                "request_example_count": len(request_examples),
                "subject_resolution_example_count": len(subject_resolution_examples),
                "session_state_example_count": len(session_state_examples),
            },
            "notes": [
                "Tracks normalized requests, subject-resolution examples, and follow-up session-state examples.",
            ],
        },
        "planning": {
            "case_count": len(STEP5_SCENARIOS),
            "case_labels": [scenario["name"] for scenario in STEP5_SCENARIOS],
            "measurements": {"scenario_count": len(STEP5_SCENARIOS)},
            "notes": [
                "Step 5 planning scenarios cover curated, fallback, unresolved, coordinates, follow-up, and batch-compare paths.",
            ],
        },
        "retrieval_assembly": {
            "case_count": len(STEP6_SCENARIOS),
            "case_labels": [scenario["name"] for scenario in STEP6_SCENARIOS],
            "measurements": {"scenario_count": len(STEP6_SCENARIOS)},
            "notes": [
                "Step 6 scenario workspaces validate retrieval, raw evidence storage, fetch logging, and universal research assembly.",
            ],
        },
        "parcel_candidates": {
            "case_count": len(STEP7_SCENARIOS),
            "case_labels": [scenario["name"] for scenario in STEP7_SCENARIOS],
            "measurements": {"scenario_count": len(STEP7_SCENARIOS)},
            "notes": [
                "Step 7 scenario workspaces validate strong, weak, competing, follow-up, and wind parcel-candidate paths.",
            ],
        },
        "report_rendering": {
            "case_count": len(STEP8_SCENARIOS),
            "case_labels": [scenario["name"] for scenario in STEP8_SCENARIOS],
            "measurements": {"scenario_count": len(STEP8_SCENARIOS)},
            "notes": [
                "Step 8 scenario workspaces validate markdown, Slack, and email rendering across strong, weak, competing, follow-up, and wind cases.",
            ],
        },
        "use_case_modules": {
            "case_count": len(STEP9_SCENARIOS),
            "case_labels": [scenario["name"] for scenario in STEP9_SCENARIOS],
            "measurements": {"scenario_count": len(STEP9_SCENARIOS)},
            "notes": [
                "Step 9 scenario workspaces validate solar, wind, agriculture, development, industrial, and rural-hold screening with economics guardrails.",
            ],
        },
        "recommendation": {
            "case_count": len(STEP10_SCENARIOS),
            "case_labels": [scenario["name"] for scenario in STEP10_SCENARIOS],
            "measurements": {"scenario_count": len(STEP10_SCENARIOS)},
            "notes": [
                "Step 10 scenario workspaces validate user-supplied ranking, open discovery, conflicted shortlist, and thin candidate-universe paths.",
            ],
        },
        "stabilization": {
            "case_count": len(STABILIZATION_BUCKET["case_labels"]),
            "case_labels": list(STABILIZATION_BUCKET["case_labels"]),
            "measurements": {"stabilization_check_count": len(STABILIZATION_BUCKET["case_labels"])},
            "notes": [
                "Step 11 retains explicit access_mode, confirmation_basis, coverage-tier rubric, and wind exercise checks from the stabilization patch.",
            ],
        },
    }


def evaluation_scope() -> dict[str, Any]:
    return {
        "phase_label": "Alice Phase 1",
        "included_steps": [
            "step3_registry",
            "step4_subject_resolution",
            "step5_planning",
            "step6_retrieval_assembly",
            "step7_parcel_candidates",
            "step8_report_rendering",
            "step9_use_case_modules",
            "step10_recommendation",
            "pre_step11_stabilization_patch",
        ],
        "validator_scripts": [
            f"scripts/{bucket['script']}" for bucket in VALIDATOR_BUCKETS
        ] + ["scripts/validate_step11.py (internal stabilization and report aggregation)"],
        "notes": [
            "The unified runner executes the committed Alice validation chain rather than rebuilding separate check logic.",
            "Step 6 through Step 10 remain mostly deterministic and fixture-backed so the evaluation suite is stable enough for regression use.",
            "Step 11 adds explicit truthfulness summaries and packaging docs, but it does not expand the product surface.",
        ],
    }


def capability_summary() -> dict[str, Any]:
    return {
        "supported_input_modes": [
            "listing URL requests",
            "APN-driven requests",
            "address or coordinate-driven requests",
            "follow-up requests with inherited session-state context",
            "batch comparison and recommendation requests",
        ],
        "supported_output_artifacts": [
            "structured request, resolution, planning, fetch, extraction, parcel-candidate, and parcel_memo JSON artifacts",
            "markdown report memo plus Slack and email summary renders",
            "recommendation candidate-universe, shortlist, and shortlist render artifacts",
            "phase-1 evaluation report JSON and packaging docs",
        ],
        "supported_live_retrieval_paths": [
            "official-source-first federal baseline retrieval for flood, wetlands, soils, elevation, broadband, and environmental screening",
            "listing-context retrieval across the currently seeded platform surface, with committed scenario coverage centered on LandWatch and Land.com",
            "pilot county and state retrieval for Hudspeth County, TX, Kern County, CA, Bakersfield local planning, Texas PUC, and California transmission context",
        ],
        "pilot_local_paths": [
            "Hudspeth County, TX assessor and public-information surfaces",
            "Kern County, CA parcel and GIS surfaces",
            "Bakersfield, CA local planning context",
        ],
        "supported_module_families": [
            "energy_solar",
            "energy_wind",
            "energy_battery",
            "agriculture_general",
            "residential_light_development",
            "industrial_storage",
            "recreational_rural_hold",
        ],
        "recommendation_modes": [
            "ranking user-supplied candidate URLs",
            "open discovery across the observed listing universe",
            "shortlist refinement with thesis-aware ranking and blocker preservation",
        ],
        "notes": [
            "Recommendation quality is explicitly conditioned on the candidate universe Alice actually observed.",
            "Parcel confirmation remains conservative and basis-aware throughout the rendered outputs.",
            "Directional economics remain directional only and do not claim final underwriting precision.",
        ],
    }


def _run_validator_bucket(bucket: dict[str, Any]) -> dict[str, Any]:
    script_path = SCRIPT_DIR / bucket["script"]
    start = time.perf_counter()
    result = subprocess.run(
        [sys.executable, str(script_path)],
        cwd=SCRIPT_DIR,
        check=False,
        capture_output=True,
        text=True,
    )
    duration_ms = int((time.perf_counter() - start) * 1000)
    stdout_summary = _last_nonempty_line(result.stdout)
    failure_message = None if result.returncode == 0 else (_compact_text(result.stderr) or _compact_text(result.stdout))
    status = "passed" if result.returncode == 0 else "failed"
    summary = stdout_summary or (
        f"{bucket['label']} validation failed."
        if status == "failed"
        else f"{bucket['label']} validation passed."
    )
    return {
        "bucket_id": bucket["bucket_id"],
        "label": bucket["label"],
        "validator": f"scripts/{bucket['script']}",
        "check_types": bucket["check_types"],
        "case_count": len(bucket["case_labels"]),
        "case_labels": bucket["case_labels"],
        "status": status,
        "duration_ms": duration_ms,
        "summary": summary,
        "stdout_summary": stdout_summary,
        "failure_message": failure_message,
        "notes": bucket["notes"],
    }


def _check_runner(
    check_id: str,
    label: str,
    func: Callable[[], tuple[str, list[str]]],
) -> dict[str, Any]:
    try:
        summary, evidence = func()
        return {
            "check_id": check_id,
            "label": label,
            "status": "passed",
            "summary": summary,
            "evidence": evidence,
        }
    except AssertionError as exc:
        return {
            "check_id": check_id,
            "label": label,
            "status": "failed",
            "summary": str(exc),
            "evidence": [],
        }


def _step7_workspace(name: str) -> Path:
    return WORKSPACES_ROOT / f"step7_{name}" / "alice"


def _step10_workspace(name: str) -> Path:
    return WORKSPACES_ROOT / f"step10_{name}" / "alice" / "recommendation"


def check_access_mode_semantics() -> tuple[str, list[str]]:
    sources = load_all_sources()
    access_mode_counts = Counter(source["access_mode"] for source in sources)
    for source in sources:
        access_mode = source["access_mode"]
        assert_true(
            access_mode in ACCESS_MODE_INTERFACE_COMPATIBILITY,
            f"Source {source['source_id']} is missing or using an invalid access_mode.",
        )
        assert_true(
            source["interface_type"] in ACCESS_MODE_INTERFACE_COMPATIBILITY[access_mode],
            (
                f"Source {source['source_id']} uses interface_type={source['interface_type']!r} "
                f"with incompatible access_mode={access_mode!r}."
            ),
        )
    evidence = [
        f"{mode}: {count}"
        for mode, count in sorted(access_mode_counts.items())
    ]
    summary = (
        f"All {len(sources)} source descriptors expose valid access_mode semantics; "
        f"distribution is {', '.join(evidence)}."
    )
    return summary, evidence


def check_confirmation_basis_semantics() -> tuple[str, list[str]]:
    evidence: list[str] = []
    checked = 0
    for workspace_dir in sorted(WORKSPACES_ROOT.glob("step7_*")):
        alice_root = workspace_dir / "alice"
        parcel_candidates = load_json(alice_root / "parcel_candidates.json")
        parcel_memo = load_json(alice_root / "parcel_memo.json")
        report_summary = load_json(alice_root / "report_summary.json")
        overall_level = parcel_memo["parcel_identity"]["overall_confirmation_level"]
        overall_basis = parcel_memo["parcel_identity"].get("overall_confirmation_basis")
        summary_level = report_summary["subject_snapshot"]["overall_confirmation_level"]
        summary_basis = report_summary["subject_snapshot"].get("overall_confirmation_basis")
        assert_true(
            overall_basis is not None,
            f"{workspace_dir.name} parcel_memo.json is missing parcel_identity.overall_confirmation_basis.",
        )
        assert_true(
            summary_basis is not None,
            f"{workspace_dir.name} report_summary.json is missing subject_snapshot.overall_confirmation_basis.",
        )
        if overall_level == "parcel_confirmed":
            assert_true(
                overall_basis in {"text_corroborated", "local_record_corroborated", "geometry_confirmed"},
                f"{workspace_dir.name} parcel_confirmed memo is missing a concrete confirmation basis.",
            )
            assert_true(
                summary_basis == overall_basis,
                f"{workspace_dir.name} report_summary confirmation basis drifted from parcel_memo.",
            )
        for candidate in parcel_candidates["candidates"]:
            checked += 1
            level = candidate["confirmation_level"]
            basis = candidate.get("confirmation_basis")
            assert_true(
                basis is not None,
                f"{workspace_dir.name} candidate {candidate['candidate_id']} is missing confirmation_basis.",
            )
            if level == "parcel_confirmed":
                assert_true(
                    basis in {"text_corroborated", "local_record_corroborated", "geometry_confirmed"},
                    (
                        f"{workspace_dir.name} candidate {candidate['candidate_id']} is parcel_confirmed "
                        "without a concrete confirmation basis."
                    ),
                )
        evidence.append(
            f"{workspace_dir.name}: overall={overall_level}/{overall_basis}, rendered={summary_level}/{summary_basis}"
        )
    summary = (
        f"All Step 7 parcel-candidate and rendered research artifacts carry explicit confirmation_basis values "
        f"across {checked} candidate records."
    )
    return summary, evidence


def check_no_geometry_confirmed() -> tuple[str, list[str]]:
    scanned = 0
    for workspace_dir in sorted(WORKSPACES_ROOT.glob("step[67]*")):
        alice_root = workspace_dir / "alice"
        parcel_candidates_path = alice_root / "parcel_candidates.json"
        parcel_memo_path = alice_root / "parcel_memo.json"
        if parcel_candidates_path.exists():
            parcel_candidates = load_json(parcel_candidates_path)
            for candidate in parcel_candidates["candidates"]:
                scanned += 1
                assert_true(
                    candidate.get("confirmation_basis") != "geometry_confirmed",
                    (
                        f"{workspace_dir.name} candidate {candidate['candidate_id']} unexpectedly claims "
                        "geometry_confirmed."
                    ),
                )
        if parcel_memo_path.exists():
            parcel_memo = load_json(parcel_memo_path)
            scanned += 1
            assert_true(
                parcel_memo["parcel_identity"].get("overall_confirmation_basis") != "geometry_confirmed",
                f"{workspace_dir.name} parcel_memo.json unexpectedly claims geometry_confirmed.",
            )
        report_summary_path = alice_root / "report_summary.json"
        if report_summary_path.exists():
            report_summary = load_json(report_summary_path)
            scanned += 1
            assert_true(
                report_summary["subject_snapshot"].get("overall_confirmation_basis") != "geometry_confirmed",
                f"{workspace_dir.name} report_summary.json unexpectedly claims geometry_confirmed.",
            )
    summary = (
        "No committed Step 6-10 scenario artifacts claim geometry_confirmed parcel footing; "
        "current parcel confirmation remains text- or local-record-corroborated."
    )
    return summary, [f"artifact_checks_scanned: {scanned}"]


def check_coverage_tier_rubric() -> tuple[str, list[str]]:
    counties = county_rows()
    override_matches = 0
    fallback_matches = 0
    for path in iter_county_override_paths():
        entry = load_json(path)
        rubric = coverage_tier_rubric(entry["capabilities"])
        assert_true(
            entry["coverage_tier"] == rubric["inferred_tier"],
            f"{path.name} coverage_tier drifted from rubric inference {rubric['inferred_tier']!r}.",
        )
        override_matches += 1
    for county in counties.values():
        entry = build_fallback_county_entry(county)
        rubric = coverage_tier_rubric(entry["capabilities"])
        assert_true(
            entry["coverage_tier"] == rubric["inferred_tier"],
            f"Fallback county {county['county_fips']} drifted from rubric inference {rubric['inferred_tier']!r}.",
        )
        fallback_matches += 1
    summary = (
        f"Coverage-tier rubric remains mechanically consistent across {override_matches} curated overrides "
        f"and {fallback_matches} fallback county entries."
    )
    return summary, [
        f"curated_override_count: {override_matches}",
        f"fallback_county_count: {fallback_matches}",
    ]


def check_weak_data_unknown_visibility() -> tuple[str, list[str]]:
    alice_root = _step7_workspace("fallback_weak_autauga")
    report_text = (alice_root / "report.md").read_text(encoding="utf-8")
    report_summary = load_json(alice_root / "report_summary.json")
    assert_true("## Unknowns" in report_text, "Fallback weak report is missing the Unknowns section.")
    assert_true(
        "Only federal baseline plus listing context is currently available" in report_text,
        "Fallback weak report is missing the thin-coverage limitation callout.",
    )
    assert_true(
        "Unresolved:" in report_text,
        "Fallback weak report is missing unresolved evidence-scope phrasing.",
    )
    warning_types = {warning["warning_type"] for warning in report_summary["warnings"]}
    assert_true(
        "federal_baseline_only" in warning_types,
        "Fallback weak summary is missing the federal_baseline_only warning.",
    )
    assert_true(
        len(report_summary["top_unknowns"]) >= 1,
        "Fallback weak summary is missing top_unknowns entries.",
    )
    summary = (
        "Weak-data reporting keeps unknowns first-class and explicitly flags federal-baseline-only coverage in the fallback county scenario."
    )
    return summary, [
        "scenario: step7_fallback_weak_autauga",
        f"warning_types: {sorted(warning_types)}",
        f"top_unknown_count: {len(report_summary['top_unknowns'])}",
    ]


def check_competing_candidate_conflict_visibility() -> tuple[str, list[str]]:
    alice_root = _step7_workspace("competing_kern")
    report_text = (alice_root / "report.md").read_text(encoding="utf-8")
    report_summary = load_json(alice_root / "report_summary.json")
    assert_true(
        "Multiple parcel candidates remain active" in report_text,
        "Competing-candidate report is missing the active conflict callout.",
    )
    assert_true(
        "Parcel-candidate:" in report_text,
        "Competing-candidate report is missing parcel-candidate evidence-scope phrasing.",
    )
    assert_true("## Unknowns" in report_text, "Competing-candidate report is missing the Unknowns section.")
    warning_types = {warning["warning_type"] for warning in report_summary["warnings"]}
    assert_true(
        "competing_candidates" in warning_types,
        "Competing-candidate summary is missing the competing_candidates warning.",
    )
    summary = (
        "Competing-candidate outputs preserve parcel-identity conflict visibility instead of collapsing the candidate set into a false single-parcel answer."
    )
    return summary, [
        "scenario: step7_competing_kern",
        f"warning_types: {sorted(warning_types)}",
    ]


def check_scope_aware_report_language() -> tuple[str, list[str]]:
    checked_reports = 0
    for report_path in sorted(WORKSPACES_ROOT.glob("step7_*/alice/report.md")):
        checked_reports += 1
        report_text = report_path.read_text(encoding="utf-8")
        assert_true(
            any(marker in report_text for marker in EVIDENCE_SCOPE_MARKERS),
            f"{report_path} is missing evidence-scope markers.",
        )
    summary = (
        f"All {checked_reports} committed Step 8 memo outputs retain evidence-scope wording so weak, competing, geography-only, and listing-derived facts stay visibly qualified."
    )
    return summary, [f"report_count: {checked_reports}"]


def check_no_exhaustive_market_language() -> tuple[str, list[str]]:
    checked_outputs = 0
    for scenario in STEP10_SCENARIOS:
        recommendation_root = scenario["workspace_dir"] / "alice" / "recommendation"
        for path in [
            recommendation_root / "shortlist_report.md",
            recommendation_root / "shortlist_slack.txt",
            recommendation_root / "shortlist_email.txt",
        ]:
            checked_outputs += 1
            lowered = path.read_text(encoding="utf-8").lower()
            for forbidden in EXHAUSTIVE_LANGUAGE:
                assert_true(
                    forbidden not in lowered,
                    f"{path} used forbidden exhaustive-market language {forbidden!r}.",
                )
            assert_true(
                "observed candidate universe" in lowered or "observed universe" in lowered,
                f"{path} is missing observed-universe limitation language.",
            )
    summary = (
        f"All {checked_outputs} recommendation renders avoid exhaustive-market wording and keep observed-universe limitations visible."
    )
    return summary, [f"recommendation_output_count: {checked_outputs}"]


def check_weak_data_economics_guardrail() -> tuple[str, list[str]]:
    alice_root = _step7_workspace("fallback_weak_autauga")
    parcel_memo = load_json(alice_root / "parcel_memo.json")
    report_text = (alice_root / "report.md").read_text(encoding="utf-8").lower()
    economics = parcel_memo["directional_economics"]
    assert_true(
        economics["status"] in {"limited", "not_enough_data"},
        "Fallback weak case should not expose fully available directional economics.",
    )
    assert_true(
        any("underwriting" in limitation.lower() for limitation in economics["limitations"]),
        "Fallback weak case should explicitly disclaim underwriting-style economics.",
    )
    assert_true(
        "no final underwriting, irr, npv, or revenue model is produced" in report_text,
        "Fallback weak report should explicitly disclaim underwriting-style economics in the memo.",
    )
    for forbidden in [
        "projected irr",
        "target irr",
        "underwritten irr",
        "modeled npv",
        "discounted cash flow",
        "revenue forecast",
    ]:
        assert_true(
            forbidden not in report_text,
            f"Fallback weak report used fake-precise economics language {forbidden!r}.",
        )
    summary = (
        "Weak-data economics remain directional only: the fallback scenario stays limited, disclaims underwriting, and avoids fake-precise finance terms."
    )
    return summary, [
        "scenario: step7_fallback_weak_autauga",
        f"economics_status: {economics['status']}",
    ]


def check_wind_scenario_exercised() -> tuple[str, list[str]]:
    alice_root = _step7_workspace("wind_hudspeth")
    parcel_memo = load_json(alice_root / "parcel_memo.json")
    report_text = (alice_root / "report.md").read_text(encoding="utf-8")
    module_index = {
        module["module_id"]: module
        for module in parcel_memo["use_case_modules"]
    }
    assert_true("energy_wind" in module_index, "Wind scenario is missing the energy_wind module.")
    wind_module = module_index["energy_wind"]
    assert_true(
        wind_module["module_status"] == "evaluated",
        "Wind scenario should run the energy_wind module in evaluated status.",
    )
    assert_true(
        wind_module["fit_assessment"] in {"favorable", "mixed", "weak", "unknown"},
        "Wind scenario returned an invalid fit assessment.",
    )
    assert_true(
        parcel_memo["directional_economics"]["status"] in {"available", "limited", "not_enough_data"},
        "Wind scenario directional economics status is invalid.",
    )
    assert_true(
        "Wind Energy Screening" in report_text,
        "Wind scenario report is missing the wind section.",
    )
    summary = (
        "The committed Hudspeth wind scenario exercises energy_wind module execution, directional economics scaffolding, and memo rendering."
    )
    return summary, [
        f"fit_assessment: {wind_module['fit_assessment']}",
        f"module_status: {wind_module['module_status']}",
        f"directional_economics_status: {parcel_memo['directional_economics']['status']}",
    ]


def run_truthfulness_checks() -> dict[str, Any]:
    checks = [
        _check_runner("access_mode_coverage", "Source access-mode semantics", check_access_mode_semantics),
        _check_runner(
            "confirmation_basis_coverage",
            "Parcel confirmation-basis semantics",
            check_confirmation_basis_semantics,
        ),
        _check_runner(
            "no_unjustified_geometry_confirmation",
            "No unjustified geometry confirmation",
            check_no_geometry_confirmed,
        ),
        _check_runner(
            "coverage_tier_rubric_consistency",
            "Coverage-tier rubric consistency",
            check_coverage_tier_rubric,
        ),
        _check_runner(
            "weak_data_unknown_visibility",
            "Unknown visibility in weak-data outputs",
            check_weak_data_unknown_visibility,
        ),
        _check_runner(
            "competing_candidate_conflict_visibility",
            "Conflict visibility in competing-candidate outputs",
            check_competing_candidate_conflict_visibility,
        ),
        _check_runner(
            "scope_aware_report_language",
            "Scope-aware report phrasing",
            check_scope_aware_report_language,
        ),
        _check_runner(
            "no_exhaustive_market_language",
            "No exhaustive-market recommendation language",
            check_no_exhaustive_market_language,
        ),
        _check_runner(
            "weak_data_economics_guardrail",
            "Weak-data economics guardrails",
            check_weak_data_economics_guardrail,
        ),
        _check_runner("wind_module_exercised", "Wind module scenario exercise", check_wind_scenario_exercised),
    ]
    return {
        "total_checks": len(checks),
        "passed_checks": sum(1 for check in checks if check["status"] == "passed"),
        "failed_checks": sum(1 for check in checks if check["status"] == "failed"),
        "warning_checks": sum(1 for check in checks if check["status"] == "warning"),
        "checks": checks,
    }


def run_stabilization_bucket(truthfulness: dict[str, Any]) -> dict[str, Any]:
    start = time.perf_counter()
    relevant_ids = {
        "access_mode_coverage",
        "confirmation_basis_coverage",
        "coverage_tier_rubric_consistency",
        "wind_module_exercised",
        "no_unjustified_geometry_confirmation",
    }
    relevant_checks = [
        check
        for check in truthfulness["checks"]
        if check["check_id"] in relevant_ids
    ]
    failed_checks = [check["check_id"] for check in relevant_checks if check["status"] == "failed"]
    status = "failed" if failed_checks else "passed"
    duration_ms = int((time.perf_counter() - start) * 1000)
    summary = (
        "Stabilization semantics passed: access_mode, confirmation_basis, coverage-tier rubric, and wind module coverage remain explicit and evaluation-friendly."
        if status == "passed"
        else f"Stabilization semantics failed for: {', '.join(failed_checks)}."
    )
    return {
        "bucket_id": STABILIZATION_BUCKET["bucket_id"],
        "label": STABILIZATION_BUCKET["label"],
        "validator": STABILIZATION_BUCKET["validator"],
        "check_types": STABILIZATION_BUCKET["check_types"],
        "case_count": len(STABILIZATION_BUCKET["case_labels"]),
        "case_labels": STABILIZATION_BUCKET["case_labels"],
        "status": status,
        "duration_ms": duration_ms,
        "summary": summary,
        "stdout_summary": None,
        "failure_message": None if status == "passed" else summary,
        "notes": STABILIZATION_BUCKET["notes"],
    }


def pass_fail_summary(
    benchmark_buckets: list[dict[str, Any]],
    *,
    truthfulness_failed: bool,
) -> dict[str, Any]:
    failed_bucket_ids = [bucket["bucket_id"] for bucket in benchmark_buckets if bucket["status"] == "failed"]
    warning_bucket_ids = [bucket["bucket_id"] for bucket in benchmark_buckets if bucket["status"] == "warning"]
    overall_status = "failed" if (failed_bucket_ids or truthfulness_failed) else ("warning" if warning_bucket_ids else "passed")
    return {
        "overall_status": overall_status,
        "total_buckets": len(benchmark_buckets),
        "passed_buckets": sum(1 for bucket in benchmark_buckets if bucket["status"] == "passed"),
        "failed_buckets": len(failed_bucket_ids),
        "warning_buckets": len(warning_bucket_ids),
        "failed_bucket_ids": failed_bucket_ids,
        "warning_bucket_ids": warning_bucket_ids,
        "total_duration_ms": sum(bucket["duration_ms"] for bucket in benchmark_buckets),
    }


def build_evaluation_report(*, generated_at: str | None = None) -> dict[str, Any]:
    counts = scenario_counts()
    benchmark_buckets = [_run_validator_bucket(bucket) for bucket in VALIDATOR_BUCKETS]
    truthfulness = run_truthfulness_checks()
    benchmark_buckets.append(run_stabilization_bucket(truthfulness))
    return {
        "schema_version": "alice.phase1_evaluation_report.v1",
        "generated_at": generated_at or iso_now(),
        "git": git_metadata(),
        "evaluation_scope": evaluation_scope(),
        "scenario_counts": counts,
        "benchmark_buckets": benchmark_buckets,
        "pass_fail_summary": pass_fail_summary(
            benchmark_buckets,
            truthfulness_failed=truthfulness["failed_checks"] > 0,
        ),
        "warnings": list(REPORT_WARNINGS),
        "capability_summary": capability_summary(),
        "truthfulness_checks": truthfulness,
        "known_limitations": list(KNOWN_LIMITATIONS),
        "recommended_next_work": list(RECOMMENDED_NEXT_WORK),
    }


def normalize_report_for_example(report: dict[str, Any]) -> dict[str, Any]:
    normalized = json.loads(json.dumps(report))
    normalized["generated_at"] = EXAMPLE_GENERATED_AT
    normalized["git"] = {
        "branch": "example-branch",
        "commit": "example-commit",
        "dirty": False,
    }
    normalized["pass_fail_summary"]["total_duration_ms"] = 0
    for bucket in normalized["benchmark_buckets"]:
        bucket["duration_ms"] = 0
    return normalized


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def validate_report_schema(path: Path) -> None:
    run_ajv(EVALUATION_SCHEMA_PATH, [path], "Step 11 evaluation report")


def compare_normalized_example(report: dict[str, Any]) -> None:
    assert_true(
        EVALUATION_EXAMPLE_PATH.exists(),
        (
            f"Committed evaluation example is missing: {EVALUATION_EXAMPLE_PATH}. "
            "Run validate_step11.py --write-example first."
        ),
    )
    expected = load_json(EVALUATION_EXAMPLE_PATH)
    actual = normalize_report_for_example(report)
    assert_true(
        expected == actual,
        (
            "Generated Step 11 evaluation example drifted from the committed artifact. "
            "Re-run validate_step11.py --write-example."
        ),
    )


def print_human_summary(report: dict[str, Any], output_path: Path | None) -> None:
    summary = report["pass_fail_summary"]
    print(
        "Alice Step 11 phase-1 evaluation "
        f"{summary['overall_status']}: "
        f"{summary['passed_buckets']}/{summary['total_buckets']} benchmark buckets passed, "
        f"{report['truthfulness_checks']['passed_checks']}/{report['truthfulness_checks']['total_checks']} truthfulness checks passed."
    )
    print("Benchmark buckets:")
    for bucket in report["benchmark_buckets"]:
        print(
            f"- {bucket['status'].upper():7s} {bucket['bucket_id']}: {bucket['summary']}"
        )
        if bucket["failure_message"]:
            print(f"  failure: {bucket['failure_message']}")
    print("Truthfulness checks:")
    for check in report["truthfulness_checks"]["checks"]:
        print(f"- {check['status'].upper():7s} {check['check_id']}: {check['summary']}")
    if output_path is not None:
        print(f"Report written to {output_path}")


def main() -> int:
    args = parse_args()
    report = build_evaluation_report(generated_at=args.generated_at)

    with tempfile.TemporaryDirectory(prefix="alice-step11-report-") as temp_dir:
        temp_report_path = Path(temp_dir) / "phase1_evaluation_report.json"
        write_json(temp_report_path, report)
        validate_report_schema(temp_report_path)

    if args.output is not None:
        write_json(args.output, report)
        validate_report_schema(args.output)

    if args.write_example:
        normalized = normalize_report_for_example(report)
        write_json(EVALUATION_EXAMPLE_PATH, normalized)
        validate_report_schema(EVALUATION_EXAMPLE_PATH)
    else:
        assert_true(
            EVALUATION_EXAMPLE_PATH.exists(),
            (
                f"Committed evaluation example is missing: {EVALUATION_EXAMPLE_PATH}. "
                "Run validate_step11.py --write-example first."
            ),
        )
        validate_report_schema(EVALUATION_EXAMPLE_PATH)
        compare_normalized_example(report)

    print_human_summary(report, args.output)
    overall_status = report["pass_fail_summary"]["overall_status"]
    if overall_status == "failed" or report["truthfulness_checks"]["failed_checks"] > 0:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
