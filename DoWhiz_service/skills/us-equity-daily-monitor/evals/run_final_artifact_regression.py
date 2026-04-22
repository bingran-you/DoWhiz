#!/usr/bin/env python3
"""
Run final-artifact investment regressions through the real DoWhiz email reply path.

This runner does not grade an intermediate model transcript. It invokes the
`investment_eval` helper, which:
1. creates a DoWhiz-style workspace
2. copies runtime skills from DoWhiz_service/skills
3. runs `run_task`
4. writes `reply_email_draft.html`
5. renders the final user-visible email artifact to `final_rendered_email.html`

The grader then checks the final rendered email artifact.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
from pathlib import Path
from typing import Any


SERVICE_ROOT = Path(__file__).resolve().parents[3]
SKILL_ROOT = Path(__file__).resolve().parents[1]
WORKSPACE_ROOT = SKILL_ROOT.parent.parent / "us-equity-daily-monitor-workspace"
REQUIRED_LABELS = [
    "rating:",
    "horizon:",
    "confidence:",
    "timing verdict:",
    "verified facts",
    "derived metrics",
    "bull case",
    "base case",
    "bear case",
    "add criteria:",
    "invalidation criteria:",
    "biggest near-term risk:",
    "biggest long-term strength:",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--iteration", default="iteration-final-artifact")
    parser.add_argument("--eval-ids", nargs="*", type=int)
    return parser.parse_args()


def load_evals() -> list[dict[str, Any]]:
    payload = json.loads((SKILL_ROOT / "evals" / "evals.json").read_text())
    return payload["evals"]


def normalize_text(raw_html: str) -> str:
    text = re.sub(r"<[^>]+>", " ", raw_html)
    text = (
        text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
    )
    return " ".join(text.split()).lower()


def extract_field(text: str, label: str) -> str | None:
    pattern = re.compile(rf"{re.escape(label)}\s*:\s*([^<\n\r]+)", re.IGNORECASE)
    match = pattern.search(text)
    if not match:
        return None
    return match.group(1).strip().lower()


def grade_contract(text: str) -> list[str]:
    return [label for label in REQUIRED_LABELS if label not in text]


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
            f"investment_eval failed for {case['name']} with code {completed.returncode}\n"
            f"{completed.stderr}"
        )

    result = json.loads(completed.stdout.strip().splitlines()[-1])
    final_rendered_path = Path(result["final_rendered_path"])
    reply_draft_path = Path(result["reply_draft_path"])
    artifact_html = final_rendered_path.read_text()

    return {
        "artifact_html": artifact_html,
        "final_rendered_path": str(final_rendered_path),
        "reply_draft_path": str(reply_draft_path),
        "recovery_note": result.get("recovery_note"),
    }


def run_fixture_eval(case: dict[str, Any], eval_dir: Path) -> dict[str, Any]:
    artifact_html = case["artifact_html"]
    rendered_path = eval_dir / "final_rendered_email.html"
    rendered_path.write_text(artifact_html)
    return {
        "artifact_html": artifact_html,
        "final_rendered_path": str(rendered_path),
        "reply_draft_path": None,
        "recovery_note": None,
    }


def grade_case(case: dict[str, Any], run_result: dict[str, Any]) -> dict[str, Any]:
    artifact_html = run_result["artifact_html"]
    text = normalize_text(artifact_html)
    checks: list[dict[str, Any]] = []

    missing_contract = grade_contract(text)
    if case.get("expected_failure"):
        checks.append(
            {
                "name": "generic_commentary_fixture_rejected",
                "passed": bool(missing_contract),
                "detail": "grader rejected fixture" if missing_contract else "fixture unexpectedly passed",
            }
        )
    elif case.get("require_contract"):
        checks.append(
            {
                "name": "required_contract_labels_present",
                "passed": not missing_contract,
                "detail": "ok"
                if not missing_contract
                else f"missing: {', '.join(missing_contract)}",
            }
        )

    if case.get("expected_horizon_contains"):
        horizon = extract_field(text, "horizon") or ""
        checks.append(
            {
                "name": "horizon_awareness",
                "passed": case["expected_horizon_contains"] in horizon,
                "detail": f"horizon={horizon or '(missing)'}",
            }
        )

    if case.get("expected_question_type_contains"):
        checks.append(
            {
                "name": "question_type_alignment",
                "passed": case["expected_question_type_contains"] in text,
                "detail": "ok"
                if case["expected_question_type_contains"] in text
                else "question type missing from final artifact",
            }
        )

    if case.get("require_live_path_alignment"):
        checks.append(
            {
                "name": "live_path_alignment",
                "passed": str(run_result["reply_draft_path"]).endswith("reply_email_draft.html")
                and str(run_result["final_rendered_path"]).endswith("final_rendered_email.html"),
                "detail": f"reply={run_result['reply_draft_path']} final={run_result['final_rendered_path']}",
            }
        )

    if case.get("must_contain_any"):
        passed = any(s.lower() in artifact_html.lower() for s in case["must_contain_any"])
        checks.append(
            {
                "name": "required_correction_or_phrase_present",
                "passed": passed,
                "detail": "ok" if passed else f"missing any of {case['must_contain_any']}",
            }
        )

    if case.get("allowed_confidence"):
        confidence = extract_field(text, "confidence") or ""
        allowed = [value.lower() for value in case["allowed_confidence"]]
        checks.append(
            {
                "name": "confidence_degrades_for_weaker_case",
                "passed": any(value in confidence for value in allowed),
                "detail": f"confidence={confidence or '(missing)'}",
            }
        )

    return {
        "checks": checks,
        "passed": all(check["passed"] for check in checks),
        "artifact_path": run_result["final_rendered_path"],
        "reply_draft_path": run_result["reply_draft_path"],
        "recovery_note": run_result.get("recovery_note"),
    }


def main() -> None:
    args = parse_args()
    evals = load_evals()
    selected = [case for case in evals if not args.eval_ids or case["id"] in args.eval_ids]
    iteration_dir = WORKSPACE_ROOT / args.iteration
    iteration_dir.mkdir(parents=True, exist_ok=True)

    summary = []
    for case in selected:
        eval_dir = iteration_dir / f"eval-{case['id']}-{case['name']}"
        eval_dir.mkdir(parents=True, exist_ok=True)
        if case["kind"] == "live_run":
            run_result = run_live_eval(case, eval_dir)
        else:
            run_result = run_fixture_eval(case, eval_dir)

        grading = grade_case(case, run_result)
        (eval_dir / "final_rendered_email.html").write_text(run_result["artifact_html"])
        (eval_dir / "grading.json").write_text(json.dumps(grading, indent=2))
        summary.append(
            {
                "id": case["id"],
                "name": case["name"],
                "passed": grading["passed"],
                "artifact_path": grading["artifact_path"],
                "reply_draft_path": grading["reply_draft_path"],
            }
        )

    summary_path = iteration_dir / "summary.json"
    summary_path.write_text(json.dumps(summary, indent=2))
    failed = [item for item in summary if not item["passed"]]
    print(json.dumps({"summary_path": str(summary_path), "failed": failed}, indent=2))
    if failed:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
