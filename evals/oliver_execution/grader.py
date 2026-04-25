from __future__ import annotations

import json
import re
from collections import Counter, defaultdict
from difflib import SequenceMatcher
from pathlib import Path
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
FIXTURES_ROOT = SCRIPT_DIR / "fixtures"
REPO_ROOT = SCRIPT_DIR.parents[1]


def load_fixture_specs(selected_ids: set[str] | None = None) -> list[dict[str, Any]]:
    fixtures: list[dict[str, Any]] = []
    for ground_truth_path in sorted(FIXTURES_ROOT.glob("*/ground_truth.json")):
        payload = json.loads(ground_truth_path.read_text())
        payload["_fixture_dir"] = str(ground_truth_path.parent)
        if selected_ids and payload["fixture_id"] not in selected_ids:
            continue
        fixtures.append(payload)
    return fixtures


def normalize_text(value: str | None) -> str:
    text = (value or "").strip().lower()
    text = re.sub(r"\b[lu]\d+\b", " ", text)
    text = re.sub(r"[^a-z0-9]+", " ", text)
    return " ".join(text.split())


def similarity(left: str | None, right: str | None) -> float:
    return SequenceMatcher(None, normalize_text(left), normalize_text(right)).ratio()


def dedupe_strings(values: list[str]) -> list[str]:
    output: list[str] = []
    seen: set[str] = set()
    for value in values:
        key = normalize_text(value)
        if not key or key in seen:
            continue
        seen.add(key)
        output.append(value.strip())
    return output


def match_lists(
    expected: list[str],
    predicted: list[str],
    *,
    threshold: float,
) -> dict[str, Any]:
    expected_values = dedupe_strings(expected)
    predicted_values = dedupe_strings(predicted)
    remaining = predicted_values[:]
    matches: list[dict[str, Any]] = []
    missed: list[str] = []

    for expected_item in expected_values:
        best_index = -1
        best_score = -1.0
        for index, predicted_item in enumerate(remaining):
            score = similarity(expected_item, predicted_item)
            if score > best_score:
                best_index = index
                best_score = score
        if best_index >= 0 and best_score >= threshold:
            predicted_item = remaining.pop(best_index)
            matches.append(
                {
                    "expected": expected_item,
                    "predicted": predicted_item,
                    "score": round(best_score, 3),
                }
            )
        else:
            missed.append(expected_item)

    unexpected = remaining
    precision = len(matches) / len(predicted_values) if predicted_values else (1.0 if not expected_values else 0.0)
    recall = len(matches) / len(expected_values) if expected_values else (1.0 if not predicted_values else 0.0)
    f1 = 0.0 if precision + recall == 0 else (2 * precision * recall) / (precision + recall)
    return {
        "expected": expected_values,
        "predicted": predicted_values,
        "matches": matches,
        "missed": missed,
        "unexpected": unexpected,
        "precision": round(precision, 3),
        "recall": round(recall, 3),
        "f1": round(f1, 3),
    }


def collect_prediction_fields(debug_payload: dict[str, Any]) -> dict[str, Any]:
    response = debug_payload["response"]
    plan = response["launch_execution_plan"]
    brief = response["readiness_brief"]

    owner_candidates: list[str] = []
    for owner in plan.get("owners", []):
        owner_candidates.append(owner.get("name", ""))
    for collection_name in ["milestones", "dependencies", "risks", "decisions", "critical_path"]:
        for item in plan.get(collection_name, []):
            owner_candidates.append(item.get("owner", ""))

    blockers: list[str] = list(brief.get("critical_blockers", []))
    for risk in plan.get("risks", []):
        if risk.get("blocker") and risk.get("status") not in {"resolved", "mitigated"}:
            blockers.append(risk.get("title", ""))
    for dependency in plan.get("dependencies", []):
        if dependency.get("status") == "blocked":
            blockers.append(dependency.get("title", ""))

    decisions: list[str] = list(brief.get("open_decisions", []))
    for decision in plan.get("decisions", []):
        if decision.get("status") != "resolved":
            decisions.append(decision.get("title", ""))

    timeline_markers: list[str] = []
    for value in [plan.get("target_date"), plan.get("launch_window")]:
        if value:
            timeline_markers.append(value)
    for collection_name in ["milestones", "dependencies", "decisions", "critical_path"]:
        field_name = "due_date" if collection_name == "decisions" else "target_date"
        for item in plan.get(collection_name, []):
            value = item.get(field_name)
            if value:
                timeline_markers.append(value)

    structured_count = (
        len(plan.get("milestones", []))
        + len(plan.get("dependencies", []))
        + len(plan.get("risks", []))
        + len(plan.get("decisions", []))
        + len([name for name in owner_candidates if normalize_text(name)])
    )

    return {
        "goal": plan.get("objective") or plan.get("title") or "",
        "owners": dedupe_strings(owner_candidates),
        "timeline_markers": dedupe_strings(timeline_markers),
        "dependencies": dedupe_strings([item.get("title", "") for item in plan.get("dependencies", [])]),
        "blockers": dedupe_strings(blockers),
        "unresolved_decisions": dedupe_strings(decisions),
        "follow_up_targets": dedupe_strings(
            [item.get("target", "") for item in plan.get("follow_up_items", [])]
        ),
        "follow_up_messages": [
            item.get("suggested_message", "") for item in plan.get("follow_up_items", [])
        ],
        "evidence_snippets": dedupe_strings(
            [item.get("snippet", "") for item in plan.get("evidence", [])]
        ),
        "readiness_status": (plan.get("readiness_status") or "").strip().lower(),
        "readiness_reason": (plan.get("readiness_reason") or "").strip(),
        "structured_count": structured_count,
        "plan_title": plan.get("title", ""),
        "plan": plan,
        "brief": brief,
    }


