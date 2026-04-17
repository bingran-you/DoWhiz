#!/usr/bin/env python3

"""Validate Alice Step 10 recommendation candidates, shortlist artifacts, and rendering."""

from __future__ import annotations

import argparse
import copy
import json
import shutil
import tempfile
from pathlib import Path

from alice_recommendation import (
    EXHAUSTIVE_LANGUAGE,
    RECOMMENDATION_CANDIDATES_SCHEMA_PATH,
    SHORTLIST_SCHEMA_PATH,
    render_recommendation_workspace,
)
from alice_subject_resolution import (
    REQUEST_SCHEMA_PATH,
    SUBJECT_RESOLUTION_SCHEMA_PATH,
    build_subject_resolution,
)
from validate_step7 import assert_true, run_ajv
from validate_step9 import validate_committed_examples as validate_step9_chain


SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
WORKSPACE_EXAMPLES_ROOT = SKILL_ROOT / "examples" / "workspaces"


SCENARIOS = [
    {
        "name": "user_supplied_ranking",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step10_user_supplied_ranking",
        "timestamp": "2026-04-17T02:00:00Z",
        "request": {
            "schema_version": "alice.request.v1",
            "request_id": "11111111-1111-4111-8111-111111111111",
            "thread_id": "slack:C0ALICE:step10-user-supplied",
            "channel": "slack",
            "request_mode": "recommendation",
            "raw_user_message": "Rank these listings for solar or long-term rural hold under $700k, and keep stronger parcel certainty ahead of weaker candidates.",
            "thesis": {
                "summary": "Rank supplied listings for solar or long-term rural hold under $700k, with parcel certainty and evidence quality carrying real weight.",
                "use_case_hypotheses": [
                    "energy",
                    "recreational_rural_hold",
                ],
                "target_geographies": [],
                "filters": {
                    "max_price": 700000,
                    "prefer_transmission_proximity": True,
                    "desired_coverage_tiers": [
                        "full",
                        "partial",
                        "minimal",
                    ],
                },
                "budget": {
                    "currency": "USD",
                    "max_purchase_price": 700000,
                },
                "hold_period": {
                    "min_years": 5,
                    "max_years": 15,
                    "target_strategy": "hold",
                },
                "return_preferences": {
                    "preferred_exit_strategies": [
                        "hold",
                        "ground_lease",
                    ],
                },
            },
            "subjects": [
                {
                    "input_kind": "listing_url",
                    "raw_value": "https://www.landwatch.com/texas-land-for-sale/hudspeth-county/property/80-acres-near-sierra-blanca/id/12345678?utm_source=test",
                },
                {
                    "input_kind": "listing_url",
                    "raw_value": "https://www.land.com/property/160-acres-near-Bakersfield-Kern-County-California/60606060/",
                },
                {
                    "input_kind": "listing_url",
                    "raw_value": "https://www.land.com/property/25-acres-in-Autauga-County-Alabama/55555555/",
                },
                {
                    "input_kind": "listing_url",
                    "raw_value": "https://www.landwatch.com/texas-land-for-sale/hudspeth-county/property/80-acres-near-sierra-blanca/id/12345678",
                },
            ],
            "output_preferences": {
                "verbosity": "standard",
                "comparison_requested": True,
                "include_markdown_report": True,
                "include_json_artifacts": True,
            },
            "conversation_state_ref": "alice/session_state.json",
        },
        "expected_candidate_count": 3,
        "expected_top_pick_contains": "Hudspeth",
        "expected_warning_types": {
            "non_exhaustive_market_coverage",
            "duplicate_inputs_collapsed",
        },
        "required_strings": [
            "duplicate",
            "observed candidate universe",
            "not exhaustive market coverage",
        ],
    },
    {
        "name": "open_discovery_rural_hold",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step10_open_discovery_rural_hold",
        "timestamp": "2026-04-17T02:05:00Z",
        "request": {
            "schema_version": "alice.request.v1",
            "request_id": "22222222-2222-4222-8222-222222222222",
            "thread_id": "email:step10-open-discovery",
            "channel": "email",
            "request_mode": "recommendation",
            "raw_user_message": "Find currently observed acreage listings under $300k that look more promising for rural hold than for heavier development.",
            "thesis": {
                "summary": "Open discovery across the observed candidate universe for rural hold under $300k.",
                "use_case_hypotheses": [
                    "recreational_rural_hold",
                ],
                "target_geographies": [],
                "filters": {
                    "max_price": 300000,
                },
                "budget": {
                    "currency": "USD",
                    "max_purchase_price": 300000,
                },
                "hold_period": {
                    "min_years": 5,
                    "max_years": 15,
                    "target_strategy": "hold",
                },
                "return_preferences": {
                    "preferred_exit_strategies": [
                        "hold",
                    ],
                },
            },
            "subjects": [],
            "output_preferences": {
                "verbosity": "standard",
                "comparison_requested": False,
                "include_markdown_report": True,
                "include_json_artifacts": True,
            },
            "conversation_state_ref": "alice/session_state.json",
        },
        "expected_candidate_count": 3,
        "expected_top_pick_contains": "Hudspeth",
        "expected_warning_types": {
            "non_exhaustive_market_coverage",
            "hard_filtered_candidates_present",
        },
        "required_strings": [
            "Filtered or Deprioritized Candidates",
            "budget ceiling",
            "observed candidate universe",
        ],
    },
    {
        "name": "conflicted_shortlist",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step10_conflicted_shortlist",
        "timestamp": "2026-04-17T02:10:00Z",
        "request": {
            "schema_version": "alice.request.v1",
            "request_id": "44444444-4444-4444-8444-444444444444",
            "thread_id": "slack:step10-conflicted",
            "channel": "slack",
            "request_mode": "recommendation",
            "raw_user_message": "Rank these observed candidates for solar first, but keep rural hold as a fallback and do not hide parcel-identity conflicts.",
            "thesis": {
                "summary": "Compare observed acreage listings for solar first, with rural hold as a fallback, while preserving parcel-identity tradeoffs.",
                "use_case_hypotheses": [
                    "energy",
                    "recreational_rural_hold",
                ],
                "target_geographies": [],
                "filters": {
                    "prefer_transmission_proximity": True,
                    "max_price": 700000,
                },
                "budget": {
                    "currency": "USD",
                    "max_purchase_price": 700000,
                },
                "hold_period": {
                    "min_years": 4,
                    "max_years": 15,
                    "target_strategy": "mixed",
                },
                "return_preferences": {
                    "preferred_exit_strategies": [
                        "ground_lease",
                        "hold",
                    ],
                },
            },
            "subjects": [
                {
                    "input_kind": "listing_url",
                    "raw_value": "https://www.land.com/property/160-acres-near-Bakersfield-Kern-County-California/60606060/",
                },
                {
                    "input_kind": "listing_url",
                    "raw_value": "https://www.landwatch.com/texas-land-for-sale/hudspeth-county/property/80-acres-near-sierra-blanca/id/12345678",
                },
                {
                    "input_kind": "listing_url",
                    "raw_value": "https://www.land.com/property/25-acres-in-Autauga-County-Alabama/55555555/",
                },
            ],
            "output_preferences": {
                "verbosity": "standard",
                "comparison_requested": True,
                "include_markdown_report": True,
                "include_json_artifacts": True,
            },
            "conversation_state_ref": "alice/session_state.json",
        },
        "expected_candidate_count": 3,
        "expected_top_pick_contains": "Hudspeth",
        "expected_warning_types": {
            "non_exhaustive_market_coverage",
            "mixed_evidence_rankings",
        },
        "required_strings": [
            "parcel-specific ranking confidence",
            "tradeoffs",
            "observed universe",
        ],
    },
    {
        "name": "weak_candidate_universe",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step10_weak_candidate_universe",
        "timestamp": "2026-04-17T02:15:00Z",
        "request": {
            "schema_version": "alice.request.v1",
            "request_id": "33333333-3333-4333-8333-333333333333",
            "thread_id": "email:step10-weak",
            "channel": "email",
            "request_mode": "recommendation",
            "raw_user_message": "Find me West Texas acreage under $260k that still has some solar or hold optionality, but stay conservative if the observed universe is thin.",
            "thesis": {
                "summary": "Thin-universe West Texas screen for solar or hold under $260k.",
                "use_case_hypotheses": [
                    "energy",
                    "recreational_rural_hold",
                ],
                "target_geographies": [
                    {
                        "kind": "region",
                        "label": "West Texas",
                        "state_code": "TX",
                    },
                ],
                "filters": {
                    "max_price": 260000,
                    "prefer_transmission_proximity": True,
                },
                "budget": {
                    "currency": "USD",
                    "max_purchase_price": 260000,
                },
                "hold_period": {
                    "min_years": 5,
                    "max_years": 15,
                    "target_strategy": "hold",
                },
                "return_preferences": {
                    "preferred_exit_strategies": [
                        "hold",
                        "ground_lease",
                    ],
                },
            },
            "subjects": [],
            "output_preferences": {
                "verbosity": "standard",
                "comparison_requested": False,
                "include_markdown_report": True,
                "include_json_artifacts": True,
            },
            "conversation_state_ref": "alice/session_state.json",
        },
        "expected_candidate_count": 1,
        "expected_top_pick_contains": "Hudspeth",
        "expected_warning_types": {
            "non_exhaustive_market_coverage",
            "thin_candidate_universe",
        },
        "required_strings": [
            "thin",
            "1 candidates",
            "observed universe",
        ],
    },
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write-examples",
        action="store_true",
        help="Write deterministic Step 10 recommendation workspaces into examples/workspaces/.",
    )
    return parser.parse_args()


