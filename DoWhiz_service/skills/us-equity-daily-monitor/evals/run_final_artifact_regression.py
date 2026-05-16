#!/usr/bin/env python3
"""
Run anti-waffle regressions for the U.S. equity daily monitor skill.

The suite supports both fixture-based contract checks and optional live runs
through the existing `investment_eval` helper. The goal is to verify the
user-visible artifact contract rather than an internal model transcript.
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

FULL_REQUIRED_LABELS = [
    "as of:",
    "price:",
    "investor question:",
    "decision card",
    "monitor status",
    "signal direction",
    "thesis impact",
    "signal quality",
    "urgency",
    "confidence",
    "new money action",
    "existing holder action",
    "one-line rationale:",
    "main risk to the signal:",
    "not personalized advice:",
    "why now",
    "dual-horizon framing",
    "near-term timing view",
    "long-term ownership view",
    "verified facts",
    "derived metrics",
    "scenarios",
    "bull case",
    "base case",
    "bear case",
    "what would change the view",
    "upgrade / review now",
    "downgrade / de-risk",
    "invalidation",
    "judgment",
]

SHORT_REQUIRED_LABELS = [
    "as of:",
    "price:",
    "investor question:",
    "decision card",
    "monitor status",
    "signal direction",
    "thesis impact",
    "signal quality",
    "urgency",
    "confidence",
    "new money action",
    "existing holder action",
    "one-line rationale:",
    "main risk to the signal:",
    "not personalized advice:",
    "why now",
    "what would change the view",
    "upgrade / review now",
    "downgrade / de-risk",
    "invalidation",
    "evidence chips",
]

FULL_ORDER = [
    "decision card",
    "why now",
    "dual-horizon framing",
    "competitive / peer context",
    "verified facts",
    "derived metrics",
    "scenarios",
    "what would change the view",
    "judgment",
]

SHORT_ORDER = [
    "decision card",
    "why now",
    "competitive / peer context",
    "what would change the view",
    "evidence chips",
]

FULL_REQUIRED_ORDER = [
    "decision card",
    "why now",
    "dual-horizon framing",
    "verified facts",
    "derived metrics",
    "scenarios",
    "what would change the view",
    "judgment",
]

SHORT_REQUIRED_ORDER = [
    "decision card",
    "why now",
    "what would change the view",
    "evidence chips",
]

DEFAULT_BANNED_PHRASES = [
    "wait and see",
    "do not rush",
    "hold for now",
    "it depends",
    "be careful",
    "markets are uncertain",
    "could go either way",
    "consult a financial advisor",
    "great business, but",
    "good company, but",
    "wait until after earnings",
    "wait for clarity",
    "do not chase",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--iteration", default="iteration-anti-waffle")
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
    for marker in [f">{heading}<", f">{heading} ", f">{heading}\n"]:
        idx = html_lower.find(marker)
        if idx != -1:
            return idx
    return None


def extract_section(html_lower: str, heading: str, headings: list[str]) -> str | None:
    start = find_heading_position(html_lower, heading)
    if start is None:
        return None
    section_start = start + 1
    end = len(html_lower)
    for next_heading in headings:
        if next_heading == heading:
            continue
        idx = find_heading_position(html_lower[section_start:], next_heading)
        if idx is not None:
            end = min(end, section_start + idx)
    return html_lower[start:end]


def count_clickable_links(html_lower: str) -> int:
    return html_lower.count('href="http') + html_lower.count("href='http")


def section_contains_link(html_lower: str, heading: str, headings: list[str]) -> bool:
    section = extract_section(html_lower, heading, headings)
    if not section:
        return False
    return 'href="http' in section or "href='http" in section


def derived_metrics_has_formula(html_lower: str) -> bool:
    section = extract_section(html_lower, "derived metrics", FULL_ORDER)
    if not section:
        return False
    text = normalize_text(section)
    return (
        "formula / inputs" in text
        or "/" in text
        or "=" in text
        or "not reliably derivable" in text
        or "<table" in section
    )


def count_numeric_hits(text: str) -> int:
    digit_runs = re.findall(r"\d+", text)
    explicit_units = text.count("%") + text.count("$") + text.count("bps") + text.count("x")
    word_hits = 0
    number_words = r"(one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve)"
    time_units = r"(day|days|week|weeks|month|months|quarter|quarters|year|years)"
    word_hits += len(re.findall(rf"\b{number_words}\s+{time_units}\b", text))
    word_hits += len(re.findall(rf"\b{number_words}\s+more\s+{time_units}\b", text))
    return len(digit_runs) + explicit_units + word_hits


def decision_card_core_precedes_actions(text: str) -> bool:
    required = [
        "monitor status",
        "signal direction",
        "thesis impact",
        "signal quality",
        "urgency",
        "confidence",
        "new money action",
        "existing holder action",
    ]
    positions = [text.find(marker) for marker in required]
    if any(position == -1 for position in positions):
        return False
    return positions == sorted(positions)


def why_now_specific(html_lower: str, contract: str) -> bool:
    headings = FULL_ORDER if contract == "full" else SHORT_ORDER
    section = extract_section(html_lower, "why now", headings)
    if not section:
        return False
    words = normalize_text(section).split()
    return len(words) >= 12


def main_risk_present(text: str) -> bool:
    return "main risk to the signal:" in text and len(text.split("main risk to the signal:", 1)[1].split()) >= 8


def change_view_ok(html_lower: str, contract: str) -> tuple[bool, str]:
    headings = FULL_ORDER if contract == "full" else SHORT_ORDER
    section = extract_section(html_lower, "what would change the view", headings)
    if not section:
        return False, "missing section"
    text = normalize_text(section)
    markers = ["upgrade / review now", "downgrade / de-risk", "invalidation"]
    if not all(marker in text for marker in markers):
        return False, "missing upgrade/downgrade/invalidation markers"
    numeric_hits = count_numeric_hits(text)
    if numeric_hits < 3:
        return False, f"not enough numeric specificity ({numeric_hits})"
    return True, f"numeric_hits={numeric_hits}"


def compliance_sentence_once(text: str) -> bool:
    return text.count("not personalized financial advice") == 1


def banned_phrases_found(text: str, extra: list[str] | None = None) -> list[str]:
    scan_text = text.split("decision card", 1)[1] if "decision card" in text else text
    phrases = list(DEFAULT_BANNED_PHRASES)
    if extra:
        phrases.extend(extra)
    return [phrase for phrase in phrases if phrase in scan_text]


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
    artifact_html = (EVALS_ROOT / case["artifact_file"]).read_text()
    rendered_path = eval_dir / "final_rendered_email.html"
    rendered_path.write_text(artifact_html)
    return {
        "artifact_html": artifact_html,
        "final_rendered_path": str(rendered_path),
        "reply_draft_path": None,
        "recovery_note": None,
    }


def audit_artifact(case: dict[str, Any], artifact_html: str) -> dict[str, Any]:
    contract = case.get("contract", "full")
    text = normalize_text(artifact_html)
    html_lower = artifact_html.lower()
    labels = FULL_REQUIRED_LABELS if contract == "full" else SHORT_REQUIRED_LABELS
    order = FULL_REQUIRED_ORDER if contract == "full" else SHORT_REQUIRED_ORDER
    section_order = FULL_ORDER if contract == "full" else SHORT_ORDER
    evidence_heading = "verified facts" if contract == "full" else "evidence chips"
    change_view_passed, change_view_detail = change_view_ok(html_lower, contract)
    peer_section = extract_section(html_lower, "competitive / peer context", section_order) or ""
    peer_section_text = normalize_text(peer_section)

    return {
        "contract": contract,
        "text": text,
        "html_lower": html_lower,
        "missing_labels": [label for label in labels if label not in text],
        "summary_first_ok": contains_in_order(text, order),
        "decision_card_near_top": text.find("decision card") != -1 and text.find("decision card") < 500,
        "decision_card_order_ok": decision_card_core_precedes_actions(text),
        "clickable_link_count": count_clickable_links(html_lower),
        "evidence_section_has_link": section_contains_link(html_lower, evidence_heading, section_order),
        "derived_metrics_ok": derived_metrics_has_formula(html_lower) if contract == "full" else True,
        "why_now_specific": why_now_specific(html_lower, contract),
        "main_risk_present": main_risk_present(text),
        "change_view_ok": change_view_passed,
        "change_view_detail": change_view_detail,
        "compliance_sentence_once": compliance_sentence_once(text),
        "banned_phrases": banned_phrases_found(text, case.get("banned_phrases")),
        "peer_context_present": bool(peer_section_text),
        "peer_section_text": peer_section_text,
        "peer_section_word_count": len(peer_section_text.split()),
    }


def make_check(name: str, passed: bool, detail: str) -> dict[str, Any]:
    return {"name": name, "passed": passed, "detail": detail}


def grade_case(case: dict[str, Any], run_result: dict[str, Any]) -> dict[str, Any]:
    audit = audit_artifact(case, run_result["artifact_html"])
    text = audit["text"]
    checks: list[dict[str, Any]] = []

    if case.get("expected_failure"):
        reasons = []
        if audit["missing_labels"]:
            reasons.append(f"missing labels: {', '.join(audit['missing_labels'])}")
        if not audit["why_now_specific"]:
            reasons.append("why now is generic")
        if not audit["main_risk_present"]:
            reasons.append("main risk missing")
        if not audit["change_view_ok"]:
            reasons.append(audit["change_view_detail"])
        if audit["banned_phrases"]:
            reasons.append(f"banned phrases: {', '.join(audit['banned_phrases'])}")
        if not audit["compliance_sentence_once"]:
            reasons.append("compliance sentence missing or repeated")
        checks.append(
            make_check(
                "anti_waffle_fixture_rejected",
                bool(reasons),
                "; ".join(reasons) if reasons else "fixture unexpectedly passed the anti-waffle contract",
            )
        )
        return {
            "checks": checks,
            "passed": all(check["passed"] for check in checks),
            "artifact_path": run_result["final_rendered_path"],
            "reply_draft_path": run_result["reply_draft_path"],
            "recovery_note": run_result.get("recovery_note"),
        }

    checks.append(
        make_check(
            "required_labels_present",
            not audit["missing_labels"],
            "ok" if not audit["missing_labels"] else f"missing={audit['missing_labels']}",
        )
    )

    checks.append(
        make_check(
            "decision_card_keeps_signal_primary",
            audit["decision_card_order_ok"],
            "ok" if audit["decision_card_order_ok"] else "core signal rows do not precede action rows",
        )
    )

    for field_name, case_key in [
        ("monitor status", "expected_status"),
        ("signal direction", "expected_signal_direction"),
        ("thesis impact", "expected_thesis_impact"),
        ("signal quality", "expected_signal_quality"),
        ("urgency", "expected_urgency"),
        ("confidence", "expected_confidence"),
    ]:
        if case_key not in case:
            continue
        expected = case[case_key].lower()
        checks.append(
            make_check(
                f"{field_name.replace(' ', '_')}_matches",
                f"{field_name} {expected}" in text,
                "ok" if f"{field_name} {expected}" in text else f"expected `{field_name} {expected}`",
            )
        )

    if case.get("require_why_now"):
        checks.append(
            make_check(
                "why_now_is_specific",
                audit["why_now_specific"],
                "ok" if audit["why_now_specific"] else "why now section missing or too generic",
            )
        )

    if case.get("require_main_risk"):
        checks.append(
            make_check(
                "main_risk_present",
                audit["main_risk_present"],
                "ok" if audit["main_risk_present"] else "main risk to the signal missing",
            )
        )

    if case.get("require_change_view"):
        checks.append(
            make_check(
                "change_view_is_specific",
                audit["change_view_ok"],
                audit["change_view_detail"],
            )
        )

    if case.get("require_citations") or case.get("min_clickable_links"):
        min_links = case.get("min_clickable_links", 0)
        checks.append(
            make_check(
                "evidence_links_present",
                audit["clickable_link_count"] >= min_links and audit["evidence_section_has_link"],
                f"links={audit['clickable_link_count']} evidence_section_has_link={audit['evidence_section_has_link']}",
            )
        )

    if case.get("require_scan_first"):
        checks.append(
            make_check(
                "scan_first_order_preserved",
                audit["summary_first_ok"] and audit["decision_card_near_top"],
                f"summary_first_ok={audit['summary_first_ok']} decision_card_near_top={audit['decision_card_near_top']}",
            )
        )

    if audit["contract"] == "full":
        checks.append(
            make_check(
                "derived_metrics_are_formula_based",
                audit["derived_metrics_ok"],
                "ok" if audit["derived_metrics_ok"] else "derived metrics block lacks formula-like content",
            )
        )

    if case.get("require_compliance_sentence"):
        checks.append(
            make_check(
                "single_concise_compliance_sentence",
                audit["compliance_sentence_once"],
                "ok" if audit["compliance_sentence_once"] else "missing or repeated compliance sentence",
            )
        )

    checks.append(
        make_check(
            "generic_waffle_phrases_absent",
            not audit["banned_phrases"],
            "ok" if not audit["banned_phrases"] else f"found={audit['banned_phrases']}",
        )
    )

    if case.get("must_contain_any"):
        lower_html = run_result["artifact_html"].lower()
        passed = any(needle.lower() in lower_html for needle in case["must_contain_any"])
        checks.append(
            make_check(
                "required_specific_phrase_present",
                passed,
                "ok" if passed else f"missing any of {case['must_contain_any']}",
            )
        )

    if case.get("must_contain_all"):
        lower_html = run_result["artifact_html"].lower()
        missing = [needle for needle in case["must_contain_all"] if needle.lower() not in lower_html]
        checks.append(
            make_check(
                "required_specific_phrases_present",
                not missing,
                "ok" if not missing else f"missing {missing}",
            )
        )

    if case.get("must_not_contain_any"):
        lower_html = run_result["artifact_html"].lower()
        found = [needle for needle in case["must_not_contain_any"] if needle.lower() in lower_html]
        checks.append(
            make_check(
                "forbidden_specific_phrases_absent",
                not found,
                "ok" if not found else f"found {found}",
            )
        )

    if "expect_peer_context" in case:
        checks.append(
            make_check(
                "peer_context_presence_matches",
                audit["peer_context_present"] == case["expect_peer_context"],
                (
                    f"peer_context_present={audit['peer_context_present']}"
                    if audit["peer_context_present"] == case["expect_peer_context"]
                    else f"expected peer_context_present={case['expect_peer_context']}, got {audit['peer_context_present']}"
                ),
            )
        )

    if case.get("peer_section_must_contain_any"):
        found = any(
            needle.lower() in audit["peer_section_text"] for needle in case["peer_section_must_contain_any"]
        )
        checks.append(
            make_check(
                "peer_section_contains_required_marker",
                found,
                "ok" if found else f"missing any of {case['peer_section_must_contain_any']}",
            )
        )

    if case.get("peer_section_must_contain_all"):
        missing = [
            needle
            for needle in case["peer_section_must_contain_all"]
            if needle.lower() not in audit["peer_section_text"]
        ]
        checks.append(
            make_check(
                "peer_section_contains_all_required_markers",
                not missing,
                "ok" if not missing else f"missing {missing}",
            )
        )

    if case.get("peer_section_must_not_contain_any"):
        found = [
            needle
            for needle in case["peer_section_must_not_contain_any"]
            if needle.lower() in audit["peer_section_text"]
        ]
        checks.append(
            make_check(
                "peer_section_forbidden_markers_absent",
                not found,
                "ok" if not found else f"found {found}",
            )
        )

    if case.get("peer_section_max_words") is not None:
        checks.append(
            make_check(
                "peer_section_stays_concise",
                audit["peer_section_word_count"] <= case["peer_section_max_words"],
                f"peer_section_word_count={audit['peer_section_word_count']}",
            )
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
        run_result = run_live_eval(case, eval_dir) if case["kind"] == "live_run" else run_fixture_eval(case, eval_dir)
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