def stage_artifact_path(artifact_root: Path, fixture_id: str, stage_name: str) -> Path:
    return artifact_root / fixture_id / stage_name / "debug_response.json"


def load_stage_debug_payload(
    artifact_root: Path, fixture_id: str, stage_name: str
) -> dict[str, Any] | None:
    artifact_path = stage_artifact_path(artifact_root, fixture_id, stage_name)
    if not artifact_path.exists():
        return None
    return json.loads(artifact_path.read_text())


def load_stage_run_error(
    artifact_root: Path, fixture_id: str, stage_name: str
) -> dict[str, Any] | None:
    error_path = artifact_root / fixture_id / stage_name / "run_error.json"
    if not error_path.exists():
        return None
    return json.loads(error_path.read_text())


def summarize_run_error(run_error: dict[str, Any] | None) -> str:
    if not run_error:
        return "Run artifact missing for this stage."
    stderr = (run_error.get("stderr") or "").strip()
    if "operation timed out" in stderr:
        return "Analyzer timed out before returning a result."
    if "not valid JSON" in stderr:
        return "Analyzer returned non-JSON output."
    if "empty response" in stderr:
        return "Analyzer returned an empty response."
    if "refresh skipped because initial stage failed" in stderr:
        return "Refresh stage was skipped because the initial stage failed."
    return stderr.splitlines()[0] if stderr else "Run artifact missing for this stage."