def _write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def _stable_subject_resolution(request: dict[str, object], timestamp: str) -> dict:
    resolution = build_subject_resolution(request)
    resolution["generated_at"] = timestamp
    return resolution


def _render_scenario(scenario: dict[str, object], workspace_dir: Path) -> None:
    request = copy.deepcopy(scenario["request"])
    subject_resolution = _stable_subject_resolution(request, scenario["timestamp"])
    render_recommendation_workspace(
        request,
        subject_resolution=subject_resolution,
        workspace_root=workspace_dir,
        generated_at=scenario["timestamp"],
        write_outputs=True,
    )


def _text_paths(workspace_dir: Path) -> tuple[Path, Path, Path]:
    recommendation_root = workspace_dir / "alice" / "recommendation"
    return (
        recommendation_root / "shortlist_report.md",
        recommendation_root / "shortlist_slack.txt",
        recommendation_root / "shortlist_email.txt",
    )


def _json_paths(workspace_dir: Path) -> tuple[Path, Path, Path, Path]:
    alice_root = workspace_dir / "alice"
    recommendation_root = alice_root / "recommendation"
    return (
        alice_root / "request_normalized.json",
        alice_root / "subject_resolution.json",
        recommendation_root / "candidate_universe.json",
        recommendation_root / "shortlist.json",
    )


