use std::fs;
use std::path::Path;

use super::errors::RunTaskError;
use super::reply_contract::{investment_request_for_workspace, reply_artifact_ready_for_workspace};

pub(super) fn maybe_write_investment_operational_failure_artifact(
    workspace_dir: &Path,
    reply_path: &Path,
    cause: &RunTaskError,
) -> Result<Option<String>, RunTaskError> {
    if !investment_request_for_workspace(workspace_dir)? {
        return Ok(None);
    }

    fs::write(reply_path, build_operational_failure_artifact())?;
    if !reply_artifact_ready_for_workspace(workspace_dir, reply_path) {
        return Ok(None);
    }

    Ok(Some(format!(
        "Sent operational failure reply after investment analysis runners failed ({})",
        summarize_failure(cause)
    )))
}

fn summarize_failure(cause: &RunTaskError) -> &'static str {
    match cause {
        RunTaskError::CommandTimeout { .. } => "analysis timed out",
        RunTaskError::CodexFailed { .. } => "Codex failed",
        RunTaskError::ClaudeFailed { .. } => "Claude failed",
        RunTaskError::OutputMissing { .. } => "analysis finished without a deliverable",
        RunTaskError::OutputContractViolation { .. } => "analysis wrote an invalid artifact",
        RunTaskError::FallbackFailed { .. } => "multiple analysis attempts failed",
        _ => "runtime failure",
    }
}

fn build_operational_failure_artifact() -> &'static str {
    r#"<html>
  <body>
    <h1>Investment analysis could not be completed</h1>
    <p>I could not complete a reliable investment analysis for this request because the analysis runtime failed before producing a valid answer.</p>
    <p>No investment recommendation is included in this email.</p>
    <p>Please retry the request. If this keeps happening, reply and the analysis can be rerun with the same thread context.</p>
  </body>
</html>
"#
}