def score_stage(
    fixture: dict[str, Any],
    stage_name: str,
    debug_payload: dict[str, Any] | None,
    run_error: dict[str, Any] | None = None,
) -> dict[str, Any]:
    stage_truth = fixture["stages"][stage_name]
    behavioral = stage_truth.get("behavioral_expectations", {})

    if debug_payload is None:
        blocked_reason = summarize_run_error(run_error)
        return {
            "stage": stage_name,
            "status": "blocked",
            "blocked_reason": blocked_reason,
            "failure_modes": [blocked_reason],
            "deterministic_checks": [],
            "field_scores": {},
            "field_details": {},
            "overall_score": 0.0,
            "passed": False,
        }

    prediction = collect_prediction_fields(debug_payload)
    plan = prediction["plan"]

    deterministic_checks = [
        {
            "name": "title_present",
            "passed": bool(prediction["plan_title"].strip()),
            "detail": prediction["plan_title"] or "missing",
        },
        {
            "name": "readiness_present",
            "passed": bool(prediction["readiness_status"]),
            "detail": prediction["readiness_status"] or "missing",
        },
        {
            "name": "readiness_reason_present",
            "passed": bool(prediction["readiness_reason"]),
            "detail": prediction["readiness_reason"] or "missing",
        },
        {
            "name": "evidence_present",
            "passed": bool(prediction["evidence_snippets"]),
            "detail": f"{len(prediction['evidence_snippets'])} evidence snippets",
        },
        {
            "name": "evidence_refs_present",
            "passed": all(
                item.get("snippet") and item.get("source_ref")
                for item in plan.get("evidence", [])
            ),
            "detail": f"{len(plan.get('evidence', []))} evidence items",
        },
    ]

    if behavioral.get("max_structured_items_when_weak_signal") is not None:
        limit = behavioral["max_structured_items_when_weak_signal"]
        deterministic_checks.append(
            {
                "name": "weak_signal_sparse_structure",
                "passed": prediction["structured_count"] <= limit,
                "detail": f"{prediction['structured_count']} structured items vs limit {limit}",
            }
        )

    if not behavioral.get("allow_follow_up_drafts", True):
        deterministic_checks.append(
            {
                "name": "no_unjustified_follow_up_drafts",
                "passed": not prediction["follow_up_targets"],
                "detail": f"{len(prediction['follow_up_targets'])} follow-up targets",
            }
        )
    elif stage_truth.get("expected_follow_up_targets"):
        deterministic_checks.append(
            {
                "name": "follow_up_drafts_present_when_expected",
                "passed": bool(prediction["follow_up_targets"]),
                "detail": f"{len(prediction['follow_up_targets'])} follow-up targets",
            }
        )

    field_details = {
        "goal": {
            "expected": stage_truth.get("expected_goal", ""),
            "predicted": prediction["goal"],
            "score": round(similarity(stage_truth.get("expected_goal", ""), prediction["goal"]), 3),
        },
        "owners": match_lists(
            stage_truth.get("expected_owners", []),
            prediction["owners"],
            threshold=0.88,
        ),
        "timeline": match_lists(
            stage_truth.get("expected_timeline_markers", []),
            prediction["timeline_markers"],
            threshold=0.68,
        ),
        "dependencies": match_lists(
            stage_truth.get("expected_dependencies", []),
            prediction["dependencies"],
            threshold=0.67,
        ),
        "blockers": match_lists(
            stage_truth.get("expected_blockers", []),
            prediction["blockers"],
            threshold=0.67,
        ),
        "decisions": match_lists(
            stage_truth.get("expected_unresolved_decisions", []),
            prediction["unresolved_decisions"],
            threshold=0.67,
        ),
        "follow_up_targets": match_lists(
            stage_truth.get("expected_follow_up_targets", []),
            prediction["follow_up_targets"],
            threshold=0.62,
        ),
        "evidence": match_lists(
            [item["snippet"] for item in stage_truth.get("required_evidence_anchors", [])],
            prediction["evidence_snippets"],
            threshold=0.55,
        ),
        "readiness": {
            "expected": stage_truth.get("expected_readiness_status", ""),
            "predicted": prediction["readiness_status"],
            "score": 1.0
            if normalize_text(stage_truth.get("expected_readiness_status", ""))
            == normalize_text(prediction["readiness_status"])
            else 0.0,
        },
    }

    field_scores = {
        "goal": field_details["goal"]["score"],
        "owners": field_details["owners"]["f1"],
        "timeline": field_details["timeline"]["f1"],
        "dependencies": field_details["dependencies"]["f1"],
        "blockers": field_details["blockers"]["f1"],
        "decisions": field_details["decisions"]["f1"],
        "follow_up_targets": field_details["follow_up_targets"]["f1"],
        "evidence": field_details["evidence"]["f1"],
        "readiness": field_details["readiness"]["score"],
    }

    deterministic_pass_rate = (
        sum(1 for item in deterministic_checks if item["passed"]) / len(deterministic_checks)
        if deterministic_checks
        else 0.0
    )
    field_average = sum(field_scores.values()) / len(field_scores)
    overall_score = round((0.4 * deterministic_pass_rate) + (0.6 * field_average), 3)

    failure_modes: list[str] = []
    if field_scores["readiness"] < 1.0:
        failure_modes.append("Readiness judgment did not match the grounded expectation.")
    if field_scores["owners"] < 0.7:
        failure_modes.append("Owner extraction was incomplete or hallucinated.")
    if field_scores["timeline"] < 0.7:
        failure_modes.append("Dates or launch windows were missing, wrong, or overly certain.")
    if field_scores["dependencies"] < 0.7:
        failure_modes.append("Dependencies were incomplete or conflated with blockers.")
    if field_scores["blockers"] < 0.7:
        failure_modes.append("Real blockers were missed or non-blockers were promoted.")
    if field_scores["decisions"] < 0.7:
        failure_modes.append("Unresolved decisions were missed or misclassified.")
    if field_scores["follow_up_targets"] < 0.7 and stage_truth.get("expected_follow_up_targets"):
        failure_modes.append("Follow-up targets were weak or aimed at the wrong thing.")
    if field_scores["evidence"] < 0.7:
        failure_modes.append("Evidence grounding was thin or did not point to the right facts.")
    for check in deterministic_checks:
        if not check["passed"] and check["name"] == "weak_signal_sparse_structure":
            failure_modes.append("Weak-signal thread was over-structured instead of treated as uncertain.")
        if not check["passed"] and check["name"] == "no_unjustified_follow_up_drafts":
            failure_modes.append("Follow-up drafts were generated even though the thread did not justify them.")
        if not check["passed"] and check["name"] == "evidence_refs_present":
            failure_modes.append("Evidence items lacked source references.")

    core_fields_ok = (
        field_scores["readiness"] == 1.0
        and field_scores["owners"] >= 0.6
        and field_scores["blockers"] >= 0.6
        and field_scores["evidence"] >= 0.6
    )
    deterministic_ok = all(item["passed"] for item in deterministic_checks)
    passed = deterministic_ok and core_fields_ok and overall_score >= 0.68

    return {
        "stage": stage_name,
        "status": "graded",
        "passed": passed,
        "overall_score": overall_score,
        "deterministic_pass_rate": round(deterministic_pass_rate, 3),
        "field_average": round(field_average, 3),
        "deterministic_checks": deterministic_checks,
        "field_scores": field_scores,
        "field_details": field_details,
        "failure_modes": dedupe_strings(failure_modes),
        "uncertainty_should_remain_explicit": stage_truth.get(
            "uncertainty_that_should_remain_explicit", []
        ),
        "prediction_snapshot": {
            "goal": prediction["goal"],
            "owners": prediction["owners"],
            "timeline_markers": prediction["timeline_markers"],
            "blockers": prediction["blockers"],
            "unresolved_decisions": prediction["unresolved_decisions"],
            "follow_up_targets": prediction["follow_up_targets"],
            "readiness_status": prediction["readiness_status"],
            "readiness_reason": prediction["readiness_reason"],
        },
    }


