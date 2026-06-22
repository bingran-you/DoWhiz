use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use base64::engine::general_purpose::{STANDARD as BASE64_STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{error, info, warn};

use scheduler_module::adapters::postmark::{PostmarkInboundAdapter, PostmarkInboundPayload};
use scheduler_module::channel::{Channel, InboundAdapter};
use scheduler_module::google_auth::{GoogleAuth, GoogleAuthConfig};
use scheduler_module::user_store::extract_emails;

use super::handlers::build_envelope_blocking;
use super::routes::resolve_route;
use super::state::{find_service_address, GatewayState};

const GMAIL_READONLY_SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
const DEFAULT_POLL_INTERVAL_SECS: u64 = 30;
const DEFAULT_LOOKBACK_DAYS: u64 = 2;
const DEFAULT_MAX_RESULTS: u32 = 25;

#[derive(Debug, Clone)]
struct GmailPollerConfig {
    poll_interval_secs: u64,
    max_results: u32,
    query: String,
    state_path: PathBuf,
    bootstrap_process_existing: bool,
    recipient: String,
}

impl GmailPollerConfig {
    fn from_env() -> Option<Self> {
        let enabled = env_flag("GMAIL_POLLER_ENABLED", false);
        if !enabled {
            return None;
        }

        let recipient = env::var("GMAIL_POLLER_RECIPIENT")
            .ok()
            .or_else(|| env::var("GOOGLE_EMPLOYEE_EMAIL").ok())
            .or_else(|| env::var("GOOGLE_SERVICE_ACCOUNT_SUBJECT").ok())
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())?;

        let lookback_days = env_u64("GMAIL_POLLER_LOOKBACK_DAYS", DEFAULT_LOOKBACK_DAYS).max(1);
        let query = env::var("GMAIL_POLLER_QUERY")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("to:{} newer_than:{}d", recipient, lookback_days));

        Some(Self {
            poll_interval_secs: env_u64("GMAIL_POLLER_INTERVAL_SECS", DEFAULT_POLL_INTERVAL_SECS)
                .max(5),
            max_results: env_u64("GMAIL_POLLER_MAX_RESULTS", DEFAULT_MAX_RESULTS as u64)
                .clamp(1, 100) as u32,
            query,
            state_path: env::var("GMAIL_POLLER_STATE_PATH")
                .ok()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".gmail_poller_processed_ids")),
            bootstrap_process_existing: env_flag("GMAIL_POLLER_BOOTSTRAP_PROCESS_EXISTING", false),
            recipient,
        })
    }
}

pub(super) fn spawn_gmail_poller(state: Arc<GatewayState>) {
    let Some(config) = GmailPollerConfig::from_env() else {
        return;
    };

    let mut auth_config = GoogleAuthConfig::from_env();
    auth_config.scopes = Some(vec![GMAIL_READONLY_SCOPE.to_string()]);
    if !auth_config.is_valid() {
        warn!("Gmail poller enabled but Google service account credentials are not configured");
        return;
    }

    info!(
        "Starting Gmail poller: recipient={}, query={:?}, interval={}s, state_path={}",
        config.recipient,
        config.query,
        config.poll_interval_secs,
        config.state_path.display()
    );

    std::thread::spawn(move || {
        let auth = match GoogleAuth::new(auth_config) {
            Ok(auth) => auth,
            Err(err) => {
                error!("failed to initialize Gmail poller auth: {}", err);
                return;
            }
        };
        let client = Client::new();
        let mut processed = load_processed_ids(&config.state_path);
        let mut bootstrapped = !processed.is_empty() || config.bootstrap_process_existing;

        loop {
            match poll_gmail(
                &client,
                &auth,
                &state,
                &config,
                &mut processed,
                &mut bootstrapped,
            ) {
                Ok(count) => {
                    if count > 0 {
                        info!("Gmail poller enqueued {} message(s)", count);
                    }
                }
                Err(err) => {
                    error!("Gmail poller error: {}", err);
                }
            }
            std::thread::sleep(Duration::from_secs(config.poll_interval_secs));
        }
    });
}

