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
                status, output
            ),
            RunTaskError::ClaudeNotFound => write!(f, "Claude CLI not found on PATH."),
            RunTaskError::ClaudeInstallFailed { output } => {
                write!(f, "Failed to install Claude CLI. Output tail:\n{}", output)
            }
            RunTaskError::ClaudeFailed { status, output } => write!(
                f,
                "Claude failed (status: {:?}). Output tail:\n{}",
                status, output
            ),
            RunTaskError::DockerNotFound => write!(f, "Docker CLI not found on PATH."),
            RunTaskError::DockerFailed { status, output } => write!(
                f,
                "Docker run failed (status: {:?}). Output tail:\n{}",
                status, output
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
                command, timeout_secs, output
            ),
            RunTaskError::Canceled { reason, output } => write!(
                f,
                "Run task canceled: {}\nOutput tail:\n{}",
                reason, output
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
                command, status, output
            ),
            RunTaskError::BrowserbaseFailed { action, output } => write!(
                f,
                "Browserbase {} failed. Output tail:\n{}",
                action, output
            ),
            RunTaskError::FallbackFailed { primary, fallback } => write!(
                f,
                "Primary runner failed:\n{}\n\nClaude fallback failed:\n{}",
                primary, fallback
            ),
            RunTaskError::OutputMissing { path, output } => {
                write!(
                    f,
                    "Expected output not found: {}\nCodex output tail:\n{}",
                    path.display(),
                    output
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
                output
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
