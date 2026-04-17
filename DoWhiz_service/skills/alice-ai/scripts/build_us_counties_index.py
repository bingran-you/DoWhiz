#!/usr/bin/env python3

"""Build the canonical Alice U.S. county index from Census code tables."""

from __future__ import annotations

import argparse
import csv
import io
import json
from collections import OrderedDict
from datetime import datetime, UTC
from pathlib import Path
from urllib.request import urlopen

from alice_registry import COUNTY_INDEX_PATH, COUNTY_INDEX_VERSION


COUNTY_CODES_URL = (
    "https://www2.census.gov/geo/docs/reference/codes2020/national_county2020.txt"
)
STATE_CODES_URL = "https://www2.census.gov/geo/docs/reference/state.txt"


def fetch_text(url: str) -> str:
    with urlopen(url, timeout=60) as response:
        return response.read().decode("utf-8")


def classify_county_equivalent_type(
    state_fips: str,
    county_name: str,
) -> str:
    if state_fips == "22" and county_name.endswith("Parish"):
        return "parish"
    if state_fips == "02":
        if county_name.endswith("Census Area"):
            return "census_area"
        if county_name.endswith("City and Borough"):
            return "city_and_borough"
        if county_name.endswith("Borough"):
            return "borough"
        if county_name.endswith("Municipality"):
            return "municipality"
    if state_fips in {"24", "29", "32", "51"} and county_name.endswith("city"):
        return "independent_city"
    if state_fips == "11":
        return "federal_district"
    if state_fips == "60":
        if county_name.endswith("District"):
            return "district"
        if county_name.endswith("Island"):
            return "island"
    if state_fips == "66":
        return "territory"
    if state_fips == "69" and county_name.endswith("Municipality"):
        return "municipality"
    if state_fips == "72" and county_name.endswith("Municipio"):
        return "municipio"
    if state_fips in {"74", "78"} and county_name.endswith("Island"):
        return "island"
    return "county"


def build_index() -> dict[str, object]:
    state_text = fetch_text(STATE_CODES_URL)
    county_text = fetch_text(COUNTY_CODES_URL)

    states = {}
    state_reader = csv.DictReader(io.StringIO(state_text), delimiter="|")
    for row in state_reader:
        states[row["STATE"]] = {
            "state_code": row["STUSAB"],
            "state_name": row["STATE_NAME"],
            "state_ns": row["STATENS"],
        }

    counties = OrderedDict()
    county_reader = csv.DictReader(io.StringIO(county_text), delimiter="|")
    for row in sorted(
        county_reader,
        key=lambda item: (item["STATEFP"], item["COUNTYFP"]),
    ):
        state_fips = row["STATEFP"]
        county_fips = f"{state_fips}{row['COUNTYFP']}"
        state = states[state_fips]
        counties[county_fips] = {
            "county_fips": county_fips,
            "state_fips": state_fips,
            "state_code": state["state_code"],
            "state_name": state["state_name"],
            "county_name": row["COUNTYNAME"],
            "county_equivalent_type": classify_county_equivalent_type(
                state_fips,
                row["COUNTYNAME"],
            ),
            "county_ns": row["COUNTYNS"],
            "census_class_code": row["CLASSFP"],
            "census_functional_status": row["FUNCSTAT"],
        }

    return {
        "registry_version": COUNTY_INDEX_VERSION,
        "generated_at": datetime.now(UTC).isoformat().replace("+00:00", "Z"),
        "sources": {
            "county_codes_url": COUNTY_CODES_URL,
            "state_codes_url": STATE_CODES_URL,
            "notes": (
                "Derived from official U.S. Census Bureau county and state "
                "reference code tables. Includes counties and county-equivalents."
            ),
        },
        "counties": counties,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=COUNTY_INDEX_PATH,
        help="Path to write the generated county index JSON.",
    )
    args = parser.parse_args()

    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)

    index = build_index()
    with output.open("w", encoding="utf-8") as handle:
        json.dump(index, handle, indent=2, ensure_ascii=False)
        handle.write("\n")

    county_count = len(index["counties"])
    print(f"Wrote {county_count} county rows to {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
