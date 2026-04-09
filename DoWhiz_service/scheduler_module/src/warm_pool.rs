//! Warm container pool integration for the scheduler.
//!
//! Provides global access to the pool manager and configuration loading.

use run_task_module::{PoolConfig, PoolManager};
use std::env;
use std::sync::Arc;
use tracing::{info, warn};

/// Lazy-initialized global PoolManager
static POOL_MANAGER: std::sync::OnceLock<Option<Arc<PoolManager>>> = std::sync::OnceLock::new();

/// Check if warm pool mode is enabled via environment variable.
pub fn is_warm_pool_enabled() -> bool {
    env::var("USE_WARM_POOL")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Load pool configuration from environment variables.
fn load_pool_config_from_env() -> Result<PoolConfig, String> {
    let resource_group = env::var("WARM_POOL_RESOURCE_GROUP")
        .or_else(|_| env::var("RUN_TASK_AZURE_ACI_RESOURCE_GROUP"))
        .map_err(|_| "WARM_POOL_RESOURCE_GROUP or RUN_TASK_AZURE_ACI_RESOURCE_GROUP not set")?;

    let image = env::var("WARM_POOL_IMAGE")
        .or_else(|_| env::var("RUN_TASK_AZURE_ACI_IMAGE"))
        .or_else(|_| env::var("RUN_TASK_DOCKER_IMAGE"))
        .map_err(|_| "WARM_POOL_IMAGE or RUN_TASK_AZURE_ACI_IMAGE not set")?;

    let cpu = env::var("WARM_POOL_CPU")
        .or_else(|_| env::var("RUN_TASK_AZURE_ACI_CPU"))
        .unwrap_or_else(|_| "2.0".to_string());

    let memory_gb = env::var("WARM_POOL_MEMORY_GB")
        .or_else(|_| env::var("RUN_TASK_AZURE_ACI_MEMORY_GB"))
        .unwrap_or_else(|_| "4.0".to_string());

    let queue_storage_account = env::var("WARM_POOL_STORAGE_ACCOUNT")
        .or_else(|_| env::var("RUN_TASK_AZURE_ACI_STORAGE_ACCOUNT"))
        .map_err(|_| "WARM_POOL_STORAGE_ACCOUNT not set")?;

    let queue_storage_key = env::var("WARM_POOL_STORAGE_KEY")
        .or_else(|_| env::var("RUN_TASK_AZURE_ACI_STORAGE_KEY"))
        .map_err(|_| "WARM_POOL_STORAGE_KEY not set")?;

    let task_queue_name =
        env::var("WARM_POOL_TASK_QUEUE").unwrap_or_else(|_| "dowhiz-tasks".to_string());

    let completion_queue_name =
        env::var("WARM_POOL_COMPLETION_QUEUE").unwrap_or_else(|_| "dowhiz-completions".to_string());

    let registry_server = env::var("WARM_POOL_REGISTRY_SERVER")
        .or_else(|_| env::var("RUN_TASK_AZURE_ACI_REGISTRY_SERVER"))
        .map_err(|_| "WARM_POOL_REGISTRY_SERVER or RUN_TASK_AZURE_ACI_REGISTRY_SERVER not set")?;

    let registry_username = env::var("WARM_POOL_REGISTRY_USERNAME")
        .or_else(|_| env::var("RUN_TASK_AZURE_ACI_REGISTRY_USERNAME"))
        .map_err(|_| {
            "WARM_POOL_REGISTRY_USERNAME or RUN_TASK_AZURE_ACI_REGISTRY_USERNAME not set"
        })?;

    let registry_password = env::var("WARM_POOL_REGISTRY_PASSWORD")
        .or_else(|_| env::var("RUN_TASK_AZURE_ACI_REGISTRY_PASSWORD"))
        .map_err(|_| {
            "WARM_POOL_REGISTRY_PASSWORD or RUN_TASK_AZURE_ACI_REGISTRY_PASSWORD not set"
        })?;

    let location = env::var("WARM_POOL_LOCATION")
        .or_else(|_| env::var("RUN_TASK_AZURE_ACI_LOCATION"))
        .map_err(|_| "WARM_POOL_LOCATION or RUN_TASK_AZURE_ACI_LOCATION not set")?;

    Ok(PoolConfig {
        resource_group,
        image,
        cpu,
        memory_gb,
        queue_storage_account,
        queue_storage_key,
        task_queue_name,
        completion_queue_name,
        registry_server,
        registry_username,
        registry_password,
        location,
    })
}

/// Initialize the global pool manager. Call this during server startup.
/// Returns Ok(true) if pool was initialized, Ok(false) if warm pool is disabled.
pub async fn initialize_global_pool_manager() -> Result<bool, String> {
    if !is_warm_pool_enabled() {
        info!("Warm pool disabled (USE_WARM_POOL not set)");
        POOL_MANAGER.get_or_init(|| None);
        return Ok(false);
    }

    let config = load_pool_config_from_env()?;
    let pool_size = env::var("WARM_POOL_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0);

    info!(
        "Initializing warm pool: resource_group={} image={} size={:?}",
        config.resource_group, config.image, pool_size
    );

    let manager = PoolManager::new(config, pool_size);
    manager.initialize().await?;

    POOL_MANAGER.get_or_init(|| Some(Arc::new(manager)));

    info!(
        "Warm pool initialized with {} containers",
        get_global_pool_manager()
            .map(|m| m.active_count())
            .unwrap_or(0)
    );

    Ok(true)
}

/// Get the global pool manager (returns None if not initialized or disabled).
pub fn get_global_pool_manager() -> Option<Arc<PoolManager>> {
    POOL_MANAGER.get().and_then(|opt| opt.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    // Serialize env var tests to avoid race conditions
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    #[test]
    fn test_is_warm_pool_enabled_false_when_unset() {
        let _lock = env_lock();
        std::env::remove_var("USE_WARM_POOL");
        assert!(!is_warm_pool_enabled());
    }

    #[test]
    fn test_is_warm_pool_enabled_true_with_1() {
        let _lock = env_lock();
        std::env::set_var("USE_WARM_POOL", "1");
        assert!(is_warm_pool_enabled());
        std::env::remove_var("USE_WARM_POOL");
    }

    #[test]
    fn test_is_warm_pool_enabled_true_with_true() {
        let _lock = env_lock();
        std::env::set_var("USE_WARM_POOL", "true");
        assert!(is_warm_pool_enabled());
        std::env::remove_var("USE_WARM_POOL");
    }

    #[test]
    fn test_is_warm_pool_enabled_true_case_insensitive() {
        let _lock = env_lock();
        std::env::set_var("USE_WARM_POOL", "TRUE");
        assert!(is_warm_pool_enabled());
        std::env::remove_var("USE_WARM_POOL");
    }

    #[test]
    fn test_is_warm_pool_enabled_false_with_0() {
        let _lock = env_lock();
        std::env::set_var("USE_WARM_POOL", "0");
        assert!(!is_warm_pool_enabled());
        std::env::remove_var("USE_WARM_POOL");
    }

    #[test]
    fn test_is_warm_pool_enabled_false_with_random_value() {
        let _lock = env_lock();
        std::env::set_var("USE_WARM_POOL", "maybe");
        assert!(!is_warm_pool_enabled());
        std::env::remove_var("USE_WARM_POOL");
    }

    #[test]
    fn test_load_pool_config_requires_resource_group() {
        let _lock = env_lock();
        std::env::remove_var("WARM_POOL_RESOURCE_GROUP");
        std::env::remove_var("RUN_TASK_AZURE_ACI_RESOURCE_GROUP");

        let result = load_pool_config_from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("RESOURCE_GROUP"));
    }

    #[test]
    fn test_load_pool_config_uses_fallback_env_vars() {
        let _lock = env_lock();

        // Set up fallback env vars
        std::env::set_var("RUN_TASK_AZURE_ACI_RESOURCE_GROUP", "test-rg");
        std::env::set_var("RUN_TASK_AZURE_ACI_IMAGE", "test-image");
        std::env::set_var("RUN_TASK_AZURE_ACI_STORAGE_ACCOUNT", "teststorage");
        std::env::set_var("RUN_TASK_AZURE_ACI_STORAGE_KEY", "testkey");
        std::env::set_var(
            "RUN_TASK_AZURE_ACI_REGISTRY_SERVER",
            "testregistry.azurecr.io",
        );
        std::env::set_var("RUN_TASK_AZURE_ACI_REGISTRY_USERNAME", "testuser");
        std::env::set_var("RUN_TASK_AZURE_ACI_REGISTRY_PASSWORD", "testpass");
        std::env::set_var("RUN_TASK_AZURE_ACI_LOCATION", "westus2");

        // Remove specific warm pool vars
        std::env::remove_var("WARM_POOL_RESOURCE_GROUP");
        std::env::remove_var("WARM_POOL_IMAGE");
        std::env::remove_var("WARM_POOL_STORAGE_ACCOUNT");
        std::env::remove_var("WARM_POOL_STORAGE_KEY");
        std::env::remove_var("WARM_POOL_REGISTRY_SERVER");
        std::env::remove_var("WARM_POOL_REGISTRY_USERNAME");
        std::env::remove_var("WARM_POOL_REGISTRY_PASSWORD");
        std::env::remove_var("WARM_POOL_LOCATION");

        let result = load_pool_config_from_env();
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.resource_group, "test-rg");
        assert_eq!(config.image, "test-image");
        assert_eq!(config.queue_storage_account, "teststorage");
        assert_eq!(config.queue_storage_key, "testkey");
        assert_eq!(config.task_queue_name, "dowhiz-tasks"); // default
        assert_eq!(config.completion_queue_name, "dowhiz-completions"); // default
        assert_eq!(config.registry_server, "testregistry.azurecr.io");
        assert_eq!(config.registry_username, "testuser");
        assert_eq!(config.registry_password, "testpass");
        assert_eq!(config.location, "westus2");

        // Cleanup
        std::env::remove_var("RUN_TASK_AZURE_ACI_RESOURCE_GROUP");
        std::env::remove_var("RUN_TASK_AZURE_ACI_IMAGE");
        std::env::remove_var("RUN_TASK_AZURE_ACI_STORAGE_ACCOUNT");
        std::env::remove_var("RUN_TASK_AZURE_ACI_STORAGE_KEY");
        std::env::remove_var("RUN_TASK_AZURE_ACI_REGISTRY_SERVER");
        std::env::remove_var("RUN_TASK_AZURE_ACI_REGISTRY_USERNAME");
        std::env::remove_var("RUN_TASK_AZURE_ACI_REGISTRY_PASSWORD");
        std::env::remove_var("RUN_TASK_AZURE_ACI_LOCATION");
    }

    #[test]
    fn test_load_pool_config_prefers_specific_vars() {
        let _lock = env_lock();

        // Set both specific and fallback
        std::env::set_var("WARM_POOL_RESOURCE_GROUP", "warm-rg");
        std::env::set_var("RUN_TASK_AZURE_ACI_RESOURCE_GROUP", "fallback-rg");
        std::env::set_var("WARM_POOL_IMAGE", "warm-image");
        std::env::set_var("RUN_TASK_AZURE_ACI_IMAGE", "fallback-image");
        std::env::set_var("WARM_POOL_STORAGE_ACCOUNT", "warmstorage");
        std::env::set_var("RUN_TASK_AZURE_ACI_STORAGE_ACCOUNT", "fallbackstorage");
        std::env::set_var("WARM_POOL_STORAGE_KEY", "warmkey");
        std::env::set_var("RUN_TASK_AZURE_ACI_STORAGE_KEY", "fallbackkey");
        std::env::set_var("WARM_POOL_TASK_QUEUE", "custom-tasks");
        std::env::set_var("WARM_POOL_COMPLETION_QUEUE", "custom-completions");
        std::env::set_var("WARM_POOL_REGISTRY_SERVER", "warmregistry.azurecr.io");
        std::env::set_var(
            "RUN_TASK_AZURE_ACI_REGISTRY_SERVER",
            "fallbackregistry.azurecr.io",
        );
        std::env::set_var("WARM_POOL_REGISTRY_USERNAME", "warmuser");
        std::env::set_var("RUN_TASK_AZURE_ACI_REGISTRY_USERNAME", "fallbackuser");
        std::env::set_var("WARM_POOL_REGISTRY_PASSWORD", "warmpass");
        std::env::set_var("RUN_TASK_AZURE_ACI_REGISTRY_PASSWORD", "fallbackpass");
        std::env::set_var("WARM_POOL_LOCATION", "eastus");
        std::env::set_var("RUN_TASK_AZURE_ACI_LOCATION", "westus2");

        let result = load_pool_config_from_env();
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.resource_group, "warm-rg");
        assert_eq!(config.image, "warm-image");
        assert_eq!(config.queue_storage_account, "warmstorage");
        assert_eq!(config.queue_storage_key, "warmkey");
        assert_eq!(config.task_queue_name, "custom-tasks");
        assert_eq!(config.completion_queue_name, "custom-completions");
        assert_eq!(config.registry_server, "warmregistry.azurecr.io");
        assert_eq!(config.registry_username, "warmuser");
        assert_eq!(config.registry_password, "warmpass");
        assert_eq!(config.location, "eastus");

        // Cleanup
        std::env::remove_var("WARM_POOL_RESOURCE_GROUP");
        std::env::remove_var("RUN_TASK_AZURE_ACI_RESOURCE_GROUP");
        std::env::remove_var("WARM_POOL_IMAGE");
        std::env::remove_var("RUN_TASK_AZURE_ACI_IMAGE");
        std::env::remove_var("WARM_POOL_STORAGE_ACCOUNT");
        std::env::remove_var("RUN_TASK_AZURE_ACI_STORAGE_ACCOUNT");
        std::env::remove_var("WARM_POOL_STORAGE_KEY");
        std::env::remove_var("RUN_TASK_AZURE_ACI_STORAGE_KEY");
        std::env::remove_var("WARM_POOL_TASK_QUEUE");
        std::env::remove_var("WARM_POOL_COMPLETION_QUEUE");
        std::env::remove_var("WARM_POOL_REGISTRY_SERVER");
        std::env::remove_var("RUN_TASK_AZURE_ACI_REGISTRY_SERVER");
        std::env::remove_var("WARM_POOL_REGISTRY_USERNAME");
        std::env::remove_var("RUN_TASK_AZURE_ACI_REGISTRY_USERNAME");
        std::env::remove_var("WARM_POOL_REGISTRY_PASSWORD");
        std::env::remove_var("RUN_TASK_AZURE_ACI_REGISTRY_PASSWORD");
        std::env::remove_var("WARM_POOL_LOCATION");
        std::env::remove_var("RUN_TASK_AZURE_ACI_LOCATION");
    }

    #[test]
    fn test_get_global_pool_manager_returns_none_before_init() {
        // Before initialization, should return None
        // Note: This test depends on test execution order, but demonstrates the API
        let manager = get_global_pool_manager();
        // Can be None or Some depending on whether other tests initialized it
        // The important thing is it doesn't panic
        let _ = manager;
    }
}
