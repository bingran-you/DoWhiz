//! Pre-flight handling for outbound email attachments.
//!
//! Postmark caps the total email request body at 10 MiB. Attachments larger
//! than that — or a batch of attachments that collectively push the request
//! over the limit — have to be offloaded somewhere before the Postmark call
//! or the send fails with `413 Payload Too Large`.
//!
//! This module runs just before [`send_emails_module::send_email`]: it scans
//! the attachments directory, moves any files that would bust the limit into
//! a sibling `reply_email_offloaded/` directory, uploads them to the Azure
//! blob container already used for inbound raw payloads, and appends a set
//! of signed download links to the email body so the recipient can still
//! fetch the files.
//!
//! On any failure (upload error, IO error, unsupported backend) we log a
//! warning and leave the file inline — Postmark will then fail with the
//! same 413 as before, so we never regress relative to the pre-offload
//! behavior.

use std::fs;
use std::path::{Path, PathBuf};

use tracing::{info, warn};
use uuid::Uuid;

use crate::raw_payload_store::{
    resolve_azure_blob_url, upload_outbound_attachment_blocking, RawPayloadStoreError,
};

/// Raw-byte budget for attachments kept inline in a single email.
///
/// Postmark's hard request limit is 10 MiB. Base64 inflates by ~4/3, so we
/// reserve ~6 MiB of raw bytes for attachments and leave the remainder for
/// the HTML body and headers.
const DEFAULT_MAX_INLINE_TOTAL_BYTES: u64 = 6 * 1024 * 1024;

/// Per-file raw-byte cap. Anything larger is always offloaded, regardless
/// of how much total budget is still available.
const DEFAULT_MAX_INLINE_SINGLE_BYTES: u64 = 5 * 1024 * 1024;

/// Sibling directory (next to `attachments_dir`) that holds files we've
/// already offloaded. Keeping them on disk is useful for audit and retry.
const OFFLOADED_DIR_NAME: &str = "reply_email_offloaded";

/// Marker placed by `send_emails_module::normalize_email_html` to delimit the
/// end of the user-visible content inside the DoWhiz email shell. We insert
/// offloaded-attachment links just before this marker so the links render
/// inside the shell regardless of whether the draft is already normalized.
const CONTENT_END_MARKER: &str = "<!-- dowhiz-email-content:end -->";

const ENV_MAX_INLINE_TOTAL: &str = "EMAIL_ATTACHMENT_MAX_TOTAL_BYTES";
const ENV_MAX_INLINE_SINGLE: &str = "EMAIL_ATTACHMENT_MAX_SINGLE_BYTES";

#[derive(Debug, thiserror::Error)]
enum EmailAttachmentError {
    #[error("io error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("blob upload failed for {path}: {source}")]
    BlobUpload {
        path: PathBuf,
        #[source]
        source: RawPayloadStoreError,
    },
    #[error("blob url resolution failed for {storage_ref}: {source}")]
    ResolveUrl {
        storage_ref: String,
        #[source]
        source: RawPayloadStoreError,
    },
}

/// Outcome of the pre-flight step, exposed for logging.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrepareSummary {
    pub inline_count: usize,
    pub offloaded_count: usize,
}

