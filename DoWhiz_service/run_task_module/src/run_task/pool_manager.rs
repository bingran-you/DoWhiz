//! Warm container pool manager with Azure Queue-based task assignment.
//!
//! Maintains a pool of pre-provisioned ACI containers that poll an Azure Queue
//! for task assignments. This eliminates the 2-4 minute cold start latency per task.
//!
//! Architecture:
//! - Pool manager provisions N containers at startup, each running `warm_worker.sh`
//! - Containers poll the task queue for work
//! - Scheduler pushes tasks to queue with share credentials
//! - Container downloads workspace, runs agent, uploads results, signals completion
//! - Scheduler polls completion queue, then calls replenish()

use std::process::Command;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

const DEFAULT_POOL_SIZE: usize = 5;
const CONTAINER_PREFIX: &str = "dwz-warm-";

/// Configuration for the warm container pool.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Azure resource group for ACI containers
    pub resource_group: String,
    /// Container image to use
    pub image: String,
    /// CPU cores per container
    pub cpu: String,
    /// Memory in GB per container
    pub memory_gb: String,
    /// Azure Storage account for queues
    pub queue_storage_account: String,
    /// Azure Storage account key
    pub queue_storage_key: String,
    /// Queue name for task submissions
    pub task_queue_name: String,
    /// Queue name for completion signals
    pub completion_queue_name: String,
    /// ACR registry server
    pub registry_server: String,
    /// ACR registry username
    pub registry_username: String,
    /// ACR registry password
    pub registry_password: String,
    /// Azure location/region
    pub location: String,
}

/// Manages a pool of warm ACI containers.
pub struct PoolManager {
    config: PoolConfig,
    target_size: usize,
    /// Tokio runtime handle captured during initialization for use in sync contexts
    runtime_handle: OnceLock<tokio::runtime::Handle>,
    /// Mutex to serialize replenish checks and prevent over-provisioning
    replenish_lock: Mutex<()>,
}

impl PoolManager {
    /// Create a new pool manager with the given configuration.
    pub fn new(config: PoolConfig, target_size: Option<usize>) -> Self {
        Self {
            config,
            target_size: target_size.unwrap_or(DEFAULT_POOL_SIZE),
            runtime_handle: OnceLock::new(),
            replenish_lock: Mutex::new(()),
        }
    }

    /// Initialize the pool by provisioning warm containers up to target size.
    /// Checks for existing containers first to avoid creating duplicates.
    pub async fn initialize(&self) -> Result<(), String> {
        // Capture the Tokio runtime handle for later use in sync contexts (replenish)
        let _ = self.runtime_handle.set(tokio::runtime::Handle::current());

        // Ensure queues exist (idempotent)
        ensure_queue_exists(&self.config, &self.config.task_queue_name)?;
        ensure_queue_exists(&self.config, &self.config.completion_queue_name)?;

        // Count existing warm containers
        let existing_count = count_existing_containers(&self.config.resource_group)?;
        let containers_needed = self.target_size.saturating_sub(existing_count);

        eprintln!(
            "[pool_manager] Found {} existing containers, need {} total, provisioning {} new",
            existing_count, self.target_size, containers_needed
        );

        if containers_needed == 0 {
            eprintln!("[pool_manager] Pool already at target size, skipping provisioning");
            return Ok(());
        }

        let mut handles = Vec::new();
        for _ in 0..containers_needed {
            let config = self.config.clone();
            handles.push(tokio::spawn(async move {
                provision_warm_container(&config).await
            }));
        }

        let mut provisioned = 0;
        for handle in handles {
            match handle.await {
                Ok(Ok(name)) => {
                    provisioned += 1;
                    eprintln!("[pool_manager] Provisioned: {}", name);
                }
                Ok(Err(e)) => eprintln!("[pool_manager] Provision failed: {}", e),
                Err(e) => eprintln!("[pool_manager] Task join failed: {}", e),
            }
        }

        eprintln!(
            "[pool_manager] Pool ready with {} containers (target: {})",
            existing_count + provisioned, self.target_size
        );
        Ok(())
    }

