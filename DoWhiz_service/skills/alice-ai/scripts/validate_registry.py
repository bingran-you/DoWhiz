#!/usr/bin/env python3

"""Validate Alice county overrides, source descriptors, and fallback entries."""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Iterable

from alice_registry import (
    COUNTY_INDEX_VERSION,
    COUNTY_SCHEMA_PATH,
    COUNTY_OVERRIDES_ROOT,
    COUNTY_REGISTRY_VERSION,
    COUNTY_INDEX_PATH,
    FEDERAL_BASELINE_SOURCE_IDS,
    SOURCE_SCHEMA_PATH,
    build_fallback_county_entry,
    coverage_tier_rubric,
    county_rows,
    get_effective_county_entry,
    iter_county_override_paths,
    iter_source_paths,
    load_all_sources,
    load_county_index,
    load_json,
    source_by_id,
)


CHUNK_SIZE = 250
MACHINE_EXECUTABLE_ACCESS_MODES = {"machine_endpoint", "mixed"}
ACCESS_MODE_INTERFACE_COMPATIBILITY = {
    "machine_endpoint": {"formal_api", "gis_service"},
    "landing_page": {"web_only", "downloadable_dataset"},
    "viewer": {"web_only", "gis_service"},
    "mixed": {"formal_api", "gis_service", "downloadable_dataset", "web_only"},
}
MACHINE_BASELINE_SOURCE_IDS = {
    "federal_fema_nfhl",
    "federal_usfws_nwi",
    "federal_usda_soil_data_access",
    "federal_usgs_3dep_elevation",
}


def require_npx() -> str:
    npx = shutil.which("npx")
    if npx is None:
        raise SystemExit("npx is required for schema validation but was not found in PATH.")
    return npx


def run_ajv(schema_path: Path, json_paths: Iterable[Path], label: str) -> None:
    paths = [path.resolve() for path in json_paths]
    if not paths:
        return

    npx = require_npx()
    for start in range(0, len(paths), CHUNK_SIZE):
        chunk = paths[start : start + CHUNK_SIZE]
        command = [
            npx,
            "--yes",
            "-p",
            "ajv-cli",
            "-p",
            "ajv-formats",
            "ajv",
            "validate",
            "--spec=draft2020",
            "-c",
            "ajv-formats",
            "-s",
            str(schema_path.resolve()),
        ]
        for path in chunk:
            command.extend(["-d", str(path)])
        result = subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            raise SystemExit(
                f"Schema validation failed for {label}:\n{result.stdout}\n{result.stderr}"
            )


