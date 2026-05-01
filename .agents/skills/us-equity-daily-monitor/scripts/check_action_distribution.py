#!/usr/bin/env python3
"""Catch distribution collapse toward neutral wait/hold outputs."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def audit_distribution(items: list[dict]) -> dict:
    statuses = {item.get("monitor status") for item in items if item.get("monitor status")}
    new_money_actions = [item.get("new money action") for item in items if item.get("new money action")]
    holder_actions = [
        item.get("existing holder action")
        for item in items
        if item.get("existing holder action")
    ]
    neutral_pairs = [
        item
        for item in items
        if item.get("new money action") == "wait"
        and item.get("existing holder action") in {"hold", "hold/do not add"}
    ]
    neutral_pair_count = len(neutral_pairs)
    total_items = len(items)
    checks = [
        {
            "name": "status_coverage",
            "passed": {"no material change", "watch closely", "review now"} <= statuses,
            "detail": sorted(statuses),
        },
        {
            "name": "new_money_not_all_wait",
            "passed": any(action != "wait" for action in new_money_actions),
            "detail": new_money_actions,
        },
        {
            "name": "holder_has_non_hold_case",
            "passed": any(action in {"add", "trim", "exit"} for action in holder_actions),
            "detail": holder_actions,
        },
        {
            "name": "neutral_pair_not_majority",
            "passed": total_items > 0 and neutral_pair_count * 2 < total_items,
            "detail": f"neutral_pairs={neutral_pair_count} total={total_items}",
        },
    ]
    return {"passed": all(check["passed"] for check in checks), "checks": checks}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("json_path", type=Path)
    args = parser.parse_args()
    payload = json.loads(args.json_path.read_text())
    print(json.dumps(audit_distribution(payload["items"]), indent=2))


if __name__ == "__main__":
    main()
