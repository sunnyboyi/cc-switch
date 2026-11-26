#[cfg(feature = "desktop")]
use crate::app_config::AppType;
use crate::error::AppError;
#[cfg(feature = "desktop")]
use tauri::State;

use crate::services::omo::{OmoLocalFileData, SLIM, STANDARD};
use crate::services::OmoService;
#[cfg(feature = "desktop")]
use crate::store::AppState;

#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn read_omo_local_file() -> Result<OmoLocalFileData, AppError> {
    OmoService::read_local_file(&STANDARD)
}

#[cfg_attr(feature = "desktop", tauri::command)]
#[cfg(feature = "desktop")]
pub async fn get_current_omo_provider_id(state: State<'_, AppState>) -> Result<String, String> {
    let provider = state
        .db
        .get_current_omo_provider(&AppType::OpenCode, "omo")?;
    Ok(provider.map(|p| p.id).unwrap_or_default())
}

#[cfg_attr(feature = "desktop", tauri::command)]
#[cfg(feature = "desktop")]
pub async fn disable_current_omo(state: State<'_, AppState>) -> Result<(), String> {
    state
        .db
        .clear_omo_category_current(&AppType::OpenCode, "omo")?;
    OmoService::delete_config_file(&STANDARD)?;
    Ok(())
}

// ── OMO Slim commands ───────────────────────────────────────

#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn read_omo_slim_local_file() -> Result<OmoLocalFileData, AppError> {
    OmoService::read_local_file(&SLIM)
}

#[cfg_attr(feature = "desktop", tauri::command)]
#[cfg(feature = "desktop")]
pub async fn get_current_omo_slim_provider_id(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let provider = state
        .db
        .get_current_omo_provider(&AppType::OpenCode, "omo-slim")?;
    Ok(provider.map(|p| p.id).unwrap_or_default())
}

#[cfg_attr(feature = "desktop", tauri::command)]
#[cfg(feature = "desktop")]
pub async fn disable_current_omo_slim(state: State<'_, AppState>) -> Result<(), String> {
    state
        .db
        .clear_omo_category_current(&AppType::OpenCode, "omo-slim")?;
    OmoService::delete_config_file(&SLIM)?;
    Ok(())
}
