use std::io;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;
use tracing::error;

use crate::domain::workspace_blueprint::StartupWorkspaceBlueprint;
use crate::employee_config::EmployeeProfile;

use super::html::{strip_html_tags, truncate_preview};
use super::startup_workspace::{
    bootstrap_workspace_plan, build_workspace_home_snapshot, StartupWorkspaceBootstrapPlan,
};
use super::BoxError;

fn thread_workspace_name(thread_key: &str) -> String {
    let hash = format!("{:x}", md5::compute(thread_key.as_bytes()));
    format!("thread_{}", hash)
}

pub(super) fn copy_skills_directory(src: &Path, dest: &Path) -> std::io::Result<()> {
    if !src.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let skill_src = entry.path();
        let skill_dest = dest.join(entry.file_name());

        if skill_src.is_dir() {
            copy_dir_recursive(&skill_src, &skill_dest)?;
        }
    }
    Ok(())
}

pub fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            copy_file_with_fallback(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

pub(super) fn ensure_workspace_employee_files(
    workspace: &Path,
    employee: &EmployeeProfile,
) -> std::io::Result<()> {
    if let Some(path) = employee.agents_path.as_ref() {
        if path.exists() {
            copy_file_with_fallback(path, &workspace.join("AGENTS.md"))?;
        }
    }
    if let Some(path) = employee.claude_path.as_ref() {
        if path.exists() {
            copy_file_with_fallback(path, &workspace.join("CLAUDE.md"))?;
        }
    }
    if let Some(path) = employee.soul_path.as_ref() {
        if path.exists() {
            copy_file_with_fallback(path, &workspace.join("SOUL.md"))?;
        }
    }
    Ok(())
}

fn copy_file_with_fallback(src: &Path, dest: &Path) -> std::io::Result<()> {
    match std::fs::copy(src, dest) {
        Ok(_) => Ok(()),
        Err(err)
            if err.kind() == std::io::ErrorKind::PermissionDenied
                || err.raw_os_error() == Some(1) =>
        {
            // Some CIFS/Azure Files mounts reject the kernel fast-copy syscall.
            // Fall back to a stream copy that is broadly supported.
            let mut input = std::fs::File::open(src)?;
            let mut output = std::fs::File::create(dest)?;
            std::io::copy(&mut input, &mut output)?;
            Ok(())
        }
        Err(err) => Err(err),
    }
}

pub(crate) fn ensure_thread_workspace(
    user_paths: &crate::user_store::UserPaths,
    user_id: &str,
    thread_key: &str,
    employee: &EmployeeProfile,
    skills_source_dir: Option<&Path>,
) -> Result<PathBuf, BoxError> {
    std::fs::create_dir_all(&user_paths.workspaces_root).map_err(|err| {
        io::Error::other(format!(
            "create_dir_all workspaces_root failed path={} error={}",
            user_paths.workspaces_root.display(),
            err
        ))
    })?;

    let workspace_name = thread_workspace_name(thread_key);
    let workspace = user_paths.workspaces_root.join(workspace_name);
    let is_new = !workspace.exists();
    if is_new {
        std::fs::create_dir_all(&workspace).map_err(|err| {
            io::Error::other(format!(
                "create_dir_all workspace failed path={} error={}",
                workspace.display(),
                err
            ))
        })?;
    }

    let incoming_email = workspace.join("incoming_email");
    let incoming_attachments = workspace.join("incoming_attachments");
    let memory = workspace.join("memory");
    let references = workspace.join("references");

    std::fs::create_dir_all(&incoming_email).map_err(|err| {
        io::Error::other(format!(
            "create_dir_all incoming_email failed path={} error={}",
            incoming_email.display(),
            err
        ))
    })?;
    std::fs::create_dir_all(&incoming_attachments).map_err(|err| {
        io::Error::other(format!(
            "create_dir_all incoming_attachments failed path={} error={}",
            incoming_attachments.display(),
            err
        ))
    })?;
    std::fs::create_dir_all(&memory).map_err(|err| {
        io::Error::other(format!(
            "create_dir_all memory failed path={} error={}",
            memory.display(),
            err
        ))
    })?;
    std::fs::create_dir_all(&references).map_err(|err| {
        io::Error::other(format!(
            "create_dir_all references failed path={} error={}",
            references.display(),
            err
        ))
    })?;

    if is_new || !references.join("past_emails").exists() {
        if let Err(err) = crate::past_emails::hydrate_past_emails(
            &user_paths.mail_root,
            &references,
            user_id,
            None,
        ) {
            error!("failed to hydrate past_emails: {}", err);
        }
    }

    ensure_workspace_employee_files(&workspace, employee).map_err(|err| {
        io::Error::other(format!(
            "ensure_workspace_employee_files failed workspace={} error={}",
            workspace.display(),
            err
        ))
    })?;

    // Copy skills to workspace for Codex/Claude runners.
    let agents_skills_dir = workspace.join(".agents").join("skills");
    if let Some(skills_src) = skills_source_dir {
        if let Err(err) = copy_skills_directory(skills_src, &agents_skills_dir) {
            error!("failed to copy base skills to workspace: {}", err);
        }
    }
    if let Some(employee_skills) = employee.skills_dir.as_deref() {
        let should_copy = skills_source_dir
            .map(|base| base != employee_skills)
            .unwrap_or(true);
        if should_copy {
            if let Err(err) = copy_skills_directory(employee_skills, &agents_skills_dir) {
                error!("failed to copy employee skills to workspace: {}", err);
            }
        }
    }

    Ok(workspace)
}

/// Bootstrap a startup workspace plan and persist it as reviewable workspace artifacts.
pub fn bootstrap_startup_workspace_files(
    workspace: &Path,
    blueprint: StartupWorkspaceBlueprint,
) -> Result<StartupWorkspaceBootstrapPlan, BoxError> {
    let plan = bootstrap_workspace_plan(blueprint)?;
    persist_startup_workspace_files(workspace, &plan)?;
    Ok(plan)
}

pub fn persist_startup_workspace_files(
    workspace: &Path,
    plan: &StartupWorkspaceBootstrapPlan,
) -> Result<PathBuf, BoxError> {
    std::fs::create_dir_all(workspace)?;

    let bootstrap_root = workspace.join("startup_workspace");
    std::fs::create_dir_all(&bootstrap_root)?;

    let workspace_home_snapshot = build_workspace_home_snapshot(plan);
    write_json_pretty(&bootstrap_root.join("blueprint.json"), &plan.blueprint)?;
    write_json_pretty(&bootstrap_root.join("resources.json"), &plan.resources)?;
    write_json_pretty(
        &bootstrap_root.join("agent_roster.json"),
        &plan.agent_roster,
    )?;
    write_json_pretty(
        &bootstrap_root.join("starter_tasks.json"),
        &plan.starter_tasks,
    )?;
    write_json_pretty(
        &bootstrap_root.join("artifact_queue.json"),
        &plan.artifact_queue,
    )?;
    write_json_pretty(
        &bootstrap_root.join("provisioning.json"),
        &plan.provisioning,
    )?;
    write_json_pretty(
        &bootstrap_root.join("workspace_home_snapshot.json"),
        &workspace_home_snapshot,
    )?;

    let placeholders_root = bootstrap_root.join("artifact_placeholders");
    std::fs::create_dir_all(&placeholders_root)?;

    let mut placeholder_index: Vec<String> = Vec::new();
    placeholder_index.push("# Startup Workspace Bootstrap".to_string());
    placeholder_index.push(String::new());
    placeholder_index.push(format!(
        "Generated at: {}",
        Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
    ));
    placeholder_index.push(String::new());
    placeholder_index.push("Generated files:".to_string());
    placeholder_index.push("- blueprint.json".to_string());
    placeholder_index.push("- resources.json".to_string());
    placeholder_index.push("- agent_roster.json".to_string());
    placeholder_index.push("- starter_tasks.json".to_string());
    placeholder_index.push("- artifact_queue.json".to_string());
    placeholder_index.push("- provisioning.json".to_string());
    placeholder_index.push("- workspace_home_snapshot.json".to_string());
    placeholder_index.push(String::new());
    placeholder_index.push("Artifact placeholders:".to_string());

    for artifact in plan.artifact_queue.artifacts.iter() {
        let file_name = format!("{}.md", slugify_filename(&artifact.id));
        let placeholder_path = placeholders_root.join(&file_name);
        let content = render_artifact_placeholder(plan, artifact);
        std::fs::write(&placeholder_path, content)?;
        placeholder_index.push(format!("- artifact_placeholders/{file_name}"));
    }

    std::fs::write(
        bootstrap_root.join("README.md"),
        placeholder_index.join("\n"),
    )?;

    Ok(bootstrap_root)
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<(), BoxError> {
    let serialized = serde_json::to_string_pretty(value)?;
    std::fs::write(path, serialized)?;
    Ok(())
}

fn slugify_filename(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
        } else if !output.ends_with('_') {
            output.push('_');
        }
    }
    let trimmed = output.trim_matches('_');
    if trimmed.is_empty() {
        "artifact".to_string()
    } else {
        trimmed.to_string()
    }
}

