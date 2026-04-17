#!/usr/bin/env python3

"""CLI wrapper for Alice Step 6 extraction and research assembly."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from alice_assemble_research import assemble_land_research, summarize_land_research
from alice_extract import extract_evidence
from alice_fetch import load_source_fetch_log
from alice_registry import load_json
from alice_subject_resolution import load_request


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--request", required=True, help="Path to request_normalized JSON.")
    parser.add_argument("--subject-resolution", required=True, help="Path to subject_resolution JSON.")
    parser.add_argument("--jurisdiction-context", required=True, help="Path to jurisdiction_context JSON.")
    parser.add_argument("--coverage-assessment", required=True, help="Path to coverage_assessment JSON.")
    parser.add_argument("--source-plan", required=True, help="Path to source_plan JSON.")
    parser.add_argument("--source-fetch-log", required=True, help="Path to source_fetch_log JSON.")
    parser.add_argument("--workspace-root", required=True, help="Workspace root containing alice/ raw evidence.")
    parser.add_argument(
        "--generated-at",
        help="Optional fixed timestamp for deterministic example generation.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    request = load_request(args.request)
    subject_resolution = load_json(Path(args.subject_resolution))
    jurisdiction_context = load_json(Path(args.jurisdiction_context))
    coverage_assessment = load_json(Path(args.coverage_assessment))
    source_plan = load_json(Path(args.source_plan))
    fetch_log = load_source_fetch_log(args.source_fetch_log)
    extracted_evidence = extract_evidence(
        fetch_log,
        workspace_root=args.workspace_root,
        request_id=request["request_id"],
        subject_resolution_id=subject_resolution["resolution_id"],
        generated_at=args.generated_at,
    )
    parcel_memo = assemble_land_research(
        request,
        subject_resolution,
        jurisdiction_context,
        coverage_assessment,
        source_plan,
        fetch_log,
        extracted_evidence,
        workspace_root=args.workspace_root,
        generated_at=args.generated_at,
    )
    print(json.dumps(summarize_land_research(parcel_memo), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