/// Inspect `attachments_dir` and offload any files that would push the
/// Postmark request over its 10 MiB limit. Offloaded files are moved to
/// `<workspace>/reply_email_offloaded/` and replaced in the HTML body by a
/// list of signed download links appended to `html_path`.
///
/// The call is infallible at the top level: per-file errors are logged and
/// the offending attachment is left inline, preserving existing behavior
/// whenever offload infrastructure is unavailable.
pub(crate) fn prepare_email_attachments(
    attachments_dir: &Path,
    html_path: &Path,
) -> PrepareSummary {
    let max_single =
        resolve_byte_env(ENV_MAX_INLINE_SINGLE).unwrap_or(DEFAULT_MAX_INLINE_SINGLE_BYTES);
    let max_total =
        resolve_byte_env(ENV_MAX_INLINE_TOTAL).unwrap_or(DEFAULT_MAX_INLINE_TOTAL_BYTES);

    let entries = match scan_attachments(attachments_dir) {
        Ok(values) => values,
        Err(err) => {
            warn!(
                "email attachment scan failed for {}: {}",
                attachments_dir.display(),
                err
            );
            return PrepareSummary::default();
        }
    };

    if entries.is_empty() {
        return PrepareSummary::default();
    }

    let plan = decide_offload_plan(&entries, max_single, max_total);

    let mut offloaded = Vec::new();
    for entry in &plan.offload {
        match offload_single(attachments_dir, entry) {
            Ok(item) => offloaded.push(item),
            Err(err) => warn!(
                "failed to offload attachment {}: {} (leaving inline)",
                entry.path.display(),
                err
            ),
        }
    }

    if !offloaded.is_empty() {
        let block = format_attachment_links_html(&offloaded);
        if let Err(err) = insert_links_into_html(html_path, &block) {
            warn!(
                "failed to insert offloaded attachment links into {}: {}",
                html_path.display(),
                err
            );
        }
    }

    let summary = PrepareSummary {
        inline_count: plan.inline.len(),
        offloaded_count: offloaded.len(),
    };
    if summary.offloaded_count > 0 {
        info!(
            "email attachments prepared: inline={}, offloaded={} (dir={})",
            summary.inline_count,
            summary.offloaded_count,
            attachments_dir.display()
        );
    }
    summary
}

#[derive(Debug, Clone)]
struct AttachmentEntry {
    path: PathBuf,
    size: u64,
}

#[derive(Debug, Default)]
struct OffloadPlan {
    inline: Vec<AttachmentEntry>,
    offload: Vec<AttachmentEntry>,
}

#[derive(Debug, Clone)]
struct OffloadedAttachment {
    name: String,
    url: String,
    size: u64,
}

fn resolve_byte_env(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.trim().parse::<u64>().ok()
}

fn scan_attachments(dir: &Path) -> std::io::Result<Vec<AttachmentEntry>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let size = entry.metadata()?.len();
        entries.push(AttachmentEntry { path, size });
    }
    entries.sort_by(|a, b| a.path.file_name().cmp(&b.path.file_name()));
    Ok(entries)
}

/// Decide which entries stay inline vs. get offloaded.
///
/// Rules applied in order:
/// 1. Anything over `max_single` is always offloaded.
/// 2. If the remaining inline total exceeds `max_total`, evict the largest
///    remaining files until the total fits.
fn decide_offload_plan(
    entries: &[AttachmentEntry],
    max_single: u64,
    max_total: u64,
) -> OffloadPlan {
    let mut inline = Vec::new();
    let mut offload = Vec::new();

    for entry in entries {
        if entry.size > max_single {
            offload.push(entry.clone());
        } else {
            inline.push(entry.clone());
        }
    }

    let mut inline_total: u64 = inline.iter().map(|e| e.size).sum();
    while inline_total > max_total {
        let Some(idx) = largest_index(&inline) else {
            break;
        };
        let evicted = inline.swap_remove(idx);
        inline_total = inline_total.saturating_sub(evicted.size);
        offload.push(evicted);
    }

    inline.sort_by(|a, b| a.path.file_name().cmp(&b.path.file_name()));
    offload.sort_by(|a, b| a.path.file_name().cmp(&b.path.file_name()));

    OffloadPlan { inline, offload }
}

fn largest_index(entries: &[AttachmentEntry]) -> Option<usize> {
    entries
        .iter()
        .enumerate()
        .max_by_key(|(_, entry)| entry.size)
        .map(|(index, _)| index)
}

