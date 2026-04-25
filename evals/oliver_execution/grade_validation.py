#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path

from grader import (
    REPO_ROOT,
    aggregate_scorecard,
    build_report,
    load_fixture_specs,
    load_stage_debug_payload,
    load_stage_run_error,
    score_stage,
    write_review_sheet,
)


DEFAULT_ARTIFACT_ROOT = REPO_ROOT / "artifacts" / "oliver_execution" / "latest"
DEFAULT_REPORT_PATH = REPO_ROOT / "docs" / "oliver-validation-report-v1.md"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture-ids", nargs="*")
    parser.add_argument("--artifact-root", type=Path, default=DEFAULT_ARTIFACT_ROOT)
    parser.add_argument("--report-path", type=Path, default=DEFAULT_REPORT_PATH)
    return parser.parse_args()


def write_json(path: Path, payload: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2))


def main() -> int:
    args = parse_args()
    selected_ids = set(args.fixture_ids or []) or None
    fixtures = load_fixture_specs(selected_ids)
    results: list[dict[str, object]] = []

    for fixture in fixtures:
        for stage_name in fixture.get("stages", {}).keys():
            artifact_dir = args.artifact_root / fixture["fixture_id"] / stage_name
            debug_payload = load_stage_debug_payload(
                args.artifact_root, fixture["fixture_id"], stage_name
            )
            run_error = load_stage_run_error(
                args.artifact_root, fixture["fixture_id"], stage_name
            )
            grade = score_stage(fixture, stage_name, debug_payload, run_error)
            write_json(artifact_dir / "grade.json", grade)
            write_review_sheet(fixture, stage_name, grade, artifact_dir)
            results.append(
                {
                    "fixture": fixture,
                    "fixture_id": fixture["fixture_id"],
                    "stage_name": stage_name,
                    "grade": grade,
                }
            )

    summary = aggregate_scorecard(results)
    write_json(args.artifact_root / "grades_summary.json", summary)
    write_json(args.artifact_root / "grades_results.json", results)

    report = build_report(results)
    args.report_path.write_text(report)
    (args.artifact_root / "validation_report.md").write_text(report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
