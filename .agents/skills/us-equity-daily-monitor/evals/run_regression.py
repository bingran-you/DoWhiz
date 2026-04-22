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


REQUIRED_HEADERS = [
    "Request framing",
    "Verified facts",
    "Derived metrics",
    "Inference / judgment",
    "Scenario analysis",
    "Timing / execution",
    "Final recommendation",
    "Sources",
    "Disclaimer",
]

RATINGS = ("Buy", "Wait", "Sell")
ENTRY_APPROACHES = ("Buy now", "Starter only", "Wait", "Avoid for now")
CONFIDENCE_LEVELS = ("Low", "Medium", "High")

EVAL_SLUGS = {
    1: "generic-summary-failure-mode",
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


def normalize(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def section_map(text: str) -> dict[str, str]:
    matches = list(re.finditer(r"(?m)^##\s+(.+?)\s*$", text))
    sections: dict[str, str] = {}
    for index, match in enumerate(matches):
        header = match.group(1).strip()
        start = match.end()
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        sections[header] = text[start:end].strip()
    return sections


def extract_field(section_text: str, label: str) -> str | None:
    pattern = re.compile(
        rf"(?im)^\s*-\s*`?{re.escape(label)}`?\s*:\s*(.+?)\s*$"
    )
    match = pattern.search(section_text)
    return match.group(1).strip() if match else None


def clean_field_value(value: str | None) -> str | None:
    if value is None:
        return None
    cleaned = value.strip()
    cleaned = re.sub(r"^`+", "", cleaned)
    cleaned = re.sub(r"`+$", "", cleaned)
    return cleaned.strip()


def pick_choice(value: str | None, choices: tuple[str, ...]) -> str | None:
    cleaned = clean_field_value(value)
    if not cleaned:
        return None
    for choice in choices:
        if cleaned.lower().startswith(choice.lower()):
            return choice
    return None


def has_formula(metrics_section: str) -> bool:
    lines = [line.strip() for line in metrics_section.splitlines() if line.strip()]
    for line in lines:
        if "=" in line or "not reliably derivable" in line.lower():
            return True
    return False


def contains_any(text: str, phrases: tuple[str, ...]) -> bool:
    lower = text.lower()
    return any(phrase.lower() in lower for phrase in phrases)


def make_expectation(text: str, passed: bool, evidence: str) -> dict:
    return {
        "text": text,
        "passed": passed,
        "evidence": evidence,
    }


def grade_generic(output_text: str) -> tuple[list[dict], dict[str, str | None]]:
    sections = section_map(output_text)
    expectations: list[dict] = []

    headers_present = all(header in sections for header in REQUIRED_HEADERS)
    missing_headers = [header for header in REQUIRED_HEADERS if header not in sections]
    expectations.append(
        make_expectation(
            "The answer uses the required section structure with Request framing, Verified facts, Derived metrics, Inference / judgment, Scenario analysis, Timing / execution, Final recommendation, Sources, and Disclaimer.",
            headers_present,
            "All required headers present."
            if headers_present
            else f"Missing headers: {', '.join(missing_headers)}.",
        )
    )

    request_section = sections.get("Request framing", "")
    derived_section = sections.get("Derived metrics", "")
    scenario_section = sections.get("Scenario analysis", "")
    timing_section = sections.get("Timing / execution", "")
    final_section = sections.get("Final recommendation", "")
    sources_section = sections.get("Sources", "")
    disclaimer_section = sections.get("Disclaimer", "")

    rating_value = clean_field_value(extract_field(final_section, "Rating"))
    rating_choice = pick_choice(rating_value, RATINGS)
    entry_value = clean_field_value(extract_field(final_section, "Entry approach"))
    entry_choice = pick_choice(entry_value, ENTRY_APPROACHES)
    confidence_value = clean_field_value(extract_field(final_section, "Confidence"))
    confidence_choice = pick_choice(confidence_value, CONFIDENCE_LEVELS)
    current_action = clean_field_value(extract_field(timing_section, "Current action"))
    current_action_choice = pick_choice(current_action, ENTRY_APPROACHES)
    request_horizon = clean_field_value(extract_field(request_section, "Horizon"))
    final_horizon = clean_field_value(extract_field(final_section, "Horizon"))

    final_fields = {
        "Rating": rating_value,
        "Horizon": final_horizon,
        "Confidence": confidence_value,
        "Entry approach": entry_value,
        "Add criteria": clean_field_value(extract_field(final_section, "Add criteria")),
        "Invalidation criteria": clean_field_value(extract_field(final_section, "Invalidation criteria")),
        "Biggest near-term risk": clean_field_value(extract_field(final_section, "Biggest near-term risk")),
        "Biggest long-term strength": clean_field_value(extract_field(final_section, "Biggest long-term strength")),
    }
    missing_final_fields = [name for name, value in final_fields.items() if not value]
    expectations.append(
        make_expectation(
            "The answer includes a final recommendation block with Rating, Horizon, Confidence, Entry approach, Add criteria, Invalidation criteria, Biggest near-term risk, and Biggest long-term strength.",
            not missing_final_fields,
            "All final recommendation fields are present."
            if not missing_final_fields
            else f"Missing final recommendation fields: {', '.join(missing_final_fields)}.",
        )
    )

    rating_and_entry_ok = (
        rating_choice is not None
        and entry_choice is not None
        and current_action_choice is not None
        and current_action_choice == entry_choice
    )
    expectations.append(
        make_expectation(
            "The answer includes exactly one final rating from Buy, Wait, or Sell and one explicit entry approach from Buy now, Starter only, Wait, or Avoid for now.",
            rating_and_entry_ok,
            (
                f"Rating={rating_choice}, entry approach={entry_choice}, current action={current_action_choice}."
                if rating_and_entry_ok
                else f"Could not validate a consistent rating and action. Rating={rating_value!r}, entry={entry_value!r}, current action={current_action!r}."
            ),
        )
    )

    fact_metric_inference_ok = (
        "Verified facts" in sections
        and "Derived metrics" in sections
        and "Inference / judgment" in sections
        and has_formula(derived_section)
    )
    expectations.append(
        make_expectation(
            "The answer separates verified facts, derived metrics, and inference rather than collapsing them into one blended summary.",
            fact_metric_inference_ok,
            "Separate sections are present and the derived metrics section includes explicit arithmetic or an explicit non-derivable statement."
            if fact_metric_inference_ok
            else "The answer is missing one of the separation sections or the derived metrics section lacks explicit formulas.",
        )
    )

    scenario_ok = all(label.lower() in scenario_section.lower() for label in ("Bull case", "Base case", "Bear case"))
    expectations.append(
        make_expectation(
            "The answer includes Bull case, Base case, and Bear case.",
            scenario_ok,
            "Bull, Base, and Bear case lines are present."
            if scenario_ok
            else "The scenario analysis section does not include all of Bull case, Base case, and Bear case.",
        )
    )

    add_criteria = clean_field_value(extract_field(timing_section, "Add criteria")) or clean_field_value(extract_field(final_section, "Add criteria"))
    invalidation_criteria = clean_field_value(extract_field(timing_section, "Invalidation criteria")) or clean_field_value(extract_field(final_section, "Invalidation criteria"))
    timing_logic_ok = (
        current_action_choice is not None
        and clean_field_value(extract_field(timing_section, "Why now")) is not None
        and add_criteria is not None
        and invalidation_criteria is not None
    )
    expectations.append(
        make_expectation(
            "The answer includes explicit add criteria and invalidation criteria instead of only vague language.",
            timing_logic_ok,
            "Timing / execution includes current action, why now, add criteria, and invalidation criteria."
            if timing_logic_ok
            else "Timing / execution is missing the direct current action, why now, add criteria, or invalidation criteria.",
        )
    )

    sources_ok = len([line for line in sources_section.splitlines() if line.strip().startswith("-")]) >= 1
    expectations.append(
        make_expectation(
            "The answer names the public sources behind the main claims.",
            sources_ok,
            "Sources section lists public sources."
            if sources_ok
            else "Sources section is missing or empty.",
        )
    )

    disclaimer_ok = "public-information-based research only" in disclaimer_section.lower()
    expectations.append(
        make_expectation(
            "The answer includes the research-only disclaimer.",
            disclaimer_ok,
            "Disclaimer section contains the research-only disclaimer."
            if disclaimer_ok
            else "Disclaimer section is missing the required disclaimer text.",
        )
    )

    context = {
        "request_horizon": request_horizon,
        "final_horizon": final_horizon,
        "question_type": clean_field_value(extract_field(request_section, "Question type")),
        "confidence_choice": confidence_choice,
        "scenario_section": scenario_section,
        "timing_section": timing_section,
        "output_text": output_text,
    }

    return expectations, context


def grade_eval_specific(eval_id: int, output_text: str, context: dict[str, str | None]) -> list[dict]:
    lower = output_text.lower()
    expectations: list[dict] = []

    if eval_id == 1:
        inferred_horizon = contains_any(
            " ".join(filter(None, [context.get("request_horizon"), context.get("final_horizon")])),
            ("inferred",),
        )
        expectations.append(
            make_expectation(
                "Because the prompt does not state a horizon, the answer marks the horizon as inferred.",
                inferred_horizon,
                f"Horizon fields: request={context.get('request_horizon')!r}, final={context.get('final_horizon')!r}."
                if inferred_horizon
                else f"Horizon fields do not show an inferred marker: request={context.get('request_horizon')!r}, final={context.get('final_horizon')!r}.",
            )
        )

    if eval_id == 2:
        horizon_text = " ".join(filter(None, [context.get("request_horizon"), context.get("final_horizon")]))
        horizon_ok = contains_any(horizon_text, ("3-month", "3 month", "3 months", "medium-term"))
        expectations.append(
            make_expectation(
                "The answer recognizes the stated 3-month holding period in the horizon framing rather than inferring a generic long-term horizon.",
                horizon_ok,
                f"Horizon fields: request={context.get('request_horizon')!r}, final={context.get('final_horizon')!r}."
                if horizon_ok
                else f"Horizon fields do not reflect the stated 3-month holding period: request={context.get('request_horizon')!r}, final={context.get('final_horizon')!r}.",
            )
        )

        question_type = context.get("question_type") or ""
        question_type_ok = not question_type.lower().startswith("long-term accumulation")
        expectations.append(
            make_expectation(
                "The answer classifies the question type as medium-term investment, short-term trade, or event-driven timing rather than long-term accumulation.",
                question_type_ok and bool(question_type),
                f"Question type: {question_type!r}."
                if question_type_ok and question_type
                else f"Question type falls back to an inappropriate long-term label: {question_type!r}.",
            )
        )

        tactical_timing_ok = contains_any(
            (context.get("timing_section") or "") + "\n" + output_text,
            (
                "this week",
                "next few",
                "next several",
                "entry this week",
                "over the next",
                "3-month",
                "3 month",
                "window is short",
                "earnings and sentiment matter",
                "earnings reaction window",
            ),
        )
        expectations.append(
            make_expectation(
                "The answer includes explicit timing or execution logic for this week instead of only a broad long-term accumulation answer.",
                tactical_timing_ok,
                "The answer discusses the short tactical window or comparable near-term timing logic."
                if tactical_timing_ok
                else "The answer does not mention a short tactical window or comparable near-term timing logic.",
            )
        )

    if eval_id == 3:
        question_type = context.get("question_type") or ""
        expectations.append(
            make_expectation(
                "The answer classifies the question as event-driven timing.",
                question_type.lower().startswith("event-driven timing"),
                f"Question type: {question_type!r}."
                if question_type.lower().startswith("event-driven timing")
                else f"Question type does not classify the prompt as event-driven timing: {question_type!r}.",
            )
        )

        correction_keywords = (
            "estimated",
            "estimate",
            "not confirmed",
            "unconfirmed",
            "currently expected",
            "calendar",
            "consensus",
            "according to",
            "could not confirm",
            "has not officially confirmed",
            "scheduled for",
        )
        other_date_match = re.search(
            r"\b(?:jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\s+\d{1,2}\b",
            lower,
        )
        may_27_qualified = "may 27" in lower and contains_any(lower, correction_keywords)
        alternate_date = other_date_match is not None and other_date_match.group(0) != "may 27"
        premise_corrected = may_27_qualified or alternate_date or contains_any(lower, ("could not confirm", "unconfirmed"))
        expectations.append(
            make_expectation(
                "The answer does not uncritically accept the user's May 27 earnings-date premise. It either corrects the date with sourced conflicting information or explicitly marks the date as estimated or unconfirmed.",
                premise_corrected,
                "The output either qualifies May 27 as estimated or unconfirmed, or provides a different date."
                if premise_corrected
                else "The output does not clearly correct or qualify the user's May 27 earnings-date premise.",
            )
        )

        pre_earnings_timing_ok = contains_any(
            context.get("timing_section") or "",
            ("earnings", "pre-earnings", "before earnings", "into earnings"),
        )
        expectations.append(
            make_expectation(
                "The answer gives a direct pre-earnings timing view with one explicit entry approach from Buy now, Starter only, Wait, or Avoid for now.",
                pre_earnings_timing_ok,
                "Timing / execution explicitly addresses the pre-earnings setup."
                if pre_earnings_timing_ok
                else "Timing / execution does not clearly address the pre-earnings setup.",
            )
        )

    if eval_id == 4:
        confidence_choice = context.get("confidence_choice") or ""
        lowered_confidence = confidence_choice in {"Low", "Medium"}
        expectations.append(
            make_expectation(
                "The answer does not use High confidence for this weaker or messier case.",
                lowered_confidence,
                f"Confidence: {confidence_choice!r}."
                if lowered_confidence
                else f"Confidence is too high for the messy-case eval: {confidence_choice!r}.",
            )
        )

        explicit_uncertainty = contains_any(
            lower,
            (
                "uncertain",
                "mixed",
                "messy",
                "limited",
                "incomplete",
                "conflicting",
                "speculative",
                "not reliably derivable",
                "noisy",
                "liquidity remains",
                "funding risk",
                "dilution",
                "headline-driven",
                "headline-sensitive",
                "volatile",
            ),
        )
        expectations.append(
            make_expectation(
                "The answer explicitly acknowledges uncertainty, mixed evidence, missing inputs, or metrics that are not reliably derivable instead of pretending certainty.",
                explicit_uncertainty,
                "The output explicitly names uncertainty, mixed evidence, or missing inputs."
                if explicit_uncertainty
                else "The output does not explicitly acknowledge uncertainty, mixed evidence, or missing inputs.",
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
        "Do not modify repository files. Return only the final investment memo. "
        "Keep the research bounded: prefer one primary company or fund source, one reliable market-data source, and at most one additional public source if needed for timing or catalyst context. "
        "If a primary source is blocked or stale, say so and move on instead of exhaustively searching for more sources. "
        "Use the exact section headers and field names from the skill. Do not rename them or compress them into a custom short memo, even for tactical prompts. "
        "Prioritize memo quality, explicit rating and timing, and auditable metric presentation over exhaustive source hunting.\n\n"
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

    start = time.time()
    completed = run_codex(wrapped_prompt, output_path, transcript_path, reasoning_effort)
    duration_seconds = time.time() - start

    timing_payload = {
        "return_code": completed.returncode,
        "duration_ms": round(duration_seconds * 1000),
        "total_duration_seconds": round(duration_seconds, 2),
    }
    timing_path.write_text(json.dumps(timing_payload, indent=2))

    if completed.returncode == 0 and output_path.exists():
        output_text = output_path.read_text()
        grading = grade_output(eval_case["id"], output_text)
    else:
        failure_message = "Codex exec failed before producing output."
        if transcript_path.exists():
            failure_message = normalize(transcript_path.read_text())[:500] or failure_message
        grading = {
            "expectations": [
                make_expectation(
                    expectation,
                    False,
                    f"Execution failed: {failure_message}",
                )
                for expectation in eval_case["expectations"]
            ],
            "summary": {
                "passed": 0,
                "failed": len(eval_case["expectations"]),
                "total": len(eval_case["expectations"]),
                "pass_rate": 0.0,
            },
        }

    grading["timing"] = timing_payload
    grading_path.write_text(json.dumps(grading, indent=2))

    return {
        "eval_id": eval_case["id"],
        "eval_name": slug,
        "return_code": completed.returncode,
        "grading": grading["summary"],
        "output_path": str(output_path),
        "transcript_path": str(transcript_path),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--workspace-dir",
        default=str(default_workspace_dir()),
        help="Directory for regression artifacts",
    )
    parser.add_argument(
        "--iteration",
        default="iteration-1",
        help="Iteration directory name under the workspace",
    )
    parser.add_argument(
        "--eval-ids",
        nargs="*",
        type=int,
        default=None,
        help="Optional subset of eval ids to run",
    )
    parser.add_argument(
        "--reasoning-effort",
        default="high",
        choices=["low", "medium", "high"],
        help="Reasoning effort passed to codex exec",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    evals_by_id = load_evals()
    eval_ids = args.eval_ids or sorted(evals_by_id)
    workspace_dir = Path(args.workspace_dir)

    results = []
    for eval_id in eval_ids:
        if eval_id not in evals_by_id:
            print(f"Unknown eval id: {eval_id}", file=sys.stderr)
            return 1
        print(f"Running eval {eval_id}: {EVAL_SLUGS.get(eval_id, f'eval-{eval_id}')}", file=sys.stderr)
        result = run_eval_case(
            eval_case=evals_by_id[eval_id],
            workspace_dir=workspace_dir,
            iteration=args.iteration,
            reasoning_effort=args.reasoning_effort,
        )
        print(
            f"  return_code={result['return_code']} pass_rate={result['grading']['pass_rate']}",
            file=sys.stderr,
        )
        results.append(result)

    overall_passed = sum(item["grading"]["passed"] for item in results)
    overall_total = sum(item["grading"]["total"] for item in results)
    summary = {
        "iteration": args.iteration,
        "workspace_dir": str(workspace_dir),
        "results": results,
        "overall": {
            "passed": overall_passed,
            "failed": overall_total - overall_passed,
            "total": overall_total,
            "pass_rate": round(overall_passed / overall_total, 4) if overall_total else 0.0,
        },
    }

    summary_path = workspace_dir / args.iteration / "summary.json"
    summary_path.parent.mkdir(parents=True, exist_ok=True)
    summary_path.write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
