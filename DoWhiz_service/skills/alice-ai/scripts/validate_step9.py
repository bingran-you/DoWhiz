#!/usr/bin/env python3

"""Validate Alice Step 9 use-case modules and directional economics."""

from __future__ import annotations

import argparse
from pathlib import Path

from alice_registry import load_json
from validate_step6 import write_examples as write_step6_examples
from validate_step7 import assert_true, write_examples as write_step7_examples
from validate_step8 import (
    validate_generated_and_committed_examples as validate_step8_chain,
    write_examples as write_step8_examples,
)


SCRIPT_DIR = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_DIR.parent
WORKSPACE_EXAMPLES_ROOT = SKILL_ROOT / "examples" / "workspaces"

SCENARIOS = [
    {
        "name": "energy_strong_hudspeth",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_strong_curated_hudspeth",
        "required_module_ids": {"energy_solar", "recreational_rural_hold"},
        "expected_fits": {
            "energy_solar": "mixed",
            "recreational_rural_hold": "favorable",
        },
        "economics_status": "available",
        "required_report_strings": [
            "## Use-Case Screening",
            "## Directional Economics",
            "Solar Energy Screening",
            "Recreational / Rural Hold Screening",
        ],
    },
    {
        "name": "wind_hudspeth",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_wind_hudspeth",
        "required_module_ids": {"energy_wind", "recreational_rural_hold"},
        "expected_fits": {
            "energy_wind": "weak",
            "recreational_rural_hold": "favorable",
        },
        "economics_status": "available",
        "required_report_strings": [
            "Wind Energy Screening",
            "No wind-resource screen, turbine setback review, or transmission-queue context has been added yet",
            "directional economics",
        ],
    },
    {
        "name": "development_uncertain_and_insufficient_autauga",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_fallback_weak_autauga",
        "required_module_ids": {"residential_light_development", "recreational_rural_hold"},
        "expected_fits": {
            "residential_light_development": "weak",
            "recreational_rural_hold": "mixed",
        },
        "economics_status": "limited",
        "required_report_strings": [
            "Residential / Light Development Screening",
            "Only federal baseline plus listing context is currently available",
            "directional only",
        ],
    },
    {
        "name": "competing_thesis_kern",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_competing_kern",
        "required_module_ids": {"energy_solar", "energy_battery", "industrial_storage"},
        "expected_fits": {
            "energy_solar": "mixed",
            "energy_battery": "weak",
            "industrial_storage": "weak",
        },
        "economics_status": "available",
        "required_report_strings": [
            "Battery Energy Storage Screening",
            "Industrial / Storage Screening",
            "Competing parcel candidates",
        ],
    },
    {
        "name": "agriculture_follow_up_hudspeth",
        "workspace_dir": WORKSPACE_EXAMPLES_ROOT / "step7_follow_up_strengthened",
        "required_module_ids": {"energy_solar", "agriculture_general", "recreational_rural_hold"},
        "expected_fits": {
            "energy_solar": "mixed",
            "agriculture_general": "mixed",
        },
        "economics_status": "limited",
        "required_report_strings": [
            "Agriculture Screening",
            "What groundwater, well, irrigation, or transferable water-right footing exists",
            "No active listing price is attached",
        ],
    },
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write-examples",
        action="store_true",
        help="Refresh deterministic Step 7 parcel memos and Step 8 report artifacts before validation.",
    )
    return parser.parse_args()


def write_examples() -> None:
    write_step6_examples()
    write_step7_examples()
    write_step8_examples()


def _module_index(parcel_memo: dict) -> dict[str, dict]:
    return {
        module["module_id"]: module
        for module in parcel_memo.get("use_case_modules", [])
    }