fn render_artifact_placeholder(
    plan: &StartupWorkspaceBootstrapPlan,
    artifact: &crate::domain::artifact_queue::ArtifactPlaceholder,
) -> String {
    let status = match artifact.status {
        crate::domain::artifact_queue::ArtifactQueueStatus::Planned => "planned",
        crate::domain::artifact_queue::ArtifactQueueStatus::PendingReview => "pending_review",
    };

    let workspace_title = if plan.blueprint.venture.name.trim().is_empty() {
        "Founder Workspace".to_string()
    } else {
        plan.blueprint.venture.name.trim().to_string()
    };

    [
        format!("# {}", artifact.title),
        String::new(),
        format!("Workspace: {workspace_title}"),
        format!("Owner Role: {}", artifact.owner_role),
        format!("Surface: {}", artifact.surface),
        format!("Status: {status}"),
        String::new(),
        "## Rationale".to_string(),
        artifact.rationale.clone(),
        String::new(),
        "## Source Context".to_string(),
        format!("- Founder: {}", plan.blueprint.founder.name),
        format!("- Thesis: {}", plan.blueprint.venture.thesis),
        format!(
            "- Goals: {}",
            if plan.blueprint.goals_30_90_days.is_empty() {
                "None listed".to_string()
            } else {
                plan.blueprint.goals_30_90_days.join("; ")
            }
        ),
        String::new(),
        "## Draft".to_string(),
        "Fill in this placeholder during bootstrap execution.".to_string(),
        String::new(),
    ]
    .join("\n")
}

