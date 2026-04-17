#!/usr/bin/env python3

"""List or lookup Alice source descriptors with lightweight filters."""

from __future__ import annotations

import argparse
import json

from alice_registry import compact_source_view, list_sources


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-id", help="Exact source_id lookup.")
    parser.add_argument("--category", help="Filter by source category.")
    parser.add_argument("--state-fips", help="Filter by state FIPS.")
    parser.add_argument("--county-fips", help="Filter by county FIPS.")
    parser.add_argument("--capability", help="Filter by source capability.")
    parser.add_argument(
        "--full",
        action="store_true",
        help="Print full source descriptor objects instead of compact rows.",
    )
    parser.add_argument(
        "--compact",
        action="store_true",
        help="Print compact JSON instead of pretty JSON.",
    )
    args = parser.parse_args()

    sources = list_sources(
        category=args.category,
        state_fips=args.state_fips,
        county_fips=args.county_fips,
        capability=args.capability,
        source_id=args.source_id,
    )
    payload = sources if args.full else [compact_source_view(source) for source in sources]
    if args.compact:
        print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
    else:
        print(json.dumps(payload, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