fn offload_single(
    attachments_dir: &Path,
    entry: &AttachmentEntry,
) -> Result<OffloadedAttachment, EmailAttachmentError> {
    let bytes = fs::read(&entry.path).map_err(|source| EmailAttachmentError::Io {
        path: entry.path.clone(),
        source,
    })?;

    let file_name = entry
        .path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("attachment")
        .to_string();

    let storage_ref = upload_outbound_attachment_blocking(Uuid::new_v4(), &file_name, &bytes)
        .map_err(|source| EmailAttachmentError::BlobUpload {
            path: entry.path.clone(),
            source,
        })?;

    let url = resolve_azure_blob_url(&storage_ref).map_err(|source| {
        EmailAttachmentError::ResolveUrl {
            storage_ref: storage_ref.clone(),
            source,
        }
    })?;

    if let Err(err) = move_to_offloaded(attachments_dir, &entry.path) {
        warn!(
            "uploaded {} to blob but failed to move local copy out of attachments dir: {}",
            entry.path.display(),
            err
        );
    }

    Ok(OffloadedAttachment {
        name: file_name,
        url,
        size: entry.size,
    })
}

fn move_to_offloaded(attachments_dir: &Path, file_path: &Path) -> std::io::Result<()> {
    let workspace = attachments_dir.parent().unwrap_or(attachments_dir);
    let offloaded_dir = workspace.join(OFFLOADED_DIR_NAME);
    fs::create_dir_all(&offloaded_dir)?;
    let file_name = file_path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("attachment"));
    fs::rename(file_path, offloaded_dir.join(file_name))
}

fn format_attachment_links_html(items: &[OffloadedAttachment]) -> String {
    let mut html = String::new();
    html.push_str("<hr /><p><strong>Large attachments (download links):</strong></p><ul>");
    for item in items {
        html.push_str("<li><a href=\"");
        html.push_str(&html_escape(&item.url));
        html.push_str("\">");
        html.push_str(&html_escape(&item.name));
        html.push_str("</a>");
        if item.size > 0 {
            html.push_str(" (");
            html.push_str(&format_byte_size(item.size));
            html.push(')');
        }
        html.push_str("</li>");
    }
    html.push_str("</ul>");
    html
}