def _assert_text_equal(expected_path: Path, actual_path: Path) -> None:
    expected = expected_path.read_text(encoding="utf-8")
    actual = actual_path.read_text(encoding="utf-8")
    assert_true(
        expected == actual,
        (
            f"Generated text artifact {actual_path} drifted from committed example {expected_path}. "
            "Re-run validate_step10.py --write-examples."
        ),
    )


def write_examples() -> None:
    for scenario in SCENARIOS:
        workspace_dir = scenario["workspace_dir"]
        if workspace_dir.exists():
            shutil.rmtree(workspace_dir)
        workspace_dir.mkdir(parents=True, exist_ok=True)
        _render_scenario(scenario, workspace_dir)


def validate_generated_and_committed_examples() -> None:
    validate_step9_chain()

    request_paths: list[Path] = []
    resolution_paths: list[Path] = []
    universe_paths: list[Path] = []
    shortlist_paths: list[Path] = []

    with tempfile.TemporaryDirectory(prefix="alice-step10-validation-") as temp_dir:
        temp_root = Path(temp_dir)
        for scenario in SCENARIOS:
            committed_workspace = scenario["workspace_dir"]
            assert_true(
                committed_workspace.exists(),
                f"Committed Step 10 workspace missing for {scenario['name']}: {committed_workspace}",
            )
            generated_workspace = temp_root / scenario["name"]
            generated_workspace.mkdir(parents=True, exist_ok=True)
            _render_scenario(scenario, generated_workspace)

            committed_json_paths = _json_paths(committed_workspace)
            generated_json_paths = _json_paths(generated_workspace)
            for expected_path, actual_path in zip(committed_json_paths, generated_json_paths):
                assert_true(
                    expected_path.exists(),
                    f"Committed artifact missing for {scenario['name']}: {expected_path}",
                )
                expected = json.loads(expected_path.read_text(encoding="utf-8"))
                actual = json.loads(actual_path.read_text(encoding="utf-8"))
                assert_true(
                    expected == actual,
                    (
                        f"Generated JSON artifact {actual_path} drifted from committed example {expected_path}. "
                        "Re-run validate_step10.py --write-examples."
                    ),
                )

            committed_text_paths = _text_paths(committed_workspace)
            generated_text_paths = _text_paths(generated_workspace)
            for expected_path, actual_path in zip(committed_text_paths, generated_text_paths):
                assert_true(
                    expected_path.exists(),
                    f"Committed text artifact missing for {scenario['name']}: {expected_path}",
                )
                _assert_text_equal(expected_path, actual_path)

            request_path, resolution_path, universe_path, shortlist_path = generated_json_paths
            request_paths.append(request_path)
            resolution_paths.append(resolution_path)
            universe_paths.append(universe_path)
            shortlist_paths.append(shortlist_path)

            candidate_universe = json.loads(universe_path.read_text(encoding="utf-8"))
            shortlist = json.loads(shortlist_path.read_text(encoding="utf-8"))
            report_text = generated_text_paths[0].read_text(encoding="utf-8")
            slack_text = generated_text_paths[1].read_text(encoding="utf-8")
            email_text = generated_text_paths[2].read_text(encoding="utf-8")

            assert_true(
                candidate_universe["candidate_count"] == scenario["expected_candidate_count"],
                (
                    f"{scenario['name']} expected candidate_count {scenario['expected_candidate_count']} "
                    f"but saw {candidate_universe['candidate_count']}."
                ),
            )
            top_pick = shortlist["shortlist_summary"]["top_pick_label"] or ""
            assert_true(
                scenario["expected_top_pick_contains"] in top_pick,
                f"{scenario['name']} top pick label drifted: {top_pick!r}",
            )
            warning_types = {warning["warning_type"] for warning in shortlist["warnings"]}
            assert_true(
                scenario["expected_warning_types"] <= warning_types,
                (
                    f"{scenario['name']} is missing warning types "
                    f"{sorted(scenario['expected_warning_types'] - warning_types)}"
                ),
            )
            assert_true(
                shortlist["candidate_universe_limits"]["exhaustive_market_scan"] is False,
                f"{scenario['name']} should explicitly deny exhaustive market coverage.",
            )
            assert_true(
                "shortlist_report.md" in slack_text and "shortlist_report.md" in email_text,
                f"{scenario['name']} Slack/email renders must point back to alice/recommendation/shortlist_report.md.",
            )

            lowered_outputs = "\n".join([report_text, slack_text, email_text]).lower()
            for forbidden in EXHAUSTIVE_LANGUAGE:
                assert_true(
                    forbidden not in lowered_outputs,
                    f"{scenario['name']} output used forbidden exhaustive-market language: {forbidden!r}",
                )

            for required_string in scenario["required_strings"]:
                assert_true(
                    required_string.lower() in lowered_outputs,
                    f"{scenario['name']} output is missing required text {required_string!r}.",
                )

            if scenario["name"] == "user_supplied_ranking":
                assert_true(
                    candidate_universe["acquisition_summary"]["duplicate_input_count"] == 1,
                    "User-supplied ranking scenario should collapse one duplicate input.",
                )
            elif scenario["name"] == "open_discovery_rural_hold":
                filtered_out = shortlist["filtered_out_items"]
                assert_true(
                    any("budget ceiling" in item["filter_reason"] for item in filtered_out),
                    "Open discovery scenario should explicitly filter a candidate by budget ceiling.",
                )
            elif scenario["name"] == "conflicted_shortlist":
                assert_true(
                    len(shortlist["ranked_items"]) >= 2,
                    "Conflicted shortlist scenario should rank at least two candidates.",
                )
                second_item = shortlist["ranked_items"][1]
                assert_true(
                    any("parcel identity" in blocker.lower() or "competing" in blocker.lower() for blocker in second_item["major_blockers"]),
                    "Conflicted shortlist scenario should preserve parcel-identity tradeoffs on the runner-up candidate.",
                )
            elif scenario["name"] == "weak_candidate_universe":
                assert_true(
                    shortlist["candidate_universe_limits"]["observed_candidate_count"] == 1,
                    "Weak universe scenario should stay explicitly thin.",
                )

        run_ajv(REQUEST_SCHEMA_PATH, request_paths, "Step 10 request examples")
        run_ajv(SUBJECT_RESOLUTION_SCHEMA_PATH, resolution_paths, "Step 10 subject-resolution examples")
        run_ajv(RECOMMENDATION_CANDIDATES_SCHEMA_PATH, universe_paths, "Step 10 candidate-universe artifacts")
        run_ajv(SHORTLIST_SCHEMA_PATH, shortlist_paths, "Step 10 shortlist artifacts")


def main() -> int:
    args = parse_args()
    if args.write_examples:
        write_examples()

    validate_generated_and_committed_examples()
    print(
        "Alice Step 10 validation passed: "
        "recommendation candidate universes, ranked shortlists, and recommendation renders stay deterministic, "
        "preserve blockers/unknowns, and avoid exhaustive-market language."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
