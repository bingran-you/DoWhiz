use std::collections::HashSet;

use kuchiki::traits::*;
use kuchiki::NodeRef;

use super::postmark::PostmarkInbound;

pub fn derive_inbound_email_text(
    text_body: Option<&str>,
    stripped_text_reply: Option<&str>,
    html_body: Option<&str>,
) -> Option<String> {
    if let Some(text) = normalize_plain_text_option(stripped_text_reply)
        .or_else(|| normalize_plain_text_option(text_body))
    {
        return Some(append_missing_html_links(text, html_body));
    }

    if let Some(html) = html_body.filter(|value| !value.trim().is_empty()) {
        let text = clean_inbound_html_to_text(html);
        if !text.trim().is_empty() {
            return Some(text);
        }
    }

    None
}

pub fn render_plain_text_as_html(input: &str) -> String {
    wrap_text_as_html(input)
}

pub(super) fn render_email_html(payload: &PostmarkInbound) -> String {
    derive_inbound_email_text(
        payload.text_body.as_deref(),
        payload.stripped_text_reply.as_deref(),
        payload.html_body.as_deref(),
    )
    .map(|text| render_plain_text_as_html(&text))
    .unwrap_or_else(|| render_plain_text_as_html("(no content)"))
}

fn sanitize_inbound_html_document(html: &str) -> NodeRef {
    let document = kuchiki::parse_html().one(html);
    remove_html_comments(&document);
    remove_elements_by_selector(
        &document,
        "head, script, style, meta, link, title, noscript",
    );
    remove_hidden_elements(&document);
    remove_tracking_pixels(&document);
    remove_footer_blocks(&document);
    sanitize_allowed_elements(&document);
    document
}

fn clean_inbound_html_to_text(html: &str) -> String {
    let document = sanitize_inbound_html_document(html);
    let mut out = String::new();
    render_document_text(&document, &mut out);
    normalize_rendered_html_text(&out)
}

fn append_missing_html_links(text: String, html_body: Option<&str>) -> String {
    let Some(html) = html_body.filter(|value| !value.trim().is_empty()) else {
        return text;
    };
    let links = extract_html_links(html);
    if links.is_empty() {
        return text;
    }

    let mut missing = Vec::new();
    let lower_text = text.to_ascii_lowercase();
    for link in links {
        if !lower_text.contains(&link.to_ascii_lowercase()) {
            missing.push(link);
        }
    }
    if missing.is_empty() {
        return text;
    }

    let mut out = text;
    if !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
    out.push_str("Links:\n");
    for link in missing {
        out.push_str("- ");
        out.push_str(&link);
        out.push('\n');
    }
    out.trim_end().to_string()
}

