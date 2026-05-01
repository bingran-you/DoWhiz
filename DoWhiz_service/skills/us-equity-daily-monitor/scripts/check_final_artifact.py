#!/usr/bin/env python3
"""Audit the final HTML artifact for the equity-monitor contract."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

from check_anti_waffle import audit_anti_waffle, normalize_text  # noqa: E402
from check_required_sections import audit_sections  # noqa: E402

DECISION_OPTIONS = {
    "monitor status": ["no material change", "watch closely", "review now"],
    "new money action": ["buy", "starter only", "wait", "avoid for now"],
    "existing holder action": [
        "add",
        "hold/do not add",
        "hold",
        "trim",
        "exit",
    ],
    "thesis impact": ["no material change", "positive", "mixed", "negative"],
    "signal quality": ["weak", "moderate", "strong"],
    "confidence": ["low", "medium", "high"],
}


def extract_choice(text: str, label: str, options: list[str]) -> str | None:
    for option in sorted(options, key=len, reverse=True):
        pattern = rf"{re.escape(label)}\s+{re.escape(option)}"
        if re.search(pattern, text):
            return option
    return None


def derived_metrics_look_formulaic(text: str) -> bool:
    start = text.find("derived metrics")
    if start == -1:
        return False
    end = text.find("scenarios", start)
    section = text[start:end] if end != -1 else text[start:]
    return "formula / inputs" in section or "/" in section or "=" in section


def audit_final_artifact(raw_html: str) -> dict:
    sections = audit_sections(raw_html)
    waffle = audit_anti_waffle(raw_html)
    text = normalize_text(raw_html)
    decisions = {
        label: extract_choice(text, label, options)
        for label, options in DECISION_OPTIONS.items()
    }
    min_links = 2 if sections["contract_type"] == "short" else 3
    checks = [
        {
            "name": "required_sections",
            "passed": not sections["missing_labels"] and sections["order_ok"],
            "detail": (
                "ok"
                if not sections["missing_labels"] and sections["order_ok"]
                else f"missing={sections['missing_labels']} order_ok={sections['order_ok']}"
            ),
        },
        {
            "name": "decision_card_fields_present",
            "passed": all(value is not None for value in decisions.values()),
            "detail": decisions,
        },
        {
            "name": "clickable_links_present",
            "passed": sections["clickable_link_count"] >= min_links,
            "detail": f"links={sections['clickable_link_count']} min_required={min_links}",
        },
        {
            "name": "anti_waffle",
            "passed": waffle["passed"],
            "detail": waffle["reasons"] or ["ok"],
        },
    ]
    if sections["contract_type"] == "full":
        checks.append(
            {
                "name": "derived_metrics_formulaic",
                "passed": derived_metrics_look_formulaic(text),
                "detail": "ok" if derived_metrics_look_formulaic(text) else "derived metrics lacks formula-like content",
            }
        )
    return {
        "passed": all(check["passed"] for check in checks),
        "checks": checks,
        "contract_type": sections["contract_type"],
        "visible_chars": sections["visible_chars"],
        "decisions": decisions,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("html_path", type=Path)
    args = parser.parse_args()
    print(json.dumps(audit_final_artifact(args.html_path.read_text()), indent=2))


if __name__ == "__main__":
    main()
