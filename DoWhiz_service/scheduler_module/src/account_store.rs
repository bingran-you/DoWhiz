use chrono::{DateTime, Utc};
use postgres_native_tls::MakeTlsConnector;
use r2d2::{Pool, PooledConnection};
use r2d2_postgres::PostgresConnectionManager;
use serde_json::Value;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::env_alias::var_with_scale_oliver;

type PgPool = Pool<PostgresConnectionManager<MakeTlsConnector>>;
type PgConn = PooledConnection<PostgresConnectionManager<MakeTlsConnector>>;

fn parse_bool_env(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn account_store_allow_invalid_certs() -> bool {
    if let Some(value) = var_with_scale_oliver("ACCOUNT_STORE_TLS_ALLOW_INVALID_CERTS") {
        return parse_bool_env(&value);
    }
    if let Some(value) = var_with_scale_oliver("INGESTION_QUEUE_TLS_ALLOW_INVALID_CERTS") {
        return parse_bool_env(&value);
    }
    env::var("DEPLOY_TARGET")
        .ok()
        .map(|value| value.trim().eq_ignore_ascii_case("staging"))
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct Account {
    pub id: Uuid,
    pub auth_user_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub tokens_to_hours: Option<f64>,
    pub purchased_hours: Option<f64>,
    pub organization_id: Option<Uuid>,
    pub organization_accept_status: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BalanceInfo {
    pub purchased_hours: f64,
    pub used_hours: f64,
    pub balance_hours: f64,
}

#[derive(Debug, Clone)]
pub struct Payment {
    pub stripe_session_id: String,
    pub account_id: Uuid,
    pub amount_cents: i32,
    pub hours_purchased: f64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AccountIdentifier {
    pub id: Uuid,
    pub account_id: Uuid,
    pub identifier_type: String,
    pub identifier: String,
    pub verified: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct EmailVerificationToken {
    pub token: String,
    pub account_id: Uuid,
    pub email: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AnalyticsEventInsert {
    pub event_name: String,
    pub source: String,
    pub event_timestamp: DateTime<Utc>,
    pub account_id: Option<Uuid>,
    pub auth_user_id: Option<Uuid>,
    pub anonymous_id: Option<String>,
    pub session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub org_id: Option<String>,
    pub plan_type: Option<String>,
    pub environment: Option<String>,
    pub app_version: Option<String>,
    pub page_path: Option<String>,
    pub route_path: Option<String>,
    pub referrer: Option<String>,
    pub utm_source: Option<String>,
    pub utm_medium: Option<String>,
    pub utm_campaign: Option<String>,
    pub utm_term: Option<String>,
    pub utm_content: Option<String>,
    pub device_type: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub event_key: Option<String>,
    pub properties: Value,
}

#[derive(Debug, Clone)]
pub struct AnalyticsEventRecord {
    pub event_name: String,
    pub source: String,
    pub event_timestamp: DateTime<Utc>,
    pub account_id: Option<Uuid>,
    pub auth_user_id: Option<Uuid>,
    pub anonymous_id: Option<String>,
    pub session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub org_id: Option<String>,
    pub plan_type: Option<String>,
    pub environment: Option<String>,
    pub app_version: Option<String>,
    pub page_path: Option<String>,
    pub route_path: Option<String>,
    pub referrer: Option<String>,
    pub utm_source: Option<String>,
    pub utm_medium: Option<String>,
    pub utm_campaign: Option<String>,
    pub utm_term: Option<String>,
    pub utm_content: Option<String>,
    pub device_type: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub event_key: Option<String>,
    pub properties: Value,
}

#[derive(Debug, Clone)]
pub struct RecommendationPreferenceRecord {
    pub account_id: Uuid,
    pub proactivity_level: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RecommendationFeedbackRecord {
    pub id: Uuid,
    pub account_id: Uuid,
    pub recommendation_key: String,
    pub state_signature: String,
    pub feedback: String,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

/// User contact directory entry for TPM cross-channel messaging.
///
/// Maps a Notion user to their Slack/Discord handles for proactive outreach.
#[derive(Debug, Clone)]
pub struct UserContact {
    pub id: Uuid,
    pub account_id: Uuid,
    /// Notion person ID (e.g., from task assignee)
    pub notion_user_id: Option<String>,
    /// Notion workspace this mapping applies to
    pub notion_workspace_id: Option<String>,
    /// Slack member ID (e.g., U12345ABC)
    pub slack_user_id: Option<String>,
    /// Slack workspace/team ID
    pub slack_workspace_id: Option<String>,
    /// Discord user ID (snowflake)
    pub discord_user_id: Option<String>,
    /// Discord guild/server ID
    pub discord_guild_id: Option<String>,
    /// Preferred contact channel: "slack", "discord", or "email"
    pub preferred_channel: Option<String>,
    /// How often to check in (days), e.g., 3 means every 3 days
    pub contact_frequency_days: Option<i32>,
    /// Last time this user was contacted for TPM follow-up
    pub last_contacted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Organization for TPM multi-tenant task management.
///
/// Organizations group accounts and link to Notion task boards.
#[derive(Debug, Clone)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    /// Notion database ID for this organization's task board
    pub notion_database_id: Option<String>,
    /// Notion workspace ID where the task board lives
    pub notion_workspace_id: Option<String>,
    /// Account ID of the organization leader (whose Notion credentials are used for TPM)
    pub leader_account_id: Option<Uuid>,
    /// Discord guild (server) ID for bug scanning during TPM syncs
    pub discord_guild_id: Option<String>,
    /// Slack team (workspace) ID for TPM notifications
    pub slack_team_id: Option<String>,
    /// GitHub organization name for scoping repo searches (prevents cross-user leakage)
    pub github_org_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Organization member info with name/email from auth.users
#[derive(Debug, Clone)]
pub struct OrgMember {
    pub account_id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub notion_user_id: Option<String>,
    pub slack_user_id: Option<String>,
    pub discord_user_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChannelInstallOnboardingState {
    pub account_id: Uuid,
    pub platform: String,
    pub workspace_id: String,
    pub workspace_name: Option<String>,
    pub installer_identifier: Option<String>,
    pub installer_identifier_source: Option<String>,
    pub public_channel_id: Option<String>,
    pub public_channel_name: Option<String>,
    pub dm_recipient_identifier: Option<String>,
    pub dm_recipient_source: Option<String>,
    pub last_event_key: Option<String>,
    pub last_public_status: Option<String>,
    pub last_public_error: Option<String>,
    pub last_dm_status: Option<String>,
    pub last_dm_error: Option<String>,
    pub last_skip_reason: Option<String>,
    pub last_attempted_at: Option<DateTime<Utc>>,
    pub last_succeeded_at: Option<DateTime<Utc>>,
    pub last_manual_resend_at: Option<DateTime<Utc>>,
}

#[derive(Debug, thiserror::Error)]
pub enum AccountStoreError {
    #[error("postgres error: {0}")]
    Postgres(#[from] postgres::Error),
    #[error("pool error: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("missing SUPABASE_DB_URL and SUPABASE_POOLER_URL")]
    MissingDbUrl,
    #[error("account not found")]
    NotFound,
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("identifier already linked to another account")]
    IdentifierTaken,
    #[error("verification token expired or invalid")]
    TokenInvalid,
    #[error("config error: {0}")]
    Config(String),
}

/// Custom error handler that logs connection errors
#[derive(Debug)]
struct LoggingErrorHandler {
    pool_name: &'static str,
}

impl r2d2::HandleError<postgres::Error> for LoggingErrorHandler {
    fn handle_error(&self, err: postgres::Error) {
        error!(
            "account_store {} postgres pool error: {:?}",
            self.pool_name, err
        );
    }
}

#[derive(Clone)]
pub struct AccountStore {
    primary_pool: Option<PgPool>,
    fallback_pool: Option<PgPool>,
    prefer_fallback: Arc<AtomicBool>,
}

fn log_record_analytics_event_error(
    context: &str,
    event: &AnalyticsEventInsert,
    err: &AccountStoreError,
) {
    let account_suffix = event
        .account_id
        .map(|id| format!(" for account {}", id))
        .unwrap_or_default();
    warn!(
        "{}: failed to record analytics event {}{}: {}",
        context, event.event_name, account_suffix, err
    );
}

impl AccountStore {
    pub fn from_env() -> Result<Self, AccountStoreError> {
        let primary_db_url = env::var("SUPABASE_DB_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(|v| v.trim().to_string());
        let fallback_db_url = env::var("SUPABASE_POOLER_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(|v| v.trim().to_string());

        if primary_db_url.is_none() && fallback_db_url.is_none() {
            return Err(AccountStoreError::MissingDbUrl);
        }

        let primary_pool = match primary_db_url {
            Some(db_url) => match Self::build_pool(&db_url, "primary") {
                Ok(pool) => Some(pool),
                Err(err) => {
                    if fallback_db_url.is_some() {
                        warn!(
                            "account_store failed to initialize SUPABASE_DB_URL pool ({}), will rely on SUPABASE_POOLER_URL fallback",
                            err
                        );
                        None
                    } else {
                        return Err(err);
                    }
                }
            },
            None => None,
        };

        let fallback_pool = match fallback_db_url {
            Some(db_url) => Some(Self::build_pool(&db_url, "pooler_fallback")?),
            None => None,
        };

        if primary_pool.is_none() && fallback_pool.is_none() {
            return Err(AccountStoreError::MissingDbUrl);
        }

        if primary_pool.is_none() && fallback_pool.is_some() {
            info!("account_store initialized with SUPABASE_POOLER_URL only");
        }

        let store = Self {
            primary_pool,
            fallback_pool,
            prefer_fallback: Arc::new(AtomicBool::new(false)),
        };
        store.ensure_core_schema()?;
        store.ensure_analytics_schema()?;
        Ok(store)
    }

    pub fn new(db_url: &str) -> Result<Self, AccountStoreError> {
        let primary_pool = Self::build_pool(db_url, "primary")?;
        let store = Self {
            primary_pool: Some(primary_pool),
            fallback_pool: None,
            prefer_fallback: Arc::new(AtomicBool::new(false)),
        };
        store.ensure_core_schema()?;
        store.ensure_analytics_schema()?;
        Ok(store)
    }

    #[cfg(test)]
    pub(crate) fn detached_for_tests() -> Self {
        Self {
            primary_pool: None,
            fallback_pool: None,
            prefer_fallback: Arc::new(AtomicBool::new(false)),
        }
    }

    fn build_pool(db_url: &str, pool_name: &'static str) -> Result<PgPool, AccountStoreError> {
        let config: postgres::Config = db_url.parse()?;

        let mut tls_builder = native_tls::TlsConnector::builder();
        if account_store_allow_invalid_certs() {
            warn!(
                "account_store {} TLS verification relaxed (invalid certs/hostnames allowed)",
                pool_name
            );
            tls_builder.danger_accept_invalid_certs(true);
            tls_builder.danger_accept_invalid_hostnames(true);
        }
        let tls_connector = tls_builder
            .build()
            .map_err(|e| AccountStoreError::Config(e.to_string()))?;
        let tls = MakeTlsConnector::new(tls_connector);

        let manager = PostgresConnectionManager::new(config, tls);
        Pool::builder()
            .max_size(10)
            .min_idle(Some(0))
            .connection_timeout(std::time::Duration::from_secs(10))
            .idle_timeout(Some(std::time::Duration::from_secs(30)))
            .max_lifetime(Some(std::time::Duration::from_secs(300)))
            .test_on_check_out(true)
            .error_handler(Box::new(LoggingErrorHandler { pool_name }))
            .build(manager)
            .map_err(AccountStoreError::from)
    }

    fn conn(&self) -> Result<PgConn, AccountStoreError> {
        if self.prefer_fallback.load(Ordering::Relaxed) {
            if let Some(pool) = self.fallback_pool.as_ref() {
                match pool.get() {
                    Ok(conn) => return Ok(conn),
                    Err(fallback_err) => {
                        warn!(
                            "account_store pooler fallback connection failed: {}, retrying primary db",
                            fallback_err
                        );
                    }
                }
            }
        }

        if let Some(pool) = self.primary_pool.as_ref() {
            match pool.get() {
                Ok(conn) => return Ok(conn),
                Err(primary_err) => {
                    if let Some(fallback_pool) = self.fallback_pool.as_ref() {
                        warn!(
                            "account_store primary db connection failed ({}), switching to SUPABASE_POOLER_URL",
                            primary_err
                        );
                        self.prefer_fallback.store(true, Ordering::Relaxed);
                        return Ok(fallback_pool.get()?);
                    }
                    return Err(primary_err.into());
                }
            }
        }

        if let Some(pool) = self.fallback_pool.as_ref() {
            return Ok(pool.get()?);
        }

        Err(AccountStoreError::Config(
            "account store pools dropped".to_string(),
        ))
    }

    fn ensure_core_schema(&self) -> Result<(), AccountStoreError> {
        let mut conn = self.conn()?;
        conn.batch_execute(
            "
            CREATE EXTENSION IF NOT EXISTS pgcrypto;

            CREATE TABLE IF NOT EXISTS organizations (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name TEXT NOT NULL UNIQUE,
                notion_database_id TEXT NULL,
                notion_workspace_id TEXT NULL,
                leader_account_id UUID NULL,
                github_org_name TEXT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS accounts (
                id UUID PRIMARY KEY,
                auth_user_id UUID NOT NULL UNIQUE,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                tokens_to_hours DOUBLE PRECISION NOT NULL DEFAULT 0,
                purchased_hours DOUBLE PRECISION NOT NULL DEFAULT 0,
                organization_id UUID NULL REFERENCES organizations(id) ON DELETE SET NULL,
                organization_accept_status TEXT NULL
            );

            CREATE TABLE IF NOT EXISTS account_identifiers (
                id UUID PRIMARY KEY,
                account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                identifier_type TEXT NOT NULL,
                identifier TEXT NOT NULL,
                verified BOOLEAN NOT NULL DEFAULT false,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE (identifier_type, identifier)
            );

            CREATE INDEX IF NOT EXISTS account_identifiers_account_idx
                ON account_identifiers (account_id);
            CREATE INDEX IF NOT EXISTS account_identifiers_lookup_idx
                ON account_identifiers (identifier_type, identifier, verified);

            CREATE TABLE IF NOT EXISTS payments (
                stripe_session_id TEXT PRIMARY KEY,
                account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                amount_cents INTEGER NOT NULL,
                hours_purchased DOUBLE PRECISION NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE INDEX IF NOT EXISTS payments_account_time_idx
                ON payments (account_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS email_verification_tokens (
                token TEXT PRIMARY KEY,
                account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                email TEXT NOT NULL,
                expires_at TIMESTAMPTZ NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE UNIQUE INDEX IF NOT EXISTS email_verification_tokens_email_idx
                ON email_verification_tokens (email);
            CREATE INDEX IF NOT EXISTS email_verification_tokens_expiry_idx
                ON email_verification_tokens (expires_at);
            ",
        )?;
        Ok(())
    }

    fn ensure_analytics_schema(&self) -> Result<(), AccountStoreError> {
        let mut conn = self.conn()?;
        conn.batch_execute(
            "
            CREATE TABLE IF NOT EXISTS analytics_events (
                id UUID PRIMARY KEY,
                event_name TEXT NOT NULL,
                source TEXT NOT NULL,
                event_timestamp TIMESTAMPTZ NOT NULL,
                account_id UUID NULL,
                auth_user_id UUID NULL,
                anonymous_id TEXT NULL,
                session_id TEXT NULL,
                workspace_id TEXT NULL,
                org_id TEXT NULL,
                plan_type TEXT NULL,
                environment TEXT NULL,
                app_version TEXT NULL,
                page_path TEXT NULL,
                route_path TEXT NULL,
                referrer TEXT NULL,
                utm_source TEXT NULL,
                utm_medium TEXT NULL,
                utm_campaign TEXT NULL,
                utm_term TEXT NULL,
                utm_content TEXT NULL,
                device_type TEXT NULL,
                browser TEXT NULL,
                os TEXT NULL,
                event_key TEXT NULL,
                properties_json TEXT NOT NULL DEFAULT '{}',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE INDEX IF NOT EXISTS analytics_events_event_time_idx
                ON analytics_events (event_timestamp);
            CREATE INDEX IF NOT EXISTS analytics_events_name_time_idx
                ON analytics_events (event_name, event_timestamp);
            CREATE INDEX IF NOT EXISTS analytics_events_account_time_idx
                ON analytics_events (account_id, event_timestamp);
            CREATE INDEX IF NOT EXISTS analytics_events_auth_user_time_idx
                ON analytics_events (auth_user_id, event_timestamp);
            CREATE INDEX IF NOT EXISTS analytics_events_anonymous_time_idx
                ON analytics_events (anonymous_id, event_timestamp);
            CREATE UNIQUE INDEX IF NOT EXISTS analytics_events_event_key_idx
                ON analytics_events (event_name, event_key)
                WHERE event_key IS NOT NULL;

            CREATE TABLE IF NOT EXISTS account_recommendation_preferences (
                account_id UUID PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
                proactivity_level TEXT NOT NULL DEFAULT 'minimal',
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS account_recommendation_feedback (
                id UUID PRIMARY KEY,
                account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                recommendation_key TEXT NOT NULL,
                state_signature TEXT NOT NULL,
                feedback TEXT NOT NULL,
                metadata_json TEXT NOT NULL DEFAULT '{}',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE INDEX IF NOT EXISTS account_recommendation_feedback_account_time_idx
                ON account_recommendation_feedback (account_id, created_at DESC);
            CREATE INDEX IF NOT EXISTS account_recommendation_feedback_key_state_time_idx
                ON account_recommendation_feedback (account_id, recommendation_key, state_signature, created_at DESC);

            CREATE TABLE IF NOT EXISTS channel_install_onboarding_state (
                account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                platform TEXT NOT NULL,
                workspace_id TEXT NOT NULL,
                workspace_name TEXT NULL,
                installer_identifier TEXT NULL,
                installer_identifier_source TEXT NULL,
                public_channel_id TEXT NULL,
                public_channel_name TEXT NULL,
                dm_recipient_identifier TEXT NULL,
                dm_recipient_source TEXT NULL,
                last_event_key TEXT NULL,
                last_public_status TEXT NULL,
                last_public_error TEXT NULL,
                last_dm_status TEXT NULL,
                last_dm_error TEXT NULL,
                last_skip_reason TEXT NULL,
                last_attempted_at TIMESTAMPTZ NULL,
                last_succeeded_at TIMESTAMPTZ NULL,
                last_manual_resend_at TIMESTAMPTZ NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                PRIMARY KEY (account_id, platform, workspace_id)
            );

            CREATE INDEX IF NOT EXISTS channel_install_onboarding_state_workspace_idx
                ON channel_install_onboarding_state (platform, workspace_id);
            CREATE INDEX IF NOT EXISTS channel_install_onboarding_state_attempted_idx
                ON channel_install_onboarding_state (last_attempted_at DESC);
            ",
        )?;
        Ok(())
    }

    /// Get a connection from the pool (primarily for tests)
    pub fn get_conn(
        &self,
    ) -> Result<PooledConnection<PostgresConnectionManager<MakeTlsConnector>>, AccountStoreError>
    {
        self.conn()
    }

    /// Create a new account linked to a Supabase auth user
    pub fn create_account(&self, auth_user_id: Uuid) -> Result<Account, AccountStoreError> {
        let mut conn = self.conn()?;
        let id = Uuid::new_v4();
        let row = conn.query_one(
            "INSERT INTO accounts (id, auth_user_id, created_at, tokens_to_hours, purchased_hours)
             VALUES ($1, $2, NOW(), 0, 0)
             RETURNING id, auth_user_id, created_at, tokens_to_hours::float8, purchased_hours::float8, organization_id, organization_accept_status",
            &[&id, &auth_user_id],
        )?;
        Ok(Account {
            id: row.get(0),
            auth_user_id: row.get(1),
            created_at: row.get(2),
            tokens_to_hours: row.get(3),
            purchased_hours: row.get(4),
            organization_id: row.get(5),
            organization_accept_status: row.get(6),
        })
    }

    /// Get account by Supabase auth user ID
    pub fn get_account_by_auth_user(
        &self,
        auth_user_id: Uuid,
    ) -> Result<Option<Account>, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_opt(
            "SELECT id, auth_user_id, created_at, tokens_to_hours::float8, purchased_hours::float8, organization_id, organization_accept_status
             FROM accounts WHERE auth_user_id = $1",
            &[&auth_user_id],
        )?;
        Ok(row.map(|r| Account {
            id: r.get(0),
            auth_user_id: r.get(1),
            created_at: r.get(2),
            tokens_to_hours: r.get(3),
            purchased_hours: r.get(4),
            organization_id: r.get(5),
            organization_accept_status: r.get(6),
        }))
    }

    /// Get account by ID
    pub fn get_account(&self, account_id: Uuid) -> Result<Option<Account>, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_opt(
            "SELECT id, auth_user_id, created_at, tokens_to_hours::float8, purchased_hours::float8, organization_id, organization_accept_status
             FROM accounts WHERE id = $1",
            &[&account_id],
        )?;
        Ok(row.map(|r| Account {
            id: r.get(0),
            auth_user_id: r.get(1),
            created_at: r.get(2),
            tokens_to_hours: r.get(3),
            purchased_hours: r.get(4),
            organization_id: r.get(5),
            organization_accept_status: r.get(6),
        }))
    }

    /// Look up account by channel identifier (for message routing)
    pub fn get_account_by_identifier(
        &self,
        identifier_type: &str,
        identifier: &str,
    ) -> Result<Option<Account>, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_opt(
            "SELECT a.id, a.auth_user_id, a.created_at, a.tokens_to_hours::float8, a.purchased_hours::float8, a.organization_id, a.organization_accept_status
             FROM accounts a
             JOIN account_identifiers ai ON ai.account_id = a.id
             WHERE ai.identifier_type = $1 AND ai.identifier = $2 AND ai.verified = true",
            &[&identifier_type, &identifier],
        )?;
        Ok(row.map(|r| Account {
            id: r.get(0),
            auth_user_id: r.get(1),
            created_at: r.get(2),
            tokens_to_hours: r.get(3),
            purchased_hours: r.get(4),
            organization_id: r.get(5),
            organization_accept_status: r.get(6),
        }))
    }

    /// Create a new identifier link (unverified by default)
    pub fn create_identifier(
        &self,
        account_id: Uuid,
        identifier_type: &str,
        identifier: &str,
    ) -> Result<AccountIdentifier, AccountStoreError> {
        let mut conn = self.conn()?;
        let id = Uuid::new_v4();

        // Check if identifier is already taken by another account
        let existing = conn.query_opt(
            "SELECT account_id FROM account_identifiers
             WHERE identifier_type = $1 AND identifier = $2",
            &[&identifier_type, &identifier],
        )?;

        if let Some(row) = existing {
            let existing_account: Uuid = row.get(0);
            if existing_account != account_id {
                return Err(AccountStoreError::IdentifierTaken);
            }
        }

        // TODO: Auto-verify for MVP. Add real verification (SMS/email codes) later.
        let row = conn.query_one(
            "INSERT INTO account_identifiers (id, account_id, identifier_type, identifier, verified, created_at)
             VALUES ($1, $2, $3, $4, true, NOW())
             ON CONFLICT (identifier_type, identifier) DO UPDATE SET account_id = $2, verified = true
             RETURNING id, account_id, identifier_type, identifier, verified, created_at",
            &[&id, &account_id, &identifier_type, &identifier],
        )?;

        Ok(AccountIdentifier {
            id: row.get(0),
            account_id: row.get(1),
            identifier_type: row.get(2),
            identifier: row.get(3),
            verified: row.get(4),
            created_at: row.get(5),
        })
    }

    /// Mark an identifier as verified
    pub fn verify_identifier(
        &self,
        account_id: Uuid,
        identifier_type: &str,
        identifier: &str,
    ) -> Result<(), AccountStoreError> {
        let mut conn = self.conn()?;
        let updated = conn.execute(
            "UPDATE account_identifiers
             SET verified = true
             WHERE account_id = $1 AND identifier_type = $2 AND identifier = $3",
            &[&account_id, &identifier_type, &identifier],
        )?;
        if updated == 0 {
            return Err(AccountStoreError::NotFound);
        }
        Ok(())
    }

    /// Delete an identifier link
    pub fn delete_identifier(
        &self,
        account_id: Uuid,
        identifier_type: &str,
        identifier: &str,
    ) -> Result<(), AccountStoreError> {
        let mut conn = self.conn()?;
        let deleted = conn.execute(
            "DELETE FROM account_identifiers
             WHERE account_id = $1 AND identifier_type = $2 AND identifier = $3",
            &[&account_id, &identifier_type, &identifier],
        )?;
        if deleted == 0 {
            return Err(AccountStoreError::NotFound);
        }
        Ok(())
    }

    /// List all identifiers for an account
    pub fn list_identifiers(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<AccountIdentifier>, AccountStoreError> {
        let mut conn = self.conn()?;
        let rows = conn.query(
            "SELECT id, account_id, identifier_type, identifier, verified, created_at
             FROM account_identifiers
             WHERE account_id = $1
             ORDER BY created_at",
            &[&account_id],
        )?;
        Ok(rows
            .iter()
            .map(|r| AccountIdentifier {
                id: r.get(0),
                account_id: r.get(1),
                identifier_type: r.get(2),
                identifier: r.get(3),
                verified: r.get(4),
                created_at: r.get(5),
            })
            .collect())
    }

    pub fn get_channel_install_onboarding_state(
        &self,
        account_id: Uuid,
        platform: &str,
        workspace_id: &str,
    ) -> Result<Option<ChannelInstallOnboardingState>, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_opt(
            "SELECT
                account_id,
                platform,
                workspace_id,
                workspace_name,
                installer_identifier,
                installer_identifier_source,
                public_channel_id,
                public_channel_name,
                dm_recipient_identifier,
                dm_recipient_source,
                last_event_key,
                last_public_status,
                last_public_error,
                last_dm_status,
                last_dm_error,
                last_skip_reason,
                last_attempted_at,
                last_succeeded_at,
                last_manual_resend_at
             FROM channel_install_onboarding_state
             WHERE account_id = $1 AND platform = $2 AND workspace_id = $3",
            &[&account_id, &platform, &workspace_id],
        )?;
        Ok(row.map(|row| ChannelInstallOnboardingState {
            account_id: row.get(0),
            platform: row.get(1),
            workspace_id: row.get(2),
            workspace_name: row.get(3),
            installer_identifier: row.get(4),
            installer_identifier_source: row.get(5),
            public_channel_id: row.get(6),
            public_channel_name: row.get(7),
            dm_recipient_identifier: row.get(8),
            dm_recipient_source: row.get(9),
            last_event_key: row.get(10),
            last_public_status: row.get(11),
            last_public_error: row.get(12),
            last_dm_status: row.get(13),
            last_dm_error: row.get(14),
            last_skip_reason: row.get(15),
            last_attempted_at: row.get(16),
            last_succeeded_at: row.get(17),
            last_manual_resend_at: row.get(18),
        }))
    }

    pub fn list_channel_install_onboarding_states(
        &self,
        account_id: Uuid,
        platform: &str,
    ) -> Result<Vec<ChannelInstallOnboardingState>, AccountStoreError> {
        let mut conn = self.conn()?;
        let rows = conn.query(
            "SELECT
                account_id,
                platform,
                workspace_id,
                workspace_name,
                installer_identifier,
                installer_identifier_source,
                public_channel_id,
                public_channel_name,
                dm_recipient_identifier,
                dm_recipient_source,
                last_event_key,
                last_public_status,
                last_public_error,
                last_dm_status,
                last_dm_error,
                last_skip_reason,
                last_attempted_at,
                last_succeeded_at,
                last_manual_resend_at
             FROM channel_install_onboarding_state
             WHERE account_id = $1 AND platform = $2
             ORDER BY last_attempted_at DESC NULLS LAST, updated_at DESC, workspace_id ASC",
            &[&account_id, &platform],
        )?;

        Ok(rows
            .into_iter()
            .map(|row| ChannelInstallOnboardingState {
                account_id: row.get(0),
                platform: row.get(1),
                workspace_id: row.get(2),
                workspace_name: row.get(3),
                installer_identifier: row.get(4),
                installer_identifier_source: row.get(5),
                public_channel_id: row.get(6),
                public_channel_name: row.get(7),
                dm_recipient_identifier: row.get(8),
                dm_recipient_source: row.get(9),
                last_event_key: row.get(10),
                last_public_status: row.get(11),
                last_public_error: row.get(12),
                last_dm_status: row.get(13),
                last_dm_error: row.get(14),
                last_skip_reason: row.get(15),
                last_attempted_at: row.get(16),
                last_succeeded_at: row.get(17),
                last_manual_resend_at: row.get(18),
            })
            .collect())
    }

    pub fn upsert_channel_install_onboarding_state(
        &self,
        state: &ChannelInstallOnboardingState,
    ) -> Result<ChannelInstallOnboardingState, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_one(
            "INSERT INTO channel_install_onboarding_state (
                account_id,
                platform,
                workspace_id,
                workspace_name,
                installer_identifier,
                installer_identifier_source,
                public_channel_id,
                public_channel_name,
                dm_recipient_identifier,
                dm_recipient_source,
                last_event_key,
                last_public_status,
                last_public_error,
                last_dm_status,
                last_dm_error,
                last_skip_reason,
                last_attempted_at,
                last_succeeded_at,
                last_manual_resend_at,
                updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, NOW()
            )
            ON CONFLICT (account_id, platform, workspace_id) DO UPDATE SET
                workspace_name = EXCLUDED.workspace_name,
                installer_identifier = EXCLUDED.installer_identifier,
                installer_identifier_source = EXCLUDED.installer_identifier_source,
                public_channel_id = EXCLUDED.public_channel_id,
                public_channel_name = EXCLUDED.public_channel_name,
                dm_recipient_identifier = EXCLUDED.dm_recipient_identifier,
                dm_recipient_source = EXCLUDED.dm_recipient_source,
                last_event_key = EXCLUDED.last_event_key,
                last_public_status = EXCLUDED.last_public_status,
                last_public_error = EXCLUDED.last_public_error,
                last_dm_status = EXCLUDED.last_dm_status,
                last_dm_error = EXCLUDED.last_dm_error,
                last_skip_reason = EXCLUDED.last_skip_reason,
                last_attempted_at = EXCLUDED.last_attempted_at,
                last_succeeded_at = EXCLUDED.last_succeeded_at,
                last_manual_resend_at = EXCLUDED.last_manual_resend_at,
                updated_at = NOW()
            RETURNING
                account_id,
                platform,
                workspace_id,
                workspace_name,
                installer_identifier,
                installer_identifier_source,
                public_channel_id,
                public_channel_name,
                dm_recipient_identifier,
                dm_recipient_source,
                last_event_key,
                last_public_status,
                last_public_error,
                last_dm_status,
                last_dm_error,
                last_skip_reason,
                last_attempted_at,
                last_succeeded_at,
                last_manual_resend_at",
            &[
                &state.account_id,
                &state.platform,
                &state.workspace_id,
                &state.workspace_name,
                &state.installer_identifier,
                &state.installer_identifier_source,
                &state.public_channel_id,
                &state.public_channel_name,
                &state.dm_recipient_identifier,
                &state.dm_recipient_source,
                &state.last_event_key,
                &state.last_public_status,
                &state.last_public_error,
                &state.last_dm_status,
                &state.last_dm_error,
                &state.last_skip_reason,
                &state.last_attempted_at,
                &state.last_succeeded_at,
                &state.last_manual_resend_at,
            ],
        )?;

        Ok(ChannelInstallOnboardingState {
            account_id: row.get(0),
            platform: row.get(1),
            workspace_id: row.get(2),
            workspace_name: row.get(3),
            installer_identifier: row.get(4),
            installer_identifier_source: row.get(5),
            public_channel_id: row.get(6),
            public_channel_name: row.get(7),
            dm_recipient_identifier: row.get(8),
            dm_recipient_source: row.get(9),
            last_event_key: row.get(10),
            last_public_status: row.get(11),
            last_public_error: row.get(12),
            last_dm_status: row.get(13),
            last_dm_error: row.get(14),
            last_skip_reason: row.get(15),
            last_attempted_at: row.get(16),
            last_succeeded_at: row.get(17),
            last_manual_resend_at: row.get(18),
        })
    }

    /// Delete an account and all its identifiers (CASCADE)
    pub fn delete_account(&self, account_id: Uuid) -> Result<(), AccountStoreError> {
        let mut conn = self.conn()?;
        let deleted = conn.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])?;
        if deleted == 0 {
            return Err(AccountStoreError::NotFound);
        }
        Ok(())
    }

    /// Add tokens to an account's running total
    pub fn add_tokens(&self, account_id: Uuid, tokens: i64) -> Result<(), AccountStoreError> {
        let mut conn = self.conn()?;
        conn.execute(
            "UPDATE accounts
             SET tokens = COALESCE(tokens, 0) + $1,
                 tokens_to_hours = (COALESCE(tokens, 0) + $1)::numeric / 20000000
             WHERE id = $2",
            &[&tokens, &account_id],
        )?;
        Ok(())
    }

    // =========================================================================
    // Billing methods
    // =========================================================================

    /// Get balance info for an account
    pub fn get_balance(&self, account_id: Uuid) -> Result<BalanceInfo, AccountStoreError> {
        let account = self
            .get_account(account_id)?
            .ok_or(AccountStoreError::NotFound)?;

        let purchased = account.purchased_hours.unwrap_or(0.0);
        let used = account.tokens_to_hours.unwrap_or(0.0);

        Ok(BalanceInfo {
            purchased_hours: purchased,
            used_hours: used,
            balance_hours: purchased - used,
        })
    }

    /// Check if account has sufficient balance (allows 1 hour grace)
    pub fn has_sufficient_balance(&self, account_id: Uuid) -> Result<bool, AccountStoreError> {
        let balance = self.get_balance(account_id)?;
        // Block if used_hours exceeds purchased_hours + 1 (1 hour grace)
        Ok(balance.used_hours <= balance.purchased_hours + 1.0)
    }

    /// Add purchased hours to an account (called after successful payment)
    pub fn add_purchased_hours(
        &self,
        account_id: Uuid,
        hours: f64,
    ) -> Result<(), AccountStoreError> {
        let mut conn = self.conn()?;
        conn.execute(
            "UPDATE accounts
             SET purchased_hours = COALESCE(purchased_hours, 0) + $1::float8
             WHERE id = $2",
            &[&hours, &account_id],
        )?;
        Ok(())
    }

    /// Record a payment in the audit table (stripe_session_id is primary key)
    pub fn record_payment(
        &self,
        account_id: Uuid,
        stripe_session_id: &str,
        amount_cents: i32,
        hours: f64,
    ) -> Result<(), AccountStoreError> {
        let mut conn = self.conn()?;
        // Use ON CONFLICT DO NOTHING for idempotency
        conn.execute(
            "INSERT INTO payments (stripe_session_id, account_id, amount_cents, hours_purchased, created_at)
             VALUES ($1, $2, $3, $4::float8, NOW())
             ON CONFLICT (stripe_session_id) DO NOTHING",
            &[&stripe_session_id, &account_id, &amount_cents, &hours],
        )?;
        Ok(())
    }

    /// Check if a payment has already been recorded (for idempotency)
    pub fn payment_exists(&self, stripe_session_id: &str) -> Result<bool, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_opt(
            "SELECT stripe_session_id FROM payments WHERE stripe_session_id = $1",
            &[&stripe_session_id],
        )?;
        Ok(row.is_some())
    }

    pub fn list_accounts_created_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Account>, AccountStoreError> {
        let mut conn = self.conn()?;
        let rows = conn.query(
            "SELECT id, auth_user_id, created_at, tokens_to_hours::float8, purchased_hours::float8, organization_id, organization_accept_status
             FROM accounts
             WHERE created_at >= $1 AND created_at < $2
             ORDER BY created_at ASC",
            &[&start, &end],
        )?;
        Ok(rows
            .iter()
            .map(|row| Account {
                id: row.get(0),
                auth_user_id: row.get(1),
                created_at: row.get(2),
                tokens_to_hours: row.get(3),
                purchased_hours: row.get(4),
                organization_id: row.get(5),
                organization_accept_status: row.get(6),
            })
            .collect())
    }

    pub fn list_payments_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Payment>, AccountStoreError> {
        let mut conn = self.conn()?;
        let rows = conn.query(
            "SELECT stripe_session_id, account_id, amount_cents, hours_purchased::float8, created_at
             FROM payments
             WHERE created_at >= $1 AND created_at < $2
             ORDER BY created_at ASC",
            &[&start, &end],
        )?;
        Ok(rows
            .iter()
            .map(|row| Payment {
                stripe_session_id: row.get(0),
                account_id: row.get(1),
                amount_cents: row.get(2),
                hours_purchased: row.get(3),
                created_at: row.get(4),
            })
            .collect())
    }

    pub fn record_analytics_event(
        &self,
        event: &AnalyticsEventInsert,
    ) -> Result<bool, AccountStoreError> {
        let mut conn = self.conn()?;
        let id = Uuid::new_v4();
        let properties_json =
            serde_json::to_string(&event.properties).unwrap_or_else(|_| "{}".to_string());
        let inserted = conn.execute(
            "INSERT INTO analytics_events (
                id,
                event_name,
                source,
                event_timestamp,
                account_id,
                auth_user_id,
                anonymous_id,
                session_id,
                workspace_id,
                org_id,
                plan_type,
                environment,
                app_version,
                page_path,
                route_path,
                referrer,
                utm_source,
                utm_medium,
                utm_campaign,
                utm_term,
                utm_content,
                device_type,
                browser,
                os,
                event_key,
                properties_json
            )
            VALUES (
                $1,  $2,  $3,  $4,  $5,  $6,  $7,  $8,  $9,  $10, $11, $12, $13,
                $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26
            )
            ON CONFLICT (event_name, event_key) WHERE event_key IS NOT NULL DO NOTHING",
            &[
                &id,
                &event.event_name,
                &event.source,
                &event.event_timestamp,
                &event.account_id,
                &event.auth_user_id,
                &event.anonymous_id,
                &event.session_id,
                &event.workspace_id,
                &event.org_id,
                &event.plan_type,
                &event.environment,
                &event.app_version,
                &event.page_path,
                &event.route_path,
                &event.referrer,
                &event.utm_source,
                &event.utm_medium,
                &event.utm_campaign,
                &event.utm_term,
                &event.utm_content,
                &event.device_type,
                &event.browser,
                &event.os,
                &event.event_key,
                &properties_json,
            ],
        )?;
        Ok(inserted > 0)
    }

    pub fn record_analytics_event_detached(
        self: &Arc<Self>,
        event: AnalyticsEventInsert,
        context: &'static str,
    ) {
        // NOTE: always use std::thread::spawn here, never tokio::spawn_blocking.
        // `record_analytics_event` ends up calling r2d2, which on connection
        // recycling drops a sync `postgres::Client`. That Drop impl calls
        // `Runtime::block_on(close_rendezvous())`, which tries to create its
        // own current-thread runtime. If the worker thread still carries a
        // tokio runtime context (as spawn_blocking threads can, via the
        // captured Handle), constructing that inner runtime panics with
        // "Cannot start a runtime from within a runtime" and, because it
        // happens in a destructor, aborts the whole process.
        // See issue #1451 for full evidence.
        let store = Arc::clone(self);
        std::mem::drop(std::thread::spawn(move || {
            if let Err(err) = store.record_analytics_event(&event) {
                log_record_analytics_event_error(context, &event, &err);
            }
        }));
    }

    pub fn count_analytics_events(
        &self,
        event_name: &str,
        account_id: Option<Uuid>,
        auth_user_id: Option<Uuid>,
    ) -> Result<i64, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = if let Some(account_id) = account_id {
            conn.query_one(
                "SELECT COUNT(*)::bigint FROM analytics_events
                 WHERE event_name = $1 AND account_id = $2",
                &[&event_name, &account_id],
            )?
        } else if let Some(auth_user_id) = auth_user_id {
            conn.query_one(
                "SELECT COUNT(*)::bigint FROM analytics_events
                 WHERE event_name = $1 AND auth_user_id = $2",
                &[&event_name, &auth_user_id],
            )?
        } else {
            conn.query_one(
                "SELECT COUNT(*)::bigint FROM analytics_events
                 WHERE event_name = $1 AND account_id IS NULL AND auth_user_id IS NULL",
                &[&event_name],
            )?
        };
        Ok(row.get(0))
    }

    pub fn list_analytics_events_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<AnalyticsEventRecord>, AccountStoreError> {
        let mut conn = self.conn()?;
        let rows = conn.query(
            "SELECT
                event_name,
                source,
                event_timestamp,
                account_id,
                auth_user_id,
                anonymous_id,
                session_id,
                workspace_id,
                org_id,
                plan_type,
                environment,
                app_version,
                page_path,
                route_path,
                referrer,
                utm_source,
                utm_medium,
                utm_campaign,
                utm_term,
                utm_content,
                device_type,
                browser,
                os,
                event_key,
                properties_json
             FROM analytics_events
             WHERE event_timestamp >= $1 AND event_timestamp < $2
             ORDER BY event_timestamp ASC, created_at ASC",
            &[&start, &end],
        )?;

        Ok(rows
            .iter()
            .map(|row| {
                let properties_json: String = row.get(24);
                let properties = serde_json::from_str(&properties_json).unwrap_or(Value::Null);
                AnalyticsEventRecord {
                    event_name: row.get(0),
                    source: row.get(1),
                    event_timestamp: row.get(2),
                    account_id: row.get(3),
                    auth_user_id: row.get(4),
                    anonymous_id: row.get(5),
                    session_id: row.get(6),
                    workspace_id: row.get(7),
                    org_id: row.get(8),
                    plan_type: row.get(9),
                    environment: row.get(10),
                    app_version: row.get(11),
                    page_path: row.get(12),
                    route_path: row.get(13),
                    referrer: row.get(14),
                    utm_source: row.get(15),
                    utm_medium: row.get(16),
                    utm_campaign: row.get(17),
                    utm_term: row.get(18),
                    utm_content: row.get(19),
                    device_type: row.get(20),
                    browser: row.get(21),
                    os: row.get(22),
                    event_key: row.get(23),
                    properties,
                }
            })
            .collect())
    }

    pub fn get_recommendation_preference(
        &self,
        account_id: Uuid,
    ) -> Result<Option<RecommendationPreferenceRecord>, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_opt(
            "SELECT account_id, proactivity_level, updated_at
             FROM account_recommendation_preferences
             WHERE account_id = $1",
            &[&account_id],
        )?;

        Ok(row.map(|value| RecommendationPreferenceRecord {
            account_id: value.get(0),
            proactivity_level: value.get(1),
            updated_at: value.get(2),
        }))
    }

    pub fn upsert_recommendation_preference(
        &self,
        account_id: Uuid,
        proactivity_level: &str,
    ) -> Result<RecommendationPreferenceRecord, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_one(
            "INSERT INTO account_recommendation_preferences (account_id, proactivity_level, updated_at)
             VALUES ($1, $2, NOW())
             ON CONFLICT (account_id)
             DO UPDATE SET proactivity_level = EXCLUDED.proactivity_level, updated_at = NOW()
             RETURNING account_id, proactivity_level, updated_at",
            &[&account_id, &proactivity_level],
        )?;

        Ok(RecommendationPreferenceRecord {
            account_id: row.get(0),
            proactivity_level: row.get(1),
            updated_at: row.get(2),
        })
    }

    pub fn record_recommendation_feedback(
        &self,
        account_id: Uuid,
        recommendation_key: &str,
        state_signature: &str,
        feedback: &str,
        metadata: &Value,
    ) -> Result<RecommendationFeedbackRecord, AccountStoreError> {
        let mut conn = self.conn()?;
        let id = Uuid::new_v4();
        let metadata_json = serde_json::to_string(metadata).unwrap_or_else(|_| "{}".to_string());
        let row = conn.query_one(
            "INSERT INTO account_recommendation_feedback (
                id,
                account_id,
                recommendation_key,
                state_signature,
                feedback,
                metadata_json,
                created_at
             )
             VALUES ($1, $2, $3, $4, $5, $6, NOW())
             RETURNING id, account_id, recommendation_key, state_signature, feedback, metadata_json, created_at",
            &[
                &id,
                &account_id,
                &recommendation_key,
                &state_signature,
                &feedback,
                &metadata_json,
            ],
        )?;

        let metadata_json: String = row.get(5);
        Ok(RecommendationFeedbackRecord {
            id: row.get(0),
            account_id: row.get(1),
            recommendation_key: row.get(2),
            state_signature: row.get(3),
            feedback: row.get(4),
            metadata: serde_json::from_str(&metadata_json).unwrap_or(Value::Null),
            created_at: row.get(6),
        })
    }

    pub fn list_recent_recommendation_feedback(
        &self,
        account_id: Uuid,
        limit: i64,
    ) -> Result<Vec<RecommendationFeedbackRecord>, AccountStoreError> {
        let mut conn = self.conn()?;
        let rows = conn.query(
            "SELECT id, account_id, recommendation_key, state_signature, feedback, metadata_json, created_at
             FROM account_recommendation_feedback
             WHERE account_id = $1
             ORDER BY created_at DESC
             LIMIT $2",
            &[&account_id, &limit],
        )?;

        Ok(rows
            .iter()
            .map(|row| {
                let metadata_json: String = row.get(5);
                RecommendationFeedbackRecord {
                    id: row.get(0),
                    account_id: row.get(1),
                    recommendation_key: row.get(2),
                    state_signature: row.get(3),
                    feedback: row.get(4),
                    metadata: serde_json::from_str(&metadata_json).unwrap_or(Value::Null),
                    created_at: row.get(6),
                }
            })
            .collect())
    }

    // =========================================================================
    // User Contact Directory (TPM Cross-Channel Messaging)
    // =========================================================================

    /// Create or update a user contact entry.
    ///
    /// Used by TPM to map Notion task assignees to their Slack/Discord handles.
    pub fn upsert_user_contact(
        &self,
        account_id: Uuid,
        notion_user_id: Option<&str>,
        notion_workspace_id: Option<&str>,
        slack_user_id: Option<&str>,
        slack_workspace_id: Option<&str>,
        discord_user_id: Option<&str>,
        discord_guild_id: Option<&str>,
        preferred_channel: Option<&str>,
        contact_frequency_days: Option<i32>,
    ) -> Result<UserContact, AccountStoreError> {
        let mut conn = self.conn()?;
        let id = Uuid::new_v4();

        let row = conn.query_one(
            "INSERT INTO user_contact_directory (
                id, account_id, notion_user_id, notion_workspace_id,
                slack_user_id, slack_workspace_id, discord_user_id, discord_guild_id,
                preferred_channel, contact_frequency_days, created_at
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
             ON CONFLICT (account_id, notion_user_id, notion_workspace_id)
             DO UPDATE SET
                slack_user_id = COALESCE(EXCLUDED.slack_user_id, user_contact_directory.slack_user_id),
                slack_workspace_id = COALESCE(EXCLUDED.slack_workspace_id, user_contact_directory.slack_workspace_id),
                discord_user_id = COALESCE(EXCLUDED.discord_user_id, user_contact_directory.discord_user_id),
                discord_guild_id = COALESCE(EXCLUDED.discord_guild_id, user_contact_directory.discord_guild_id),
                preferred_channel = COALESCE(EXCLUDED.preferred_channel, user_contact_directory.preferred_channel),
                contact_frequency_days = COALESCE(EXCLUDED.contact_frequency_days, user_contact_directory.contact_frequency_days)
             RETURNING id, account_id, notion_user_id, notion_workspace_id,
                       slack_user_id, slack_workspace_id, discord_user_id, discord_guild_id,
                       preferred_channel, contact_frequency_days, last_contacted_at, created_at",
            &[
                &id,
                &account_id,
                &notion_user_id,
                &notion_workspace_id,
                &slack_user_id,
                &slack_workspace_id,
                &discord_user_id,
                &discord_guild_id,
                &preferred_channel,
                &contact_frequency_days,
            ],
        )?;

        Ok(UserContact {
            id: row.get(0),
            account_id: row.get(1),
            notion_user_id: row.get(2),
            notion_workspace_id: row.get(3),
            slack_user_id: row.get(4),
            slack_workspace_id: row.get(5),
            discord_user_id: row.get(6),
            discord_guild_id: row.get(7),
            preferred_channel: row.get(8),
            contact_frequency_days: row.get(9),
            last_contacted_at: row.get(10),
            created_at: row.get(11),
        })
    }

    /// Get user contact by Notion user ID within a workspace.
    ///
    /// This is the primary lookup for TPM: given a task assignee (Notion user),
    /// find their Slack/Discord handles for follow-up.
    pub fn get_user_contact_by_notion_user(
        &self,
        account_id: Uuid,
        notion_workspace_id: &str,
        notion_user_id: &str,
    ) -> Result<Option<UserContact>, AccountStoreError> {
        let mut conn = self.conn()?;
        let rows = conn.query(
            "SELECT id, account_id, notion_user_id, notion_workspace_id,
                    slack_user_id, slack_workspace_id, discord_user_id, discord_guild_id,
                    preferred_channel, contact_frequency_days, last_contacted_at, created_at
             FROM user_contact_directory
             WHERE account_id = $1 AND notion_workspace_id = $2 AND notion_user_id = $3",
            &[&account_id, &notion_workspace_id, &notion_user_id],
        )?;

        if rows.is_empty() {
            return Ok(None);
        }

        let row = &rows[0];
        Ok(Some(UserContact {
            id: row.get(0),
            account_id: row.get(1),
            notion_user_id: row.get(2),
            notion_workspace_id: row.get(3),
            slack_user_id: row.get(4),
            slack_workspace_id: row.get(5),
            discord_user_id: row.get(6),
            discord_guild_id: row.get(7),
            preferred_channel: row.get(8),
            contact_frequency_days: row.get(9),
            last_contacted_at: row.get(10),
            created_at: row.get(11),
        }))
    }

    /// List all user contacts for an account within a Notion workspace.
    pub fn list_user_contacts(
        &self,
        account_id: Uuid,
        notion_workspace_id: Option<&str>,
    ) -> Result<Vec<UserContact>, AccountStoreError> {
        let mut conn = self.conn()?;

        let rows = if let Some(ws_id) = notion_workspace_id {
            conn.query(
                "SELECT id, account_id, notion_user_id, notion_workspace_id,
                        slack_user_id, slack_workspace_id, discord_user_id, discord_guild_id,
                        preferred_channel, contact_frequency_days, last_contacted_at, created_at
                 FROM user_contact_directory
                 WHERE account_id = $1 AND notion_workspace_id = $2
                 ORDER BY created_at DESC",
                &[&account_id, &ws_id],
            )?
        } else {
            conn.query(
                "SELECT id, account_id, notion_user_id, notion_workspace_id,
                        slack_user_id, slack_workspace_id, discord_user_id, discord_guild_id,
                        preferred_channel, contact_frequency_days, last_contacted_at, created_at
                 FROM user_contact_directory
                 WHERE account_id = $1
                 ORDER BY created_at DESC",
                &[&account_id],
            )?
        };

        Ok(rows
            .iter()
            .map(|row| UserContact {
                id: row.get(0),
                account_id: row.get(1),
                notion_user_id: row.get(2),
                notion_workspace_id: row.get(3),
                slack_user_id: row.get(4),
                slack_workspace_id: row.get(5),
                discord_user_id: row.get(6),
                discord_guild_id: row.get(7),
                preferred_channel: row.get(8),
                contact_frequency_days: row.get(9),
                last_contacted_at: row.get(10),
                created_at: row.get(11),
            })
            .collect())
    }

    /// Update the last_contacted_at timestamp for a user contact.
    ///
    /// Called by TPM after sending a follow-up message.
    pub fn update_user_contact_last_contacted(
        &self,
        contact_id: Uuid,
    ) -> Result<(), AccountStoreError> {
        let mut conn = self.conn()?;
        conn.execute(
            "UPDATE user_contact_directory SET last_contacted_at = NOW() WHERE id = $1",
            &[&contact_id],
        )?;
        Ok(())
    }

    /// Delete a user contact entry.
    pub fn delete_user_contact(&self, contact_id: Uuid) -> Result<(), AccountStoreError> {
        let mut conn = self.conn()?;
        conn.execute(
            "DELETE FROM user_contact_directory WHERE id = $1",
            &[&contact_id],
        )?;
        Ok(())
    }

    // =========================================================================
    // Organization methods (TPM Multi-Tenant)
    // =========================================================================

    /// Get an organization by name.
    ///
    /// Used by TPM to look up organization settings (e.g., Notion database ID).
    pub fn get_organization_by_name(
        &self,
        name: &str,
    ) -> Result<Option<Organization>, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_opt(
            "SELECT id, name, notion_database_id, notion_workspace_id, leader_account_id, discord_guild_id, slack_team_id, github_org_name, created_at
             FROM organizations
             WHERE name = $1",
            &[&name],
        )?;

        Ok(row.map(|r| Organization {
            id: r.get(0),
            name: r.get(1),
            notion_database_id: r.get(2),
            notion_workspace_id: r.get(3),
            leader_account_id: r.get(4),
            discord_guild_id: r.get(5),
            slack_team_id: r.get(6),
            github_org_name: r.get(7),
            created_at: r.get(8),
        }))
    }

    /// Get an organization by ID.
    pub fn get_organization_by_id(
        &self,
        org_id: Uuid,
    ) -> Result<Option<Organization>, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_opt(
            "SELECT id, name, notion_database_id, notion_workspace_id, leader_account_id, discord_guild_id, slack_team_id, github_org_name, created_at
             FROM organizations
             WHERE id = $1",
            &[&org_id],
        )?;

        Ok(row.map(|r| Organization {
            id: r.get(0),
            name: r.get(1),
            notion_database_id: r.get(2),
            notion_workspace_id: r.get(3),
            leader_account_id: r.get(4),
            discord_guild_id: r.get(5),
            slack_team_id: r.get(6),
            github_org_name: r.get(7),
            created_at: r.get(8),
        }))
    }

    /// Update the Notion database ID and workspace ID for an organization.
    ///
    /// Called after `tpm_cli setup-board` creates a new Notion task board.
    pub fn update_organization_notion_database_id(
        &self,
        organization_name: &str,
        notion_database_id: &str,
    ) -> Result<Organization, AccountStoreError> {
        self.update_organization_notion_config(organization_name, notion_database_id, None)
    }

    /// Update the Notion database ID and optionally the workspace ID for an organization.
    ///
    /// Called after `tpm_cli setup-board` creates a new Notion task board,
    /// or via PUT /auth/organization/:name/database endpoint.
    pub fn update_organization_notion_config(
        &self,
        organization_name: &str,
        notion_database_id: &str,
        notion_workspace_id: Option<&str>,
    ) -> Result<Organization, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = match notion_workspace_id {
            Some(ws_id) => conn.query_opt(
                "UPDATE organizations
                 SET notion_database_id = $1, notion_workspace_id = $2
                 WHERE name = $3
                 RETURNING id, name, notion_database_id, notion_workspace_id, leader_account_id, discord_guild_id, slack_team_id, github_org_name, created_at",
                &[&notion_database_id, &ws_id, &organization_name],
            )?,
            None => conn.query_opt(
                "UPDATE organizations
                 SET notion_database_id = $1
                 WHERE name = $2
                 RETURNING id, name, notion_database_id, notion_workspace_id, leader_account_id, discord_guild_id, slack_team_id, github_org_name, created_at",
                &[&notion_database_id, &organization_name],
            )?,
        };

        match row {
            Some(r) => Ok(Organization {
                id: r.get(0),
                name: r.get(1),
                notion_database_id: r.get(2),
                notion_workspace_id: r.get(3),
                leader_account_id: r.get(4),
                discord_guild_id: r.get(5),
                slack_team_id: r.get(6),
                github_org_name: r.get(7),
                created_at: r.get(8),
            }),
            None => Err(AccountStoreError::NotFound),
        }
    }

    /// Set the leader account for an organization.
    ///
    /// The leader's Notion credentials are used for all TPM operations in the org.
    /// Typically set when the first member connects Notion OAuth.
    pub fn set_organization_leader(
        &self,
        organization_name: &str,
        leader_account_id: Uuid,
    ) -> Result<Organization, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_opt(
            "UPDATE organizations
             SET leader_account_id = $1
             WHERE name = $2
             RETURNING id, name, notion_database_id, notion_workspace_id, leader_account_id, discord_guild_id, slack_team_id, github_org_name, created_at",
            &[&leader_account_id, &organization_name],
        )?;

        match row {
            Some(r) => Ok(Organization {
                id: r.get(0),
                name: r.get(1),
                notion_database_id: r.get(2),
                notion_workspace_id: r.get(3),
                leader_account_id: r.get(4),
                discord_guild_id: r.get(5),
                slack_team_id: r.get(6),
                github_org_name: r.get(7),
                created_at: r.get(8),
            }),
            None => Err(AccountStoreError::NotFound),
        }
    }

    /// Set the Discord guild (server) ID for an organization.
    ///
    /// Used for TPM bug scanning in Discord channels.
    pub fn update_organization_discord_guild(
        &self,
        organization_name: &str,
        discord_guild_id: &str,
    ) -> Result<Organization, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_opt(
            "UPDATE organizations
             SET discord_guild_id = $1
             WHERE name = $2
             RETURNING id, name, notion_database_id, notion_workspace_id, leader_account_id, discord_guild_id, slack_team_id, github_org_name, created_at",
            &[&discord_guild_id, &organization_name],
        )?;

        match row {
            Some(r) => Ok(Organization {
                id: r.get(0),
                name: r.get(1),
                notion_database_id: r.get(2),
                notion_workspace_id: r.get(3),
                leader_account_id: r.get(4),
                discord_guild_id: r.get(5),
                slack_team_id: r.get(6),
                github_org_name: r.get(7),
                created_at: r.get(8),
            }),
            None => Err(AccountStoreError::NotFound),
        }
    }

    /// Set the Slack team (workspace) ID for an organization.
    ///
    /// Used for TPM notifications in Slack channels.
    pub fn update_organization_slack_team(
        &self,
        organization_name: &str,
        slack_team_id: &str,
    ) -> Result<Organization, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_opt(
            "UPDATE organizations
             SET slack_team_id = $1
             WHERE name = $2
             RETURNING id, name, notion_database_id, notion_workspace_id, leader_account_id, discord_guild_id, slack_team_id, github_org_name, created_at",
            &[&slack_team_id, &organization_name],
        )?;

        match row {
            Some(r) => Ok(Organization {
                id: r.get(0),
                name: r.get(1),
                notion_database_id: r.get(2),
                notion_workspace_id: r.get(3),
                leader_account_id: r.get(4),
                discord_guild_id: r.get(5),
                slack_team_id: r.get(6),
                github_org_name: r.get(7),
                created_at: r.get(8),
            }),
            None => Err(AccountStoreError::NotFound),
        }
    }

    /// Set the GitHub organization name for an organization.
    ///
    /// Used to scope GitHub repo searches and prevent cross-user leakage.
    pub fn update_organization_github(
        &self,
        organization_name: &str,
        github_org_name: &str,
    ) -> Result<Organization, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_opt(
            "UPDATE organizations
             SET github_org_name = $1
             WHERE name = $2
             RETURNING id, name, notion_database_id, notion_workspace_id, leader_account_id, discord_guild_id, slack_team_id, github_org_name, created_at",
            &[&github_org_name, &organization_name],
        )?;

        match row {
            Some(r) => Ok(Organization {
                id: r.get(0),
                name: r.get(1),
                notion_database_id: r.get(2),
                notion_workspace_id: r.get(3),
                leader_account_id: r.get(4),
                discord_guild_id: r.get(5),
                slack_team_id: r.get(6),
                github_org_name: r.get(7),
                created_at: r.get(8),
            }),
            None => Err(AccountStoreError::NotFound),
        }
    }

    /// Create a new organization.
    ///
    /// Returns error if an organization with the same name already exists.
    pub fn create_organization(&self, name: &str) -> Result<Organization, AccountStoreError> {
        let mut conn = self.conn()?;

        // Check if organization already exists
        let existing = conn.query_opt("SELECT id FROM organizations WHERE name = $1", &[&name])?;

        if existing.is_some() {
            return Err(AccountStoreError::AlreadyExists(format!(
                "Organization '{}' already exists",
                name
            )));
        }

        let row = conn.query_one(
            "INSERT INTO organizations (name) VALUES ($1)
             RETURNING id, name, notion_database_id, notion_workspace_id, leader_account_id, discord_guild_id, slack_team_id, github_org_name, created_at",
            &[&name],
        )?;

        Ok(Organization {
            id: row.get(0),
            name: row.get(1),
            notion_database_id: row.get(2),
            notion_workspace_id: row.get(3),
            leader_account_id: row.get(4),
            discord_guild_id: row.get(5),
            slack_team_id: row.get(6),
            github_org_name: row.get(7),
            created_at: row.get(8),
        })
    }

    /// List organizations, optionally filtering by search term.
    ///
    /// Search is case-insensitive and matches partial names.
    pub fn list_organizations(
        &self,
        search: Option<&str>,
    ) -> Result<Vec<Organization>, AccountStoreError> {
        let mut conn = self.conn()?;

        let rows = match search {
            Some(term) => {
                let pattern = format!("%{}%", term.to_lowercase());
                conn.query(
                    "SELECT id, name, notion_database_id, notion_workspace_id, leader_account_id, discord_guild_id, slack_team_id, github_org_name, created_at
                     FROM organizations
                     WHERE LOWER(name) LIKE $1
                     ORDER BY name
                     LIMIT 50",
                    &[&pattern],
                )?
            }
            None => conn.query(
                "SELECT id, name, notion_database_id, notion_workspace_id, leader_account_id, discord_guild_id, slack_team_id, github_org_name, created_at
                 FROM organizations
                 ORDER BY name
                 LIMIT 50",
                &[],
            )?,
        };

        Ok(rows
            .iter()
            .map(|r| Organization {
                id: r.get(0),
                name: r.get(1),
                notion_database_id: r.get(2),
                notion_workspace_id: r.get(3),
                leader_account_id: r.get(4),
                discord_guild_id: r.get(5),
                slack_team_id: r.get(6),
                github_org_name: r.get(7),
                created_at: r.get(8),
            })
            .collect())
    }

    /// Set an account's organization by organization name.
    /// If the org has no leader, the joining account becomes leader and is auto-accepted.
    pub fn set_account_organization(
        &self,
        account_id: Uuid,
        organization_name: &str,
    ) -> Result<Account, AccountStoreError> {
        let mut conn = self.conn()?;

        // Look up organization by name
        let org_row = conn.query_opt(
            "SELECT id, leader_account_id FROM organizations WHERE name = $1",
            &[&organization_name],
        )?;

        let (org_id, leader_account_id): (Uuid, Option<Uuid>) = match org_row {
            Some(r) => (r.get(0), r.get(1)),
            None => return Err(AccountStoreError::NotFound),
        };

        let is_first_member = leader_account_id.is_none();

        // If no leader, set this account as leader
        if is_first_member {
            conn.execute(
                "UPDATE organizations SET leader_account_id = $1 WHERE id = $2",
                &[&account_id, &org_id],
            )?;
        }

        // Update account's organization_id - auto-accept if first member, pending otherwise
        let status = if is_first_member {
            "accepted"
        } else {
            "pending"
        };
        let row = conn.query_opt(
            "UPDATE accounts
             SET organization_id = $1, organization_accept_status = $2
             WHERE id = $3
             RETURNING id, auth_user_id, created_at, tokens_to_hours::float8, purchased_hours::float8, organization_id, organization_accept_status",
            &[&org_id, &status, &account_id],
        )?;

        match row {
            Some(r) => Ok(Account {
                id: r.get(0),
                auth_user_id: r.get(1),
                created_at: r.get(2),
                tokens_to_hours: r.get(3),
                purchased_hours: r.get(4),
                organization_id: r.get(5),
                organization_accept_status: r.get(6),
            }),
            None => Err(AccountStoreError::NotFound),
        }
    }

    /// Remove an account from its organization.
    pub fn clear_account_organization(
        &self,
        account_id: Uuid,
    ) -> Result<Account, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_opt(
            "UPDATE accounts
             SET organization_id = NULL, organization_accept_status = NULL
             WHERE id = $1
             RETURNING id, auth_user_id, created_at, tokens_to_hours::float8, purchased_hours::float8, organization_id, organization_accept_status",
            &[&account_id],
        )?;

        match row {
            Some(r) => Ok(Account {
                id: r.get(0),
                auth_user_id: r.get(1),
                created_at: r.get(2),
                tokens_to_hours: r.get(3),
                purchased_hours: r.get(4),
                organization_id: r.get(5),
                organization_accept_status: r.get(6),
            }),
            None => Err(AccountStoreError::NotFound),
        }
    }

    pub fn accept_organization_member(
        &self,
        account_id: Uuid,
    ) -> Result<Account, AccountStoreError> {
        let mut conn = self.conn()?;
        let row = conn.query_opt(
            "UPDATE accounts
             SET organization_accept_status = 'accepted'
             WHERE id = $1 AND organization_id IS NOT NULL
             RETURNING id, auth_user_id, created_at, tokens_to_hours::float8, purchased_hours::float8, organization_id, organization_accept_status",
            &[&account_id],
        )?;

        match row {
            Some(r) => Ok(Account {
                id: r.get(0),
                auth_user_id: r.get(1),
                created_at: r.get(2),
                tokens_to_hours: r.get(3),
                purchased_hours: r.get(4),
                organization_id: r.get(5),
                organization_accept_status: r.get(6),
            }),
            None => Err(AccountStoreError::NotFound),
        }
    }

    /// Get the number of members in an organization by name.
    pub fn get_organization_member_count(
        &self,
        organization_name: &str,
    ) -> Result<i64, AccountStoreError> {
        // Use existing method to look up organization
        let org = self
            .get_organization_by_name(organization_name)?
            .ok_or(AccountStoreError::NotFound)?;

        // Count accounts with this organization_id
        let mut conn = self.conn()?;
        let count_row = conn.query_one(
            "SELECT COUNT(*) FROM accounts WHERE organization_id = $1",
            &[&org.id],
        )?;

        Ok(count_row.get(0))
    }

    /// List all accounts in an organization by organization ID.
    pub fn list_accounts_by_organization_id(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<Account>, AccountStoreError> {
        let mut conn = self.conn()?;
        let rows = conn.query(
            "SELECT id, auth_user_id, created_at, tokens_to_hours::float8, purchased_hours::float8, organization_id, organization_accept_status
             FROM accounts WHERE organization_id = $1",
            &[&organization_id],
        )?;

        Ok(rows
            .iter()
            .map(|r| Account {
                id: r.get(0),
                auth_user_id: r.get(1),
                created_at: r.get(2),
                tokens_to_hours: r.get(3),
                purchased_hours: r.get(4),
                organization_id: r.get(5),
                organization_accept_status: r.get(6),
            })
            .collect())
    }

    /// List all org members with their name/email from auth.users.
    /// Joins accounts with auth.users to get user info, and account_identifiers for channel IDs.
    pub fn list_org_members_with_info(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<OrgMember>, AccountStoreError> {
        let mut conn = self.conn()?;
        let rows = conn.query(
            "SELECT a.id, u.email, u.raw_user_meta_data->>'full_name' as name,
                    (SELECT ai.identifier FROM account_identifiers ai
                     WHERE ai.account_id = a.id AND ai.identifier_type = 'notion' AND ai.verified = true
                     ORDER BY ai.created_at DESC LIMIT 1) as notion_user_id,
                    (SELECT ai.identifier FROM account_identifiers ai
                     WHERE ai.account_id = a.id AND ai.identifier_type = 'slack' AND ai.verified = true
                     ORDER BY ai.created_at DESC LIMIT 1) as slack_user_id,
                    (SELECT ai.identifier FROM account_identifiers ai
                     WHERE ai.account_id = a.id AND ai.identifier_type = 'discord' AND ai.verified = true
                     ORDER BY ai.created_at DESC LIMIT 1) as discord_user_id
             FROM accounts a
             JOIN auth.users u ON a.auth_user_id = u.id
             WHERE a.organization_id = $1",
            &[&organization_id],
        )?;

        Ok(rows
            .iter()
            .map(|r| OrgMember {
                account_id: r.get(0),
                email: r.get::<_, Option<String>>(1).unwrap_or_default(),
                name: r.get(2),
                notion_user_id: r.get(3),
                slack_user_id: r.get(4),
                discord_user_id: r.get(5),
            })
            .collect())
    }

    /// Create an email verification token (expires in 24 hours)
    pub fn create_email_verification_token(
        &self,
        account_id: Uuid,
        email: &str,
    ) -> Result<EmailVerificationToken, AccountStoreError> {
        let mut conn = self.conn()?;
        let token = Uuid::new_v4().to_string();
        let expires_at = Utc::now() + chrono::Duration::hours(24);

        // Delete any existing tokens for this email
        conn.execute(
            "DELETE FROM email_verification_tokens WHERE email = $1",
            &[&email],
        )?;

        let row = conn.query_one(
            "INSERT INTO email_verification_tokens (token, account_id, email, expires_at, created_at)
             VALUES ($1, $2, $3, $4, NOW())
             RETURNING token, account_id, email, expires_at, created_at",
            &[&token, &account_id, &email, &expires_at],
        )?;

        Ok(EmailVerificationToken {
            token: row.get(0),
            account_id: row.get(1),
            email: row.get(2),
            expires_at: row.get(3),
            created_at: row.get(4),
        })
    }

    /// Verify an email token and link the email to the account
    pub fn verify_email_token(&self, token: &str) -> Result<AccountIdentifier, AccountStoreError> {
        let normalized_token = token.trim();
        if normalized_token.is_empty() {
            return Err(AccountStoreError::TokenInvalid);
        }
        if Uuid::parse_str(normalized_token).is_err() {
            return Err(AccountStoreError::TokenInvalid);
        }
        let mut conn = self.conn()?;

        // Look up the token
        let row = conn.query_opt(
            "SELECT token, account_id, email, expires_at
             FROM email_verification_tokens
             WHERE token::text = $1",
            &[&normalized_token],
        )?;

        let verification = match row {
            Some(r) => EmailVerificationToken {
                token: r.get(0),
                account_id: r.get(1),
                email: r.get(2),
                expires_at: r.get(3),
                created_at: Utc::now(), // Not needed for verification
            },
            None => return Err(AccountStoreError::TokenInvalid),
        };

        // Check if expired
        if Utc::now() > verification.expires_at {
            // Delete expired token
            conn.execute(
                "DELETE FROM email_verification_tokens WHERE token::text = $1",
                &[&normalized_token],
            )?;
            return Err(AccountStoreError::TokenInvalid);
        }

        // Create or update the identifier as verified
        let id = Uuid::new_v4();
        let row = conn.query_one(
            "INSERT INTO account_identifiers (id, account_id, identifier_type, identifier, verified, created_at)
             VALUES ($1, $2, 'email', $3, true, NOW())
             ON CONFLICT (identifier_type, identifier) DO UPDATE SET account_id = $2, verified = true
             RETURNING id, account_id, identifier_type, identifier, verified, created_at",
            &[&id, &verification.account_id, &verification.email],
        )?;

        // Delete the used token
        conn.execute(
            "DELETE FROM email_verification_tokens WHERE token::text = $1",
            &[&normalized_token],
        )?;

        Ok(AccountIdentifier {
            id: row.get(0),
            account_id: row.get(1),
            identifier_type: row.get(2),
            identifier: row.get(3),
            verified: row.get(4),
            created_at: row.get(5),
        })
    }
}

impl Drop for AccountStore {
    fn drop(&mut self) {
        let primary_pool = self.primary_pool.take();
        let fallback_pool = self.fallback_pool.take();
        if primary_pool.is_some() || fallback_pool.is_some() {
            std::thread::spawn(move || {
                drop(primary_pool);
                drop(fallback_pool);
            });
        }
    }
}

// ============================================================================
// Global AccountStore accessor
// ============================================================================

/// Lazy-initialized global AccountStore
static ACCOUNT_STORE: std::sync::OnceLock<Option<Arc<AccountStore>>> = std::sync::OnceLock::new();

/// Get or initialize the global AccountStore (returns None if not configured)
pub fn get_global_account_store() -> Option<Arc<AccountStore>> {
    ACCOUNT_STORE
        .get_or_init(|| match AccountStore::from_env() {
            Ok(store) => {
                tracing::info!("AccountStore initialized for account lookups");
                Some(Arc::new(store))
            }
            Err(e) => {
                tracing::info!(
                    "AccountStore not available ({}), account lookups disabled",
                    e
                );
                None
            }
        })
        .clone()
}

/// Best-effort check for whether an owner id is a real unified account id.
///
/// Channel-scoped user ids are also UUIDs, so the only reliable distinction we
/// have at runtime is whether the UUID resolves to an account in AccountStore.
/// Fail open on lookup errors so task scheduling does not break when account
/// storage is unavailable.
pub fn is_global_account_id(owner_id: &str) -> bool {
    let Ok(account_id) = Uuid::parse_str(owner_id) else {
        return false;
    };
    let Some(store) = get_global_account_store() else {
        return false;
    };

    match store.get_account(account_id) {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(err) => {
            warn!(
                "failed to classify owner id {} against AccountStore: {}",
                owner_id, err
            );
            false
        }
    }
}

/// Map a channel type to the identifier_type used in account_identifiers table
pub fn channel_to_identifier_type(channel: &crate::channel::Channel) -> &'static str {
    use crate::channel::Channel;
    match channel {
        Channel::Email => "email",
        Channel::Sms => "phone",
        Channel::WhatsApp => "phone",
        Channel::Telegram => "telegram",
        Channel::Slack => "slack",
        Channel::Discord => "discord",
        Channel::BlueBubbles => "phone",
        Channel::WeChat => "wechat",
        Channel::WeChatMp => "wechat_mp",
        Channel::Lark => "lark",
        Channel::GoogleDocs | Channel::GoogleSheets | Channel::GoogleSlides => "email",
        Channel::Notion => "email", // Notion accounts are linked by email
        Channel::Zoom => "zoom",    // Zoom meetings identified by meeting_uuid
    }
}