    /// Called after a task completes to replenish the pool.
    /// Queries Azure for actual container count to avoid sync issues.
    pub fn replenish(&self) {
        // Lock to serialize replenish calls
        let _guard = match self.replenish_lock.lock() {
            Ok(guard) => guard,
            Err(e) => {
                eprintln!("[pool_manager] Failed to acquire replenish lock: {}", e);
                return;
            }
        };

        // Query Azure for actual container count
        let current = match count_existing_containers(&self.config.resource_group) {
            Ok(count) => count,
            Err(e) => {
                eprintln!("[pool_manager] Failed to count containers: {}", e);
                return;
            }
        };

        eprintln!(
            "[pool_manager] Replenish check: {} containers exist, target is {}",
            current, self.target_size
        );

        if current >= self.target_size {
            eprintln!("[pool_manager] Pool at target size, skipping replenish");
            return;
        }

        let handle = match self.runtime_handle.get() {
            Some(h) => h,
            None => {
                eprintln!("[pool_manager] No runtime handle available, skipping replenish");
                return;
            }
        };

        let config = self.config.clone();

        drop(_guard); // Release lock before spawning async work

        handle.spawn(async move {
            eprintln!("[pool_manager] Replenishing pool...");
            match provision_warm_container(&config).await {
                Ok(name) => {
                    eprintln!("[pool_manager] Replenished with: {}", name);
                }
                Err(e) => {
                    eprintln!("[pool_manager] Replenish failed: {}", e);
                }
            }
        });
    }

    /// Get current number of active containers by querying Azure.
    pub fn active_count(&self) -> usize {
        count_existing_containers(&self.config.resource_group).unwrap_or(0)
    }

    /// Get target pool size.
    pub fn target_size(&self) -> usize {
        self.target_size
    }

    /// Get task queue name.
    pub fn task_queue(&self) -> &str {
        &self.config.task_queue_name
    }

    /// Get completion queue name.
    pub fn completion_queue(&self) -> &str {
        &self.config.completion_queue_name
    }

    /// Get storage account name.
    pub fn storage_account(&self) -> &str {
        &self.config.queue_storage_account
    }

    /// Get storage account key.
    pub fn storage_key(&self) -> &str {
        &self.config.queue_storage_key
    }

    /// Get the pool configuration.
    pub fn config(&self) -> &PoolConfig {
        &self.config
    }
}