pub(super) fn write_thread_history(
    incoming_email: &Path,
    incoming_attachments: &Path,
) -> Result<(), BoxError> {
    let entries = collect_thread_entries(incoming_email, incoming_attachments)?;
    if entries.is_empty() {
        return Ok(());
    }

    let mut output = String::new();
    output.push_str("# Thread history (inbound)\n");
    output.push_str("Auto-generated from incoming_email/entries. Latest entry is last.\n\n");

    for entry in entries {
        let entry_name = entry.entry_name;
        output.push_str(&format!("## {entry_name}\n"));
        if let Some(summary) = entry.summary {
            output.push_str(&format!("Subject: {}\n", summary.subject));
            output.push_str(&format!("From: {}\n", summary.from));
            output.push_str(&format!("To: {}\n", summary.to));
            if !summary.cc.is_empty() {
                output.push_str(&format!("Cc: {}\n", summary.cc));
            }
            if !summary.bcc.is_empty() {
                output.push_str(&format!("Bcc: {}\n", summary.bcc));
            }
            if let Some(date) = summary.date.as_deref() {
                output.push_str(&format!("Date: {}\n", date));
            }
            if !summary.message_id.is_empty() {
                output.push_str(&format!("Message-ID: {}\n", summary.message_id));
            }
            let preview = build_preview(&summary);
            if let Some(preview) = preview {
                output.push_str("Preview:\n```text\n");
                output.push_str(&preview);
                output.push_str("\n```\n");
            }
        }

        output.push_str("Files:\n");
        output.push_str(&format!(
            "- incoming_email/entries/{entry_name}/{}\n",
            entry.email_file
        ));
        output.push_str(&format!(
            "- incoming_email/entries/{entry_name}/postmark_payload.json\n"
        ));
        if !entry.attachments.is_empty() {
            output.push_str(&format!(
                "- incoming_attachments/entries/{entry_name}/ ({})\n",
                entry.attachments.join(", ")
            ));
        } else {
            output.push_str("- incoming_attachments/entries/(none)\n");
        }
        output.push('\n');
    }

    std::fs::write(incoming_email.join("thread_history.md"), output)?;
    Ok(())
}

