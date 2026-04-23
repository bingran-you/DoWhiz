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
EVALS_ROOT = SKILL_ROOT / "evals"
WORKSPACE_ROOT = SKILL_ROOT.parent.parent / "us-equity-daily-monitor-workspace"
REQUIRED_LABELS = [
    "request framing",
    "ticker:",
    "name:",
    "type:",
    "research mode:",
    "user objective:",
    "horizon basis:",
    "question type:",
    "decision card",
    "new money action:",
    "existing holder action:",
    "near-term timing view:",
    "long-term ownership view:",
    "confidence:",
    "one-line rationale:",
    "why in 3 bullets",
    "what is priced in:",
    "what keeps this from being stronger:",
    "what would change the view:",
    "trigger block",
    "upgrade / add triggers:",
    "stay wait unless:",
    "invalidation criteria:",
    "verified facts",
    "derived metrics",
    "expectations",
    "what the next catalyst must show:",
    "what could disappoint even if fundamentals are fine:",
    "opportunity-cost / peer check",
    "inference / judgment",
    "bull case:",
    "base case:",
    "bear case:",
    "source notes",
    "disclaimer",
]
SUMMARY_FIRST_HEADINGS = [
    "request framing",
    "decision card",
    "why in 3 bullets",
    "trigger block",
    "verified facts",
]
SECTION_HEADINGS = [
    "request framing",
    "decision card",
    "why in 3 bullets",
    "trigger block",
    "verified facts",
    "derived metrics",
    "expectations",
    "opportunity-cost / peer check",
    "inference / judgment",
    "scenario analysis",
    "source notes",
    "disclaimer",
]
LINKED_EVIDENCE_SECTIONS = [
    "verified facts",
    "derived metrics",
    "expectations",
    "source notes",
]
NEW_MONEY_ACTIONS = ["buy", "wait", "starter only", "avoid for now"]
EXISTING_HOLDER_ACTIONS = ["hold", "add", "trim", "exit", "hold / do not add"]


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


def find_field_value(text: str, label: str, allowed: list[str]) -> str | None:
    for value in allowed:
        if f"{label}: {value}" in text:
            return value
    return None


def extract_section(html_lower: str, heading: str) -> str | None:
    start = find_heading_position(html_lower, heading)
    if start is None:
        return None
    section_start = start + 1
    end = len(html_lower)
    for next_heading in SECTION_HEADINGS:
        if next_heading == heading:
            continue
        idx = find_heading_position(html_lower[section_start:], next_heading)
        if idx is not None:
            end = min(end, section_start + idx)
    return html_lower[start:end]


def find_heading_position(html_lower: str, heading: str) -> int | None:
    for marker in [f">{heading}</", f">{heading}<"]:
        idx = html_lower.find(marker)
        if idx != -1:
            return idx
    return None


def section_contains_link(html_lower: str, heading: str) -> bool:
    section = extract_section(html_lower, heading)
    if not section:
        return False
    return 'href="http' in section or "href='http" in section


def count_evidence_chips(html_lower: str) -> int:
    return html_lower.count("dw-evidence-chip")


def has_source_tier(html_lower: str, tier: str) -> bool:
    return f'data-source-tier="{tier}"' in html_lower or f"data-source-tier='{tier}'" in html_lower


def extract_field(text: str, label: str) -> str | None:
    pattern = re.compile(rf"{re.escape(label)}\s*:\s*([^<\n\r]+)", re.IGNORECASE)
    match = pattern.search(text)
    if not match:
        return None
    return match.group(1).strip().lower()


def audit_artifact(artifact_html: str) -> dict[str, Any]:
    text = normalize_text(artifact_html)
    html_lower = artifact_html.lower()
    missing_labels = [label for label in REQUIRED_LABELS if label not in text]
    decision_card_pos = text.find("decision card")

    return {
        "normalized_text": text,
        "missing_labels": missing_labels,
        "new_money_action": find_field_value(text, "new money action", NEW_MONEY_ACTIONS),
        "existing_holder_action": find_field_value(
            text, "existing holder action", EXISTING_HOLDER_ACTIONS
        ),
        "summary_first_ok": contains_in_order(text, SUMMARY_FIRST_HEADINGS),
        "decision_card_near_top": decision_card_pos != -1 and decision_card_pos < 900,
        "evidence_chip_count": count_evidence_chips(html_lower),
        "source_tiers": {
            tier: has_source_tier(html_lower, tier)
            for tier in ["primary", "independent", "reference"]
        },
        "linked_evidence_sections": {
            heading: section_contains_link(html_lower, heading)
            for heading in LINKED_EVIDENCE_SECTIONS
        },
        "confidence": extract_field(text, "confidence"),
        "html_lower": html_lower,
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
            failed_reasons.append("summary-first order missing")
        if not audit["new_money_action"]:
            failed_reasons.append("new money action missing or invalid")
        if audit["evidence_chip_count"] < 4:
            failed_reasons.append("evidence chips missing")
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
                "passed": not audit["missing_labels"],
                "detail": "ok"
                if not audit["missing_labels"]
                else f"missing: {', '.join(audit['missing_labels'])}",
            }
        )

    if case.get("require_actions"):
        checks.append(
            {
                "name": "new_money_vs_existing_holder_actions",
                "passed": bool(audit["new_money_action"] and audit["existing_holder_action"]),
                "detail": (
                    f"new_money={audit['new_money_action'] or '(missing)'} "
                    f"existing_holder={audit['existing_holder_action'] or '(missing)'}"
                ),
            }
        )

    if case.get("require_dual_horizon"):
        checks.append(
            {
                "name": "dual_horizon_present",
                "passed": "near-term timing view:" in text and "long-term ownership view:" in text,
                "detail": "ok"
                if "near-term timing view:" in text and "long-term ownership view:" in text
                else "near-term or long-term view missing",
            }
        )

    if case.get("require_expectations"):
        expectations_ok = (
            "what is priced in:" in text
            and "what the next catalyst must show:" in text
            and "what could disappoint even if fundamentals are fine:" in text
        )
        checks.append(
            {
                "name": "expectations_layer_present",
                "passed": expectations_ok,
                "detail": "ok" if expectations_ok else "expectations layer missing pieces",
            }
        )

    if case.get("require_evidence_chips"):
        linked_sections_ok = all(audit["linked_evidence_sections"].values())
        checks.append(
            {
                "name": "clickable_evidence_present",
                "passed": audit["evidence_chip_count"] >= 4 and linked_sections_ok,
                "detail": (
                    f"chips={audit['evidence_chip_count']} "
                    f"linked_sections={audit['linked_evidence_sections']}"
                ),
            }
        )

    if case.get("require_source_tiers"):
        tiers_ok = all(audit["source_tiers"].values())
        checks.append(
            {
                "name": "tiered_sources_present",
                "passed": tiers_ok,
                "detail": f"tiers={audit['source_tiers']}",
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

    if case.get("require_opportunity_cost"):
        checks.append(
            {
                "name": "opportunity_cost_present",
                "passed": "opportunity-cost / peer check" in text,
                "detail": "ok"
                if "opportunity-cost / peer check" in text
                else "opportunity-cost block missing",
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
        confidence = audit["confidence"] or ""
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