def write_review_sheet(
    fixture: dict[str, Any],
    stage_name: str,
    grade: dict[str, Any],
    artifact_dir: Path,
) -> None:
    stage_truth = fixture["stages"][stage_name]
    review_path = artifact_dir / "review.md"
    auto_result = "PASS" if grade.get("passed") else grade.get("status", "blocked").upper()
    snapshot = grade.get("prediction_snapshot", {})
    lines = [
        f"# Review Sheet: {fixture['title']} ({stage_name})",
        "",
        f"- Fixture ID: `{fixture['fixture_id']}`",
        f"- Provenance: `{fixture['provenance']}`",
        f"- Categories: {', '.join(fixture.get('categories', []))}",
        f"- Signal profile: `{fixture.get('signal_profile', 'unknown')}`",
        f"- Auto result: `{auto_result}`",
        f"- Auto overall score: `{grade.get('overall_score', 0.0)}`",
        "",
        "## Expected Behavior",
        "",
        f"- Goal: {stage_truth.get('expected_goal', 'n/a')}",
        f"- Expected readiness: `{stage_truth.get('expected_readiness_status', 'n/a')}`",
        f"- Expected owners: {', '.join(stage_truth.get('expected_owners', [])) or 'none'}",
        f"- Expected blockers: {', '.join(stage_truth.get('expected_blockers', [])) or 'none'}",
        f"- Expected decisions: {', '.join(stage_truth.get('expected_unresolved_decisions', [])) or 'none'}",
        f"- Expected follow-up targets: {', '.join(stage_truth.get('expected_follow_up_targets', [])) or 'none'}",
        "",
        "## Actual Snapshot",
        "",
        f"- Readiness: `{snapshot.get('readiness_status', 'missing')}`",
        f"- Readiness reason: {snapshot.get('readiness_reason', 'missing')}",
        f"- Owners: {', '.join(snapshot.get('owners', [])) or 'none'}",
        f"- Timeline markers: {', '.join(snapshot.get('timeline_markers', [])) or 'none'}",
        f"- Blockers: {', '.join(snapshot.get('blockers', [])) or 'none'}",
        f"- Open decisions: {', '.join(snapshot.get('unresolved_decisions', [])) or 'none'}",
        f"- Follow-up targets: {', '.join(snapshot.get('follow_up_targets', [])) or 'none'}",
        "",
        "## Auto Failure Modes",
        "",
    ]
    if grade.get("failure_modes"):
        lines.extend([f"- {item}" for item in grade["failure_modes"]])
    else:
        lines.append("- No major offline failure mode flagged.")
    lines.extend(
        [
            "",
            "## Human Review Questions",
            "",
            "- [ ] Did Oliver identify the real execution structure of the thread?",
            "- [ ] Did Oliver hallucinate owners, dates, blockers, or decisions?",
            "- [ ] Is the readiness judgment believable based on the thread evidence?",
            "- [ ] Are the follow-up drafts useful enough to approve with light edits?",
            "- [ ] Did Oliver surface something operationally important that a summary-only tool might miss?",
            "- [ ] Did Oliver preserve uncertainty where the thread stayed incomplete or conflicted?",
            "",
            "## Reviewer Notes",
            "",
            "- Trust this brief?",
            "- Approve/send any draft follow-up?",
            "- Would you give Oliver a second thread after seeing this output?",
            "- What felt wrong, noisy, or overconfident?",
            "",
        ]
    )
    review_path.write_text("\n".join(lines))