pub(super) fn refresh_thread_input_snapshot(
    incoming_email: &Path,
    incoming_attachments: &Path,
) -> Result<(), BoxError> {
    let manifest = rebuild_thread_attachment_view(incoming_attachments)?;
    write_thread_history(incoming_email, incoming_attachments)?;
    write_thread_request(incoming_email, incoming_attachments, &manifest)?;
    Ok(())
}

fn write_thread_request(
    incoming_email: &Path,
    incoming_attachments: &Path,
    manifest: &[MergedAttachmentManifestEntry],
) -> Result<(), BoxError> {
    let entries = collect_thread_entries(incoming_email, incoming_attachments)?;
    let Some(latest) = entries.last() else {
        return Ok(());
    };

    let latest_preview = latest
        .summary
        .as_ref()
        .and_then(build_preview)
        .unwrap_or_else(|| "(no preview)".to_string());

    let mut output = String::new();
    output.push_str("# Canonical thread request\n");
    output.push_str("Auto-generated merged view for reruns after follow-up messages.\n\n");
    output.push_str("Rules:\n");
    output.push_str("- Treat the latest inbound message as the newest instruction.\n");
    output.push_str(
        "- Earlier messages remain active context unless the latest message overrides them.\n",
    );
    output.push_str(
        "- Use `incoming_attachments/` as the merged attachment view for the whole thread.\n",
    );
    output.push_str("- Raw per-message artifacts remain under `incoming_email/entries/` and `incoming_attachments/entries/`.\n\n");
    output.push_str("## Latest inbound message\n");
    if let Some(summary) = latest.summary.as_ref() {
        output.push_str(&format!("Entry: {}\n", latest.entry_name));
        output.push_str(&format!("Subject: {}\n", summary.subject));
        output.push_str(&format!("From: {}\n", summary.from));
        if let Some(date) = summary.date.as_deref() {
            output.push_str(&format!("Date: {}\n", date));
        }
        output.push_str("Preview:\n```text\n");
        output.push_str(&latest_preview);
        output.push_str("\n```\n\n");
    } else {
        output.push_str(&format!("Entry: {}\n\n", latest.entry_name));
    }

    output.push_str("## Thread timeline\n");
    for (index, entry) in entries.iter().enumerate() {
        output.push_str(&format!("{}. {}", index + 1, entry.entry_name));
        if let Some(summary) = entry.summary.as_ref() {
            if !summary.subject.trim().is_empty() {
                output.push_str(&format!(" | Subject: {}", summary.subject));
            }
            if let Some(date) = summary.date.as_deref() {
                output.push_str(&format!(" | Date: {}", date));
            }
            output.push('\n');
            if let Some(preview) = build_preview(summary) {
                output.push_str("   Preview:\n");
                output.push_str("   ```text\n");
                output.push_str(&preview);
                output.push_str("\n   ```\n");
            }
        } else {
            output.push('\n');
        }
        if !entry.attachments.is_empty() {
            output.push_str(&format!(
                "   Attachments: {}\n",
                entry.attachments.join(", ")
            ));
        }
    }
    output.push('\n');

    output.push_str("## Merged attachments for this rerun\n");
    if manifest.is_empty() {
        output.push_str("- `(none)`\n");
    } else {
        output.push_str(
            "- `incoming_attachments/` contains every attachment needed for the rerun.\n",
        );
        output.push_str("- If the same filename appeared multiple times, the newest copy keeps the original name and older copies are prefixed with the entry id.\n");
        for item in manifest {
            output.push_str(&format!(
                "- `{}` from `{}` (source: `incoming_attachments/entries/{}/{}`)\n",
                item.display_name, item.original_name, item.source_entry, item.source_file
            ));
        }
    }
    output.push('\n');
    output
        .push_str("See `thread_history.md` for a file-by-file map of the raw inbound artifacts.\n");

    std::fs::write(incoming_email.join("thread_request.md"), output)?;
    Ok(())
}

