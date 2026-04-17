#!/usr/bin/env python3

"""CLI helpers for Alice parcel-candidate synthesis."""

from __future__ import annotations

import argparse
import json

from alice_extract import load_extracted_evidence
from alice_fetch import load_source_fetch_log
from alice_jurisdiction import load_jurisdiction_context, load_subject_resolution
from alice_parcel_candidates import (
    build_parcel_candidates,
    load_parcel_candidates,
    summarize_parcel_candidates,
)
from alice_subject_resolution import load_request


def emit_json(payload: object, compact: bool) -> None:
    if compact:
        print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
    else:
        print(json.dumps(payload, ensure_ascii=False, indent=2))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    build_parser = subparsers.add_parser("build", help="Build parcel candidates from Alice Step 4-6 artifacts.")
    build_parser.add_argument("request_path")
    build_parser.add_argument("subject_resolution_path")
    build_parser.add_argument("jurisdiction_context_path")
    build_parser.add_argument("source_fetch_log_path")
    build_parser.add_argument("extracted_evidence_path")
    build_parser.add_argument("--workspace-root", default=".")
    build_parser.add_argument("--summary", action="store_true")
    build_parser.add_argument("--compact", action="store_true")

    summary_parser = subparsers.add_parser("summary", help="Summarize an existing parcel-candidates artifact.")
    summary_parser.add_argument("parcel_candidates_path")
    summary_parser.add_argument("--compact", action="store_true")

    args = parser.parse_args()

    if args.command == "summary":
        artifact = load_parcel_candidates(args.parcel_candidates_path)
        emit_json(summarize_parcel_candidates(artifact), args.compact)
        return 0

    request = load_request(args.request_path)
    subject_resolution = load_subject_resolution(args.subject_resolution_path)
    jurisdiction_context = load_jurisdiction_context(args.jurisdiction_context_path)
    fetch_log = load_source_fetch_log(args.source_fetch_log_path)
    extracted_evidence = load_extracted_evidence(args.extracted_evidence_path)
    artifact = build_parcel_candidates(
        request,
        subject_resolution,
        jurisdiction_context,
        fetch_log,
        extracted_evidence,
        workspace_root=args.workspace_root,
    )
    if args.summary:
        emit_json(summarize_parcel_candidates(artifact), args.compact)
    else:
        emit_json(artifact, args.compact)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