def validate_committed_examples() -> None:
    validate_step8_chain()

    for scenario in SCENARIOS:
        workspace_dir = scenario["workspace_dir"]
        alice_root = workspace_dir / "alice"
        parcel_memo = load_json(alice_root / "parcel_memo.json")
        report_summary = load_json(alice_root / "report_summary.json")
        report_text = (alice_root / "report.md").read_text(encoding="utf-8")
        slack_text = (alice_root / "report_slack.txt").read_text(encoding="utf-8")
        email_text = (alice_root / "report_email.txt").read_text(encoding="utf-8")
        module_index = _module_index(parcel_memo)

        required_module_ids = scenario["required_module_ids"]
        assert_true(
            required_module_ids <= set(module_index),
            f"{scenario['name']} is missing expected modules: {sorted(required_module_ids - set(module_index))}",
        )

        for module_id, expected_fit in scenario["expected_fits"].items():
            assert_true(
                module_index[module_id]["fit_assessment"] == expected_fit,
                (
                    f"{scenario['name']} expected {module_id} fit {expected_fit!r} "
                    f"but saw {module_index[module_id]['fit_assessment']!r}."
                ),
            )

        for module in parcel_memo["use_case_modules"]:
            assert_true(
                {"module_id", "module_family", "module_label", "module_status", "fit_assessment", "confidence", "evidence_scope_summary"} <= set(module),
                f"{scenario['name']} module {module['module_id']} is missing required Step 9 keys.",
            )
            scope_summary = module["evidence_scope_summary"]
            assert_true(
                {"dominant_scope", "supporting_scopes", "notes"} <= set(scope_summary),
                f"{scenario['name']} module {module['module_id']} has an incomplete evidence_scope_summary.",
            )

        economics = parcel_memo["directional_economics"]
        assert_true(
            economics["status"] == scenario["economics_status"],
            (
                f"{scenario['name']} expected directional economics status {scenario['economics_status']!r} "
                f"but saw {economics['status']!r}."
            ),
        )
        assert_true(
            set(economics["active_module_ids"]) == set(module_index),
            f"{scenario['name']} directional economics active_module_ids drifted from use_case_modules.",
        )
        assert_true(
            "## Use-Case Screening" in report_text and "## Directional Economics" in report_text,
            f"{scenario['name']} report is missing Step 9 memo sections.",
        )
        assert_true(
            any(finding["section"] == "use_case_modules" for finding in report_summary["top_findings"]),
            f"{scenario['name']} report summary should surface at least one module-aware top finding.",
        )
        assert_true(
            "Use-case screens:" in slack_text,
            f"{scenario['name']} Slack render should expose use-case screens.",
        )
        assert_true(
            "Use-case screens" in email_text,
            f"{scenario['name']} email render should expose use-case screens.",
        )

        for required_text in scenario["required_report_strings"]:
            assert_true(
                required_text in report_text or required_text in email_text or required_text in slack_text,
                f"{scenario['name']} output is missing required Step 9 text {required_text!r}.",
            )

        if scenario["name"] == "agriculture_follow_up_hudspeth":
            agriculture_module = module_index["agriculture_general"]
            assert_true(
                any("water" in unknown.lower() or "irrig" in unknown.lower() for unknown in agriculture_module["key_unknowns"]),
                "Agriculture follow-up case should preserve water uncertainty explicitly.",
            )
        elif scenario["name"] == "wind_hudspeth":
            wind_module = module_index["energy_wind"]
            assert_true(
                any("wind-resource" in unknown.lower() or "setback" in unknown.lower() for unknown in wind_module["key_unknowns"]),
                "Wind Hudspeth case should preserve wind-resource or setback uncertainty explicitly.",
            )
        elif scenario["name"] == "competing_thesis_kern":
            fit_set = {module_index[module_id]["fit_assessment"] for module_id in required_module_ids}
            assert_true(
                len(fit_set) >= 2,
                "Competing Kern case should produce differentiated module outcomes on the same parcel.",
            )
        elif scenario["name"] == "development_uncertain_and_insufficient_autauga":
            assert_true(
                parcel_memo["directional_economics"]["status"] != "available",
                "Fallback Autauga should not surface fully available directional economics.",
            )
            assert_true(
                any(
                    "does not produce final underwriting" in limitation.lower()
                    or "no final underwriting" in limitation.lower()
                    for limitation in parcel_memo["directional_economics"]["limitations"]
                ),
                "Fallback Autauga should explicitly disclaim underwriting-style economics.",
            )


def main() -> int:
    args = parse_args()
    if args.write_examples:
        write_examples()

    validate_committed_examples()
    print(
        "Alice Step 9 validation passed: "
        "Step 7/8 foundations still validate, "
        "use-case modules are populated, "
        "directional economics stays conservative, "
        "and rendered outputs expose thesis-aware screening."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