fn rebuild_thread_attachment_view(
    incoming_attachments: &Path,
) -> Result<Vec<MergedAttachmentManifestEntry>, BoxError> {
    let entries_root = incoming_attachments.join("entries");
    clear_dir_except(incoming_attachments, &entries_root)?;
    if !entries_root.exists() {
        return Ok(Vec::new());
    }

    let attachment_sources = collect_attachment_sources(&entries_root)?;
    let mut last_index_by_name = std::collections::HashMap::new();
    let mut counts_by_name = std::collections::HashMap::new();
    for (index, source) in attachment_sources.iter().enumerate() {
        last_index_by_name.insert(source.file_name.clone(), index);
        *counts_by_name
            .entry(source.file_name.clone())
            .or_insert(0usize) += 1;
    }

    let mut used_display_names = std::collections::HashSet::new();
    let mut manifest = Vec::with_capacity(attachment_sources.len());
    for (index, source) in attachment_sources.into_iter().enumerate() {
        let duplicate_count = counts_by_name.get(&source.file_name).copied().unwrap_or(1);
        let preferred_name = if duplicate_count > 1
            && last_index_by_name.get(&source.file_name).copied() != Some(index)
        {
            format!("{}__{}", source.entry_name, source.file_name)
        } else {
            source.file_name.clone()
        };
        let display_name = make_unique_attachment_name(&preferred_name, &mut used_display_names);
        copy_file_with_fallback(&source.path, &incoming_attachments.join(&display_name))?;
        manifest.push(MergedAttachmentManifestEntry {
            display_name,
            original_name: source.file_name,
            source_entry: source.entry_name,
            source_file: source.source_file,
        });
    }

    std::fs::write(
        incoming_attachments.join("thread_manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(manifest)
}

fn clear_dir_except(root: &Path, keep: &Path) -> Result<(), std::io::Error> {
    if !root.exists() {
        std::fs::create_dir_all(root)?;
        return Ok(());
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path == keep {
            continue;
        }
        if path.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct MergedAttachmentManifestEntry {
    display_name: String,
    original_name: String,
    source_entry: String,
    source_file: String,
}

#[derive(Debug, Clone)]
struct AttachmentSource {
    entry_name: String,
    file_name: String,
    source_file: String,
    path: PathBuf,
}

fn collect_attachment_sources(entries_root: &Path) -> Result<Vec<AttachmentSource>, BoxError> {
    let mut entry_dirs = Vec::new();
    for entry in std::fs::read_dir(entries_root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            entry_dirs.push(entry.path());
        }
    }
    entry_dirs.sort_by_key(|path| {
        path.file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default()
    });

    let mut sources = Vec::new();
    for entry_dir in entry_dirs {
        let entry_name = entry_dir
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "entry".to_string());
        let mut files = Vec::new();
        for file_entry in std::fs::read_dir(&entry_dir)? {
            let file_entry = file_entry?;
            if file_entry.file_type()?.is_file() {
                files.push(file_entry.path());
            }
        }
        files.sort_by_key(|path| {
            path.file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default()
        });
        for path in files {
            let file_name = path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| "attachment".to_string());
            sources.push(AttachmentSource {
                entry_name: entry_name.clone(),
                source_file: file_name.clone(),
                file_name,
                path,
            });
        }
    }

    Ok(sources)
}

fn make_unique_attachment_name(
    preferred_name: &str,
    used_display_names: &mut std::collections::HashSet<String>,
) -> String {
    if used_display_names.insert(preferred_name.to_string()) {
        return preferred_name.to_string();
    }

    let path = Path::new(preferred_name);
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| preferred_name.to_string());
    let extension = path
        .extension()
        .map(|value| format!(".{}", value.to_string_lossy()))
        .unwrap_or_default();
    for suffix in 2..1000 {
        let candidate = format!("{stem}_{suffix}{extension}");
        if used_display_names.insert(candidate.clone()) {
            return candidate;
        }
    }
    preferred_name.to_string()
}

#[derive(Debug, Clone)]
struct InboundThreadEntry {
    entry_name: String,
    email_file: String,
    summary: Option<PayloadSummary>,
    attachments: Vec<String>,
}

