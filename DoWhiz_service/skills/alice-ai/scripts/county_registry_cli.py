#!/usr/bin/env python3

"""Lookup the effective Alice county coverage entry for a county FIPS code."""

from __future__ import annotations

import argparse
import json

from alice_registry import get_effective_county_entry


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("county_fips", help="Five-digit county FIPS code.")
    parser.add_argument(
        "--compact",
        action="store_true",
        help="Print compact JSON instead of pretty JSON.",
    )
    args = parser.parse_args()

    entry = get_effective_county_entry(args.county_fips)
    if args.compact:
        print(json.dumps(entry, ensure_ascii=False, separators=(",", ":")))
    else:
        print(json.dumps(entry, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