fn poll_gmail(
    client: &Client,
    auth: &GoogleAuth,
    state: &GatewayState,
    config: &GmailPollerConfig,
    processed: &mut HashSet<String>,
    bootstrapped: &mut bool,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let token = auth.get_access_token()?;
    let messages = list_messages(client, &token, &config.query, config.max_results)?;
    if !*bootstrapped {
        for message in &messages {
            processed.insert(message.id.clone());
        }
        save_processed_ids(&config.state_path, processed)?;
        *bootstrapped = true;
        info!(
            "Gmail poller bootstrapped {} existing message(s) without enqueue",
            messages.len()
        );
        return Ok(0);
    }

    let mut enqueued = 0usize;
    for summary in messages.into_iter().rev() {
        if processed.contains(&summary.id) {
            continue;
        }

        match process_gmail_message(client, &token, state, config, &summary.id) {
            Ok((should_mark_processed, inserted)) => {
                if should_mark_processed {
                    processed.insert(summary.id.clone());
                    save_processed_ids(&config.state_path, processed)?;
                }
                if inserted {
                    enqueued += 1;
                }
            }
            Err(err) => {
                warn!("failed to process Gmail message {}: {}", summary.id, err);
            }
        }
    }

    Ok(enqueued)
}

fn process_gmail_message(
    client: &Client,
    token: &str,
    state: &GatewayState,
    config: &GmailPollerConfig,
    gmail_message_id: &str,
) -> Result<(bool, bool), Box<dyn std::error::Error + Send + Sync>> {
    let message = get_message(client, token, gmail_message_id)?;
    let raw_payload = gmail_to_postmark_payload(client, token, &message, &config.recipient)?;
    let payload: PostmarkInboundPayload = serde_json::from_slice(&raw_payload)?;

    let Some(address) = find_service_address(&payload, &state.employee_directory.service_addresses)
    else {
        info!(
            "Gmail poller no service address found for gmail_message_id={} subject={:?}",
            gmail_message_id, payload.subject
        );
        return Ok((true, false));
    };

    let Some(route) = resolve_route(Channel::Email, &address, state) else {
        info!(
            "Gmail poller no route for service address={} gmail_message_id={}",
            address, gmail_message_id
        );
        return Ok((true, false));
    };

    let adapter = PostmarkInboundAdapter::new(state.employee_directory.service_addresses.clone());
    let inbound = adapter.parse(&raw_payload)?;
    let external_message_id = payload
        .header_message_id()
        .or(payload.message_id.as_deref())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| Some(format!("gmail:{}", gmail_message_id)));

    let envelope = build_envelope_blocking(
        route,
        Channel::Email,
        external_message_id,
        &inbound,
        &raw_payload,
    )?;
    match state.queue.enqueue(&envelope) {
        Ok(result) => {
            if result.inserted {
                info!(
                    "Gmail poller enqueued message gmail_id={} subject={:?}",
                    gmail_message_id, payload.subject
                );
            } else {
                info!(
                    "Gmail poller skipped duplicate message gmail_id={} subject={:?}",
                    gmail_message_id, payload.subject
                );
            }
            Ok((true, result.inserted))
        }
        Err(err) => Err(Box::new(err)),
    }
}

#[derive(Debug, Deserialize)]
struct GmailListResponse {
    #[serde(default)]
    messages: Vec<GmailMessageSummary>,
}

#[derive(Debug, Deserialize)]
struct GmailMessageSummary {
    id: String,
}

#[derive(Debug, Deserialize)]
struct GmailMessage {
    id: String,
    #[serde(rename = "threadId")]
    thread_id: String,
    payload: GmailPart,
}

