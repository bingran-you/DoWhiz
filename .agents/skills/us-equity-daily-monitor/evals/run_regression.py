#!/usr/bin/env python3
"""Run and grade bounded regressions for the U.S. equity investment skill."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path


REQUIRED_SECTIONS = [
    "Decision Card",
    "Dual-Horizon Framing",
    "Verified Facts",
    "Derived Metrics",
    "Scenarios",
    "Triggers",
    "Judgment",
]
EVAL_SLUGS = {
    1: "default-dual-horizon",
    2: "horizon-awareness",
    3: "wrong-premise-correction",
    4: "uncertainty-handling",
}


def script_path() -> Path:
    return Path(__file__).resolve()


def skill_dir() -> Path:
    return script_path().parents[1]


def repo_root() -> Path:
    return script_path().parents[4]


def default_workspace_dir() -> Path:
    return skill_dir().parent / f"{skill_dir().name}-workspace"


def load_evals() -> dict[int, dict]:
    evals_path = skill_dir() / "evals" / "evals.json"
    payload = json.loads(evals_path.read_text())
    return {item["id"]: item for item in payload["evals"]}


def section_map(text: str) -> dict[str, str]:
    matches = list(re.finditer(r"(?m)^##\s+(.+?)\s*$", text))
    sections: dict[str, str] = {}
    for index, match in enumerate(matches):
        header = match.group(1).strip()
        start = match.end()
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        sections[header] = text[start:end].strip()
    return sections


def get_section_by_prefix(sections: dict[str, str], prefix: str) -> str:
    for header, content in sections.items():
        if header.lower().startswith(prefix.lower()):
            return content
    return ""


def count_markdown_links(text: str) -> int:
    return len(re.findall(r"\]\(https?://", text))


def contains_in_order(text: str, markers: list[str]) -> bool:
    lower = text.lower()
    start = 0
    for marker in markers:
        idx = lower.find(marker.lower(), start)
        if idx == -1:
            return False
        start = idx + len(marker)
    return True


def make_expectation(text: str, passed: bool, evidence: str) -> dict:
    return {
        "text": text,
        "passed": passed,
        "evidence": evidence,
    }


def grade_generic(output_text: str) -> tuple[list[dict], dict[str, str | int | None]]:
    sections = section_map(output_text)
    expectations: list[dict] = []

    missing_sections = []
    for header in REQUIRED_SECTIONS:
        if not get_section_by_prefix(sections, header):
            missing_sections.append(header)

    expectations.append(
        make_expectation(
            "The answer uses the required memo structure with Decision Card, Dual-Horizon Framing, Verified Facts, Derived Metrics, Scenarios, Triggers, and Judgment.",
            not missing_sections,
            "All required sections are present."
            if not missing_sections
            else f"Missing sections: {', '.join(missing_sections)}.",
        )
    )

    decision_card = get_section_by_prefix(sections, "Decision Card")
    dual_horizon = get_section_by_prefix(sections, "Dual-Horizon Framing")
    verified_facts = get_section_by_prefix(sections, "Verified Facts")
    derived_metrics = get_section_by_prefix(sections, "Derived Metrics")
    scenarios = get_section_by_prefix(sections, "Scenarios")
    triggers = get_section_by_prefix(sections, "Triggers")
    judgment = get_section_by_prefix(sections, "Judgment")

    decision_card_ok = all(
        marker in decision_card
        for marker in [
            "| Audience | Action | Confidence |",
            "| New money |",
            "| Existing holder |",
            "**One-line rationale:**",
        ]
    )
    expectations.append(
        make_expectation(
            "The answer includes a Decision Card with separate new-money and existing-holder rows plus a one-line rationale.",
            decision_card_ok,
            "Decision Card table and rationale are present."
            if decision_card_ok
            else "Decision Card is missing the audience table or the one-line rationale.",
        )
    )

    dual_horizon_ok = all(
        marker in dual_horizon
        for marker in [
            "### Near-Term Timing View",
            "### Long-Term Ownership View",
        ]
    )
    expectations.append(
        make_expectation(
            "The answer includes both the near-term timing view and the long-term ownership view.",
            dual_horizon_ok,
            "Both dual-horizon sub-sections are present."
            if dual_horizon_ok
            else "Dual-Horizon Framing is missing one of the required sub-sections.",
        )
    )

    citations_ok = count_markdown_links(output_text) >= 3 and "](http" in verified_facts
    expectations.append(
        make_expectation(
            "The answer cites at least three clickable sources and keeps citations in the Verified Facts block.",
            citations_ok,
            "At least three markdown links are present and Verified Facts includes citations."
            if citations_ok
            else "The memo has fewer than three markdown links or the Verified Facts section lacks clickable citations.",
        )
    )

    derived_metrics_ok = (
        "| Metric | Value | Formula / Inputs |" in derived_metrics
        and (
            "/" in derived_metrics
            or "=" in derived_metrics
            or "not reliably derivable" in derived_metrics.lower()
        )
    )
    expectations.append(
        make_expectation(
            "The answer includes a Derived Metrics table with formulas or an explicit non-derivable note.",
            derived_metrics_ok,
            "Derived Metrics includes formula-bearing rows."
            if derived_metrics_ok
            else "Derived Metrics is missing the standard table header or formula-like content.",
        )
    )

    scenarios_ok = all(
        marker in scenarios
        for marker in [
            "### Bull Case",
            "### Base Case",
            "### Bear Case",
        ]
    )
    expectations.append(
        make_expectation(
            "The answer includes Bull Case, Base Case, and Bear Case.",
            scenarios_ok,
            "All scenario sub-sections are present."
            if scenarios_ok
            else "The Scenarios section is missing one of Bull/Base/Bear.",
        )
    )

    triggers_ok = (
        "Upgrade to Buy" in triggers
        and "Add" in triggers
        and ("Trim/Exit" in triggers or "Trim" in triggers or "Exit" in triggers)
    )
    expectations.append(
        make_expectation(
            "The answer includes quantified trigger conditions for upgrade/add plus trim or exit.",
            triggers_ok,
            "Triggers section contains upgrade/add and trim/exit markers."
            if triggers_ok
            else "Triggers section is missing upgrade/add or trim/exit conditions.",
        )
    )

    judgment_ok = "confidence" in judgment.lower()
    expectations.append(
        make_expectation(
            "The Judgment section states confidence explicitly.",
            judgment_ok,
            "Judgment names confidence."
            if judgment_ok
            else "Judgment does not state confidence explicitly.",
        )
    )

    summary_first_ok = contains_in_order(
        output_text,
        [
            "## Decision Card",
            "## Dual-Horizon Framing",
            "## Verified Facts",
            "## Derived Metrics",
            "## Scenarios",
            "## Triggers",
            "## Judgment",
        ],
    )
    expectations.append(
        make_expectation(
            "The memo keeps the scan-first section order from the skill template.",
            summary_first_ok,
            "Section order matches the template."
            if summary_first_ok
            else "Section order does not follow the template.",
        )
    )

    context = {
        "output_text": output_text,
        "judgment": judgment,
    }

    return expectations, context


def grade_eval_specific(eval_id: int, output_text: str, context: dict[str, str | int | None]) -> list[dict]:
    lower = output_text.lower()
    expectations: list[dict] = []

    if eval_id == 1:
        dual_horizon_ok = "### near-term timing view" in lower and "### long-term ownership view" in lower
        expectations.append(
            make_expectation(
                "Because the prompt does not state a horizon, the answer still provides both near-term timing and long-term ownership views.",
                dual_horizon_ok,
                "Both horizon sections are present."
                if dual_horizon_ok
                else "The memo does not preserve both horizon views when the prompt leaves horizon unspecified.",
            )
        )

    if eval_id == 2:
        horizon_ok = any(
            phrase in lower for phrase in ["3-month", "3 month", "3 months", "three-month"]
        )
        expectations.append(
            make_expectation(
                "The answer recognizes the stated 3-month holding period in the memo itself.",
                horizon_ok,
                "The memo references the 3-month horizon."
                if horizon_ok
                else "The memo does not explicitly reflect the stated 3-month holding period.",
            )
        )

        tactical_timing_ok = any(
            phrase in lower
            for phrase in [
                "this week",
                "next 1",
                "next 2 quarters",
                "next few",
                "entry this week",
                "short window",
            ]
        )
        expectations.append(
            make_expectation(
                "The answer includes explicit tactical timing logic for the short holding window.",
                tactical_timing_ok,
                "The memo discusses the tactical window explicitly."
                if tactical_timing_ok
                else "The memo does not explicitly discuss the short tactical window.",
            )
        )

    if eval_id == 3:
        premise_corrected = (
            "may 20, 2026" in lower
            or "may 20 2026" in lower
            or ("may 27" in lower and any(keyword in lower for keyword in ["estimated", "unconfirmed", "not confirmed"]))
        )
        expectations.append(
            make_expectation(
                "The answer does not silently accept the user's May 27 earnings-date premise.",
                premise_corrected,
                "The memo either corrects the date or qualifies May 27."
                if premise_corrected
                else "The memo does not clearly correct or qualify the May 27 premise.",
            )
        )

        earnings_timing_ok = "earnings" in lower and "near-term timing view" in lower
        expectations.append(
            make_expectation(
                "The answer gives a direct pre-earnings timing view.",
                earnings_timing_ok,
                "The memo ties its timing view to the earnings event."
                if earnings_timing_ok
                else "The memo does not clearly tie the timing view to the earnings event.",
            )
        )

    if eval_id == 4:
        lowered_confidence = any(
            phrase in (context.get("judgment") or "").lower()
            for phrase in ["low confidence", "medium confidence"]
        )
        expectations.append(
            make_expectation(
                "The answer does not use High confidence for this weaker or messier case.",
                lowered_confidence,
                "Judgment uses Low or Medium confidence."
                if lowered_confidence
                else "Judgment does not clearly degrade confidence for the weaker case.",
            )
        )

        explicit_uncertainty = any(
            phrase in lower
            for phrase in [
                "uncertain",
                "mixed",
                "messy",
                "limited",
                "incomplete",
                "conflicting",
                "speculative",
                "not reliably derivable",
                "noisy",
                "funding risk",
                "dilution",
                "volatile",
            ]
        )
        expectations.append(
            make_expectation(
                "The answer explicitly acknowledges uncertainty, mixed evidence, or missing inputs.",
                explicit_uncertainty,
                "The memo names uncertainty or missing inputs."
                if explicit_uncertainty
                else "The memo does not explicitly acknowledge uncertainty or missing inputs.",
            )
        )

    return expectations


def grade_output(eval_id: int, output_text: str) -> dict:
    generic_expectations, context = grade_generic(output_text)
    expectations = generic_expectations + grade_eval_specific(eval_id, output_text, context)
    passed = sum(1 for item in expectations if item["passed"])
    total = len(expectations)
    return {
        "expectations": expectations,
        "summary": {
            "passed": passed,
            "failed": total - passed,
            "total": total,
            "pass_rate": round(passed / total, 4) if total else 0.0,
        },
    }


def run_codex(prompt: str, output_path: Path, transcript_path: Path, reasoning_effort: str) -> subprocess.CompletedProcess[str]:
    command = [
        "codex",
        "--search",
        "-c",
        f'model_reasoning_effort="{reasoning_effort}"',
        "exec",
        "-C",
        str(repo_root()),
        "--output-last-message",
        str(output_path),
        prompt,
    ]
    with transcript_path.open("w") as transcript_file:
        return subprocess.run(
            command,
            cwd=repo_root(),
            text=True,
            stdout=transcript_file,
            stderr=subprocess.STDOUT,
            check=False,
        )


def run_eval_case(eval_case: dict, workspace_dir: Path, iteration: str, reasoning_effort: str) -> dict:
    slug = EVAL_SLUGS.get(eval_case["id"], f"eval-{eval_case['id']}")
    eval_dir = workspace_dir / iteration / f"eval-{eval_case['id']}-{slug}"
    outputs_dir = eval_dir / "with_skill" / "outputs"
    outputs_dir.mkdir(parents=True, exist_ok=True)

    output_path = outputs_dir / "final.md"
    transcript_path = eval_dir / "with_skill" / "transcript.txt"
    timing_path = eval_dir / "with_skill" / "timing.json"
    grading_path = eval_dir / "with_skill" / "grading.json"
    metadata_path = eval_dir / "eval_metadata.json"

    notes_block = ""
    if eval_case.get("notes"):
        notes_block = "Trusted current source notes:\n- " + "\n- ".join(eval_case["notes"]) + "\n\n"

    wrapped_prompt = (
        f"Read and follow this skill exactly before answering: {skill_dir() / 'SKILL.md'}. "
        "Do not modify repository files. Return only the final markdown investment memo. "
        "Keep the response in the exact one-page structure from the skill template, including the Decision Card, Dual-Horizon Framing, Verified Facts, Derived Metrics, Scenarios, Triggers, and Judgment sections. "
        "Use clickable markdown links in Verified Facts and keep Derived Metrics formula-driven rather than source-dumped.\n\n"
        f"{notes_block}"
        f"User request:\n{eval_case['prompt']}\n"
    )

    metadata = {
        "eval_id": eval_case["id"],
        "eval_name": slug,
        "prompt": eval_case["prompt"],
        "assertions": eval_case["expectations"],
    }
    metadata_path.write_text(json.dumps(metadata, indent=2))

    started_at = time.time()
    completed = run_codex(wrapped_prompt, output_path, transcript_path, reasoning_effort)
    duration_ms = int((time.time() - started_at) * 1000)
    timing_path.write_text(
        json.dumps(
            {
                "duration_ms": duration_ms,
                "total_duration_seconds": round(duration_ms / 1000, 1),
            },
            indent=2,
        )
    )

    if completed.returncode != 0:
        raise RuntimeError(
            f"codex exec failed for eval {eval_case['id']} with code {completed.returncode}"
        )

    output_text = output_path.read_text()
    grading = grade_output(eval_case["id"], output_text)
    grading_path.write_text(json.dumps(grading, indent=2))

    return {
        "eval_id": eval_case["id"],
        "eval_name": slug,
        "grading": grading,
        "output_path": str(output_path),
        "transcript_path": str(transcript_path),
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--eval-ids", nargs="*", type=int)
    parser.add_argument("--workspace-dir", type=Path, default=default_workspace_dir())
    parser.add_argument("--iteration", default="iteration-1")
    parser.add_argument(
        "--reasoning-effort",
        default="high",
        choices=["minimal", "low", "medium", "high"],
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    evals = load_evals()
    selected_ids = args.eval_ids or sorted(evals)
    selected = [evals[eval_id] for eval_id in selected_ids]

    args.workspace_dir.mkdir(parents=True, exist_ok=True)

    results = [
        run_eval_case(eval_case, args.workspace_dir, args.iteration, args.reasoning_effort)
        for eval_case in selected
    ]

    summary = {
        "results": [
            {
                "eval_id": result["eval_id"],
                "eval_name": result["eval_name"],
                "summary": result["grading"]["summary"],
                "output_path": result["output_path"],
                "transcript_path": result["transcript_path"],
            }
            for result in results
        ]
    }
    summary_path = args.workspace_dir / args.iteration / "summary.json"
    summary_path.parent.mkdir(parents=True, exist_ok=True)
    summary_path.write_text(json.dumps(summary, indent=2))
    print(summary_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
