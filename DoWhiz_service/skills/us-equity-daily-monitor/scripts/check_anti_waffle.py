#!/usr/bin/env python3
"""Reject polished but generic investment commentary."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

GENERIC_PHRASES = [
    "good company, but do not chase",
    "great business, but wait",
    "hold for now",
    "buy in tranches",
    "wait for clarity",
    "do not chase",
    "not a broken asset",
]
TRIGGER_LABELS = [
    "upgrade / review now",
    "downgrade / de-risk",
    "invalidation",
]


def normalize_text(raw_html: str) -> str:
    text = re.sub(r"<[^>]+>", " ", raw_html)
    text = text.replace("&nbsp;", " ").replace("&amp;", "&")
    text = text.replace("&lt;", "<").replace("&gt;", ">")
    return " ".join(text.split()).lower()


def extract_trigger_slice(text: str) -> str:
    start = text.find("triggers")
    if start == -1:
        return ""
    end = text.find("judgment", start)
    return text[start:end] if end != -1 else text[start:]


def has_numeric_signal(text: str) -> bool:
    return bool(re.search(r"(\$|\d|\b\d+%|\bbps\b|\bx\b)", text))


def audit_anti_waffle(raw_html: str) -> dict:
    text = normalize_text(raw_html)
    trigger_slice = extract_trigger_slice(text)
    generic_hits = [phrase for phrase in GENERIC_PHRASES if phrase in text]
    neutral_actions = any(
        phrase in text
        for phrase in [
            "new money action wait",
            "existing holder action hold",
            "existing holder action hold/do not add",
        ]
    )
    missing_trigger_labels = [
        label for label in TRIGGER_LABELS if label not in trigger_slice
    ]
    numeric_hits = re.findall(r"(\$?\d+(?:\.\d+)?%?|\bbps\b|\bx\b)", trigger_slice)
    no_material_change_too_long = (
        "monitor status no material change" in text and len(text) > 2200
    )
    passed = True
    reasons: list[str] = []

    if generic_hits and (missing_trigger_labels or len(numeric_hits) < 3):
        passed = False
        reasons.append(
            f"generic phrasing without concrete movement logic: {', '.join(generic_hits)}"
        )

    if neutral_actions and missing_trigger_labels:
        passed = False
        reasons.append(
            f"neutral stance missing trigger labels: {', '.join(missing_trigger_labels)}"
        )

    if neutral_actions and len(numeric_hits) < 3:
        passed = False
        reasons.append("neutral stance lacks enough concrete numeric trigger detail")

    if no_material_change_too_long:
        passed = False
        reasons.append("No Material Change artifact exceeds short-output budget")

    return {
        "passed": passed,
        "generic_hits": generic_hits,
        "neutral_actions": neutral_actions,
        "missing_trigger_labels": missing_trigger_labels,
        "numeric_hits": numeric_hits,
        "reasons": reasons,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("html_path", type=Path)
    args = parser.parse_args()
    print(json.dumps(audit_anti_waffle(args.html_path.read_text()), indent=2))


if __name__ == "__main__":
    main()