/// Insert `block` into the draft HTML at the correct spot.
///
/// - If the draft has already been wrapped by
///   `send_emails_module::normalize_email_html`, locate the
///   `CONTENT_END_MARKER` and insert the block right before it so the links
///   render inside the DoWhiz shell.
/// - Otherwise the draft is still a raw fragment - append the block to the
///   end so the subsequent normalize step wraps it together with the body.
fn insert_links_into_html(html_path: &Path, block: &str) -> std::io::Result<()> {
    let current = if html_path.exists() {
        fs::read_to_string(html_path)?
    } else {
        String::new()
    };
    let new_html = match current.find(CONTENT_END_MARKER) {
        Some(idx) => {
            let (head, tail) = current.split_at(idx);
            format!("{}{}{}", head.trim_end(), block, tail)
        }
        None if current.trim().is_empty() => block.to_string(),
        None => format!("{}\n{}", current.trim_end(), block),
    };
    fs::write(html_path, new_html)
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn format_byte_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let value = bytes as f64;
    if value >= GIB {
        format!("{:.1} GB", value / GIB)
    } else if value >= MIB {
        format!("{:.1} MB", value / MIB)
    } else if value >= KIB {
        format!("{:.1} KB", value / KIB)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, size: u64) -> AttachmentEntry {
        AttachmentEntry {
            path: PathBuf::from(name),
            size,
        }
    }

    #[test]
    fn empty_input_produces_empty_plan() {
        let plan = decide_offload_plan(&[], 10, 20);
        assert!(plan.inline.is_empty());
        assert!(plan.offload.is_empty());
    }

    #[test]
    fn single_oversized_file_is_offloaded_even_with_room_in_total_budget() {
        let entries = vec![entry("big.pptx", 50)];
        let plan = decide_offload_plan(&entries, 10, 100);
        assert_eq!(plan.inline.len(), 0);
        assert_eq!(plan.offload.len(), 1);
        assert_eq!(plan.offload[0].path, PathBuf::from("big.pptx"));
    }

    #[test]
    fn files_under_both_caps_stay_inline() {
        let entries = vec![entry("a.txt", 100), entry("b.txt", 200)];
        let plan = decide_offload_plan(&entries, 1024, 1024);
        assert_eq!(plan.inline.len(), 2);
        assert!(plan.offload.is_empty());
    }

    #[test]
    fn total_budget_evicts_largest_until_fits() {
        // All under per-file cap, but combined (100+400+500 = 1000) they
        // exceed max_total (900). Evicting only the largest (500) drops the
        // inline total to 500, which fits — so `medium.txt` should stay
        // inline alongside `small.txt`.
        let entries = vec![
            entry("small.txt", 100),
            entry("medium.txt", 400),
            entry("large.txt", 500),
        ];
        let plan = decide_offload_plan(&entries, 1000, 900);
        let inline_names: Vec<_> = plan
            .inline
            .iter()
            .map(|e| e.path.to_string_lossy().into_owned())
            .collect();
        let offload_names: Vec<_> = plan
            .offload
            .iter()
            .map(|e| e.path.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            inline_names,
            vec!["medium.txt".to_string(), "small.txt".to_string()]
        );
        assert_eq!(offload_names, vec!["large.txt".to_string()]);
    }

    #[test]
    fn total_budget_evicts_repeatedly_when_one_eviction_is_not_enough() {
        // 300+300+300 = 900, all under per-file cap (400), but max_total = 500.
        // Evict one 300 → 600 still > 500. Evict another → 300 ≤ 500.
        let entries = vec![
            entry("a.txt", 300),
            entry("b.txt", 300),
            entry("c.txt", 300),
        ];
        let plan = decide_offload_plan(&entries, 400, 500);
        assert_eq!(plan.inline.len(), 1);
        assert_eq!(plan.offload.len(), 2);
    }

    #[test]
    fn format_links_html_escapes_user_content() {
        let items = vec![OffloadedAttachment {
            name: "weird & name<>.pptx".to_string(),
            url: "https://example.com/?a=1&b=2".to_string(),
            size: 1024,
        }];
        let html = format_attachment_links_html(&items);
        assert!(html.contains("weird &amp; name&lt;&gt;.pptx"));
        assert!(html.contains("https://example.com/?a=1&amp;b=2"));
        assert!(html.contains("1.0 KB"));
    }

    #[test]
    fn format_byte_size_scales_correctly() {
        assert_eq!(format_byte_size(0), "0 B");
        assert_eq!(format_byte_size(512), "512 B");
        assert_eq!(format_byte_size(1024), "1.0 KB");
        assert_eq!(format_byte_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_byte_size(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn insert_links_appends_when_draft_is_raw_fragment() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("body.html");
        fs::write(&path, "<p>Hello</p>").expect("write");
        insert_links_into_html(&path, "<p>Attachments</p>").expect("insert");
        let content = fs::read_to_string(&path).expect("read");
        assert_eq!(content, "<p>Hello</p>\n<p>Attachments</p>");
    }

    #[test]
    fn insert_links_creates_file_when_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("body.html");
        insert_links_into_html(&path, "<p>Attachments</p>").expect("insert");
        let content = fs::read_to_string(&path).expect("read");
        assert_eq!(content, "<p>Attachments</p>");
    }

    #[test]
    fn insert_links_places_block_before_shell_end_marker_when_normalized() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("body.html");
        let normalized = format!(
            "<html><body><p>Hello</p>{}\n</body></html>",
            CONTENT_END_MARKER
        );
        fs::write(&path, &normalized).expect("write");
        insert_links_into_html(&path, "<p>Attachments</p>").expect("insert");
        let content = fs::read_to_string(&path).expect("read");
        assert_eq!(
            content,
            format!(
                "<html><body><p>Hello</p><p>Attachments</p>{}\n</body></html>",
                CONTENT_END_MARKER
            )
        );
        // Links must end up inside the shell, not after </html>.
        assert!(content.contains("<p>Attachments</p></body>")
            || content.contains("<p>Attachments</p><!--"));
    }
}
