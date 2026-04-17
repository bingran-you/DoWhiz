#!/usr/bin/env python3

"""CLI helpers for Alice subject normalization and resolution."""

from __future__ import annotations

import argparse
import json

from alice_subject_resolution import (
    build_subject_resolution,
    load_request,
    normalize_address,
    normalize_apn,
    normalize_coordinates,
    normalize_listing_url,
    summarize_subject_resolution,
)


def emit_json(payload: object, compact: bool) -> None:
    if compact:
        print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
    else:
        print(json.dumps(payload, ensure_ascii=False, indent=2))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    url_parser = subparsers.add_parser("normalize-url", help="Normalize a listing URL without network access.")
    url_parser.add_argument("url")
    url_parser.add_argument("--compact", action="store_true")

    apn_parser = subparsers.add_parser("normalize-apn", help="Normalize an APN conservatively.")
    apn_parser.add_argument("apn")
    apn_parser.add_argument("--county-fips")
    apn_parser.add_argument("--state-fips")
    apn_parser.add_argument("--compact", action="store_true")

    address_parser = subparsers.add_parser("normalize-address", help="Normalize a freeform address string.")
    address_parser.add_argument("address")
    address_parser.add_argument("--compact", action="store_true")

    coordinates_parser = subparsers.add_parser("normalize-coordinates", help="Validate and normalize a coordinate string.")
    coordinates_parser.add_argument("coordinates")
    coordinates_parser.add_argument("--compact", action="store_true")

    request_parser = subparsers.add_parser("from-request", help="Build a subject-resolution artifact from a normalized Alice request.")
    request_parser.add_argument("request_path")
    request_parser.add_argument("--summary", action="store_true", help="Print a compact summary instead of the full artifact.")
    request_parser.add_argument("--compact", action="store_true")

    args = parser.parse_args()

    if args.command == "normalize-url":
        emit_json(normalize_listing_url(args.url), args.compact)
        return 0
    if args.command == "normalize-apn":
        emit_json(
            normalize_apn(args.apn, county_fips=args.county_fips, state_fips=args.state_fips),
            args.compact,
        )
        return 0
    if args.command == "normalize-address":
        emit_json(normalize_address(args.address), args.compact)
        return 0
    if args.command == "normalize-coordinates":
        emit_json(normalize_coordinates(args.coordinates), args.compact)
        return 0

    request = load_request(args.request_path)
    resolution = build_subject_resolution(request)
    if args.summary:
        emit_json(summarize_subject_resolution(resolution), args.compact)
    else:
        emit_json(resolution, args.compact)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

