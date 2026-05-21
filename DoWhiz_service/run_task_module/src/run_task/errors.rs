use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum RunTaskError {
    Io(io::Error),
    MissingEnv {
        key: &'static str,
    },
    InvalidPath {
        label: &'static str,
        path: PathBuf,
        reason: &'static str,
    },
    CodexNotFound,
    CodexFailed {
        status: Option<i32>,
        output: String,
    },
    ClaudeNotFound,
    ClaudeInstallFailed {
        output: String,
    },
    ClaudeFailed {
        status: Option<i32>,
        output: String,
    },
    DockerNotFound,
    DockerFailed {
        status: Option<i32>,
        output: String,
    },
    AzureCliNotFound,
    LocalExecutionForbidden {
        deploy_target: String,
    },
    CommandTimeout {
        command: &'static str,
        timeout_secs: u64,
        output: String,
    },
    Canceled {
        reason: String,
        output: String,
    },
    GitHubAuthCommandNotFound {
        command: &'static str,
    },
    GitHubAuthFailed {
        command: &'static str,
        status: Option<i32>,
        output: String,
    },
    BrowserbaseFailed {
        action: &'static str,
        output: String,
    },
    FallbackFailed {
        primary: String,
        fallback: String,
    },
    OutputMissing {
        path: PathBuf,
        output: String,
    },
    OutputContractViolation {
        path: PathBuf,
        reason: String,
        output: String,
    },
}

const REDACTED_SECRET: &str = "REDACTED";
const SECRET_TOKEN_PREFIXES: &[&str] = &[
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "GOCSPX-",
    "ya29.",
    "ntn_",
    "secret_",
];
const SECRET_ASSIGNMENT_KEYS: &[&str] = &[
    "access_token",
    "api_key",
    "apikey",
    "auth_token",
    "client_secret",
    "connection_string",
    "notion_api_token",
    "oauth_token",
    "password",
    "private_key",
    "refresh_token",
    "secret",
    "storage_key",
];
const SECRET_QUERY_PARAMS: &[&str] = &[
    "access_token",
    "code",
    "sig",
    "se",
    "sp",
    "sr",
    "ss",
    "srt",
    "st",
    "sv",
];

pub fn redact_sensitive_text_for_display(input: &str) -> String {
    let mut redacted = redact_sensitive_assignment_lines(input);
    for prefix in SECRET_TOKEN_PREFIXES {
        redacted = redact_prefixed_token(&redacted, prefix);
    }
    for key in SECRET_QUERY_PARAMS {
        redacted = redact_query_param(&redacted, key);
    }
    redacted
}

fn redact_sensitive_assignment_lines(input: &str) -> String {
    let mut redacted = String::with_capacity(input.len());
    for line in input.split_inclusive('\n') {
        let (body, newline) = line
            .strip_suffix('\n')
            .map(|body| (body, "\n"))
            .unwrap_or((line, ""));
        let lower = body.to_ascii_lowercase();
        if SECRET_ASSIGNMENT_KEYS.iter().any(|key| lower.contains(key)) {
            if let Some(idx) = body.find(':').or_else(|| body.find('=')) {
                redacted.push_str(body[..=idx].trim_end());
                redacted.push(' ');
                redacted.push_str(REDACTED_SECRET);
            } else {
                redacted.push_str(REDACTED_SECRET);
            }
        } else {
            redacted.push_str(body);
        }
        redacted.push_str(newline);
    }
    redacted
}

fn redact_prefixed_token(input: &str, prefix: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut remaining = input;
    while let Some(idx) = remaining.find(prefix) {
        let token_start = idx + prefix.len();
        output.push_str(&remaining[..token_start]);
        output.push_str(REDACTED_SECRET);
        let tail = &remaining[token_start..];
        let token_end = tail
            .char_indices()
            .find_map(|(offset, ch)| {
                if is_token_boundary(ch) {
                    Some(offset)
                } else {
                    None
                }
            })
            .unwrap_or(tail.len());
        remaining = &tail[token_end..];
    }
    output.push_str(remaining);
    output
}

