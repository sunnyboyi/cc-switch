#[cfg(feature = "desktop")]
use crate::app_config::AppType;
#[cfg(feature = "desktop")]
use crate::provider::UsageScript;
#[cfg(feature = "desktop")]
use crate::services::pi_state::{PiCurrentState, PiStateService};
#[cfg(feature = "desktop")]
use crate::services::ProviderService;
use crate::session_manager::providers::pi::PiSessionDiscovery;
#[cfg(feature = "desktop")]
use crate::store::AppState;
#[cfg(feature = "desktop")]
use std::str::FromStr;
#[cfg(feature = "desktop")]
use tauri::State;

#[cfg(feature = "desktop")]
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn get_pi_current_state(state: State<'_, AppState>) -> Result<PiCurrentState, String> {
    PiStateService::current(state.inner()).map_err(|error| error.to_string())
}

#[cfg(feature = "desktop")]
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn update_pi_provider_usage_script(
    state: State<'_, AppState>,
    app: String,
    id: String,
    #[allow(non_snake_case)] usageScript: UsageScript,
) -> Result<bool, String> {
    let app_type = AppType::from_str(&app).map_err(|e| e.to_string())?;
    ProviderService::update_pi_usage_script(state.inner(), app_type, &id, usageScript)
        .map_err(|error| error.to_string())
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn get_pi_session_discovery() -> PiSessionDiscovery {
    crate::session_manager::providers::pi::session_discovery()
}
