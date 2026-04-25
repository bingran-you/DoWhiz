pub mod agent_market;
pub mod analytics;
pub mod auth;
pub mod billing;
mod browser_handoff;
mod chat_history;
mod config;
mod email;
pub mod grocery;
mod html;
mod inbound;
mod ingestion;
pub mod launch_execution;
mod onboarding;
mod postmark;
mod recipients;
mod scheduler;
mod server;
pub mod startup_workspace;
mod state;
pub mod task_ops;
mod workspace;

pub(crate) type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub use crate::thread_state::{bump_thread_state, default_thread_state_path};

pub use config::{ServiceConfig, DEFAULT_INBOUND_BODY_MAX_BYTES};
pub use email::{process_inbound_payload, PostmarkInbound};
pub use html::{derive_inbound_email_text, render_plain_text_as_html};
pub use onboarding::InstallOnboardingConfig;
pub use scheduler::cancel_pending_thread_tasks;
pub use server::run_server;
pub(crate) use workspace::ensure_thread_workspace;
pub use workspace::{bootstrap_startup_workspace_files, copy_dir_recursive};

pub(crate) use chat_history::{
    write_discord_chat_history_scope_file, write_slack_chat_history_scope_file,
};
pub(crate) use config::{default_employee_config_path, resolve_telegram_bot_token};
pub(crate) use inbound::{
    build_discord_message_text_with_quote, build_discord_router_context,
    hydrate_discord_attachments, hydrate_discord_context_files, persist_discord_ingest_context,
};