def assert_true(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def validate_county_index() -> None:
    county_index = load_county_index()
    assert_true(
        county_index.get("registry_version") == COUNTY_INDEX_VERSION,
        "County index registry_version is unexpected.",
    )
    counties = county_index.get("counties")
    assert_true(isinstance(counties, dict), "County index must expose a counties object.")
    assert_true(len(counties) >= 3200, "County index is unexpectedly incomplete.")

    for county_fips, row in counties.items():
        assert_true(re.fullmatch(r"[0-9]{5}", county_fips) is not None, f"Bad county key: {county_fips}")
        assert_true(row["county_fips"] == county_fips, f"County row key mismatch for {county_fips}")
        assert_true(
            re.fullmatch(r"[0-9]{2}", row["state_fips"]) is not None,
            f"Bad state_fips for {county_fips}",
        )
        assert_true(bool(row["state_code"]), f"Missing state_code for {county_fips}")
        assert_true(bool(row["state_name"]), f"Missing state_name for {county_fips}")
        assert_true(bool(row["county_name"]), f"Missing county_name for {county_fips}")
        assert_true(
            bool(row["county_equivalent_type"]),
            f"Missing county_equivalent_type for {county_fips}",
        )


def validate_override_integrity() -> None:
    counties = county_rows()
    known_sources = source_by_id()
    override_paths = iter_county_override_paths()
    run_ajv(COUNTY_SCHEMA_PATH, override_paths, "county overrides")

    for path in override_paths:
        override = load_json(path)
        county_fips = override["county_fips"]
        assert_true(county_fips in counties, f"Override references unknown county_fips: {county_fips}")
        county = counties[county_fips]
        assert_true(path.stem == county_fips, f"Override filename mismatch: {path}")
        assert_true(path.parent.name == county["state_code"], f"Override state directory mismatch: {path}")
        assert_true(
            override["state_fips"] == county["state_fips"],
            f"Override state_fips mismatch: {path}",
        )
        assert_true(
            override["county_name"] == county["county_name"],
            f"Override county_name mismatch: {path}",
        )
        for source_id in override["preferred_source_ids"] + override["discovered_source_ids"]:
            assert_true(
                source_id in known_sources,
                f"Override {path} references unknown source_id: {source_id}",
            )
        rubric = coverage_tier_rubric(override["capabilities"])
        assert_true(
            override["coverage_tier"] == rubric["inferred_tier"],
            (
                f"Override {path} declares coverage_tier={override['coverage_tier']!r}, "
                f"but the rubric infers {rubric['inferred_tier']!r} "
                f"(baseline={rubric['baseline_footing']}, identity={rubric['identity_footing']}, "
                f"planning={rubric['planning_footing']}, infrastructure={rubric['infrastructure_footing']})."
            ),
        )
        non_federal_source_ids = [
            source_id
            for source_id in override["discovered_source_ids"]
            if known_sources[source_id]["category"] != "federal"
        ]
        assert_true(
            bool(non_federal_source_ids),
            f"Curated override {path} must retain at least one non-federal discovered source.",
        )


def validate_source_integrity() -> None:
    counties = county_rows()
    source_paths = iter_source_paths()
    run_ajv(SOURCE_SCHEMA_PATH, source_paths, "source descriptors")

    seen_source_ids = set()
    machine_executable_sources = set()
    for path in source_paths:
        source = load_json(path)
        source_id = source["source_id"]
        assert_true(source_id not in seen_source_ids, f"Duplicate source_id: {source_id}")
        seen_source_ids.add(source_id)

        access_mode = source["access_mode"]
        assert_true(
            source["interface_type"] in ACCESS_MODE_INTERFACE_COMPATIBILITY[access_mode],
            (
                f"Source {source_id} uses interface_type={source['interface_type']!r} "
                f"with incompatible access_mode={access_mode!r}."
            ),
        )
        if access_mode in MACHINE_EXECUTABLE_ACCESS_MODES:
            machine_executable_sources.add(source_id)

        geography = source["geography_scope"]
        state_fips = geography["state_fips"]
        county_fips = geography["county_fips"]
        if county_fips is not None:
            assert_true(county_fips in counties, f"Source {source_id} references unknown county_fips")
            assert_true(
                state_fips == counties[county_fips]["state_fips"],
                f"Source {source_id} county/state mismatch",
            )
        elif state_fips is not None:
            assert_true(
                any(row["state_fips"] == state_fips for row in counties.values()),
                f"Source {source_id} references unknown state_fips",
            )

    for source_id in MACHINE_BASELINE_SOURCE_IDS:
        assert_true(
            source_id in machine_executable_sources,
            f"Expected {source_id} to be machine-executable or mixed for Step 6 evaluation clarity.",
        )


def validate_fallback_entries() -> None:
    counties = county_rows()
    source_ids = source_by_id()
    for source_id in FEDERAL_BASELINE_SOURCE_IDS:
        assert_true(
            source_id in source_ids,
            f"Fallback baseline source_id is missing from the registry: {source_id}",
        )

    with tempfile.TemporaryDirectory(prefix="alice-fallback-counties-") as tmp_dir:
        tmp_root = Path(tmp_dir)
        temp_paths = []
        for county_fips, county in counties.items():
            entry = build_fallback_county_entry(county)
            assert_true(
                entry["registry_version"] == COUNTY_REGISTRY_VERSION,
                f"Fallback entry registry version mismatch for {county_fips}",
            )
            rubric = coverage_tier_rubric(entry["capabilities"])
            assert_true(
                entry["coverage_tier"] == rubric["inferred_tier"],
                (
                    f"Fallback entry for {county_fips} declares coverage_tier={entry['coverage_tier']!r}, "
                    f"but the rubric infers {rubric['inferred_tier']!r}."
                ),
            )
            path = tmp_root / f"{county_fips}.json"
            path.write_text(json.dumps(entry, indent=2), encoding="utf-8")
            temp_paths.append(path)
        run_ajv(COUNTY_SCHEMA_PATH, temp_paths, "fallback county entries")


def smoke_test_effective_lookups() -> None:
    curated = get_effective_county_entry("06029")
    fallback = get_effective_county_entry("01001")
    assert_true(curated["coverage_tier"] == "full", "Expected Kern County curated override.")
    assert_true(fallback["coverage_tier"] == "minimal", "Expected fallback county coverage tier.")


def main() -> int:
    assert_true(COUNTY_INDEX_PATH.exists(), f"County index file not found: {COUNTY_INDEX_PATH}")
    validate_county_index()
    validate_source_integrity()
    validate_override_integrity()
    validate_fallback_entries()
    smoke_test_effective_lookups()

    source_count = len(load_all_sources())
    override_count = len(iter_county_override_paths())
    county_count = len(county_rows())
    print(
        "Alice registry validation passed: "
        f"{county_count} counties, {override_count} county overrides, {source_count} sources."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
