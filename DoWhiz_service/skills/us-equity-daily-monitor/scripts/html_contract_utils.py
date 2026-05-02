#!/usr/bin/env python3
"""Shared helpers for final-artifact contract validation."""

from __future__ import annotations

from html.parser import HTMLParser
from typing import Iterable
from urllib.parse import urlparse
import re

DOWHIZ_EMAIL_CONTENT_START = "<!-- dowhiz-email-content:start -->"
DOWHIZ_EMAIL_CONTENT_END = "<!-- dowhiz-email-content:end -->"
EMAIL_SHELL_BOILERPLATE = [
    "DoWhiz digital employee",
    "Reply directly to continue this thread with DoWhiz.",
    "Sent by DoWhiz. If you reply, the same task thread will continue.",
]
SYNTHETIC_SCENARIO_KEYWORDS = [
    "assume ",
    "assumption-based",
    "synthetic",
    "company x",
    "company y",
]
INCOMPLETE_FAIL_SOFT_MARKERS = [
    "incomplete research artifact",
    "research did not complete within budget",
    "best-available timed artifact",
    "best-available timed reply",
    "incomplete monitor check",
]
INCOMPLETE_RESEARCH_MARKERS = [
    "incomplete research artifact",
    "research did not complete within budget",
    "best-available timed artifact",
    "best-available timed reply",
]
GENERIC_PLACEHOLDER_DOMAINS = {
    "reuters.com",
    "www.reuters.com",
    "bloomberg.com",
    "www.bloomberg.com",
    "nasdaq.com",
    "www.nasdaq.com",
    "finance.yahoo.com",
    "marketwatch.com",
    "www.marketwatch.com",
    "wsj.com",
    "www.wsj.com",
    "ft.com",
    "www.ft.com",
    "seekingalpha.com",
    "www.seekingalpha.com",
    "sec.gov",
    "www.sec.gov",
}
GENERIC_PLACEHOLDER_PATHS = {
    "",
    "markets",
    "investing",
    "quote",
    "quotes",
    "stocks",
    "news",
    "finance",
    "research",
}
BLOCK_TAGS = {
    "address",
    "article",
    "aside",
    "blockquote",
    "br",
    "dd",
    "div",
    "dl",
    "dt",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "ul",
}


def extract_contract_html(raw_html: str) -> str:
    start = raw_html.find(DOWHIZ_EMAIL_CONTENT_START)
    if start == -1:
        return raw_html
    start += len(DOWHIZ_EMAIL_CONTENT_START)
    end = raw_html.find(DOWHIZ_EMAIL_CONTENT_END, start)
    if end == -1:
        return raw_html
    return raw_html[start:end]


class VisibleTextParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self._skip_stack: list[bool] = []
        self._chunks: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        current_skip = self._skip_stack[-1] if self._skip_stack else False
        attr_map = {name.lower(): (value or "") for name, value in attrs}
        hidden = current_skip or _tag_is_hidden(tag.lower(), attr_map)
        self._skip_stack.append(hidden)
        if not hidden and tag.lower() in BLOCK_TAGS:
            self._chunks.append(" ")

    def handle_endtag(self, tag: str) -> None:
        hidden = self._skip_stack.pop() if self._skip_stack else False
        if not hidden and tag.lower() in BLOCK_TAGS:
            self._chunks.append(" ")

    def handle_data(self, data: str) -> None:
        if any(self._skip_stack):
            return
        self._chunks.append(data)

    def text(self) -> str:
        text = "".join(self._chunks)
        for phrase in EMAIL_SHELL_BOILERPLATE:
            text = text.replace(phrase, " ")
        return text


def _tag_is_hidden(tag: str, attrs: dict[str, str]) -> bool:
    if tag in {"style", "script", "noscript", "template"}:
        return True
    if "hidden" in attrs:
        return True
    if attrs.get("aria-hidden", "").lower() == "true":
        return True
    classes = {part for part in attrs.get("class", "").split() if part}
    if "dw-preheader" in classes:
        return True
    style = "".join(attrs.get("style", "").lower().split())
    return any(
        token in style for token in ("display:none", "visibility:hidden", "mso-hide:all")
    )


def visible_text(raw_html: str) -> str:
    parser = VisibleTextParser()
    parser.feed(extract_contract_html(raw_html))
    parser.close()
    return parser.text()


def normalize_text(raw_html: str) -> str:
    text = visible_text(raw_html)
    text = text.replace("&nbsp;", " ").replace("&amp;", "&")
    text = text.replace("&lt;", "<").replace("&gt;", ">")
    return " ".join(text.split()).lower()


def collect_clickable_links(raw_html: str) -> list[str]:
    fragment = extract_contract_html(raw_html)
    return re.findall(r"""href\s*=\s*['"](http[^'"]+)['"]""", fragment, flags=re.IGNORECASE)


def count_clickable_links(raw_html: str) -> int:
    return len(collect_clickable_links(raw_html))


def detect_synthetic_request(request_text: str | None) -> bool:
    if not request_text:
        return False
    normalized = " ".join(request_text.lower().split())
    return any(keyword in normalized for keyword in SYNTHETIC_SCENARIO_KEYWORDS)


def detect_incomplete_fail_soft_mode(text: str) -> bool:
    normalized = " ".join(text.lower().split())
    return any(marker in normalized for marker in INCOMPLETE_FAIL_SOFT_MARKERS)


def detect_incomplete_research_mode(text: str) -> bool:
    normalized = " ".join(text.lower().split())
    return any(marker in normalized for marker in INCOMPLETE_RESEARCH_MARKERS)


def is_generic_placeholder_link(link: str) -> bool:
    parsed = urlparse(link)
    domain = parsed.netloc.lower()
    path = parsed.path.strip("/").lower()
    return domain in GENERIC_PLACEHOLDER_DOMAINS and path in GENERIC_PLACEHOLDER_PATHS


def link_looks_specific(link: str) -> bool:
    parsed = urlparse(link)
    path = parsed.path.strip("/").lower()
    if not path:
        return False
    segments = [segment for segment in path.split("/") if segment]
    return (
        len(segments) >= 2
        or path.endswith(".html")
        or any(token in path for token in ("filing", "earnings", "article"))
    )


def any_generic_placeholder_links(links: Iterable[str]) -> bool:
    return any(is_generic_placeholder_link(link) for link in links)