/// Provision a single warm container that polls the task queue.
/// Collect environment variables to pass to warm containers.
/// These are needed for Codex and other tools to function.
fn collect_warm_container_env_vars(config: &PoolConfig) -> Vec<String> {
    // Workspace location in the container (must match warm_worker.sh)
    let workspace_dir = "/app/.workspace/task";

    let mut env_vars = vec![
        format!("TASK_QUEUE_NAME={}", config.task_queue_name),
        format!("COMPLETION_QUEUE_NAME={}", config.completion_queue_name),
        format!("QUEUE_STORAGE_ACCOUNT={}", config.queue_storage_account),
        format!("QUEUE_STORAGE_KEY={}", config.queue_storage_key),
        // HOME and CODEX_HOME are required for Codex to find its config
        format!("HOME={}", workspace_dir),
        format!("CODEX_HOME={}/.codex", workspace_dir),
        format!("WORKSPACE_LOCAL_DIR={}", workspace_dir),
    ];

    // API keys and endpoints needed for Codex/LLM calls
    let passthrough_keys = [
        "OPENAI_API_KEY",
        "AZURE_OPENAI_API_KEY",
        "AZURE_OPENAI_API_KEY_BACKUP",
        "AZURE_OPENAI_ENDPOINT",
        "AZURE_OPENAI_ENDPOINT_BACKUP",
        "ANTHROPIC_API_KEY",
        // Payment (GOATX402, GOAT, X402)
        "STRIPE_SECRET_KEY",
        "GOATX402_API_URL",
        "GOATX402_MERCHANT_ID",
        "GOATX402_API_KEY",
        "GOATX402_API_SECRET",
        "GOATX402_WALLET_ADDRESS",
        "GOATX402_AGENT_ID",
        "GOATX402_CHAIN_ID",
        "GOATX402_RPC_URL",
        "GOATX402_EXPLORER_URL",
        "GOATX402_USDC_ADDRESS",
        "GOATX402_USDT_ADDRESS",
        "GOAT_WALLET_ADDRESS",
        "GOAT_AGENT_ID",
        "GOAT_CHAIN_ID",
        "GOAT_RPC_URL",
        "GOAT_EXPLORER_URL",
        "GOAT_USDC_ADDRESS",
        "GOAT_USDT_ADDRESS",
        "X402_API_URL",
        "X402_MERCHANT_ID",
        "X402_API_KEY",
        "X402_API_SECRET",
        // Bright Data
        "BRIGHT_DATA_API_KEY",
        "BRIGHTDATA_API_KEY",
        "BRIGHT_DATA_XIAOHONGSHU_COLLECTOR",
        "BRIGHT_DATA_XIAOHONGSHU_TRIGGER_URL",
        // Google
        "GOOGLE_APPLICATION_CREDENTIALS",
        "GOOGLE_PASSWORD",
        // Browserbase
        "BROWSERBASE_API_KEY",
        "BROWSERBASE_PROJECT_ID",
        "BROWSERBASE_STATE_DIR",
        "BROWSERBASE_ACTIVE_SESSION_PATH",
        // Browser handoff
        "BROWSER_HANDOFF_BASE_URL",
        "BROWSER_HANDOFF_SIGNING_SECRET",
        // Human approval gate
        "HUMAN_APPROVAL_GATE_URL",
        "POSTMARK_SERVER_TOKEN",
        "POSTMARK_API_BASE_URL",
        "HUMAN_APPROVAL_FROM",
        "HUMAN_APPROVAL_REPLY_TO",
        // Lark
        "LARK_APP_ID",
        "LARK_APP_SECRET",
        // Discord
        "DISCORD_BOT_TOKEN",
        "DISCORD_BOT_USER_ID",
        "DISCORD_CLIENT_ID",
        "DISCORD_CLIENT_SECRET",
        "DISCORD_REDIRECT_URI",
        "DISCORD_API_BASE_URL",
        // Slack
        "SLACK_BOT_TOKEN",
        "SLACK_BOT_USER_ID",
        "SLACK_CLIENT_ID",
        "SLACK_CLIENT_SECRET",
        "SLACK_AUTH_REDIRECT_URI",
        "SLACK_REDIRECT_URI",
        "SLACK_SIGNING_SECRET",
        "SLACK_APP_ID",
        "SLACK_API_BASE_URL",
        // WeChat
        "WECHAT_TOKEN",
        "WECHAT_ENCODING_AES_KEY",
        "WECHAT_CORP_ID",
        "WECHAT_AGENT_ID",
        "WECHAT_SECRET",
        // Twilio
        "TWILIO_ACCOUNT_SID",
        "TWILIO_AUTH_TOKEN",
        "TWILIO_API_BASE_URL",
        "TWILIO_WEBHOOK_URL",
        // Notion
        "NOTION_WEBHOOK_SECRET",
        "NOTION_INTEGRATION_ID",
        "NOTION_CLIENT_ID",
        "NOTION_CLIENT_SECRET",
        "NOTION_REDIRECT_URI",
        "NOTION_API_TOKEN",
        // GitHub
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GITHUB_PERSONAL_ACCESS_TOKEN",
        "GITHUB_USERNAME",
        // Google Workspace CLI
        "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_ID",
        "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_CLIENT_SECRET",
        "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_REFRESH_TOKEN",
        "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE_TYPE",
        "GOOGLE_ACCESS_TOKEN",
        // Deploy target
        "DEPLOY_TARGET",
        "EMPLOYEE_ID",
    ];

    for key in passthrough_keys {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                env_vars.push(format!("{}={}", key, value));
            }
        }
    }

    env_vars
}