def build_report(results: list[dict[str, Any]]) -> str:
    total_stages = len(results)
    passed = [item for item in results if item["grade"].get("passed")]
    failed = [item for item in results if not item["grade"].get("passed")]
    scored = [item for item in results if item["grade"].get("status") == "graded"]

    dataset_counter = Counter()
    category_counter = Counter()
    provenance_counter = Counter()
    signal_counter = Counter()
    failure_mode_counter = Counter()
    for item in results:
        fixture = item["fixture"]
        dataset_counter["fixtures"] += 1 if item["stage_name"] == "initial" else 0
        provenance_counter[fixture["provenance"]] += 1 if item["stage_name"] == "initial" else 0
        signal_counter[fixture.get("signal_profile", "unknown")] += 1 if item["stage_name"] == "initial" else 0
        if item["stage_name"] == "initial":
            category_counter.update(fixture.get("categories", []))
        for failure_mode in item["grade"].get("failure_modes", []):
            failure_mode_counter[failure_mode] += 1

    if scored:
        avg_overall = sum(item["grade"]["overall_score"] for item in scored) / len(scored)
        readiness_accuracy = sum(
            item["grade"]["field_scores"]["readiness"] for item in scored
        ) / len(scored)
        owners_avg = sum(item["grade"]["field_scores"]["owners"] for item in scored) / len(scored)
        blockers_avg = sum(item["grade"]["field_scores"]["blockers"] for item in scored) / len(scored)
        followups_avg = sum(
            item["grade"]["field_scores"]["follow_up_targets"] for item in scored
        ) / len(scored)
        evidence_avg = sum(item["grade"]["field_scores"]["evidence"] for item in scored) / len(scored)
    else:
        avg_overall = readiness_accuracy = owners_avg = blockers_avg = followups_avg = evidence_avg = 0.0

    best_examples = sorted(
        scored,
        key=lambda item: item["grade"]["overall_score"],
        reverse=True,
    )[:2]
    worst_examples = sorted(
        scored,
        key=lambda item: item["grade"]["overall_score"],
    )[:2]

    pass_rate = (len(passed) / total_stages) if total_stages else 0.0
    recommendation = (
        "Ready for limited concierge testing"
        if pass_rate >= 0.4
        and avg_overall >= 0.75
        and readiness_accuracy >= 0.85
        and followups_avg >= 0.6
        and evidence_avg >= 0.6
        else "Not yet ready for concierge testing"
    )

    lines = [
        "# Oliver Validation Report V1",
        "",
        "This report summarizes the offline validation pass for the current Oliver v1 launch-execution workflow.",
        "",
        "Synthetic and repo-derived fixtures are useful for surfacing extraction and grounding failures, but they are not a substitute for live human trials.",
        "",
        "## Dataset Overview",
        "",
        f"- Initial fixtures: {sum(1 for item in results if item['stage_name'] == 'initial')}",
        f"- Total evaluated stages: {total_stages}",
        f"- Provenance mix: {', '.join(f'{key}={value}' for key, value in sorted(provenance_counter.items()))}",
        f"- Signal mix: {', '.join(f'{key}={value}' for key, value in sorted(signal_counter.items()))}",
        f"- Top categories: {', '.join(f'{key}={value}' for key, value in category_counter.most_common(8))}",
        "",
        "## Aggregate Scores",
        "",
        f"- Pass rate: {len(passed)}/{total_stages}",
        f"- Blocked stages: {sum(1 for item in results if item['grade'].get('status') == 'blocked')}",
        f"- Average overall score: {avg_overall:.3f}",
        f"- Readiness accuracy: {readiness_accuracy:.3f}",
        f"- Owner extraction average: {owners_avg:.3f}",
        f"- Blocker extraction average: {blockers_avg:.3f}",
        f"- Follow-up target average: {followups_avg:.3f}",
        f"- Evidence grounding average: {evidence_avg:.3f}",
        "",
        "## Per-Case Summary",
        "",
    ]

    for item in results:
        fixture = item["fixture"]
        grade = item["grade"]
        status = "PASS" if grade.get("passed") else grade.get("status", "FAIL").upper()
        lines.append(
            f"- `{fixture['fixture_id']}:{item['stage_name']}` -> {status}, overall={grade.get('overall_score', 0.0):.3f}, readiness={grade.get('field_scores', {}).get('readiness', 0.0):.3f}"
        )

    lines.extend(
        [
            "",
            "## Top Recurring Failure Modes",
            "",
        ]
    )
    if failure_mode_counter:
        for failure_mode, count in failure_mode_counter.most_common(5):
            lines.append(f"- {failure_mode} ({count} stage(s))")
    else:
        lines.append("- No recurring failure mode was recorded in this run.")

    lines.extend(
        [
            "",
            "## Good Output Examples",
            "",
        ]
    )
    if best_examples:
        for item in best_examples:
            snapshot = item["grade"].get("prediction_snapshot", {})
            lines.append(
                f"- `{item['fixture']['fixture_id']}:{item['stage_name']}` scored {item['grade']['overall_score']:.3f}. Readiness `{snapshot.get('readiness_status', 'missing')}` with blockers `{', '.join(snapshot.get('blockers', [])[:2]) or 'none'}` and follow-up targets `{', '.join(snapshot.get('follow_up_targets', [])[:2]) or 'none'}`."
            )
    else:
        lines.append("- No strong examples were available.")

    lines.extend(
        [
            "",
            "## Bad Output Examples",
            "",
        ]
    )
    if worst_examples:
        for item in worst_examples:
            lines.append(
                f"- `{item['fixture']['fixture_id']}:{item['stage_name']}` scored {item['grade']['overall_score']:.3f}. Main issues: {', '.join(item['grade'].get('failure_modes', [])[:3]) or 'n/a'}."
            )
    else:
        lines.append("- No weak examples were available.")

    lines.extend(
        [
            "",
            "## Recommendation",
            "",
            f"- Offline recommendation: **{recommendation}**",
            "- Approval/send workflows should stay off until live reviewers confirm the readiness brief is trustworthy and the drafted follow-ups are approval-worthy.",
            "",
            "## What To Fix Before Approval/Send Workflows",
            "",
        ]
    )
    if failure_mode_counter:
        for failure_mode, _count in failure_mode_counter.most_common(5):
            lines.append(f"- {failure_mode}")
    else:
        lines.append("- No blocking offline issue surfaced, but live trust testing is still required.")

    lines.extend(
        [
            "",
            "## Offline vs Live Validation Limits",
            "",
            "- Validated offline here: repeated extraction quality, readiness alignment, evidence grounding, weak-signal handling, and follow-up target quality proxies.",
            "- Not validated offline here: whether PMs trust the brief in a live workflow, whether they would approve/send the drafts, and whether the product creates enough confidence to hand it a second thread.",
        ]
    )

    return "\n".join(lines) + "\n"


def summarize_failure_modes(results: list[dict[str, Any]]) -> dict[str, int]:
    counter: Counter[str] = Counter()
    for item in results:
        for failure_mode in item["grade"].get("failure_modes", []):
            counter[failure_mode] += 1
    return dict(counter)


def aggregate_scorecard(results: list[dict[str, Any]]) -> dict[str, Any]:
    by_stage: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for item in results:
        by_stage[item["stage_name"]].append(item)
    return {
        "total_results": len(results),
        "passed": sum(1 for item in results if item["grade"].get("passed")),
        "failed": sum(1 for item in results if not item["grade"].get("passed")),
        "failure_modes": summarize_failure_modes(results),
        "stage_breakdown": {
            stage_name: {
                "count": len(items),
                "pass_count": sum(1 for item in items if item["grade"].get("passed")),
            }
            for stage_name, items in by_stage.items()
        },
    }
