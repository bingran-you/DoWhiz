#!/usr/bin/env python3

"""CLI wrapper for Alice Step 6 source fetching."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from alice_fetch import FixtureTransport, LiveTransport, execute_source_plan, summarize_fetch_log
from alice_registry import load_json
from alice_subject_resolution import load_request


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--request", required=True, help="Path to request_normalized JSON.")
    parser.add_argument("--subject-resolution", required=True, help="Path to subject_resolution JSON.")
    parser.add_argument("--jurisdiction-context", required=True, help="Path to jurisdiction_context JSON.")
    parser.add_argument("--source-plan", required=True, help="Path to source_plan JSON.")
    parser.add_argument("--workspace-root", required=True, help="Workspace root where alice/ artifacts will be written.")
    parser.add_argument(
        "--fixture-manifest",
        help="Optional fixture manifest for deterministic fetch replay instead of live HTTP.",
    )
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
    source_plan = load_json(Path(args.source_plan))
    transport = (
        FixtureTransport(args.fixture_manifest)
        if args.fixture_manifest
        else LiveTransport()
    )
    fetch_log = execute_source_plan(
        request,
        subject_resolution,
        jurisdiction_context,
        source_plan,
        workspace_root=args.workspace_root,
        transport=transport,
        generated_at=args.generated_at,
    )
    print(json.dumps(summarize_fetch_log(fetch_log), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
