#!/usr/bin/env python3
"""Run final-artifact regressions for the U.S. equity daily monitor skill."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

SERVICE_ROOT = Path(__file__).resolve().parents[3]
SKILL_ROOT = Path(__file__).resolve().parents[1]
EVALS_ROOT = SKILL_ROOT / "evals"
WORKSPACE_ROOT = SKILL_ROOT.parent.parent / "us-equity-daily-monitor-workspace"

sys.path.insert(0, str(SKILL_ROOT / "scripts"))

from check_action_distribution import audit_distribution  # noqa: E402
from check_final_artifact import audit_final_artifact  # noqa: E402


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--iteration", default="iteration-final-artifact")
    parser.add_argument("--eval-ids", nargs="*", type=int)
    parser.add_argument("--include-live", action="store_true")
    return parser.parse_args()


def load_evals() -> list[dict[str, Any]]:
    return json.loads((EVALS_ROOT / "evals.json").read_text())["evals"]


def run_live_eval(case: dict[str, Any], eval_dir: Path) -> dict[str, Any]:
    workspace_dir = eval_dir / "workspace"
    workspace_dir.mkdir(parents=True, exist_ok=True)
    cmd = [
        "cargo",
        "run",
        "-q",
        "-p",
        "scheduler_module",
        "--bin",
        "investment_eval",
        "--",
        "--workspace-dir",
        str(workspace_dir),
        "--subject",
        case["subject"],
        "--prompt",
        case["prompt"],
        "--employee",
        "little_bear",
    ]
    env = os.environ.copy()
    env.setdefault("CARGO_INCREMENTAL", "0")
    env.setdefault("CARGO_BUILD_JOBS", "1")
    env.setdefault("RUSTFLAGS", "-C debuginfo=0")
    completed = subprocess.run(
        cmd,
        cwd=SERVICE_ROOT,
        capture_output=True,
        text=True,
        check=False,
        env=env,
    )
    (eval_dir / "stdout.log").write_text(completed.stdout)
    (eval_dir / "stderr.log").write_text(completed.stderr)
    if completed.returncode != 0:
        raise RuntimeError(
            f"investment_eval failed for {case['name']} with code {completed.returncode}\n{completed.stderr}"
        )
    result = json.loads(completed.stdout.strip().splitlines()[-1])
    final_rendered_path = Path(result["final_rendered_path"])
    return {
        "artifact_html": final_rendered_path.read_text(),
        "final_rendered_path": str(final_rendered_path),
        "reply_draft_path": result.get("reply_draft_path"),
    }


def run_fixture_eval(case: dict[str, Any], eval_dir: Path) -> dict[str, Any]:
    if case.get("artifact_file"):
        artifact_html = (EVALS_ROOT / case["artifact_file"]).read_text()
    else:
        artifact_html = case["artifact_html"]
    rendered_path = eval_dir / "final_rendered_email.html"
    rendered_path.write_text(artifact_html)
    return {
        "artifact_html": artifact_html,
        "final_rendered_path": str(rendered_path),
        "reply_draft_path": None,
    }


def grade_case(case: dict[str, Any], run_result: dict[str, Any]) -> dict[str, Any]:
    audit = audit_final_artifact(
        run_result["artifact_html"], request_text=case.get("prompt")
    )
    checks = list(audit["checks"])

    if case.get("expected_monitor_status"):
        actual = audit["decisions"].get("monitor status")
        expected = case["expected_monitor_status"]
        checks.append(
            {
                "name": "expected_monitor_status",
                "passed": actual == expected,
                "detail": f"actual={actual} expected={expected}",
            }
        )

    if case.get("expected_contract_type"):
        actual = audit["contract_type"]
        expected = case["expected_contract_type"]
        checks.append(
            {
                "name": "expected_contract_type",
                "passed": actual == expected,
                "detail": f"actual={actual} expected={expected}",
            }
        )

    passed = all(check["passed"] for check in checks)
    if case.get("expected_failure"):
        passed = not audit["passed"]
        checks = [
            {
                "name": "artifact_rejected",
                "passed": passed,
                "detail": "generic or under-specified artifact failed as expected"
                if passed
                else "artifact unexpectedly passed the stronger final-artifact checks",
            }
        ]

    return {
        "passed": passed,
        "checks": checks,
        "artifact_path": run_result["final_rendered_path"],
        "reply_draft_path": run_result["reply_draft_path"],
        "audit": audit,
    }


def main() -> None:
    args = parse_args()
    cases = load_evals()
    selected = [case for case in cases if not args.eval_ids or case["id"] in args.eval_ids]
    iteration_dir = WORKSPACE_ROOT / args.iteration
    iteration_dir.mkdir(parents=True, exist_ok=True)

    summary = []
    distribution_items = []

    for case in selected:
        if case["kind"] == "live_run" and not args.include_live:
            continue

        eval_dir = iteration_dir / f"eval-{case['id']}-{case['name']}"
        eval_dir.mkdir(parents=True, exist_ok=True)
        run_result = run_live_eval(case, eval_dir) if case["kind"] == "live_run" else run_fixture_eval(case, eval_dir)
        grading = grade_case(case, run_result)
        (eval_dir / "final_rendered_email.html").write_text(run_result["artifact_html"])
        (eval_dir / "grading.json").write_text(json.dumps(grading, indent=2))

        if grading["passed"] and case.get("count_toward_distribution"):
            distribution_items.append(grading["audit"]["decisions"])

        summary.append(
            {
                "id": case["id"],
                "name": case["name"],
                "passed": grading["passed"],
                "checks": grading["checks"],
                "artifact_path": grading["artifact_path"],
                "reply_draft_path": grading["reply_draft_path"],
            }
        )

    distribution_payload = {"items": distribution_items}
    distribution_path = iteration_dir / "action_distribution.json"
    distribution_path.write_text(json.dumps(distribution_payload, indent=2))
    distribution = audit_distribution(distribution_items) if distribution_items else {"passed": False, "checks": []}
    (iteration_dir / "distribution_grading.json").write_text(json.dumps(distribution, indent=2))

    summary_path = iteration_dir / "summary.json"
    summary_path.write_text(
        json.dumps(
            {
                "results": summary,
                "summary_checks": [
                    {
                        "name": "action_distribution",
                        "passed": distribution["passed"],
                        "detail": distribution["checks"],
                    }
                ],
            },
            indent=2,
        )
    )
    print(summary_path)


if __name__ == "__main__":
    main()