#[derive(Debug, Deserialize)]
struct GmailPart {
    #[serde(rename = "mimeType", default)]
    mime_type: Option<String>,
    #[serde(default)]
    filename: Option<String>,
    #[serde(default)]
    headers: Vec<GmailHeader>,
    #[serde(default)]
    body: GmailBody,
    #[serde(default)]
    parts: Vec<GmailPart>,
}

#[derive(Debug, Default, Deserialize)]
struct GmailBody {
    #[serde(default)]
    data: Option<String>,
    #[serde(rename = "attachmentId", default)]
    attachment_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GmailHeader {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct GmailAttachment {
    data: String,
}

#[derive(Debug, Default)]
struct GmailBodyParts {
    text: Vec<String>,
    html: Vec<String>,
    attachments: Vec<Value>,
}

fn list_messages(
    client: &Client,
    token: &str,
    query: &str,
    max_results: u32,
) -> Result<Vec<GmailMessageSummary>, Box<dyn std::error::Error + Send + Sync>> {
    let response = client
        .get("https://gmail.googleapis.com/gmail/v1/users/me/messages")
        .bearer_auth(token)
        .query(&[
            ("q", query.to_string()),
            ("maxResults", max_results.to_string()),
        ])
        .send()?;
    if !response.status().is_success() {
        return Err(format!(
            "Gmail list failed: {} - {}",
            response.status(),
            response.text().unwrap_or_default()
        )
        .into());
    }
    Ok(response.json::<GmailListResponse>()?.messages)
}

fn get_message(
    client: &Client,
    token: &str,
    message_id: &str,
) -> Result<GmailMessage, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}",
        message_id
    );
    let response = client
        .get(url)
        .bearer_auth(token)
        .query(&[("format", "full")])
        .send()?;
    if !response.status().is_success() {
        return Err(format!(
            "Gmail get failed for {}: {} - {}",
            message_id,
            response.status(),
            response.text().unwrap_or_default()
        )
        .into());
    }
    Ok(response.json()?)
}

fn get_attachment(
    client: &Client,
    token: &str,
    message_id: &str,
    attachment_id: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/attachments/{}",
        message_id, attachment_id
    );
    let response = client.get(url).bearer_auth(token).send()?;
    if !response.status().is_success() {
        return Err(format!(
            "Gmail attachment get failed for {}: {} - {}",
            message_id,
            response.status(),
            response.text().unwrap_or_default()
        )
        .into());
    }
    let attachment: GmailAttachment = response.json()?;
    decode_gmail_base64(&attachment.data)
}

fn gmail_to_postmark_payload(
    client: &Client,
    token: &str,
    message: &GmailMessage,
    default_recipient: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let headers = &message.payload.headers;
    let from = header_value(headers, "From").unwrap_or_default();
    let to = header_value(headers, "To").unwrap_or_else(|| default_recipient.to_string());
    let cc = header_value(headers, "Cc");
    let bcc = header_value(headers, "Bcc");
    let reply_to = header_value(headers, "Reply-To");
    let subject = header_value(headers, "Subject");
    let message_id = header_value(headers, "Message-ID").unwrap_or_else(|| message.id.clone());
    let service_recipient = select_service_recipient(&to, default_recipient);

    let mut body_parts = GmailBodyParts::default();
    collect_part_bodies(
        client,
        token,
        &message.id,
        &message.payload,
        &mut body_parts,
    )?;

    let headers_json = headers
        .iter()
        .map(|header| json!({ "Name": header.name, "Value": header.value }))
        .collect::<Vec<_>>();

    let payload = json!({
        "From": from,
        "To": to,
        "Cc": cc,
        "Bcc": bcc,
        "ReplyTo": reply_to,
        "Subject": subject,
        "TextBody": join_body_parts(&body_parts.text),
        "StrippedTextReply": join_body_parts(&body_parts.text),
        "HtmlBody": join_body_parts(&body_parts.html),
        "MessageID": message_id,
        "Headers": headers_json,
        "Attachments": body_parts.attachments,
        "OriginalRecipient": service_recipient,
        "ToFull": [{ "Email": service_recipient, "Name": null, "MailboxHash": null }],
        "MailboxHash": "",
        "Date": header_value(headers, "Date"),
        "GmailMessageID": message.id,
        "GmailThreadID": message.thread_id,
    });

    Ok(serde_json::to_vec(&payload)?)
}

