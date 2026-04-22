use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use kuchiki::traits::*;
use kuchiki::NodeRef;
use mime_guess::MimeGuess;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SendEmailParams {
    pub subject: String,
    pub html_path: PathBuf,
    pub attachments_dir: PathBuf,
    pub from: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    /// Reply-To address - where replies should be sent
    /// If set, this overrides the default reply behavior
    pub reply_to: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostmarkSendResponse {
    pub error_code: i64,
    pub message: String,
    #[serde(rename = "MessageID", alias = "MessageId")]
    pub message_id: String,
    pub submitted_at: String,
    pub to: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SendEmailError {
    #[error("missing environment variable: {0}")]
    MissingEnv(&'static str),
    #[error("missing from address")]
    MissingFrom,
    #[error("missing recipient in To list")]
    MissingRecipient,
    #[error("failed to read file: {0}")]
    Io(#[from] std::io::Error),
    #[error("postmark request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("postmark returned error: {0}")]
    Postmark(String),
    #[error("failed to parse json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct PostmarkSendRequest {
    from: String,
    to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bcc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_to: Option<String>,
    subject: String,
    text_body: String,
    html_body: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    headers: Vec<PostmarkHeader>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachments: Vec<PostmarkAttachment>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct PostmarkAttachment {
    name: String,
    content: String,
    content_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct PostmarkHeader {
    name: String,
    value: String,
}

const DOWHIZ_EMAIL_SHELL_MARKER: &str = r#"data-dowhiz-email-shell="true""#;
const DOWHIZ_EMAIL_CONTENT_START: &str = "<!-- dowhiz-email-content:start -->";
const DOWHIZ_EMAIL_CONTENT_END: &str = "<!-- dowhiz-email-content:end -->";
const DOWHIZ_EMAIL_CONTENT_ROOT_ATTR: &str = "data-dw-email-content-root";
const DOWHIZ_EMAIL_CONTENT_ROOT_SELECTOR: &str = r#"div[data-dw-email-content-root="true"]"#;
const DOWHIZ_EMAIL_TABLE_WRAP_MARKER: &str = r#"data-dw-table-wrap="true""#;
const DOWHIZ_EMAIL_TABLE_SCROLL_MARKER: &str = r#"data-dw-table-scroll="true""#;
const DOWHIZ_EMAIL_TABLE_INNER_MARKER: &str = r#"data-dw-table-inner="true""#;
const DOWHIZ_EMAIL_DATA_TABLE_ATTR: &str = "data-dw-enhanced-table";
const DEFAULT_EMAIL_SUBJECT: &str = "DoWhiz update";
const EMAIL_PREHEADER_MAX_CHARS: usize = 140;

pub fn normalize_email_html(subject: &str, raw_html: &str) -> String {
    if is_already_normalized_email(raw_html) {
        return raw_html.to_string();
    }

    let normalized_subject = normalized_email_subject(subject);
    let escaped_subject = html_escape(&normalized_subject);
    let extra_styles = extract_style_blocks(raw_html);
    let body_source = extract_html_body(raw_html).trim();
    let content_html = if body_source.is_empty() {
        "<p>(no content)</p>".to_string()
    } else if looks_like_html_fragment(body_source) {
        body_source.to_string()
    } else {
        wrap_plain_text_body(body_source)
    };
    let content_html = enhance_email_content_html(&content_html);
    let preheader = html_escape(&build_preheader(&normalized_subject, &content_html));

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>{escaped_subject}</title>
    <style>
      body,
      table,
      td,
      a {{
        -webkit-text-size-adjust: 100%;
        -ms-text-size-adjust: 100%;
      }}

      table,
      td {{
        mso-table-lspace: 0pt;
        mso-table-rspace: 0pt;
      }}

      img {{
        border: 0;
        outline: none;
        text-decoration: none;
        -ms-interpolation-mode: bicubic;
      }}

      body {{
        margin: 0 !important;
        padding: 0 !important;
        width: 100% !important;
        min-width: 100% !important;
        background-color: #f6f8fb;
        color: #1f1f22;
      }}

      a {{
        color: #8d3b16;
      }}

      a[x-apple-data-detectors] {{
        color: inherit !important;
        text-decoration: none !important;
      }}

      .dw-preheader {{
        display: none !important;
        visibility: hidden;
        opacity: 0;
        color: transparent;
        height: 0;
        width: 0;
        overflow: hidden;
        mso-hide: all;
        font-size: 1px;
        line-height: 1px;
      }}

      .dw-card {{
        width: 100%;
        max-width: 780px;
      }}

      .dw-card-shell {{
        width: 100%;
        background-color: #ffffff;
        border: 1px solid rgba(16, 18, 22, 0.10);
        border-radius: 16px;
        overflow: hidden;
      }}

      .dw-content,
      .dw-content p,
      .dw-content li,
      .dw-content div,
      .dw-content span,
      .dw-content td,
      .dw-content th,
      .dw-content blockquote,
      .dw-content a,
      .dw-content code,
      .dw-content pre {{
        word-break: break-word;
        word-wrap: break-word;
        overflow-wrap: anywhere;
      }}

      .dw-content > div,
      .dw-content > section,
      .dw-content > article,
      .dw-content > table {{
        width: 100% !important;
        max-width: 100% !important;
        margin-left: 0 !important;
        margin-right: 0 !important;
      }}

      .dw-content p {{
        margin: 0 0 1.15em;
      }}

      .dw-content ul,
      .dw-content ol {{
        margin: 0 0 1.25em 1.25em;
        padding: 0;
      }}

      .dw-content li {{
        margin: 0 0 0.65em;
      }}

      .dw-content h1,
      .dw-content h2,
      .dw-content h3,
      .dw-content h4,
      .dw-content h5,
      .dw-content h6 {{
        margin: 0 0 0.7em;
        color: #1f1f22;
        line-height: 1.22;
      }}

      .dw-content blockquote {{
        margin: 0 0 1.25em;
        padding: 0 0 0 16px;
        border-left: 3px solid #d7dde6;
        color: #5b616d;
      }}

      .dw-content pre {{
        margin: 0 0 1.25em;
        padding: 16px;
        border: 1px solid #e4e8ee;
        border-radius: 12px;
        background-color: #f8fafc;
        color: #1f1f22;
        white-space: pre-wrap !important;
        font-size: 14px;
        line-height: 1.6;
        font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
      }}

      .dw-content code {{
        font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
      }}

      .dw-content img {{
        display: block;
        max-width: 100% !important;
        height: auto !important;
        border-radius: 12px;
      }}

      .dw-table-wrap {{
        width: 100%;
        max-width: 100%;
        margin: 0 0 1.35em;
      }}

      .dw-table-hint {{
        margin: 0 0 8px;
        font-size: 12px;
        line-height: 1.4;
        color: #6b7280;
      }}

      .dw-table-scroll {{
        width: 100%;
        max-width: 100%;
        overflow-x: auto;
        overflow-y: hidden;
        -webkit-overflow-scrolling: touch;
        border: 1px solid #e4e8ee;
        border-radius: 14px;
        background-color: #ffffff;
      }}

      .dw-table-inner {{
        min-width: 100%;
      }}

      .dw-content table {{
        width: 100% !important;
        max-width: 100% !important;
        border-collapse: collapse;
      }}

      .dw-content table.dw-data-table {{
        width: 100% !important;
        max-width: none !important;
        border-collapse: separate !important;
        border-spacing: 0 !important;
        table-layout: auto !important;
      }}

      .dw-content th,
      .dw-content td {{
        border: 1px solid #e4e8ee;
        padding: 10px 12px;
        vertical-align: top;
      }}

      .dw-content table.dw-data-table th {{
        background-color: #f6f8fb;
        color: #1f1f22;
        font-weight: 700;
        text-align: left;
      }}

      .dw-content table.dw-data-table th,
      .dw-content table.dw-data-table td {{
        font-size: 14px;
        line-height: 1.55;
      }}

      .dw-content hr {{
        margin: 1.5em 0;
        border: 0;
        border-top: 1px solid #e4e8ee;
      }}

      @media screen and (max-width: 640px) {{
        .dw-shell-pad {{
          padding: 10px !important;
        }}

        .dw-card-hero {{
          padding: 20px 18px 16px !important;
        }}

        .dw-card-body {{
          padding: 24px 18px 20px !important;
          font-size: 15px !important;
          line-height: 1.72 !important;
        }}

        .dw-card-footer {{
          padding: 0 18px 18px !important;
        }}

        .dw-subject {{
          font-size: 28px !important;
        }}

        .dw-table-wrap {{
          margin-bottom: 1.1em !important;
        }}

        .dw-table-scroll {{
          border-radius: 12px !important;
        }}

        .dw-content table.dw-data-table th,
        .dw-content table.dw-data-table td {{
          padding: 9px 10px !important;
          font-size: 13px !important;
          line-height: 1.45 !important;
        }}
      }}
    </style>
{extra_styles}
  </head>
  <body style="margin: 0; padding: 0; background-color: #f6f8fb;">
    <div class="dw-preheader">{preheader}</div>
    <table role="presentation" cellpadding="0" cellspacing="0" border="0" width="100%" style="width: 100%; background-color: #f6f8fb;">
      <tr>
        <td align="center" class="dw-shell-pad" style="padding: 20px 12px 36px;">
          <table role="presentation" cellpadding="0" cellspacing="0" border="0" width="100%" class="dw-card" {marker} style="width: 100%; max-width: 780px;">
            <tr>
              <td style="padding: 0;">
                <table role="presentation" cellpadding="0" cellspacing="0" border="0" width="100%" class="dw-card-shell" style="width: 100%; background-color: #ffffff; border: 1px solid rgba(16, 18, 22, 0.10); border-radius: 16px; overflow: hidden;">
                  <tr>
                    <td class="dw-card-hero" style="padding: 24px 32px 18px; background-color: #ffffff; border-bottom: 1px solid #e4e8ee;">
                      <p style="margin: 0 0 16px; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Noto Sans SC', 'Microsoft YaHei', sans-serif;">
                        <span style="display: inline-block; padding: 8px 14px; border-radius: 999px; border: 1px solid rgba(141, 59, 22, 0.12); background-color: #fff7ee; font-size: 13px; line-height: 1; font-weight: 600; color: #8d3b16;">
                          DoWhiz digital employee
                        </span>
                      </p>
                      <h1 class="dw-subject" style="margin: 0 0 10px; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Noto Sans SC', 'Microsoft YaHei', sans-serif; font-size: 34px; line-height: 1.12; font-weight: 700; letter-spacing: -0.01em; color: #1f1f22;">
                        {escaped_subject}
                      </h1>
                      <p style="margin: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Noto Sans SC', 'Microsoft YaHei', sans-serif; font-size: 15px; line-height: 1.6; color: #5b616d;">
                        Reply directly to continue this thread with DoWhiz.
                      </p>
                    </td>
                  </tr>
                  <tr>
                    <td class="dw-card-body" style="padding: 28px 32px 20px; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Noto Sans SC', 'Microsoft YaHei', sans-serif; font-size: 16px; line-height: 1.76; color: #1f1f22;">
                      <div class="dw-content" style="font-size: 16px; line-height: 1.76; color: #1f1f22;">
                        {content_start}{content_html}{content_end}
                      </div>
                    </td>
                  </tr>
                  <tr>
                    <td class="dw-card-footer" style="padding: 0 32px 24px; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Noto Sans SC', 'Microsoft YaHei', sans-serif; font-size: 13px; line-height: 1.6; color: #838a96;">
                      Sent by DoWhiz. If you reply, the same task thread will continue.
                    </td>
                  </tr>
                </table>
              </td>
            </tr>
          </table>
        </td>
      </tr>
    </table>
  </body>
</html>
"#,
        escaped_subject = escaped_subject,
        preheader = preheader,
        extra_styles = if extra_styles.trim().is_empty() {
            String::new()
        } else {
            format!("    {}\n", extra_styles.trim())
        },
        marker = DOWHIZ_EMAIL_SHELL_MARKER,
        content_start = DOWHIZ_EMAIL_CONTENT_START,
        content_html = content_html,
        content_end = DOWHIZ_EMAIL_CONTENT_END,
    )
}

pub fn normalize_email_html_file(subject: &str, html_path: &Path) -> Result<(), std::io::Error> {
    let raw_html = fs::read_to_string(html_path)?;
    let normalized = normalize_email_html(subject, &raw_html);
    if normalized != raw_html {
        fs::write(html_path, normalized)?;
    }
    Ok(())
}

pub fn send_email(params: &SendEmailParams) -> Result<PostmarkSendResponse, SendEmailError> {
    dotenvy::dotenv().ok();

    let token = env::var("POSTMARK_SERVER_TOKEN")
        .map_err(|_| SendEmailError::MissingEnv("POSTMARK_SERVER_TOKEN"))?;
    if token.trim().is_empty() {
        return Err(SendEmailError::MissingEnv("POSTMARK_SERVER_TOKEN"));
    }
    let from = params
        .from
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .ok_or(SendEmailError::MissingFrom)?;

    let to = join_recipients(&params.to).ok_or(SendEmailError::MissingRecipient)?;
    let cc = join_recipients(&params.cc);
    let mut bcc_list = params.bcc.clone();
    if !bcc_list
        .iter()
        .any(|addr| addr.trim().eq_ignore_ascii_case(&from))
    {
        bcc_list.push(from.clone());
    }
    let bcc = join_recipients(&bcc_list);

    let raw_html_body = fs::read_to_string(&params.html_path)?;
    let html_body = normalize_email_html(&params.subject, &raw_html_body);
    let mut text_body = plain_text_body_from_html(&html_body);
    if text_body.trim().is_empty() {
        text_body = "(no content)".to_string();
    }

    let attachments = load_attachments(&params.attachments_dir)?;

    let mut headers = Vec::new();
    if let Some(value) = clean_header_value(&params.in_reply_to) {
        headers.push(PostmarkHeader {
            name: "In-Reply-To".to_string(),
            value,
        });
    }
    if let Some(value) = clean_header_value(&params.references) {
        headers.push(PostmarkHeader {
            name: "References".to_string(),
            value,
        });
    }

    let reply_to = clean_header_value(&params.reply_to);

    let payload = PostmarkSendRequest {
        from,
        to,
        cc,
        bcc,
        reply_to,
        subject: params.subject.clone(),
        text_body,
        html_body,
        headers,
        attachments,
    };

    let api_base = env::var("POSTMARK_API_BASE_URL")
        .unwrap_or_else(|_| "https://api.postmarkapp.com".to_string());
    let url = format!("{}/email", api_base.trim_end_matches('/'));

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("X-Postmark-Server-Token", token)
        .json(&payload)
        .send()?;

    let status = response.status();
    let body = response.text()?;
    if !status.is_success() {
        return Err(SendEmailError::Postmark(format!(
            "status {}: {}",
            status, body
        )));
    }

    Ok(serde_json::from_str(&body)?)
}

fn join_recipients(list: &[String]) -> Option<String> {
    let mut cleaned = Vec::new();
    for entry in list {
        let trimmed = entry.trim();
        if !trimmed.is_empty() {
            let sanitized = sanitize_recipient(trimmed);
            if !sanitized.is_empty() {
                cleaned.push(sanitized);
            }
        }
    }
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.join(", "))
    }
}

fn sanitize_recipient(value: &str) -> String {
    if has_unbalanced_quotes(value) {
        if let Some(email) = extract_email_address(value) {
            return email;
        }
    }
    value.to_string()
}

fn has_unbalanced_quotes(value: &str) -> bool {
    value.chars().filter(|ch| *ch == '"').count() % 2 == 1
}

fn extract_email_address(value: &str) -> Option<String> {
    if let Some(start) = value.find('<') {
        let remainder = &value[start + 1..];
        if let Some(end) = remainder.find('>') {
            return normalize_email(&remainder[..end]);
        }
    }
    for token in value.split([',', ';', ' ', '\t', '\n', '\r']) {
        if let Some(email) = normalize_email(token) {
            return Some(email);
        }
    }
    None
}

fn normalize_email(raw: &str) -> Option<String> {
    let mut value = raw.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(stripped) = value.strip_prefix("mailto:") {
        value = stripped.trim();
    }
    value = value.trim_matches(|ch: char| matches!(ch, '<' | '>' | '"' | '\'' | ',' | ';'));
    if !value.contains('@') {
        return None;
    }
    let mut parts = value.splitn(2, '@');
    let local = parts.next().unwrap_or("").trim();
    let domain = parts.next().unwrap_or("").trim();
    if local.is_empty() || domain.is_empty() {
        return None;
    }
    Some(format!("{}@{}", local, domain))
}

fn clean_header_value(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|trimmed| !trimmed.is_empty())
        .map(|trimmed| trimmed.to_string())
}

fn plain_text_body_from_html(html: &str) -> String {
    let source = extract_shell_content_html(html).unwrap_or(html);
    let text = render_html_text(source);
    if text.trim().is_empty() {
        "(no content)".to_string()
    } else {
        text
    }
}

fn build_preheader(subject: &str, content_html: &str) -> String {
    let preview_body = render_html_text(content_html);
    let compact_body = compact_whitespace(&preview_body);
    let combined = if compact_body.is_empty() {
        subject.trim().to_string()
    } else if subject.trim().is_empty() {
        compact_body
    } else {
        format!("{} | {}", subject.trim(), compact_body)
    };
    truncate_chars(&combined, EMAIL_PREHEADER_MAX_CHARS)
}

fn normalized_email_subject(subject: &str) -> String {
    let trimmed = subject.trim();
    if trimmed.is_empty() {
        DEFAULT_EMAIL_SUBJECT.to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_already_normalized_email(html: &str) -> bool {
    html.contains(DOWHIZ_EMAIL_SHELL_MARKER)
}

fn extract_shell_content_html(html: &str) -> Option<&str> {
    let start = html.find(DOWHIZ_EMAIL_CONTENT_START)?;
    let content_start = start + DOWHIZ_EMAIL_CONTENT_START.len();
    let end_rel = html[content_start..].find(DOWHIZ_EMAIL_CONTENT_END)?;
    Some(&html[content_start..content_start + end_rel])
}

fn extract_html_body(raw_html: &str) -> &str {
    let lower = raw_html.to_ascii_lowercase();
    let Some(body_start) = lower.find("<body") else {
        return raw_html;
    };
    let Some(open_end_rel) = lower[body_start..].find('>') else {
        return raw_html;
    };
    let content_start = body_start + open_end_rel + 1;
    let Some(close_rel) = lower[content_start..].rfind("</body>") else {
        return &raw_html[content_start..];
    };
    &raw_html[content_start..content_start + close_rel]
}

fn extract_style_blocks(raw_html: &str) -> String {
    let lower = raw_html.to_ascii_lowercase();
    let mut search_start = 0;
    let mut styles = Vec::new();

    while let Some(open_rel) = lower[search_start..].find("<style") {
        let open = search_start + open_rel;
        let Some(tag_end_rel) = lower[open..].find('>') else {
            break;
        };
        let content_start = open + tag_end_rel + 1;
        let Some(close_rel) = lower[content_start..].find("</style>") else {
            break;
        };
        let close = content_start + close_rel + "</style>".len();
        styles.push(raw_html[open..close].to_string());
        search_start = close;
    }

    styles.join("\n")
}

fn enhance_email_content_html(content_html: &str) -> String {
    if !content_html.to_ascii_lowercase().contains("<table") {
        return content_html.to_string();
    }

    let document = kuchiki::parse_html().one(format!(
        r#"<!DOCTYPE html><html><body><div {root_attr}="true">{content_html}</div></body></html>"#,
        root_attr = DOWHIZ_EMAIL_CONTENT_ROOT_ATTR,
        content_html = content_html
    ));
    let tables: Vec<NodeRef> = match document.select("table") {
        Ok(nodes) => nodes.map(|node| node.as_node().clone()).collect(),
        Err(_) => return content_html.to_string(),
    };

    let mut changed = false;
    for table in tables {
        if !should_enhance_data_table(&table) {
            continue;
        }
        let column_count = table_max_columns(&table);
        if column_count < 2 {
            continue;
        }
        if wrap_table_for_mobile(&table, column_count) {
            changed = true;
        }
    }

    if !changed {
        return content_html.to_string();
    }

    match document.select_first(DOWHIZ_EMAIL_CONTENT_ROOT_SELECTOR) {
        Ok(root) => children_as_html(root.as_node()),
        Err(_) => content_html.to_string(),
    }
}

fn should_enhance_data_table(table: &NodeRef) -> bool {
    let Some(element) = table.as_element() else {
        return false;
    };
    if element.name.local.as_ref() != "table" {
        return false;
    }
    if table_has_ancestor(table, "table") {
        return false;
    }

    let attrs = element.attributes.borrow();
    if attrs.contains(DOWHIZ_EMAIL_DATA_TABLE_ATTR) {
        return false;
    }
    !matches!(
        attrs.get("role")
            .map(|value| value.trim().to_ascii_lowercase()),
        Some(role) if role == "presentation" || role == "none"
    )
}

fn wrap_table_for_mobile(table: &NodeRef, column_count: usize) -> bool {
    let min_width = preferred_table_min_width(column_count);
    let Some((wrapper, inner)) = build_table_wrapper(min_width, column_count >= 4) else {
        return false;
    };

    append_class(table, "dw-data-table");
    set_attribute(table, DOWHIZ_EMAIL_DATA_TABLE_ATTR, "true");
    append_style(
        table,
        "width: 100% !important; max-width: none !important; border-collapse: separate; border-spacing: 0; table-layout: auto;",
    );
    style_table_cells(table);
    table.insert_before(wrapper);
    inner.append(table.clone());
    true
}

fn build_table_wrapper(min_width: usize, show_hint: bool) -> Option<(NodeRef, NodeRef)> {
    let document = kuchiki::parse_html().one(format!(
        r#"<!DOCTYPE html>
<html>
  <body>
    <div class="dw-table-wrap" {wrap_marker} style="width: 100%; max-width: 100%; margin: 0 0 20px;">
      {hint_html}
      <div class="dw-table-scroll" {scroll_marker} style="width: 100%; max-width: 100%; overflow-x: auto; overflow-y: hidden; -webkit-overflow-scrolling: touch; border: 1px solid #e4e8ee; border-radius: 14px; background-color: #ffffff;">
        <div class="dw-table-inner" {inner_marker} style="min-width: {min_width}px;"></div>
      </div>
    </div>
  </body>
</html>"#,
        wrap_marker = DOWHIZ_EMAIL_TABLE_WRAP_MARKER,
        hint_html = if show_hint {
            r#"<p class="dw-table-hint" aria-hidden="true" style="margin: 0 0 8px; font-size: 12px; line-height: 1.4; color: #6b7280;">Swipe horizontally to view all columns.</p>"#
        } else {
            ""
        },
        scroll_marker = DOWHIZ_EMAIL_TABLE_SCROLL_MARKER,
        inner_marker = DOWHIZ_EMAIL_TABLE_INNER_MARKER,
        min_width = min_width
    ));
    let wrapper = document
        .select_first("div.dw-table-wrap")
        .ok()?
        .as_node()
        .clone();
    let inner = document
        .select_first("div.dw-table-inner")
        .ok()?
        .as_node()
        .clone();
    Some((wrapper, inner))
}

fn style_table_cells(table: &NodeRef) {
    if let Ok(headers) = table.select("th") {
        for header in headers {
            append_style(
                header.as_node(),
                "padding: 12px 14px; border: 1px solid #e4e8ee; vertical-align: top; text-align: left; background-color: #f6f8fb; font-weight: 700;",
            );
        }
    }
    if let Ok(cells) = table.select("td") {
        for cell in cells {
            append_style(
                cell.as_node(),
                "padding: 12px 14px; border: 1px solid #e4e8ee; vertical-align: top;",
            );
        }
    }
}

fn append_class(node: &NodeRef, class_name: &str) {
    let Some(element) = node.as_element() else {
        return;
    };
    let mut attrs = element.attributes.borrow_mut();
    let existing = attrs.get("class").unwrap_or("").trim().to_string();
    if existing
        .split_whitespace()
        .any(|value| value.eq_ignore_ascii_case(class_name))
    {
        return;
    }
    if existing.is_empty() {
        attrs.insert("class", class_name.to_string());
    } else {
        attrs.insert("class", format!("{existing} {class_name}"));
    }
}

fn set_attribute(node: &NodeRef, name: &str, value: &str) {
    let Some(element) = node.as_element() else {
        return;
    };
    element
        .attributes
        .borrow_mut()
        .insert(name, value.to_string());
}

fn append_style(node: &NodeRef, style_snippet: &str) {
    let Some(element) = node.as_element() else {
        return;
    };
    let cleaned = style_snippet.trim().trim_end_matches(';');
    if cleaned.is_empty() {
        return;
    }

    let mut attrs = element.attributes.borrow_mut();
    if let Some(existing) = attrs.get_mut("style") {
        let trimmed = existing.trim();
        if trimmed.is_empty() {
            *existing = format!("{cleaned};");
            return;
        }
        if !trimmed.ends_with(';') {
            existing.push(';');
        }
        if !existing.ends_with(' ') {
            existing.push(' ');
        }
        existing.push_str(cleaned);
        existing.push(';');
        return;
    }
    attrs.insert("style", format!("{cleaned};"));
}

fn table_has_ancestor(node: &NodeRef, tag_name: &str) -> bool {
    node.ancestors().any(|ancestor| {
        ancestor
            .as_element()
            .map(|element| element.name.local.as_ref() == tag_name)
            .unwrap_or(false)
    })
}

fn table_max_columns(table: &NodeRef) -> usize {
    let Ok(rows) = table.select("tr") else {
        return 0;
    };
    rows.map(|row| row_column_count(row.as_node()))
        .max()
        .unwrap_or(0)
}

fn row_column_count(row: &NodeRef) -> usize {
    row.children().filter_map(table_cell_span).sum()
}

fn table_cell_span(node: NodeRef) -> Option<usize> {
    let element = node.as_element()?;
    if !matches!(element.name.local.as_ref(), "td" | "th") {
        return None;
    }
    let span = element
        .attributes
        .borrow()
        .get("colspan")
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1);
    Some(span)
}

fn preferred_table_min_width(column_count: usize) -> usize {
    column_count.saturating_mul(140).clamp(360, 960)
}

fn children_as_html(node: &NodeRef) -> String {
    node.children()
        .map(|child| child.to_string())
        .collect::<Vec<_>>()
        .join("")
}

fn render_html_text(html: &str) -> String {
    let document = kuchiki::parse_html().one(format!(
        r#"<!DOCTYPE html><html><body><div {root_attr}="true">{html}</div></body></html>"#,
        root_attr = DOWHIZ_EMAIL_CONTENT_ROOT_ATTR,
        html = html
    ));
    let Ok(root) = document.select_first(DOWHIZ_EMAIL_CONTENT_ROOT_SELECTOR) else {
        return String::new();
    };

    let mut out = String::new();
    for child in root.as_node().children() {
        render_html_text_node(&child, &mut out, false);
    }
    normalize_rendered_text(&out)
}

fn render_html_text_node(node: &NodeRef, out: &mut String, preserve_whitespace: bool) {
    if let Some(text) = node.as_text() {
        append_rendered_text(out, &text.borrow(), preserve_whitespace);
        return;
    }

    let Some(element) = node.as_element() else {
        for child in node.children() {
            render_html_text_node(&child, out, preserve_whitespace);
        }
        return;
    };

    let tag = element.name.local.as_ref();
    if element
        .attributes
        .borrow()
        .get("aria-hidden")
        .map(|value| value.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return;
    }
    match tag {
        "br" => push_rendered_line_break(out),
        "p" | "div" | "section" | "article" | "header" | "footer" | "blockquote" | "h1" | "h2"
        | "h3" | "h4" | "h5" | "h6" | "table" | "thead" | "tbody" => {
            push_rendered_block_break(out);
            for child in node.children() {
                render_html_text_node(&child, out, preserve_whitespace);
            }
            push_rendered_block_break(out);
        }
        "ul" | "ol" => {
            push_rendered_block_break(out);
            for child in node.children() {
                render_html_text_node(&child, out, preserve_whitespace);
            }
            push_rendered_block_break(out);
        }
        "li" => {
            push_rendered_list_break(out);
            out.push_str("- ");
            for child in node.children() {
                render_html_text_node(&child, out, preserve_whitespace);
            }
            push_rendered_line_break(out);
        }
        "tr" => {
            push_rendered_list_break(out);
            let mut first_cell = true;
            for child in node.children() {
                if is_rendered_table_cell(&child) {
                    if !first_cell {
                        trim_rendered_trailing_whitespace(out);
                        out.push_str(" | ");
                    }
                    first_cell = false;
                }
                render_html_text_node(&child, out, preserve_whitespace);
            }
            push_rendered_line_break(out);
        }
        "pre" => {
            push_rendered_block_break(out);
            for child in node.children() {
                render_html_text_node(&child, out, true);
            }
            push_rendered_block_break(out);
        }
        _ => {
            for child in node.children() {
                render_html_text_node(&child, out, preserve_whitespace);
            }
        }
    }
}

fn append_rendered_text(out: &mut String, text: &str, preserve_whitespace: bool) {
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

fn push_rendered_block_break(out: &mut String) {
    trim_rendered_trailing_whitespace(out);
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

fn push_rendered_list_break(out: &mut String) {
    trim_rendered_trailing_whitespace(out);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

fn push_rendered_line_break(out: &mut String) {
    trim_rendered_trailing_whitespace(out);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

fn trim_rendered_trailing_whitespace(out: &mut String) {
    while matches!(out.chars().last(), Some(' ' | '\t')) {
        out.pop();
    }
}

fn is_rendered_table_cell(node: &NodeRef) -> bool {
    node.as_element()
        .map(|element| matches!(element.name.local.as_ref(), "td" | "th"))
        .unwrap_or(false)
}

fn normalize_rendered_text(input: &str) -> String {
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

fn looks_like_html_fragment(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.contains("<p")
        || lower.contains("<div")
        || lower.contains("<span")
        || lower.contains("<table")
        || lower.contains("<tbody")
        || lower.contains("<tr")
        || lower.contains("<td")
        || lower.contains("<th")
        || lower.contains("<br")
        || lower.contains("<ul")
        || lower.contains("<ol")
        || lower.contains("<li")
        || lower.contains("<a ")
        || lower.contains("<img")
        || lower.contains("<h1")
        || lower.contains("<h2")
        || lower.contains("<h3")
        || lower.contains("<blockquote")
        || lower.contains("</")
}

fn wrap_plain_text_body(raw: &str) -> String {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let paragraphs = normalized
        .split("\n\n")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let lines = segment
                .lines()
                .map(|line| html_escape(line.trim_end()))
                .collect::<Vec<_>>()
                .join("<br />");
            format!("<p>{}</p>", lines)
        })
        .collect::<Vec<_>>();

    if paragraphs.is_empty() {
        "<p>(no content)</p>".to_string()
    } else {
        paragraphs.join("\n")
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn compact_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let total_chars = value.chars().count();
    if total_chars <= max_chars {
        return value.to_string();
    }

    let mut out = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    out.push('…');
    out
}

fn ascii_safe_attachment_name(path: &Path, used_names: &mut HashSet<String>) -> String {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let mut base = sanitize_ascii_attachment_stem(stem);
    if base.is_empty() {
        base = "attachment".to_string();
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(sanitize_ascii_attachment_extension)
        .filter(|value| !value.is_empty());

    uniquify_attachment_name(base, extension.as_deref(), used_names)
}

fn sanitize_ascii_attachment_stem(value: &str) -> String {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    let mut deduped = Vec::new();
    for token in tokens {
        let duplicate = deduped
            .last()
            .map(|last: &String| last.eq_ignore_ascii_case(&token))
            .unwrap_or(false);
        if !duplicate {
            deduped.push(token);
        }
    }

    deduped.join("_")
}

fn sanitize_ascii_attachment_extension(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn uniquify_attachment_name(
    base: String,
    extension: Option<&str>,
    used_names: &mut HashSet<String>,
) -> String {
    let mut suffix = 1;

    loop {
        let stem = if suffix == 1 {
            base.clone()
        } else {
            format!("{}_{}", base, suffix)
        };
        let candidate = match extension {
            Some(ext) if !ext.is_empty() => format!("{}.{}", stem, ext),
            _ => stem,
        };
        if used_names.insert(candidate.to_ascii_lowercase()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn load_attachments(dir: &Path) -> Result<Vec<PostmarkAttachment>, std::io::Error> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut attachments = Vec::new();
    let mut used_names = HashSet::new();
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let content = fs::read(&path)?;
        let mime = MimeGuess::from_path(&path)
            .first_or_octet_stream()
            .essence_str()
            .to_string();
        let attachment = PostmarkAttachment {
            name: ascii_safe_attachment_name(&path, &mut used_names),
            content: BASE64_STANDARD.encode(content),
            content_type: mime,
        };
        attachments.push(attachment);
    }

    Ok(attachments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_email_html_wraps_fragment_with_branded_responsive_shell() {
        let normalized = normalize_email_html(
            "Status update",
            r#"<div style="max-width: 520px; margin: 0 auto;"><p>Hello team</p></div>"#,
        );

        assert!(normalized.contains(DOWHIZ_EMAIL_SHELL_MARKER));
        assert!(normalized.contains("max-width: 780px"));
        assert!(normalized.contains("overflow-wrap: anywhere"));
        assert!(normalized.contains("DoWhiz digital employee"));
        assert!(normalized.contains("Status update"));
        assert!(normalized
            .contains(r#"<div style="max-width: 520px; margin: 0 auto;"><p>Hello team</p></div>"#));
    }

    #[test]
    fn normalize_email_html_is_idempotent() {
        let once = normalize_email_html("Status update", "<p>Hello team</p>");
        let twice = normalize_email_html("Status update", &once);

        assert_eq!(once, twice);
    }

    #[test]
    fn normalize_email_html_wraps_plain_text_into_paragraphs() {
        let normalized = normalize_email_html("Plain text", "First line\n\nSecond line");

        assert!(normalized.contains("<p>First line</p>"));
        assert!(normalized.contains("<p>Second line</p>"));
    }

    #[test]
    fn plain_text_body_from_html_uses_shell_content_only() {
        let normalized =
            normalize_email_html("Status update", "<p>Hello <strong>team</strong></p>");

        assert_eq!(plain_text_body_from_html(&normalized), "Hello team");
    }

    #[test]
    fn plain_text_body_from_html_keeps_table_structure_readable() {
        let normalized = normalize_email_html(
            "Weekly metrics",
            r#"
            <table>
              <tr>
                <th>Metric</th>
                <th>Monday</th>
                <th>Tuesday</th>
              </tr>
              <tr>
                <td>New tickets</td>
                <td>12</td>
                <td>15</td>
              </tr>
            </table>
            "#,
        );

        assert_eq!(
            plain_text_body_from_html(&normalized),
            "Metric | Monday | Tuesday\nNew tickets | 12 | 15"
        );
    }

    #[test]
    fn normalize_email_html_preserves_investment_contract_labels() {
        let normalized = normalize_email_html(
            "NVDA investment memo",
            r#"
            <h2>Final Recommendation</h2>
            <ul>
              <li><strong>Rating:</strong> Wait</li>
              <li><strong>Horizon:</strong> Medium-term (stated)</li>
              <li><strong>Confidence:</strong> Medium</li>
              <li><strong>Timing Verdict:</strong> Wait</li>
              <li><strong>Add Criteria:</strong> Better entry after earnings.</li>
              <li><strong>Invalidation Criteria:</strong> Margin guide weakens.</li>
              <li><strong>Biggest Near-Term Risk:</strong> Earnings volatility.</li>
              <li><strong>Biggest Long-Term Strength:</strong> AI compute leadership.</li>
            </ul>
            <h2>Verified Facts</h2><ul><li>Fact one.</li></ul>
            <h2>Derived Metrics</h2><ul><li>Metric: price / eps = 10x</li></ul>
            <h2>Scenario Analysis</h2>
            <p><strong>Bull Case:</strong> Demand stays strong.</p>
            <p><strong>Base Case:</strong> Growth normalizes.</p>
            <p><strong>Bear Case:</strong> Spending slows.</p>
            "#,
        );

        let text = plain_text_body_from_html(&normalized);
        for label in [
            "Rating:",
            "Horizon:",
            "Confidence:",
            "Timing Verdict:",
            "Verified Facts",
            "Derived Metrics",
            "Bull Case:",
            "Base Case:",
            "Bear Case:",
            "Add Criteria:",
            "Invalidation Criteria:",
            "Biggest Near-Term Risk:",
            "Biggest Long-Term Strength:",
        ] {
            assert!(
                normalized.contains(label) || text.contains(label),
                "expected normalized email content to preserve label {label}"
            );
        }
    }

    #[test]
    fn normalize_email_html_wraps_data_tables_in_scroll_container() {
        let normalized = normalize_email_html(
            "Weekly metrics",
            r#"
            <table>
              <thead>
                <tr>
                  <th>Metric</th>
                  <th>Monday</th>
                  <th>Tuesday</th>
                  <th>Wednesday</th>
                </tr>
              </thead>
              <tbody>
                <tr>
                  <td>New tickets</td>
                  <td>12</td>
                  <td>15</td>
                  <td>8</td>
                </tr>
              </tbody>
            </table>
            "#,
        );

        assert!(normalized.contains(DOWHIZ_EMAIL_TABLE_WRAP_MARKER));
        assert!(normalized.contains(DOWHIZ_EMAIL_TABLE_SCROLL_MARKER));
        assert!(normalized.contains(r#"data-dw-enhanced-table="true""#));
        assert!(normalized.contains(r#"class="dw-data-table""#));
        assert!(normalized.contains("overflow-x: auto"));
        assert!(normalized.contains("min-width: 560px"));
    }

    #[test]
    fn normalize_email_html_skips_presentation_tables() {
        let normalized = normalize_email_html(
            "Layout table",
            r#"
            <table role="presentation">
              <tr>
                <td>Left</td>
                <td>Right</td>
              </tr>
            </table>
            "#,
        );

        assert!(!normalized.contains(DOWHIZ_EMAIL_TABLE_WRAP_MARKER));
        assert!(!normalized.contains(DOWHIZ_EMAIL_TABLE_SCROLL_MARKER));
        assert!(!normalized.contains(r#"data-dw-enhanced-table="true""#));
        assert!(!normalized.contains(r#"class="dw-data-table""#));
    }

    #[test]
    fn normalize_email_html_counts_colspan_when_sizing_tables() {
        let normalized = normalize_email_html(
            "Capacity plan",
            r#"
            <table>
              <tr>
                <th>Team</th>
                <th colspan="2">Q2 capacity</th>
                <th>Owner</th>
              </tr>
              <tr>
                <td>Platform</td>
                <td>Committed</td>
                <td>Stretch</td>
                <td>Riley</td>
              </tr>
            </table>
            "#,
        );

        assert!(normalized.contains(DOWHIZ_EMAIL_TABLE_WRAP_MARKER));
        assert!(normalized.contains("min-width: 560px"));
    }
}
