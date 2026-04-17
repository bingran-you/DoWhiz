#!/usr/bin/env python3

"""Validate Alice Step 8 report-rendering artifacts."""

from __future__ import annotations

import argparse
import shutil
import tempfile
from pathlib import Path

from alice_registry import load_json
from alice_render_report import REPORT_SUMMARY_SCHEMA_PATH, render_workspace_report
from validate_step7 import assert_true, run_ajv, validate_generated_and_committed_examples as validate_step7_chain


SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
EXAMPLES_ROOT = SKILL_ROOT / "examples"
WORKSPACE_EXAMPLES_ROOT = EXAMPLES_ROOT / "workspaces"

SCENARIOS = [
    {
        "name": "strong_curated_hudspeth",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_strong_curated_hudspeth",
        "expected_title_contains": "Hudspeth",
        "required_report_strings": [
            "Parcel-confirmed:",
            "Listing-derived:",
            "County-level:",
            "## Unknowns",
        ],
    },
    {
        "name": "wind_hudspeth",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_wind_hudspeth",
        "expected_title_contains": "Hudspeth",
        "required_report_strings": [
            "Wind Energy Screening",
            "Confirmation basis:",
            "## Unknowns",
        ],
    },
    {
        "name": "fallback_weak_autauga",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_fallback_weak_autauga",
        "expected_title_contains": "Autauga",
        "required_report_strings": [
            "Only federal baseline plus listing context is currently available",
            "Unresolved:",
            "## Unknowns",
            "## Recommended Next Actions",
        ],
    },
    {
        "name": "competing_kern",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_competing_kern",
        "expected_title_contains": "Competing parcel candidates",
        "required_report_strings": [
            "Multiple parcel candidates remain active",
            "123-450-18-00-1",
            "123-450-17-00-5",
            "Parcel-candidate:",
        ],
    },
    {
        "name": "follow_up_strengthened",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_follow_up_strengthened",
        "expected_title_contains": "Hudspeth",
        "required_report_strings": [
            "follow-up",
            "Geography-only:",
            "Parcel-confirmed:",
            "## Sources and Citations",
        ],
    },
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write-examples",
        action="store_true",
        help="Write deterministic Step 8 rendered artifacts into the committed scenario workspaces.",
    )
    return parser.parse_args()


def _render_workspace(workspace_dir: Path) -> None:
    parcel_memo_path = workspace_dir / "alice" / "parcel_memo.json"
    parcel_memo = load_json(parcel_memo_path)
    render_workspace_report(
        workspace_dir,
        render_mode="generic_summary",
        generated_at=parcel_memo["created_at"],
        write_outputs=True,
    )


def _assert_text_equal(expected_path: Path, actual_path: Path) -> None:
    expected = expected_path.read_text(encoding="utf-8")
    actual = actual_path.read_text(encoding="utf-8")
    assert_true(
        expected == actual,
        (
            f"Generated text artifact {actual_path} drifted from committed example {expected_path}. "
            "Re-run validate_step8.py --write-examples."
        ),
    )


def _scenario_artifact_paths(workspace_dir: Path) -> tuple[Path, Path, Path, Path]:
    alice_root = workspace_dir / "alice"
    return (
        alice_root / "report_summary.json",
        alice_root / "report.md",
        alice_root / "report_slack.txt",
        alice_root / "report_email.txt",
    )


def write_examples() -> None:
    for scenario in SCENARIOS:
        _render_workspace(scenario["workspace_dir"])