fn collect_thread_entries(
    incoming_email: &Path,
    incoming_attachments: &Path,
) -> Result<Vec<InboundThreadEntry>, BoxError> {
    let entries_email = incoming_email.join("entries");
    if !entries_email.exists() {
        return Ok(Vec::new());
    }

    let mut entry_dirs: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&entries_email)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            entry_dirs.push(entry.path());
        }
    }
    entry_dirs.sort_by_key(|path| {
        path.file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default()
    });

    let mut entries = Vec::with_capacity(entry_dirs.len());
    for entry_dir in entry_dirs {
        let entry_name = entry_dir
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "entry".to_string());
        let payload_path = entry_dir.join("postmark_payload.json");
        let summary = load_payload_summary(&payload_path);
        let attachments_dir = incoming_attachments.join("entries").join(&entry_name);
        let attachments = list_attachment_names(&attachments_dir).unwrap_or_default();
        let email_file = if entry_dir.join("email.html").exists() {
            "email.html".to_string()
        } else if entry_dir.join("email.txt").exists() {
            "email.txt".to_string()
        } else {
            "email.html".to_string()
        };
        entries.push(InboundThreadEntry {
            entry_name,
            email_file,
            summary,
            attachments,
        });
    }
    Ok(entries)
}

#[derive(Debug, Default, Clone)]
struct PayloadSummary {
    subject: String,
    from: String,
    to: String,
    cc: String,
    bcc: String,
    date: Option<String>,
    message_id: String,
    text_body: Option<String>,
    html_body: Option<String>,
}

fn load_payload_summary(payload_path: &Path) -> Option<PayloadSummary> {
    let payload_data = std::fs::read_to_string(payload_path).ok()?;
    let payload_json: serde_json::Value = serde_json::from_str(&payload_data).ok()?;
    Some(PayloadSummary {
        subject: json_string(&payload_json, "Subject").unwrap_or_default(),
        from: json_string(&payload_json, "From").unwrap_or_default(),
        to: json_string(&payload_json, "To").unwrap_or_default(),
        cc: json_string(&payload_json, "Cc").unwrap_or_default(),
        bcc: json_string(&payload_json, "Bcc").unwrap_or_default(),
        date: json_string(&payload_json, "Date")
            .or_else(|| json_string(&payload_json, "ReceivedAt")),
        message_id: json_string(&payload_json, "MessageID")
            .or_else(|| json_string(&payload_json, "MessageId"))
            .unwrap_or_default(),
        text_body: json_string(&payload_json, "StrippedTextReply")
            .or_else(|| json_string(&payload_json, "TextBody")),
        html_body: json_string(&payload_json, "HtmlBody"),
    })
}

fn json_string(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn list_attachment_names(dir: &Path) -> Result<Vec<String>, std::io::Error> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            names.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    names.sort();
    Ok(names)
}

fn build_preview(summary: &PayloadSummary) -> Option<String> {
    let mut preview = summary
        .text_body
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    if preview.is_empty() {
        preview = summary
            .html_body
            .as_deref()
            .map(strip_html_tags)
            .unwrap_or_default();
    }
    let preview = preview.trim();
    if preview.is_empty() {
        return None;
    }
    Some(truncate_preview(preview, 1200))
}

