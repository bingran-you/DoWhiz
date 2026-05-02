use clap::Parser;
use run_task_module::{run_task, RunTaskParams, UserIdentities};
use send_emails_module::normalize_email_html;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "investment_eval")]
struct Args {
    #[arg(long)]
    workspace_dir: PathBuf,
    #[arg(long)]
    prompt: String,
    #[arg(long)]
    subject: String,
    #[arg(long, default_value = "little_bear")]
    employee: String,
    #[arg(long, default_value = "gpt-5.4")]
    model: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    prepare_workspace(&args.workspace_dir)?;
    write_inbound_email(&args.workspace_dir, &args.subject, &args.prompt)?;
    install_runtime_skills_and_guidance(&args.workspace_dir, &args.employee)?;

    let output = run_task(&RunTaskParams {
        workspace_dir: args.workspace_dir.clone(),
        input_email_dir: PathBuf::from("incoming_email"),
        input_attachments_dir: PathBuf::from("incoming_attachments"),
        memory_dir: PathBuf::from("memory"),
        reference_dir: PathBuf::from("references"),
        reply_to: vec!["user@example.com".to_string()],
        model_name: args.model.clone(),
        runner: "codex".to_string(),
        codex_disabled: false,
        channel: "email".to_string(),
        google_access_token: None,
        notion_access_token: None,
        has_unified_account: true,
        user_identities: UserIdentities::default(),
        thread_epoch: None,
        thread_state_path: None,
    })?;

    let raw_reply = fs::read_to_string(&output.reply_html_path)?;
    let final_rendered_path = args.workspace_dir.join("final_rendered_email.html");
    let final_rendered_html = normalize_email_html(&args.subject, &raw_reply);
    fs::write(&final_rendered_path, &final_rendered_html)?;

    println!(
        "{}",
        serde_json::json!({
            "workspace_dir": args.workspace_dir,
            "reply_draft_path": output.reply_html_path,
            "final_rendered_path": final_rendered_path,
            "reply_attachments_dir": output.reply_attachments_dir,
            "recovery_note": output.recovery_note,
        })
    );

    Ok(())
}

fn prepare_workspace(workspace_dir: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(workspace_dir.join("incoming_email"))?;
    fs::create_dir_all(workspace_dir.join("incoming_email").join("entries"))?;
    fs::create_dir_all(workspace_dir.join("incoming_attachments"))?;
    fs::create_dir_all(workspace_dir.join("incoming_attachments").join("entries"))?;
    fs::create_dir_all(workspace_dir.join("memory"))?;
    fs::create_dir_all(workspace_dir.join("references"))?;
    Ok(())
}

fn write_inbound_email(
    workspace_dir: &Path,
    subject: &str,
    prompt: &str,
) -> Result<(), std::io::Error> {
    let payload = serde_json::json!({
        "Subject": subject,
        "TextBody": prompt,
        "HtmlBody": format!("<p>{}</p>", prompt),
    });
    fs::write(
        workspace_dir
            .join("incoming_email")
            .join("postmark_payload.json"),
        serde_json::to_string_pretty(&payload).map_err(std::io::Error::other)?,
    )?;
    fs::write(
        workspace_dir.join("incoming_email").join("email.html"),
        format!("<p>{}</p>", prompt),
    )?;
    fs::write(
        workspace_dir
            .join("incoming_email")
            .join("thread_request.md"),
        format!("{prompt}\n"),
    )?;
    fs::write(
        workspace_dir.join("incoming_email").join("thread_history.md"),
        format!(
            "# Thread history\n\n- Subject: {subject}\n- Canonical request: incoming_email/thread_request.md\n- Latest HTML body: incoming_email/email.html\n- Raw payload: incoming_email/postmark_payload.json\n- Historical entries directory: incoming_email/entries/\n"
        ),
    )?;
    fs::write(
        workspace_dir
            .join("incoming_email")
            .join("entries")
            .join("0001_latest_email.html"),
        format!("<p>{}</p>", prompt),
    )?;
    fs::write(
        workspace_dir
            .join("incoming_email")
            .join("entries")
            .join("0001_postmark_payload.json"),
        serde_json::to_string_pretty(&payload).map_err(std::io::Error::other)?,
    )?;
    Ok(())
}

fn install_runtime_skills_and_guidance(
    workspace_dir: &Path,
    employee_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let service_root = manifest_dir
        .parent()
        .expect("scheduler_module lives under DoWhiz_service");
    let skills_root = service_root.join("skills");
    let employee_root = service_root.join("employees").join(employee_id);

    scheduler_module::service::copy_dir_recursive(
        &skills_root,
        &workspace_dir.join(".agents").join("skills"),
    )?;

    for filename in ["AGENTS.md", "CLAUDE.md", "SOUL.md"] {
        let src = employee_root.join(filename);
        if src.exists() {
            fs::copy(src, workspace_dir.join(filename))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_inbound_email_populates_thread_files_for_local_live_eval() {
        let temp = tempfile::tempdir().expect("tempdir");
        prepare_workspace(temp.path()).expect("prepare workspace");
        write_inbound_email(
            temp.path(),
            "NVDA monitor check",
            "Check whether anything material changed for NVDA since your last note. Only tell me if I should act.",
        )
        .expect("write inbound email");

        let incoming = temp.path().join("incoming_email");
        assert_eq!(
            fs::read_to_string(incoming.join("thread_request.md")).expect("thread request"),
            "Check whether anything material changed for NVDA since your last note. Only tell me if I should act.\n"
        );
        let thread_history =
            fs::read_to_string(incoming.join("thread_history.md")).expect("thread history");
        assert!(thread_history.contains("incoming_email/thread_request.md"));
        assert!(thread_history.contains("incoming_email/entries/"));
        assert!(incoming
            .join("entries")
            .join("0001_latest_email.html")
            .exists());
        assert!(incoming
            .join("entries")
            .join("0001_postmark_payload.json")
            .exists());
        assert!(temp
            .path()
            .join("incoming_attachments")
            .join("entries")
            .is_dir());
    }
}
