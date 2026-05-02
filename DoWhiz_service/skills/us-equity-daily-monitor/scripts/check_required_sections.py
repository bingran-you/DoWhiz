#!/usr/bin/env python3
"""Validate the visible section contract for the final HTML artifact."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from html_contract_utils import count_clickable_links, normalize_text

FULL_ORDER = [
    "decision card",
    "dual-horizon framing",
    "verified facts",
    "derived metrics",
    "scenarios",
    "triggers",
    "judgment",
]
SHORT_ORDER = [
    "decision card",
    "why now",
    "what would change the view",
    "evidence chips",
]

FULL_LABELS = [
    "as of:",
    "price:",
    "investor question:",
    "decision card",
    "monitor status",
    "new money action",
    "existing holder action",
    "thesis impact",
    "signal quality",
    "confidence",
    "one-line rationale:",
    "dual-horizon framing",
    "near-term timing view",
    "long-term ownership view",
    "verified facts",
    "derived metrics",
    "scenarios",
    "bull case",
    "base case",
    "bear case",
    "triggers",
    "upgrade / review now",
    "downgrade / de-risk",
    "invalidation",
    "judgment",
]
SHORT_LABELS = [
    "as of:",
    "price:",
    "investor question:",
    "decision card",
    "monitor status",
    "new money action",
    "existing holder action",
    "thesis impact",
    "signal quality",
    "confidence",
    "one-line rationale:",
    "why now",
    "what would change the view",
    "evidence chips",
    "upgrade / review now",
    "downgrade / de-risk",
    "invalidation",
]
def contains_in_order(text: str, markers: list[str]) -> bool:
    start = 0
    for marker in markers:
        idx = text.find(marker, start)
        if idx == -1:
            return False
        start = idx + len(marker)
    return True


def detect_contract_type(text: str) -> str:
    if (
        "why now" in text
        and "what would change the view" in text
        and "evidence chips" in text
        and "dual-horizon framing" not in text
    ):
        return "short"
    return "full"


def audit_sections(raw_html: str) -> dict:
    text = normalize_text(raw_html)
    contract_type = detect_contract_type(text)
    labels = SHORT_LABELS if contract_type == "short" else FULL_LABELS
    order = SHORT_ORDER if contract_type == "short" else FULL_ORDER
    missing_labels = [label for label in labels if label not in text]
    return {
        "contract_type": contract_type,
        "visible_chars": len(text),
        "missing_labels": missing_labels,
        "order_ok": contains_in_order(text, order),
        "clickable_link_count": count_clickable_links(raw_html),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("html_path", type=Path)
    args = parser.parse_args()
    print(json.dumps(audit_sections(args.html_path.read_text()), indent=2))


if __name__ == "__main__":
    main()