pub(super) fn create_unique_dir(root: &Path, base: &str) -> Result<PathBuf, std::io::Error> {
    let mut candidate = root.join(base);
    if !candidate.exists() {
        std::fs::create_dir_all(&candidate)?;
        return Ok(candidate);
    }
    for idx in 1..1000 {
        let name = format!("{}_{}", base, idx);
        candidate = root.join(name);
        if !candidate.exists() {
            std::fs::create_dir_all(&candidate)?;
            return Ok(candidate);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "failed to create unique workspace directory",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::workspace_blueprint::StartupWorkspaceBlueprint;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn bootstrap_startup_workspace_files_writes_plan_and_placeholders() {
        let workspace_root = tempdir().expect("tempdir should be created");
        let workspace_dir = workspace_root.path().join("workspace");

        let mut blueprint = StartupWorkspaceBlueprint::default();
        blueprint.founder.name = "Founder".to_string();
        blueprint.founder.email = "founder@example.com".to_string();
        blueprint.venture.name = "Acme".to_string();
        blueprint.venture.thesis = "Build an agent-native startup workspace".to_string();
        blueprint.goals_30_90_days = vec!["Launch alpha".to_string()];

        let plan = bootstrap_startup_workspace_files(&workspace_dir, blueprint)
            .expect("bootstrap workspace files should succeed");

        assert!(!plan.resources.resources.is_empty());
        assert!(!plan.agent_roster.assignments.is_empty());
        assert!(!plan.artifact_queue.artifacts.is_empty());

        let startup_workspace_dir = workspace_dir.join("startup_workspace");
        assert!(startup_workspace_dir.join("blueprint.json").exists());
        assert!(startup_workspace_dir.join("resources.json").exists());
        assert!(startup_workspace_dir.join("agent_roster.json").exists());
        assert!(startup_workspace_dir.join("starter_tasks.json").exists());
        assert!(startup_workspace_dir.join("artifact_queue.json").exists());
        assert!(startup_workspace_dir.join("provisioning.json").exists());
        assert!(startup_workspace_dir
            .join("workspace_home_snapshot.json")
            .exists());
        assert!(startup_workspace_dir.join("README.md").exists());

        let placeholder_count =
            std::fs::read_dir(startup_workspace_dir.join("artifact_placeholders"))
                .expect("artifact placeholders directory should exist")
                .filter_map(Result::ok)
                .count();
        assert!(placeholder_count > 0);

        let resources_json = std::fs::read_to_string(startup_workspace_dir.join("resources.json"))
            .expect("resources.json should exist");
        assert!(resources_json.contains("manual_next_step"));
    }

    #[test]
    fn refresh_thread_input_snapshot_builds_merged_request_and_attachments() {
        let temp = tempdir().expect("tempdir");
        let incoming_email = temp.path().join("incoming_email");
        let incoming_attachments = temp.path().join("incoming_attachments");
        let entry_1 = incoming_email.join("entries").join("00001_hello_1");
        let entry_2 = incoming_email.join("entries").join("00002_hello_2");
        let attach_1 = incoming_attachments.join("entries").join("00001_hello_1");
        let attach_2 = incoming_attachments.join("entries").join("00002_hello_2");
        fs::create_dir_all(&entry_1).expect("entry 1");
        fs::create_dir_all(&entry_2).expect("entry 2");
        fs::create_dir_all(&attach_1).expect("attach 1");
        fs::create_dir_all(&attach_2).expect("attach 2");

        fs::write(
            entry_1.join("postmark_payload.json"),
            serde_json::to_string_pretty(&json!({
                "Subject": "Hello 1",
                "From": "Alice <alice@example.com>",
                "To": "Service <service@example.com>",
                "TextBody": "First message",
                "Date": "2026-03-25T00:00:00Z",
                "MessageID": "msg-1@example.com"
            }))
            .expect("payload 1"),
        )
        .expect("write payload 1");
        fs::write(entry_1.join("email.html"), "<p>First message</p>").expect("email 1");
        fs::write(attach_1.join("brief_v1.txt"), "v1").expect("attach file 1");

        fs::write(
            entry_2.join("postmark_payload.json"),
            serde_json::to_string_pretty(&json!({
                "Subject": "Hello 2",
                "From": "Alice <alice@example.com>",
                "To": "Service <service@example.com>",
                "TextBody": "Second message",
                "Date": "2026-03-25T00:01:00Z",
                "MessageID": "msg-2@example.com"
            }))
            .expect("payload 2"),
        )
        .expect("write payload 2");
        fs::write(entry_2.join("email.html"), "<p>Second message</p>").expect("email 2");
        fs::write(attach_2.join("brief_v2.txt"), "v2").expect("attach file 2");

        refresh_thread_input_snapshot(&incoming_email, &incoming_attachments)
            .expect("refresh thread snapshot");

        let thread_request =
            fs::read_to_string(incoming_email.join("thread_request.md")).expect("thread_request");
        assert!(thread_request.contains("First message"));
        assert!(thread_request.contains("Second message"));
        assert!(thread_request.contains("Latest inbound message"));

        let thread_history =
            fs::read_to_string(incoming_email.join("thread_history.md")).expect("thread_history");
        assert!(thread_history.contains("00001_hello_1"));
        assert!(thread_history.contains("00002_hello_2"));

        assert!(incoming_attachments.join("brief_v1.txt").exists());
        assert!(incoming_attachments.join("brief_v2.txt").exists());
        assert!(incoming_attachments.join("thread_manifest.json").exists());
    }
}