async fn provision_warm_container(config: &PoolConfig) -> Result<String, String> {
    let container_name = format!("{}{}", CONTAINER_PREFIX, Uuid::new_v4().simple());

    eprintln!("[pool_manager] Provisioning container: {}", container_name);

    let mut env_vars = collect_warm_container_env_vars(config);
    // Pass container name so it can be included in completion message
    env_vars.push(format!("CONTAINER_NAME={}", container_name));
    env_vars.push(format!("RESOURCE_GROUP={}", config.resource_group));

    let output = tokio::task::spawn_blocking({
        let config = config.clone();
        let container_name = container_name.clone();
        move || {
            let mut cmd = Command::new("az");
            cmd.arg("container")
                .arg("create")
                .arg("--resource-group")
                .arg(&config.resource_group)
                .arg("--name")
                .arg(&container_name)
                .arg("--image")
                .arg(&config.image)
                .arg("--cpu")
                .arg(&config.cpu)
                .arg("--memory")
                .arg(&config.memory_gb)
                .arg("--restart-policy")
                .arg("Never")
                .arg("--os-type")
                .arg("Linux")
                .arg("--registry-login-server")
                .arg(&config.registry_server)
                .arg("--registry-username")
                .arg(&config.registry_username)
                .arg("--registry-password")
                .arg(&config.registry_password)
                .arg("--environment-variables");

            for env_var in &env_vars {
                cmd.arg(env_var);
            }

            cmd.arg("--command-line")
                .arg("/bin/bash -lc 'warm_worker.sh'")
                .arg("--location")
                .arg(&config.location)
                .arg("--only-show-errors")
                .output()
        }
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| format!("az command failed: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "az container create failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(container_name)
}

/// Count existing warm containers in the resource group.
fn count_existing_containers(resource_group: &str) -> Result<usize, String> {
    let output = Command::new("az")
        .arg("container")
        .arg("list")
        .arg("--resource-group")
        .arg(resource_group)
        .arg("--query")
        .arg(format!("[?starts_with(name, '{}')].name", CONTAINER_PREFIX))
        .arg("-o")
        .arg("tsv")
        .output()
        .map_err(|e| format!("az command failed: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "az container list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let count = stdout.lines().filter(|l| !l.trim().is_empty()).count();

    Ok(count)
}

/// Ensure an Azure Storage Queue exists (idempotent).
fn ensure_queue_exists(config: &PoolConfig, queue_name: &str) -> Result<(), String> {
    eprintln!("[pool_manager] Ensuring queue exists: {}", queue_name);

    let output = Command::new("az")
        .arg("storage")
        .arg("queue")
        .arg("create")
        .arg("--name")
        .arg(queue_name)
        .arg("--account-name")
        .arg(&config.queue_storage_account)
        .arg("--account-key")
        .arg(&config.queue_storage_key)
        .arg("--output")
        .arg("none")
        .output()
        .map_err(|e| format!("az command failed: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "az storage queue create failed for {}: {}",
            queue_name,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Delete an ACI container.
pub fn delete_container(resource_group: &str, container_name: &str) -> Result<(), String> {
    let output = Command::new("az")
        .arg("container")
        .arg("delete")
        .arg("--resource-group")
        .arg(resource_group)
        .arg("--name")
        .arg(container_name)
        .arg("--yes")
        .output()
        .map_err(|e| format!("az command failed: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "az container delete failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_manager_new() {
        let config = PoolConfig {
            resource_group: "test-rg".to_string(),
            image: "test-image".to_string(),
            cpu: "2.0".to_string(),
            memory_gb: "4.0".to_string(),
            queue_storage_account: "teststorage".to_string(),
            queue_storage_key: "testkey".to_string(),
            task_queue_name: "test-tasks".to_string(),
            completion_queue_name: "test-completions".to_string(),
            registry_server: "testregistry.azurecr.io".to_string(),
            registry_username: "testuser".to_string(),
            registry_password: "testpass".to_string(),
            location: "westus2".to_string(),
        };

        let manager = PoolManager::new(config, Some(5));
        assert_eq!(manager.target_size(), 5);
        assert_eq!(manager.task_queue(), "test-tasks");
        assert_eq!(manager.completion_queue(), "test-completions");
    }

    #[test]
    fn test_default_pool_size() {
        let config = PoolConfig {
            resource_group: "test-rg".to_string(),
            image: "test-image".to_string(),
            cpu: "2.0".to_string(),
            memory_gb: "4.0".to_string(),
            queue_storage_account: "teststorage".to_string(),
            queue_storage_key: "testkey".to_string(),
            task_queue_name: "test-tasks".to_string(),
            completion_queue_name: "test-completions".to_string(),
            registry_server: "testregistry.azurecr.io".to_string(),
            registry_username: "testuser".to_string(),
            registry_password: "testpass".to_string(),
            location: "westus2".to_string(),
        };

        let manager = PoolManager::new(config, None);
        assert_eq!(manager.target_size(), 5); // DEFAULT_POOL_SIZE
    }

    fn test_config() -> PoolConfig {
        PoolConfig {
            resource_group: "test-rg".to_string(),
            image: "test-image".to_string(),
            cpu: "2.0".to_string(),
            memory_gb: "4.0".to_string(),
            queue_storage_account: "teststorage".to_string(),
            queue_storage_key: "testkey".to_string(),
            task_queue_name: "test-tasks".to_string(),
            completion_queue_name: "test-completions".to_string(),
            registry_server: "testregistry.azurecr.io".to_string(),
            registry_username: "testuser".to_string(),
            registry_password: "testpass".to_string(),
            location: "westus2".to_string(),
        }
    }

    #[test]
    fn test_collect_warm_container_env_vars_includes_queue_config() {
        let config = test_config();
        let env_vars = collect_warm_container_env_vars(&config);

        assert!(env_vars.contains(&"TASK_QUEUE_NAME=test-tasks".to_string()));
        assert!(env_vars.contains(&"COMPLETION_QUEUE_NAME=test-completions".to_string()));
        assert!(env_vars.contains(&"QUEUE_STORAGE_ACCOUNT=teststorage".to_string()));
        assert!(env_vars.contains(&"QUEUE_STORAGE_KEY=testkey".to_string()));
    }

    #[test]
    fn test_collect_warm_container_env_vars_includes_home_and_codex_home() {
        let config = test_config();
        let env_vars = collect_warm_container_env_vars(&config);

        assert!(env_vars.contains(&"HOME=/app/.workspace/task".to_string()));
        assert!(env_vars.contains(&"CODEX_HOME=/app/.workspace/task/.codex".to_string()));
        assert!(env_vars.contains(&"WORKSPACE_LOCAL_DIR=/app/.workspace/task".to_string()));
    }

    #[test]
    fn test_collect_warm_container_env_vars_passes_through_api_keys() {
        let config = test_config();

        // Set some env vars
        std::env::set_var("OPENAI_API_KEY", "test-openai-key");
        std::env::set_var("AZURE_OPENAI_ENDPOINT", "https://test.openai.azure.com");

        let env_vars = collect_warm_container_env_vars(&config);

        assert!(env_vars.contains(&"OPENAI_API_KEY=test-openai-key".to_string()));
        assert!(env_vars.contains(&"AZURE_OPENAI_ENDPOINT=https://test.openai.azure.com".to_string()));

        // Cleanup
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("AZURE_OPENAI_ENDPOINT");
    }

    #[test]
    fn test_collect_warm_container_env_vars_skips_empty_values() {
        let config = test_config();

        // Set an empty env var
        std::env::set_var("ANTHROPIC_API_KEY", "");

        let env_vars = collect_warm_container_env_vars(&config);

        // Should not include empty values
        assert!(!env_vars.iter().any(|v| v.starts_with("ANTHROPIC_API_KEY=")));

        // Cleanup
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn test_collect_warm_container_env_vars_skips_whitespace_only_values() {
        let config = test_config();

        // Set a whitespace-only env var
        std::env::set_var("STRIPE_SECRET_KEY", "   ");

        let env_vars = collect_warm_container_env_vars(&config);

        // Should not include whitespace-only values
        assert!(!env_vars.iter().any(|v| v.starts_with("STRIPE_SECRET_KEY=")));

        // Cleanup
        std::env::remove_var("STRIPE_SECRET_KEY");
    }

    #[test]
    fn test_containers_needed_calculation() {
        // Test the saturating_sub logic used in initialize()
        let target_size: usize = 5;

        // No existing containers -> need all 5
        assert_eq!(target_size.saturating_sub(0), 5);

        // 3 existing -> need 2 more
        assert_eq!(target_size.saturating_sub(3), 2);

        // Already at target -> need 0
        assert_eq!(target_size.saturating_sub(5), 0);

        // More than target (shouldn't happen, but handle gracefully) -> need 0
        assert_eq!(target_size.saturating_sub(7), 0);
    }

    #[test]
    fn test_container_prefix_constant() {
        // Ensure prefix is what we expect for az queries
        assert_eq!(CONTAINER_PREFIX, "dwz-warm-");
    }

}
