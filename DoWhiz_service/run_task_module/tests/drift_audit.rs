use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct Finding {
    category: &'static str,
    location: String,
    detail: String,
}

impl Finding {
    fn new(category: &'static str, location: String, detail: impl Into<String>) -> Self {
        Self {
            category,
            location,
            detail: detail.into(),
        }
    }

    fn render(&self) -> String {
        format!("[{}] {}: {}", self.category, self.location, self.detail)
    }
}

fn service_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("run_task_module lives under DoWhiz_service")
        .to_path_buf()
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn prompt_path(root: &Path) -> PathBuf {
    root.join("run_task_module/src/run_task/prompt.rs")
}

fn skills_root(root: &Path) -> PathBuf {
    root.join("skills")
}

fn collect_skill_markdown_files(root: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(root)
        .expect("read skills directory")
        .map(|entry| entry.expect("skills entry").path())
        .collect();
    entries.sort();

    for path in entries {
        if path.is_dir() {
            collect_skill_markdown_files(&path, out);
        } else if path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md") {
            out.push(path);
        }
    }
}

fn collect_audit_targets(root: &Path) -> Vec<PathBuf> {
    let mut files = vec![prompt_path(root)];
    collect_skill_markdown_files(&skills_root(root), &mut files);
    files
}

fn extract_skill_refs(text: &str) -> Vec<String> {
    const PREFIXES: &[&str] = &["skills/", ".agents/skills/"];
    const SUFFIX: &str = "/SKILL.md";

    let mut refs = BTreeSet::new();
    for prefix in PREFIXES {
        let mut cursor = 0usize;
        while let Some(relative_idx) = text[cursor..].find(prefix) {
            let start = cursor + relative_idx;
            let after_prefix = &text[start + prefix.len()..];
            let Some(suffix_idx) = after_prefix.find(SUFFIX) else {
                break;
            };
            let end = start + prefix.len() + suffix_idx + SUFFIX.len();
            refs.insert(text[start..end].to_string());
            cursor = end;
        }
    }

    refs.into_iter().collect()
}

fn skill_name_from_ref(raw: &str) -> Option<&str> {
    raw.strip_prefix(".agents/skills/")
        .or_else(|| raw.strip_prefix("skills/"))?
        .strip_suffix("/SKILL.md")
}

fn audit_prompt_skill_paths(root: &Path, findings: &mut Vec<Finding>) {
    let prompt = prompt_path(root);
    let text = fs::read_to_string(&prompt).expect("read prompt.rs");
    let prompt_rel = relative_path(root, &prompt);

    for raw_ref in extract_skill_refs(&text) {
        if raw_ref.starts_with("skills/") {
            findings.push(Finding::new(
                "prompt-skill-path",
                format!("{prompt_rel} :: {raw_ref}"),
                "prompt references bare skills/... even though runtime workspaces copy shared skills into .agents/skills/",
            ));
        }

        let Some(skill_name) = skill_name_from_ref(&raw_ref) else {
            continue;
        };

        if skill_name.contains('*') {
            findings.push(Finding::new(
                "prompt-skill-path",
                format!("{prompt_rel} :: {raw_ref}"),
                "prompt references a wildcard skill path, which does not exist as a literal runtime path",
            ));
            continue;
        }

        if !skills_root(root).join(skill_name).join("SKILL.md").exists() {
            findings.push(Finding::new(
                "prompt-skill-path",
                format!("{prompt_rel} :: {raw_ref}"),
                format!(
                    "referenced runtime skill '{skill_name}' is missing from DoWhiz_service/skills"
                ),
            ));
        }
    }
}

fn audit_legacy_cli_references(root: &Path, findings: &mut Vec<Finding>) {
    const NEEDLES: &[(&str, &str)] = &[
        (
            "discord_cli send-message",
            "legacy Discord subcommand; current CLI exposes send/send-channel instead",
        ),
        (
            "discord_cli send-reply",
            "legacy Discord subcommand; current CLI uses send --reply-to or send-channel --reply-to",
        ),
        (
            "discord_cli send-dm <user_id>",
            "legacy positional Discord DM syntax; current CLI requires --user-id",
        ),
        (
            "discord_cli list-guild-members <guild_id>",
            "legacy positional Discord guild lookup; current CLI requires --guild-id",
        ),
        (
            "slack_cli send-dm --user ",
            "stale Slack flag name; current CLI requires --user-id",
        ),
        (
            "slack_cli send-channel --channel ",
            "stale Slack flag name; current CLI requires --channel-id",
        ),
    ];

    for path in collect_audit_targets(root) {
        let rel = relative_path(root, &path);
        let text = fs::read_to_string(&path).expect("read audit target");
        for (line_idx, line) in text.lines().enumerate() {
            for (needle, detail) in NEEDLES {
                if line.contains(needle) {
                    findings.push(Finding::new(
                        "legacy-cli-reference",
                        format!("{rel}:{}", line_idx + 1),
                        *detail,
                    ));
                }
            }

            if line.contains("google-docs share <") && !line.contains("--email") {
                findings.push(Finding::new(
                    "legacy-cli-reference",
                    format!("{rel}:{}", line_idx + 1),
                    "positional google-docs share syntax drift; current CLI requires --email and optional --role flags",
                ));
            }
        }
    }
}

fn audit_google_docs_usage_dispatch(root: &Path, findings: &mut Vec<Finding>) {
    let path = root.join("scheduler_module/src/bin/google_docs_cli.rs");
    let rel = relative_path(root, &path);
    let text = fs::read_to_string(&path).expect("read google_docs_cli.rs");

    const COMMANDS: &[&str] = &[
        "list-documents",
        "read-document",
        "list-comments",
        "read-comment",
        "reply-comment",
        "apply-edit",
        "insert-text",
        "delete-text",
        "insert-image",
        "search-image",
        "get-styles",
        "set-style",
        "mark-deletion",
        "insert-suggestion",
        "suggest-replace",
        "apply-suggestions",
        "discard-suggestions",
        "create-document",
        "share",
        "get-link",
        "list-permissions",
        "remove-permission",
        "move-to-folder",
        "create-folder",
        "list-folders",
    ];

    for command in COMMANDS {
        let documented = text.contains(&format!("  {command}"));
        let dispatched = text.contains(&format!("\"{command}\" =>"));
        if documented && !dispatched {
            findings.push(Finding::new(
                "cli-usage-drift",
                rel.clone(),
                format!(
                    "google-docs documents '{command}' in print_usage(), but main() has no matching dispatcher arm"
                ),
            ));
        }
    }
}

#[test]
#[ignore = "manual drift audit; run explicitly with cargo test -p run_task_module --test drift_audit -- --ignored --nocapture"]
fn manual_drift_audit_reports_contract_drift() {
    let root = service_root();
    let mut findings = Vec::new();

    audit_prompt_skill_paths(&root, &mut findings);
    audit_legacy_cli_references(&root, &mut findings);
    audit_google_docs_usage_dispatch(&root, &mut findings);

    findings.sort_by_key(|finding| finding.render());

    if findings.is_empty() {
        eprintln!("No drift findings detected.");
    } else {
        eprintln!("Drift findings ({}):", findings.len());
        for finding in &findings {
            eprintln!("- {}", finding.render());
        }
    }

    assert!(
        findings.is_empty(),
        "manual drift audit found {} finding(s)",
        findings.len()
    );
}