fn normalize_plain_text_option(value: Option<&str>) -> Option<String> {
    let value = value?;
    let normalized = normalize_plain_text(value);
    if normalized.trim().is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn normalize_plain_text(input: &str) -> String {
    input
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn remove_html_comments(document: &NodeRef) {
    let nodes: Vec<NodeRef> = document.descendants().collect();
    for node in nodes {
        if node.as_comment().is_some() {
            node.detach();
        }
    }
}

fn remove_elements_by_selector(document: &NodeRef, selector: &str) {
    if let Ok(nodes) = document.select(selector) {
        for node in nodes {
            node.as_node().detach();
        }
    }
}

fn remove_hidden_elements(document: &NodeRef) {
    let nodes: Vec<NodeRef> = document.descendants().collect();
    for node in nodes {
        let element = match node.as_element() {
            Some(value) => value,
            None => continue,
        };
        if is_hidden_element(element) {
            node.detach();
        }
    }
}

fn remove_tracking_pixels(document: &NodeRef) {
    let nodes: Vec<NodeRef> = document.descendants().collect();
    for node in nodes {
        let element = match node.as_element() {
            Some(value) => value,
            None => continue,
        };
        if element.name.local.as_ref() == "img" && is_tracking_pixel(element) {
            node.detach();
        }
    }
}

fn remove_footer_blocks(document: &NodeRef) {
    let nodes: Vec<NodeRef> = document.descendants().collect();
    for node in nodes {
        let element = match node.as_element() {
            Some(value) => value,
            None => continue,
        };
        let tag = element.name.local.as_ref();
        if !is_footer_candidate(tag) {
            continue;
        }
        if element_has_footer_marker(element) {
            node.detach();
            continue;
        }
        let text = node.text_contents();
        if text_contains_footer_hint(&text) {
            node.detach();
        }
    }
}

fn sanitize_allowed_elements(document: &NodeRef) {
    let nodes: Vec<NodeRef> = document.descendants().collect();
    for node in nodes {
        let element = match node.as_element() {
            Some(value) => value,
            None => continue,
        };
        let tag = element.name.local.as_ref();
        if is_drop_tag(tag) {
            node.detach();
            continue;
        }
        if !is_allowed_tag(tag) {
            unwrap_node(&node);
            continue;
        }
        if tag == "img" {
            node.detach();
            continue;
        }
        prune_attributes(tag, element);
    }
}

fn render_document_text(document: &NodeRef, out: &mut String) {
    if let Ok(mut bodies) = document.select("body") {
        if let Some(body) = bodies.next() {
            for child in body.as_node().children() {
                render_node_text(&child, out, false);
            }
            return;
        }
    }
    for child in document.children() {
        render_node_text(&child, out, false);
    }
}

fn render_node_text(node: &NodeRef, out: &mut String, preserve_whitespace: bool) {
    if let Some(text) = node.as_text() {
        append_text(out, &text.borrow(), preserve_whitespace);
        return;
    }

    let Some(element) = node.as_element() else {
        for child in node.children() {
            render_node_text(&child, out, preserve_whitespace);
        }
        return;
    };

    let tag = element.name.local.as_ref();
    match tag {
        "br" => push_line_break(out),
        "p" | "div" | "section" | "article" | "header" | "footer" | "blockquote" | "h1" | "h2"
        | "h3" | "h4" | "h5" | "h6" => {
            push_block_break(out);
            for child in node.children() {
                render_node_text(&child, out, preserve_whitespace);
            }
            push_block_break(out);
        }
        "ul" | "ol" | "table" | "thead" | "tbody" => {
            push_block_break(out);
            for child in node.children() {
                render_node_text(&child, out, preserve_whitespace);
            }
            push_block_break(out);
        }
        "li" => {
            push_list_break(out);
            out.push_str("- ");
            for child in node.children() {
                render_node_text(&child, out, preserve_whitespace);
            }
            push_line_break(out);
        }
        "tr" => {
            push_list_break(out);
            let mut first_cell = true;
            for child in node.children() {
                if is_table_cell(&child) {
                    if !first_cell {
                        trim_trailing_inline_whitespace(out);
                        out.push_str(" | ");
                    }
                    first_cell = false;
                }
                render_node_text(&child, out, preserve_whitespace);
            }
            push_line_break(out);
        }
        "pre" => {
            push_block_break(out);
            for child in node.children() {
                render_node_text(&child, out, true);
            }
            push_block_break(out);
        }
        "a" => {
            let href = element
                .attributes
                .borrow()
                .get("href")
                .filter(|value| is_safe_link(value))
                .map(|value| value.trim().to_string());
            let start_len = out.len();
            for child in node.children() {
                render_node_text(&child, out, preserve_whitespace);
            }
            if let Some(href) = href {
                let rendered = out[start_len..].trim().to_string();
                if rendered.is_empty() {
                    append_text(out, &href, false);
                } else if !rendered.contains(&href) {
                    out.push_str(" (");
                    out.push_str(&href);
                    out.push(')');
                }
            }
        }
        _ => {
            for child in node.children() {
                render_node_text(&child, out, preserve_whitespace);
            }
        }
    }
}

fn append_text(out: &mut String, text: &str, preserve_whitespace: bool) {
    if preserve_whitespace {
        out.push_str(text);
        return;
    }

    let mut pending_space = out
        .chars()
        .last()
        .map(|ch| ch.is_whitespace())
        .unwrap_or(false);
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !pending_space {
                out.push(' ');
                pending_space = true;
            }
        } else {
            out.push(ch);
            pending_space = false;
        }
    }
}

fn push_block_break(out: &mut String) {
    trim_trailing_inline_whitespace(out);
    if out.is_empty() {
        return;
    }
    if out.ends_with("\n\n") {
        return;
    }
    if out.ends_with('\n') {
        out.push('\n');
    } else {
        out.push_str("\n\n");
    }
}

