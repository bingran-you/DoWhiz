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

The grader then checks the final rendered email artifact against the updated
U.S. equity decision-memo contract.
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
EVALS_ROOT = SKILL_ROOT / "evals"
WORKSPACE_ROOT = SKILL_ROOT.parent.parent / "us-equity-daily-monitor-workspace"
REQUIRED_LABELS = [
    "as of:",
    "price:",
    "investor question:",
    "decision card",
    "audience",
    "action",
    "confidence",
    "new money",
    "existing holder",
    "one-line rationale:",
    "dual-horizon framing",
    "near-term timing view",
    "long-term ownership view",
    "verified facts",
    "derived metrics",
    "scenarios",
    "bull case",
    "base case",
    "bear case",
    "triggers",
    "judgment",
]
SECTION_ORDER = [
    "decision card",
    "dual-horizon framing",
    "verified facts",
    "derived metrics",
    "scenarios",
    "triggers",
    "judgment",
]
LINKED_EVIDENCE_SECTIONS = ["verified facts"]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--iteration", default="iteration-final-artifact")
    parser.add_argument("--eval-ids", nargs="*", type=int)
    return parser.parse_args()


def load_evals() -> list[dict[str, Any]]:
    payload = json.loads((EVALS_ROOT / "evals.json").read_text())
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


def contains_in_order(text: str, markers: list[str]) -> bool:
    start = 0
    for marker in markers:
        idx = text.find(marker, start)
        if idx == -1:
            return False
        start = idx + len(marker)
    return True


def find_heading_position(html_lower: str, heading: str) -> int | None:
    for marker in [f">{heading}", f">{heading} "]:
        idx = html_lower.find(marker)
        if idx != -1:
            return idx
    return None


def extract_section(html_lower: str, heading: str) -> str | None:
    start = find_heading_position(html_lower, heading)
    if start is None:
        return None
    section_start = start + 1
    end = len(html_lower)
    for next_heading in SECTION_ORDER:
        if next_heading == heading:
            continue
        idx = find_heading_position(html_lower[section_start:], next_heading)
        if idx is not None:
            end = min(end, section_start + idx)
    return html_lower[start:end]


def section_contains_link(html_lower: str, heading: str) -> bool:
    section = extract_section(html_lower, heading)
    if not section:
        return False
    return 'href="http' in section or "href='http" in section


def count_clickable_links(html_lower: str) -> int:
    return html_lower.count('href="http') + html_lower.count("href='http")


def derived_metrics_has_formula(html_lower: str) -> bool:
    section = extract_section(html_lower, "derived metrics")
    if not section:
        return False
    text = normalize_text(section)
    return (
        "/" in text
        or "=" in text
        or "not reliably derivable" in text
        or "formula" in text
        or "<table" in section
    )


def triggers_section_has_required_markers(html_lower: str) -> bool:
    section = extract_section(html_lower, "triggers")
    if not section:
        return False
    text = normalize_text(section)
    has_upgrade = "upgrade to buy" in text
    has_add = "add" in text
    has_trim_or_exit = "trim/exit" in text or "trim" in text or "exit" in text
    return has_upgrade and has_add and has_trim_or_exit


def extract_confidence_phrase(text: str) -> str | None:
    for phrase in ["low confidence", "medium confidence", "high confidence"]:
        if phrase in text:
            return phrase
    return None


def audit_artifact(artifact_html: str) -> dict[str, Any]:
    text = normalize_text(artifact_html)
    html_lower = artifact_html.lower()
    missing_labels = [label for label in REQUIRED_LABELS if label not in text]
    decision_card_pos = text.find("decision card")
    return {
        "normalized_text": text,
        "missing_labels": missing_labels,
        "decision_card_rows_ok": all(
            label in text
            for label in ["audience", "action", "confidence", "new money", "existing holder"]
        ),
        "dual_horizon_ok": all(
            label in text for label in ["near-term timing view", "long-term ownership view"]
        ),
        "scenarios_ok": all(label in text for label in ["bull case", "base case", "bear case"]),
        "triggers_ok": triggers_section_has_required_markers(html_lower),
        "derived_metrics_ok": derived_metrics_has_formula(html_lower),
        "summary_first_ok": contains_in_order(text, SECTION_ORDER),
        "decision_card_near_top": decision_card_pos != -1 and decision_card_pos < 700,
        "clickable_link_count": count_clickable_links(html_lower),
        "linked_evidence_sections": {
            heading: section_contains_link(html_lower, heading)
            for heading in LINKED_EVIDENCE_SECTIONS
        },
        "confidence_phrase": extract_confidence_phrase(text),
    }


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
        "recovery_note": None,
    }


