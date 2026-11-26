use crate::error::AppError;
use std::collections::HashMap;
#[cfg(feature = "desktop")]
use tauri::State;

#[cfg(feature = "desktop")]
use super::run_db_task;
use crate::openclaw_config;
#[cfg(feature = "desktop")]
use crate::store::AppState;

// ============================================================================
// OpenClaw Provider Commands (migrated from provider.rs)
// ============================================================================

/// Import providers from OpenClaw live config to database.
///
/// OpenClaw uses additive mode — users may already have providers
/// configured in openclaw.json.
#[cfg_attr(feature = "desktop", tauri::command)]
#[cfg(feature = "desktop")]
pub async fn import_openclaw_providers_from_live(
    state: State<'_, AppState>,
) -> Result<usize, String> {
    run_db_task(&state, |state| {
        crate::services::provider::import_openclaw_providers_from_live(state)
    })
    .await
}

/// Get provider IDs in the OpenClaw live config.
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn get_openclaw_live_provider_ids() -> Result<Vec<String>, AppError> {
    openclaw_config::get_providers().map(|providers| providers.keys().cloned().collect())
}

/// Get a single OpenClaw provider fragment from live config.
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn get_openclaw_live_provider(
    #[allow(non_snake_case)] providerId: String,
) -> Result<Option<serde_json::Value>, AppError> {
    openclaw_config::get_provider(&providerId)
}

/// Scan openclaw.json for known configuration hazards.
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn scan_openclaw_config_health() -> Result<Vec<openclaw_config::OpenClawHealthWarning>, AppError>
{
    openclaw_config::scan_openclaw_config_health()
}

// ============================================================================
// Agents Configuration Commands
// ============================================================================

/// Get OpenClaw default model config (agents.defaults.model)
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn get_openclaw_default_model(
) -> Result<Option<openclaw_config::OpenClawDefaultModel>, AppError> {
    openclaw_config::get_default_model()
}

/// Set OpenClaw default model config (agents.defaults.model)
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn set_openclaw_default_model(
    model: openclaw_config::OpenClawDefaultModel,
) -> Result<openclaw_config::OpenClawWriteOutcome, AppError> {
    openclaw_config::set_default_model(&model)
}

/// Get OpenClaw model catalog/allowlist (agents.defaults.models)
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn get_openclaw_model_catalog(
) -> Result<Option<HashMap<String, openclaw_config::OpenClawModelCatalogEntry>>, AppError> {
    openclaw_config::get_model_catalog()
}

/// Set OpenClaw model catalog/allowlist (agents.defaults.models)
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn set_openclaw_model_catalog(
    catalog: HashMap<String, openclaw_config::OpenClawModelCatalogEntry>,
) -> Result<openclaw_config::OpenClawWriteOutcome, AppError> {
    openclaw_config::set_model_catalog(&catalog)
}

/// Get full agents.defaults config (all fields)
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn get_openclaw_agents_defaults(
) -> Result<Option<openclaw_config::OpenClawAgentsDefaults>, AppError> {
    openclaw_config::get_agents_defaults()
}

/// Set full agents.defaults config (all fields)
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn set_openclaw_agents_defaults(
    defaults: openclaw_config::OpenClawAgentsDefaults,
) -> Result<openclaw_config::OpenClawWriteOutcome, AppError> {
    openclaw_config::set_agents_defaults(&defaults)
}

// ============================================================================
// Env Configuration Commands
// ============================================================================

/// Get OpenClaw env config (env section of openclaw.json)
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn get_openclaw_env() -> Result<openclaw_config::OpenClawEnvConfig, AppError> {
    openclaw_config::get_env_config()
}

/// Set OpenClaw env config (env section of openclaw.json)
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn set_openclaw_env(
    env: openclaw_config::OpenClawEnvConfig,
) -> Result<openclaw_config::OpenClawWriteOutcome, AppError> {
    openclaw_config::set_env_config(&env)
}

// ============================================================================
// Tools Configuration Commands
// ============================================================================

/// Get OpenClaw tools config (tools section of openclaw.json)
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn get_openclaw_tools() -> Result<openclaw_config::OpenClawToolsConfig, AppError> {
    openclaw_config::get_tools_config()
}

/// Set OpenClaw tools config (tools section of openclaw.json)
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn set_openclaw_tools(
    tools: openclaw_config::OpenClawToolsConfig,
) -> Result<openclaw_config::OpenClawWriteOutcome, AppError> {
    openclaw_config::set_tools_config(&tools)
}