fn is_token_boundary(ch: char) -> bool {
    !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

fn redact_query_param(input: &str, key: &str) -> String {
    let pattern = format!("{key}=");
    let mut output = String::with_capacity(input.len());
    let mut remaining = input;
    while let Some(idx) = remaining.find(&pattern) {
        let value_start = idx + pattern.len();
        output.push_str(&remaining[..value_start]);
        output.push_str(REDACTED_SECRET);
        let tail = &remaining[value_start..];
        let value_end = tail
            .char_indices()
            .find_map(|(offset, ch)| match ch {
                '&' | ' ' | '\n' | '\r' | '\t' | '"' | '\'' | '<' | '>' => Some(offset),
                _ => None,
            })
            .unwrap_or(tail.len());
        remaining = &tail[value_end..];
    }
    output.push_str(remaining);
    output
}

impl fmt::Display for RunTaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunTaskError::Io(err) => write!(f, "I/O error: {}", err),
            RunTaskError::MissingEnv { key } => write!(f, "Missing environment variable: {}", key),
            RunTaskError::InvalidPath {
                label,
                path,
                reason,
            } => write!(
                f,
                "Invalid path for {}: {} ({})",
                label,
                path.display(),
                reason
            ),
            RunTaskError::CodexNotFound => write!(f, "Codex CLI not found on PATH."),
            RunTaskError::CodexFailed { status, output } => write!(
                f,
                "Codex failed (status: {:?}). Output tail:\n{}",
                status,
                redact_sensitive_text_for_display(output)
            ),
            RunTaskError::ClaudeNotFound => write!(f, "Claude CLI not found on PATH."),
            RunTaskError::ClaudeInstallFailed { output } => {
                write!(
                    f,
                    "Failed to install Claude CLI. Output tail:\n{}",
                    redact_sensitive_text_for_display(output)
                )
            }
            RunTaskError::ClaudeFailed { status, output } => write!(
                f,
                "Claude failed (status: {:?}). Output tail:\n{}",
                status,
                redact_sensitive_text_for_display(output)
            ),
            RunTaskError::DockerNotFound => write!(f, "Docker CLI not found on PATH."),
            RunTaskError::DockerFailed { status, output } => write!(
                f,
                "Docker run failed (status: {:?}). Output tail:\n{}",
                status,
                redact_sensitive_text_for_display(output)
            ),
            RunTaskError::AzureCliNotFound => write!(f, "Azure CLI (az) not found on PATH."),
            RunTaskError::LocalExecutionForbidden { deploy_target } => write!(
                f,
                "Local Codex execution is forbidden for DEPLOY_TARGET='{}'. Configure RUN_TASK_EXECUTION_BACKEND=azure_aci and required Azure ACI settings.",
                deploy_target
            ),
            RunTaskError::CommandTimeout {
                command,
                timeout_secs,
                output,
            } => write!(
                f,
                "Command timed out ({} after {}s). Output tail:\n{}",
                command,
                timeout_secs,
                redact_sensitive_text_for_display(output)
            ),
            RunTaskError::Canceled { reason, output } => write!(
                f,
                "Run task canceled: {}\nOutput tail:\n{}",
                reason,
                redact_sensitive_text_for_display(output)
            ),
            RunTaskError::GitHubAuthCommandNotFound { command } => {
                write!(f, "GitHub auth command not found on PATH: {}", command)
            }
            RunTaskError::GitHubAuthFailed {
                command,
                status,
                output,
            } => write!(
                f,
                "GitHub auth command failed ({} status: {:?}). Output tail:\n{}",
                command,
                status,
                redact_sensitive_text_for_display(output)
            ),
            RunTaskError::BrowserbaseFailed { action, output } => write!(
                f,
                "Browserbase {} failed. Output tail:\n{}",
                action,
                redact_sensitive_text_for_display(output)
            ),
            RunTaskError::FallbackFailed { primary, fallback } => write!(
                f,
                "Primary runner failed:\n{}\n\nClaude fallback failed:\n{}",
                redact_sensitive_text_for_display(primary),
                redact_sensitive_text_for_display(fallback)
            ),
            RunTaskError::OutputMissing { path, output } => {
                write!(
                    f,
                    "Expected output not found: {}\nCodex output tail:\n{}",
                    path.display(),
                    redact_sensitive_text_for_display(output)
                )
            }
            RunTaskError::OutputContractViolation {
                path,
                reason,
                output,
            } => write!(
                f,
                "Output contract violation at {}: {}\nCodex output tail:\n{}",
                path.display(),
                reason,
                redact_sensitive_text_for_display(output)
            ),
        }
    }
}

impl std::error::Error for RunTaskError {}

impl From<io::Error> for RunTaskError {
    fn from(err: io::Error) -> Self {
        RunTaskError::Io(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_redacts_common_secret_shapes() {
        let err = RunTaskError::CodexFailed {
            status: Some(1),
            output: r#"--- .config/gh/hosts.yml ---
oauth_token: ghp_abcdefghijklmnopqrstuvwxyz1234567890
--- .secrets/google_workspace_cli_credentials.json ---
{
  "client_secret": "GOCSPX-secretvalue",
  "refresh_token": "1//06refreshsecret"
}
https://example.blob.core.windows.net/share?sig=leakedSig&se=2099-01-01
"#
            .to_string(),
        };

        let rendered = err.to_string();
        assert!(rendered.contains("REDACTED"));
        assert!(!rendered.contains("abcdefghijklmnopqrstuvwxyz"));
        assert!(!rendered.contains("GOCSPX-secretvalue"));
        assert!(!rendered.contains("06refreshsecret"));
        assert!(!rendered.contains("leakedSig"));
    }
}