fn identifier_lookup_candidates(identifier_type: &str, identifier: &str) -> Vec<String> {
    let trimmed = identifier.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut candidates = vec![trimmed.to_string()];
    match identifier_type {
        "email" | "github" => {
            let lower = trimmed.to_ascii_lowercase();
            if lower != trimmed {
                candidates.push(lower);
            }
        }
        "slack" => {
            let upper = trimmed.to_ascii_uppercase();
            if upper != trimmed {
                candidates.push(upper);
            }
        }
        "phone" => {
            if let Some(normalized) = crate::user_store::normalize_phone(trimmed) {
                if normalized != trimmed {
                    candidates.push(normalized);
                }
            }
        }
        _ => {}
    }

    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

/// Look up account by identifier type and identifier.
/// Returns the account_id if found and verified, None otherwise.
pub fn lookup_account_by_identifier(identifier_type: &str, identifier: &str) -> Option<Uuid> {
    let store = get_global_account_store()?;
    let candidates = identifier_lookup_candidates(identifier_type, identifier);
    if candidates.is_empty() {
        return None;
    }

    for candidate in candidates {
        match store.get_account_by_identifier(identifier_type, &candidate) {
            Ok(Some(account)) => {
                tracing::debug!(
                    "Found account {} for {}:{}",
                    account.id,
                    identifier_type,
                    candidate
                );
                return Some(account.id);
            }
            Ok(None) => {
                continue;
            }
            Err(e) => {
                tracing::warn!(
                    "Error looking up account for {}:{}: {}",
                    identifier_type,
                    candidate,
                    e
                );
            }
        }
    }

    tracing::debug!(
        "No account found for {}:{}, using local storage",
        identifier_type,
        identifier
    );
    None
}

/// Look up account by channel and identifier
/// Returns the account_id if found and verified, None otherwise
pub fn lookup_account_by_channel(
    channel: &crate::channel::Channel,
    identifier: &str,
) -> Option<Uuid> {
    use crate::channel::Channel;
    if *channel == Channel::WeChatMp {
        // Backward compatibility: some environments store official-account open_id
        // as `wechat_mp_open_id` instead of `wechat_mp`.
        return lookup_account_by_identifier("wechat_mp", identifier)
            .or_else(|| lookup_account_by_identifier("wechat_mp_open_id", identifier));
    }
    let identifier_type = channel_to_identifier_type(channel);
    lookup_account_by_identifier(identifier_type, identifier)
}

#[cfg(test)]
mod tests {
    use super::{account_store_allow_invalid_certs, AccountStore, AnalyticsEventInsert};
    use chrono::Utc;
    use serde_json::json;
    use std::env;
    use std::sync::{Arc, Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn reset_tls_env() {
        env::remove_var("DEPLOY_TARGET");
        env::remove_var("INGESTION_QUEUE_TLS_ALLOW_INVALID_CERTS");
        env::remove_var("ACCOUNT_STORE_TLS_ALLOW_INVALID_CERTS");
        env::remove_var("SCALE_OLIVER_INGESTION_QUEUE_TLS_ALLOW_INVALID_CERTS");
        env::remove_var("SCALE_OLIVER_ACCOUNT_STORE_TLS_ALLOW_INVALID_CERTS");
    }

    #[test]
    fn account_store_tls_defaults_enabled_on_staging() {
        let _guard = env_lock().lock().expect("env lock");
        reset_tls_env();
        env::set_var("DEPLOY_TARGET", "staging");
        assert!(account_store_allow_invalid_certs());
        reset_tls_env();
    }

    #[test]
    fn account_store_tls_defaults_disabled_on_production() {
        let _guard = env_lock().lock().expect("env lock");
        reset_tls_env();
        env::set_var("DEPLOY_TARGET", "production");
        assert!(!account_store_allow_invalid_certs());
        reset_tls_env();
    }

    #[test]
    fn account_store_tls_respects_explicit_override() {
        let _guard = env_lock().lock().expect("env lock");
        reset_tls_env();
        env::set_var("DEPLOY_TARGET", "staging");
        env::set_var("ACCOUNT_STORE_TLS_ALLOW_INVALID_CERTS", "0");
        assert!(!account_store_allow_invalid_certs());
        env::set_var("ACCOUNT_STORE_TLS_ALLOW_INVALID_CERTS", "1");
        assert!(account_store_allow_invalid_certs());
        reset_tls_env();
    }

    #[test]
    fn detached_analytics_recording_can_be_called_inside_tokio_runtime() {
        let store = Arc::new(AccountStore::detached_for_tests());
        let event = AnalyticsEventInsert {
            event_name: "auth_smoke".to_string(),
            source: "server".to_string(),
            event_timestamp: Utc::now(),
            account_id: None,
            auth_user_id: None,
            anonymous_id: None,
            session_id: None,
            workspace_id: None,
            org_id: None,
            plan_type: None,
            environment: Some("test".to_string()),
            app_version: None,
            page_path: None,
            route_path: Some("/auth/signup".to_string()),
            referrer: None,
            utm_source: None,
            utm_medium: None,
            utm_campaign: None,
            utm_term: None,
            utm_content: None,
            device_type: None,
            browser: None,
            os: None,
            event_key: Some("auth_smoke".to_string()),
            properties: json!({}),
        };

        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async move {
            store.record_analytics_event_detached(event, "test");
            tokio::task::yield_now().await;
        });
    }
}