def validate_generated_and_committed_examples() -> None:
    validate_step7_chain()

    summary_paths: list[Path] = []
    with tempfile.TemporaryDirectory(prefix="alice-step8-validation-") as temp_dir:
        temp_root = Path(temp_dir)
        for scenario in SCENARIOS:
            committed_workspace = scenario["workspace_dir"]
            assert_true(
                committed_workspace.exists(),
                f"Committed Step 7 workspace missing for {scenario['name']}: {committed_workspace}",
            )
            generated_workspace = temp_root / scenario["name"]
            shutil.copytree(committed_workspace, generated_workspace)
            _render_workspace(generated_workspace)

            generated_summary, generated_report, generated_slack, generated_email = _scenario_artifact_paths(generated_workspace)
            summary_paths.append(generated_summary)
            committed_summary, committed_report, committed_slack, committed_email = _scenario_artifact_paths(committed_workspace)

            assert_true(
                all(
                    path.exists()
                    for path in [committed_summary, committed_report, committed_slack, committed_email]
                ),
                (
                    f"Committed Step 8 artifacts missing for {scenario['name']}. "
                    "Run validate_step8.py --write-examples first."
                ),
            )

            expected_summary = load_json(committed_summary)
            actual_summary = load_json(generated_summary)
            assert_true(
                expected_summary == actual_summary,
                (
                    f"Generated report summary drifted for {scenario['name']}. "
                    "Re-run validate_step8.py --write-examples."
                ),
            )
            _assert_text_equal(committed_report, generated_report)
            _assert_text_equal(committed_slack, generated_slack)
            _assert_text_equal(committed_email, generated_email)

            report_text = generated_report.read_text(encoding="utf-8")
            slack_text = generated_slack.read_text(encoding="utf-8")
            email_text = generated_email.read_text(encoding="utf-8")
            summary = actual_summary

            assert_true(
                scenario["expected_title_contains"] in summary["title"],
                f"{scenario['name']} title missing expected text {scenario['expected_title_contains']!r}.",
            )
            assert_true(
                "report.md" in slack_text and "report.md" in email_text,
                f"{scenario['name']} Slack/email renders must point back to alice/report.md.",
            )
            if actual_summary["subject_snapshot"]["overall_confirmation_level"] == "parcel_confirmed":
                assert_true(
                    actual_summary["subject_snapshot"]["overall_confirmation_basis"] in {
                        "text_corroborated",
                        "local_record_corroborated",
                        "geometry_confirmed",
                    },
                    f"{scenario['name']} should expose a non-unknown overall_confirmation_basis in report_summary.json.",
                )
            assert_true(
                any(
                    scope in report_text
                    for scope in [
                        "Parcel-confirmed:",
                        "Parcel-candidate:",
                        "Listing-derived:",
                        "County-level:",
                        "Geography-only:",
                        "Inferred:",
                        "Unresolved:",
                    ]
                ),
                f"{scenario['name']} report is missing evidence-scope-aware wording.",
            )
            for required in scenario["required_report_strings"]:
                assert_true(
                    required in report_text or required in summary["one_line_conclusion"],
                    f"{scenario['name']} output missing required string {required!r}.",
                )

            if scenario["name"] == "competing_kern":
                warning_types = {warning["warning_type"] for warning in summary["warnings"]}
                assert_true(
                    "competing_candidates" in warning_types,
                    "Competing Kern summary should surface a competing_candidates warning.",
                )
            elif scenario["name"] == "fallback_weak_autauga":
                warning_types = {warning["warning_type"] for warning in summary["warnings"]}
                assert_true(
                    "federal_baseline_only" in warning_types,
                    "Fallback Autauga summary should surface federal_baseline_only.",
                )
                assert_true(
                    "## Unknowns" in report_text,
                    "Fallback Autauga memo should surface unknowns as a first-class section.",
                )
            elif scenario["name"] == "follow_up_strengthened":
                warning_types = {warning["warning_type"] for warning in summary["warnings"]}
                assert_true(
                    "follow_up_inherited_context" in warning_types,
                    "Follow-up summary should note inherited follow-up context.",
                )
            elif scenario["name"] == "wind_hudspeth":
                assert_true(
                    "Wind Energy Screening" in report_text,
                    "Wind Hudspeth memo should render the wind module section.",
                )

        run_ajv(REPORT_SUMMARY_SCHEMA_PATH, summary_paths, "Step 8 report summaries")


def main() -> int:
    args = parse_args()
    if args.write_examples:
        write_examples()

    validate_generated_and_committed_examples()
    print(
        "Alice Step 8 validation passed: "
        f"{len(SCENARIOS)} scenario workspaces, "
        "schema-valid report summaries, and deterministic markdown/Slack/email outputs."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