fn collect_part_bodies(
    client: &Client,
    token: &str,
    message_id: &str,
    part: &GmailPart,
    out: &mut GmailBodyParts,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mime_type = part.mime_type.as_deref().unwrap_or("");
    let filename = part.filename.as_deref().unwrap_or("").trim();

    if let Some(data) = part.body.data.as_deref() {
        let bytes = decode_gmail_base64(data)?;
        if filename.is_empty() && mime_type.starts_with("text/plain") {
            out.text.push(String::from_utf8_lossy(&bytes).to_string());
        } else if filename.is_empty() && mime_type.starts_with("text/html") {
            out.html.push(String::from_utf8_lossy(&bytes).to_string());
        } else if !filename.is_empty() {
            out.attachments.push(json!({
                "Name": filename,
                "Content": BASE64_STANDARD.encode(bytes),
                "ContentType": if mime_type.is_empty() { "application/octet-stream" } else { mime_type },
            }));
        }
    } else if !filename.is_empty() {
        if let Some(attachment_id) = part.body.attachment_id.as_deref() {
            let bytes = get_attachment(client, token, message_id, attachment_id)?;
            out.attachments.push(json!({
                "Name": filename,
                "Content": BASE64_STANDARD.encode(bytes),
                "ContentType": if mime_type.is_empty() { "application/octet-stream" } else { mime_type },
            }));
        }
    }

    for child in &part.parts {
        collect_part_bodies(client, token, message_id, child, out)?;
    }

    Ok(())
}

fn decode_gmail_base64(value: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let compact = value.trim().replace(['\r', '\n'], "");
    URL_SAFE_NO_PAD
        .decode(compact.as_bytes())
        .or_else(|_| URL_SAFE.decode(compact.as_bytes()))
        .map_err(|err| err.into())
}

fn header_value(headers: &[GmailHeader], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn select_service_recipient(to_header: &str, fallback: &str) -> String {
    extract_emails(to_header)
        .into_iter()
        .find(|email| email.eq_ignore_ascii_case(fallback))
        .unwrap_or_else(|| fallback.to_string())
}

fn join_body_parts(parts: &[String]) -> Option<String> {
    let joined = parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

fn load_processed_ids(path: &PathBuf) -> HashSet<String> {
    fs::read_to_string(path)
        .map(|contents| {
            contents
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn save_processed_ids(
    path: &PathBuf,
    ids: &HashSet<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let mut sorted = ids.iter().cloned().collect::<Vec<_>>();
    sorted.sort();
    fs::write(path, format!("{}\n", sorted.join("\n")))?;
    Ok(())
}

fn env_flag(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_gmail_base64_accepts_urlsafe_without_padding() {
        let encoded = URL_SAFE_NO_PAD.encode("hello world");
        let decoded = decode_gmail_base64(&encoded).expect("decode");
        assert_eq!(decoded, b"hello world");
    }

    #[test]
    fn select_service_recipient_prefers_matching_address() {
        let selected = select_service_recipient(
            "Oliver <oliver@dowhiz.com>, other@example.com",
            "oliver@dowhiz.com",
        );
        assert_eq!(selected, "oliver@dowhiz.com");
    }

    #[test]
    fn join_body_parts_drops_empty_segments() {
        assert_eq!(
            join_body_parts(&[" first ".to_string(), "".to_string(), "second".to_string()])
                .as_deref(),
            Some("first\n\nsecond")
        );
    }
}
