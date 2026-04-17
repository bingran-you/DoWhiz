#!/usr/bin/env python3

"""CLI wrapper for Alice Step 8 report rendering."""

from __future__ import annotations

import argparse
import json

from alice_render_report import render_workspace_report


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace-root", required=True, help="Workspace root containing alice/ artifacts.")
    parser.add_argument(
        "--render-mode",
        default="generic_summary",
        choices=["generic_summary", "markdown_memo", "slack_summary", "email_summary"],
        help="Render mode stored on the report_summary artifact.",
    )
    parser.add_argument(
        "--generated-at",
        help="Optional fixed timestamp for deterministic example generation.",
    )
    parser.add_argument(
        "--no-write",
        action="store_true",
        help="Build render outputs without writing report files into the workspace.",
    )
    parser.add_argument(
        "--compact",
        action="store_true",
        help="Emit compact JSON instead of pretty JSON.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    rendered = render_workspace_report(
        args.workspace_root,
        render_mode=args.render_mode,
        generated_at=args.generated_at,
        write_outputs=not args.no_write,
    )
    summary = rendered["report_summary"]
    output = {
        "report_summary_id": summary["report_summary_id"],
        "title": summary["title"],
        "one_line_conclusion": summary["one_line_conclusion"],
        "canonical_markdown_artifact_ref": summary["channel_hints"]["canonical_markdown_artifact_ref"],
        "slack_artifact_ref": summary["channel_hints"]["slack_artifact_ref"],
        "email_artifact_ref": summary["channel_hints"]["email_artifact_ref"],
    }
    if args.compact:
        print(json.dumps(output, separators=(",", ":")))
    else:
        print(json.dumps(output, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
