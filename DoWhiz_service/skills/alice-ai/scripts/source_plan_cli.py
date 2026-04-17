#!/usr/bin/env python3

"""CLI helpers for Alice Step 5 source planning."""

from __future__ import annotations

import argparse
import json

from alice_coverage_assessment import build_coverage_assessment, load_jurisdiction_context
from alice_jurisdiction import build_jurisdiction_context, load_subject_resolution
from alice_source_plan import build_source_plan, load_coverage_assessment, summarize_source_plan
from alice_subject_resolution import load_request, load_session_state


def emit_json(payload: object, compact: bool) -> None:
    if compact:
        print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
    else:
        print(json.dumps(payload, ensure_ascii=False, indent=2))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("request_path")
    parser.add_argument("subject_resolution_path")
    parser.add_argument("--session-state")
    parser.add_argument("--jurisdiction-context")
    parser.add_argument("--coverage-assessment")
    parser.add_argument("--summary", action="store_true")
    parser.add_argument("--compact", action="store_true")
    args = parser.parse_args()

    request = load_request(args.request_path)
    subject_resolution = load_subject_resolution(args.subject_resolution_path)
    session_state = load_session_state(args.session_state) if args.session_state else None

    if args.jurisdiction_context:
        jurisdiction_context = load_jurisdiction_context(args.jurisdiction_context)
    else:
        jurisdiction_context = build_jurisdiction_context(request, subject_resolution, session_state)

    if args.coverage_assessment:
        coverage_assessment = load_coverage_assessment(args.coverage_assessment)
    else:
        coverage_assessment = build_coverage_assessment(request, subject_resolution, jurisdiction_context)

    plan = build_source_plan(request, subject_resolution, jurisdiction_context, coverage_assessment)
    emit_json(summarize_source_plan(plan) if args.summary else plan, args.compact)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