fn push_list_break(out: &mut String) {
    trim_trailing_inline_whitespace(out);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

fn push_line_break(out: &mut String) {
    trim_trailing_inline_whitespace(out);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

fn trim_trailing_inline_whitespace(out: &mut String) {
    while matches!(out.chars().last(), Some(' ' | '\t')) {
        out.pop();
    }
}

fn is_table_cell(node: &NodeRef) -> bool {
    node.as_element()
        .map(|element| matches!(element.name.local.as_ref(), "td" | "th"))
        .unwrap_or(false)
}

fn extract_html_links(html: &str) -> Vec<String> {
    let document = sanitize_inbound_html_document(html);
    let mut seen = HashSet::new();
    let mut links = Vec::new();
    if let Ok(nodes) = document.select("a[href]") {
        for node in nodes {
            let attrs = node.attributes.borrow();
            let Some(href) = attrs.get("href").map(str::trim) else {
                continue;
            };
            if href.is_empty() || !is_safe_link(href) {
                continue;
            }
            let normalized = href.to_string();
            if seen.insert(normalized.clone()) {
                links.push(normalized);
            }
        }
    }
    links
}

fn normalize_rendered_html_text(input: &str) -> String {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = Vec::new();
    let mut previous_blank = false;
    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !previous_blank {
                lines.push(String::new());
            }
            previous_blank = true;
            continue;
        }
        lines.push(trimmed.to_string());
        previous_blank = false;
    }

    while matches!(lines.first(), Some(value) if value.is_empty()) {
        lines.remove(0);
    }
    while matches!(lines.last(), Some(value) if value.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn unwrap_node(node: &NodeRef) {
    if node.parent().is_none() {
        return;
    }
    let children: Vec<NodeRef> = node.children().collect();
    for child in children {
        node.insert_before(child);
    }
    node.detach();
}

fn is_allowed_tag(tag: &str) -> bool {
    matches!(
        tag,
        "html"
            | "body"
            | "p"
            | "br"
            | "div"
            | "span"
            | "a"
            | "ul"
            | "ol"
            | "li"
            | "strong"
            | "em"
            | "b"
            | "i"
            | "u"
            | "blockquote"
            | "pre"
            | "code"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "table"
            | "thead"
            | "tbody"
            | "tr"
            | "td"
            | "th"
    )
}

fn is_drop_tag(tag: &str) -> bool {
    matches!(
        tag,
        "script" | "style" | "head" | "meta" | "link" | "title" | "noscript"
    )
}

fn is_footer_candidate(tag: &str) -> bool {
    matches!(
        tag,
        "div" | "p" | "span" | "td" | "li" | "section" | "footer"
    )
}

fn element_has_footer_marker(element: &kuchiki::ElementData) -> bool {
    let attrs = element.attributes.borrow();
    for key in ["class", "id"] {
        if let Some(value) = attrs.get(key) {
            let lower = value.to_ascii_lowercase();
            if lower.contains("footer")
                || lower.contains("unsubscribe")
                || lower.contains("notification")
                || lower.contains("preferences")
            {
                return true;
            }
        }
    }
    false
}

fn text_contains_footer_hint(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let hints = [
        "unsubscribe",
        "notification settings",
        "manage notifications",
        "email preferences",
        "manage your email",
        "view this email in your browser",
        "view in browser",
        "you are receiving this",
        "to stop receiving",
        "opt out",
        "reply to this email directly",
    ];
    hints.iter().any(|hint| lower.contains(hint))
}

fn is_hidden_element(element: &kuchiki::ElementData) -> bool {
    let attrs = element.attributes.borrow();
    if attrs.contains("hidden") {
        return true;
    }
    if let Some(value) = attrs.get("aria-hidden") {
        if value.trim().eq_ignore_ascii_case("true") {
            return true;
        }
    }
    if let Some(style) = attrs.get("style") {
        if style_contains_hidden(style) {
            return true;
        }
    }
    false
}

fn is_tracking_pixel(element: &kuchiki::ElementData) -> bool {
    let attrs = element.attributes.borrow();
    if let Some(style) = attrs.get("style") {
        if style_contains_hidden(style) {
            return true;
        }
    }
    let src = attrs.get("src").unwrap_or("");
    let src_lower = src.to_ascii_lowercase();
    if src_lower.contains("tracking")
        || src_lower.contains("pixel")
        || src_lower.contains("beacon")
        || src_lower.contains("open.gif")
    {
        return true;
    }
    let width = attrs.get("width").and_then(parse_dimension).or_else(|| {
        attrs
            .get("style")
            .and_then(|style| style_dimension(style, "width"))
    });
    let height = attrs.get("height").and_then(parse_dimension).or_else(|| {
        attrs
            .get("style")
            .and_then(|style| style_dimension(style, "height"))
    });
    matches_1x1(width, height)
}

fn matches_1x1(width: Option<u32>, height: Option<u32>) -> bool {
    match (width, height) {
        (Some(w), Some(h)) => w <= 1 && h <= 1,
        (Some(w), None) => w <= 1,
        (None, Some(h)) => h <= 1,
        (None, None) => false,
    }
}

fn style_contains_hidden(style: &str) -> bool {
    let normalized: String = style
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    normalized.contains("display:none")
        || normalized.contains("visibility:hidden")
        || normalized.contains("opacity:0")
        || normalized.contains("max-height:0")
}

fn style_dimension(style: &str, key: &str) -> Option<u32> {
    for part in style.split(';') {
        let mut iter = part.splitn(2, ':');
        let name = iter.next().unwrap_or("").trim().to_ascii_lowercase();
        if name == key {
            let value = iter.next().unwrap_or("").trim();
            return parse_dimension(value);
        }
    }
    None
}

fn parse_dimension(raw: &str) -> Option<u32> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let digits: String = trimmed
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

fn prune_attributes(tag: &str, element: &kuchiki::ElementData) {
    let mut attrs = element.attributes.borrow_mut();
    let mut to_remove = Vec::new();
    for (name, _) in attrs.map.iter() {
        let local = name.local.as_ref();
        let keep = match tag {
            "a" => matches!(local, "href"),
            "img" => matches!(local, "src" | "alt" | "width" | "height"),
            _ => false,
        };
        if !keep {
            to_remove.push(name.clone());
        }
    }
    for name in to_remove {
        attrs.map.remove(&name);
    }
    if tag == "a" {
        if let Some(href) = attrs.get("href").map(|value| value.to_string()) {
            if !is_safe_link(&href) {
                attrs.remove("href");
            }
        }
    }
    if tag == "img" {
        if let Some(src) = attrs.get("src").map(|value| value.to_string()) {
            if !is_safe_image_src(&src) {
                attrs.remove("src");
            }
        }
    }
}

fn is_safe_link(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    !(lower.starts_with("javascript:") || lower.starts_with("vbscript:"))
}

fn is_safe_image_src(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    !(lower.starts_with("javascript:") || lower.starts_with("vbscript:"))
}

fn wrap_text_as_html(input: &str) -> String {
    format!("<pre>{}</pre>", escape_html(input))
}

fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

pub(super) fn strip_html_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

pub(super) fn truncate_preview(input: &str, max_len: usize) -> String {
    if input.len() <= max_len {
        return input.to_string();
    }
    let mut end = max_len;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &input[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_inbound_email_text_prefers_text_body_and_appends_missing_links() {
        let text = derive_inbound_email_text(
            Some("Please finish this course for me."),
            None,
            Some(
                r#"<p>Please finish this course for me.</p><p><a href="https://learning.edx.org/course/123">Open course</a></p>"#,
            ),
        )
        .expect("text");

        assert!(text.contains("Please finish this course for me."));
        assert!(text.contains("https://learning.edx.org/course/123"));
    }

    #[test]
    fn derive_inbound_email_text_prefers_stripped_reply_when_text_body_missing() {
        let text = derive_inbound_email_text(
            None,
            Some("Reply body"),
            Some(r#"<p>Reply body</p><p><a href="https://example.com/doc">Open doc</a></p>"#),
        )
        .expect("text");

        assert!(text.starts_with("Reply body"));
        assert!(text.contains("Links:\n- https://example.com/doc"));
    }

    #[test]
    fn derive_inbound_email_text_prefers_stripped_reply_over_full_text_body() {
        let text = derive_inbound_email_text(
            Some(
                "Can you help me do some deep research for the competitors, and the upstream/downstream for Nvidia.\n\nOn Sat, Apr 18, 2026 at 1:14 PM Logan wrote:\n> Give me a deep research about the Nvidia stock, and tell me whether it is a good time to buy",
            ),
            Some(
                "Can you help me do some deep research for the competitors, and the upstream/downstream for Nvidia.",
            ),
            Some("<p>Can you help me do some deep research for the competitors, and the upstream/downstream for Nvidia.</p>"),
        )
        .expect("text");

        assert!(text.contains("competitors"));
        assert!(!text.contains("Give me a deep research about the Nvidia stock"));
    }

    #[test]
    fn derive_inbound_email_text_from_html_preserves_visible_text_and_links() {
        let text = derive_inbound_email_text(
            None,
            None,
            Some(r#"<div>Hello</div><p>See <a href="https://example.com/doc">the doc</a>.</p>"#),
        )
        .expect("text");

        assert!(text.contains("Hello"));
        assert!(text.contains("the doc (https://example.com/doc)"));
    }

    #[test]
    fn render_email_html_ignores_inline_data_images() {
        let payload = PostmarkInbound {
            from: Some("wjke@uchicago.edu".to_string()),
            to: Some("oliver@dowhiz.com".to_string()),
            cc: None,
            bcc: None,
            to_full: None,
            cc_full: None,
            bcc_full: None,
            reply_to: None,
            subject: Some("Re:".to_string()),
            text_body: Some("Main text".to_string()),
            stripped_text_reply: None,
            html_body: Some(
                r#"<p>Main text</p><img src="data:image/png;base64,AAAA" alt="image.png">"#
                    .to_string(),
            ),
            message_id: Some("msg-1".to_string()),
            headers: None,
            attachments: None,
        };

        let html = render_email_html(&payload);
        assert_eq!(html, "<pre>Main text</pre>");
        assert!(!html.contains("data:image"));
    }
}