def grade_case(case: dict[str, Any], run_result: dict[str, Any]) -> dict[str, Any]:
    audit = audit_artifact(run_result["artifact_html"])
    text = audit["normalized_text"]
    checks: list[dict[str, Any]] = []

    if case.get("expected_failure"):
        failed_reasons = []
        if audit["missing_labels"]:
            failed_reasons.append(f"missing labels: {', '.join(audit['missing_labels'])}")
        if not audit["summary_first_ok"]:
            failed_reasons.append("section order missing")
        if not audit["decision_card_rows_ok"]:
            failed_reasons.append("decision card rows missing")
        if audit["clickable_link_count"] < 3:
            failed_reasons.append("citations missing")
        if audit["derived_metrics_ok"]:
            failed_reasons.append("generic fixture unexpectedly contains metric structure")
        checks.append(
            {
                "name": "generic_commentary_fixture_rejected",
                "passed": bool(failed_reasons),
                "detail": "; ".join(failed_reasons)
                if failed_reasons
                else "fixture unexpectedly passed the stronger contract",
            }
        )
    elif case.get("require_contract"):
        checks.append(
            {
                "name": "required_contract_labels_present",
                "passed": not audit["missing_labels"] and audit["derived_metrics_ok"],
                "detail": "ok"
                if not audit["missing_labels"] and audit["derived_metrics_ok"]
                else (
                    f"missing={audit['missing_labels']} derived_metrics_ok={audit['derived_metrics_ok']}"
                ),
            }
        )

    if case.get("require_decision_card"):
        checks.append(
            {
                "name": "decision_card_rows_present",
                "passed": audit["decision_card_rows_ok"],
                "detail": "ok" if audit["decision_card_rows_ok"] else "missing audience/action/confidence rows",
            }
        )

    if case.get("require_dual_horizon"):
        checks.append(
            {
                "name": "dual_horizon_present",
                "passed": audit["dual_horizon_ok"],
                "detail": "ok"
                if audit["dual_horizon_ok"]
                else "near-term or long-term view missing",
            }
        )

    if case.get("require_scenarios"):
        checks.append(
            {
                "name": "scenario_block_present",
                "passed": audit["scenarios_ok"],
                "detail": "ok" if audit["scenarios_ok"] else "bull/base/bear missing",
            }
        )

    if case.get("require_triggers"):
        checks.append(
            {
                "name": "trigger_block_present",
                "passed": audit["triggers_ok"],
                "detail": "ok"
                if audit["triggers_ok"]
                else "missing upgrade/add plus trim or exit conditions",
            }
        )

    if case.get("require_citations"):
        linked_sections_ok = all(audit["linked_evidence_sections"].values())
        checks.append(
            {
                "name": "clickable_citations_present",
                "passed": audit["clickable_link_count"] >= 3 and linked_sections_ok,
                "detail": (
                    f"links={audit['clickable_link_count']} "
                    f"linked_sections={audit['linked_evidence_sections']}"
                ),
            }
        )

    if case.get("require_scan_first"):
        checks.append(
            {
                "name": "scan_first_information_architecture",
                "passed": audit["summary_first_ok"] and audit["decision_card_near_top"],
                "detail": (
                    f"summary_first_ok={audit['summary_first_ok']} "
                    f"decision_card_near_top={audit['decision_card_near_top']}"
                ),
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
        passed = any(s.lower() in run_result["artifact_html"].lower() for s in case["must_contain_any"])
        checks.append(
            {
                "name": "required_correction_or_phrase_present",
                "passed": passed,
                "detail": "ok" if passed else f"missing any of {case['must_contain_any']}",
            }
        )

    if case.get("allowed_confidence"):
        confidence_phrase = audit["confidence_phrase"] or ""
        allowed = [f"{value.lower()} confidence" for value in case["allowed_confidence"]]
        checks.append(
            {
                "name": "confidence_degrades_for_weaker_case",
                "passed": any(value in confidence_phrase for value in allowed),
                "detail": f"confidence={confidence_phrase or '(missing)'}",
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
                "checks": grading["checks"],
                "artifact_path": grading["artifact_path"],
                "reply_draft_path": grading["reply_draft_path"],
            }
        )

    summary_path = iteration_dir / "summary.json"
    summary_path.write_text(json.dumps({"results": summary}, indent=2))
    print(summary_path)


if __name__ == "__main__":
    main()
