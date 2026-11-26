#[cfg(feature = "desktop")]
mod agy_account;
#[cfg(any(feature = "desktop", test))]
mod auth;
#[cfg(feature = "desktop")]
mod balance;
#[cfg(feature = "desktop")]
mod claude_account;
#[cfg(feature = "desktop")]
mod codebuddy_account;
#[cfg(feature = "desktop")]
mod codex_oauth;
#[cfg(feature = "desktop")]
mod coding_plan;
mod config;
#[cfg(feature = "desktop")]
mod copilot;
#[cfg(feature = "desktop")]
mod copilot_vscode;
#[cfg(feature = "desktop")]
mod custom_icon;
#[cfg(feature = "desktop")]
mod deeplink;
#[cfg(feature = "desktop")]
mod dsh;
mod env;
mod failover;
mod global_proxy;
mod hermes;
mod import_export;
#[cfg(feature = "desktop")]
mod kimi_oauth;
mod mcp;
mod misc;
mod model_fetch;
mod omo;
#[cfg(feature = "desktop")]
mod omp;
mod openclaw;
mod pi;
#[cfg(feature = "desktop")]
mod plugin;
#[cfg(feature = "desktop")]
mod plugin_manager;
mod profile;
mod prompt;
mod provider;
mod proxy;
#[cfg(feature = "desktop")]
mod session_manager;
mod settings;
pub mod skill;
#[cfg(feature = "desktop")]
mod stream_check;
#[cfg(feature = "desktop")]
mod subscription;
mod sync_support;
#[cfg(feature = "desktop")]
mod terminal_launch;
#[cfg(feature = "desktop")]
mod xai_oauth;

#[cfg(feature = "desktop")]
mod s3_sync;
mod usage;
#[cfg(feature = "desktop")]
mod webdav_sync;
#[cfg(feature = "desktop")]
mod workbuddy_account;
#[cfg(feature = "desktop")]
mod workspace;

#[cfg(all(test, feature = "desktop"))]
mod provider_concurrency_tests;
#[cfg(all(test, feature = "desktop"))]
mod startup_tests;

#[cfg(feature = "desktop")]
pub use agy_account::*;
#[cfg(feature = "desktop")]
pub use auth::*;
#[cfg(all(test, not(feature = "desktop")))]
pub(crate) use auth::*;
#[cfg(feature = "desktop")]
pub use balance::*;
#[cfg(feature = "desktop")]
pub use claude_account::*;
#[cfg(feature = "desktop")]
pub use codebuddy_account::*;
#[cfg(feature = "desktop")]
pub use codex_oauth::*;
#[cfg(feature = "desktop")]
pub use coding_plan::*;
pub use config::*;
#[cfg(feature = "desktop")]
pub use copilot::*;
#[cfg(feature = "desktop")]
pub use copilot_vscode::*;
#[cfg(feature = "desktop")]
pub use custom_icon::*;
#[cfg(feature = "desktop")]
pub use deeplink::*;
#[cfg(feature = "desktop")]
pub use dsh::*;
pub use env::*;
#[cfg(feature = "desktop")]
pub use failover::*;
#[cfg(not(feature = "desktop"))]
pub(crate) use failover::*;
pub use global_proxy::*;
pub use hermes::*;
#[cfg(feature = "desktop")]
pub use import_export::*;
#[cfg(not(feature = "desktop"))]
pub(crate) use import_export::*;
#[cfg(feature = "desktop")]
pub use kimi_oauth::*;
pub use mcp::*;
pub use misc::*;
pub use model_fetch::*;
pub use omo::*;
#[cfg(feature = "desktop")]
pub(crate) use omp::*;
pub use openclaw::*;
#[cfg(feature = "desktop")]
pub(crate) use pi::*;
#[cfg(feature = "desktop")]
pub use plugin::*;
#[cfg(feature = "desktop")]
pub use plugin_manager::*;
pub use profile::*;
pub use prompt::*;
pub use provider::*;
pub use proxy::*;
#[cfg(feature = "desktop")]
pub use session_manager::*;
pub use settings::*;
pub use skill::*;
#[cfg(feature = "desktop")]
pub use stream_check::*;
#[cfg(feature = "desktop")]
pub use subscription::*;
#[cfg(feature = "desktop")]
pub use terminal_launch::*;
#[cfg(feature = "desktop")]
pub use xai_oauth::*;

#[cfg(feature = "desktop")]
pub use s3_sync::*;
pub use usage::*;
#[cfg(feature = "desktop")]
pub use webdav_sync::*;
#[cfg(feature = "desktop")]
pub use workbuddy_account::*;
#[cfg(feature = "desktop")]
pub use workspace::*;

/// 同步命令在 Tauri 2 里跑在主线程上：要等数据库连接锁（或等切换锁，切换时要拿库）的
/// 命令放到阻塞线程池里跑，否则会话同步占着连接锁时整个窗口和托盘都跟着卡住。
/// 同 `usage.rs` 的 `run_db_query`，只是闭包拿 `AppState`（服务层大多收它）、错误是 `String`。
#[cfg(feature = "desktop")]
async fn run_db_task<T, E, F>(
    state: &tauri::State<'_, crate::store::AppState>,
    task: F,
) -> Result<T, String>
where
    T: Send + 'static,
    E: Into<String> + Send + 'static,
    F: FnOnce(&crate::store::AppState) -> Result<T, E> + Send + 'static,
{
    let state = state.inner().clone();
    crate::runtime::spawn_blocking(move || task(&state))
        .await
        .map_err(|error| format!("数据库任务失败: {error}"))?
        .map_err(Into::into)
}

#[cfg(not(target_os = "windows"))]
pub(crate) use misc::tool_discovery::{cached_login_shell_path, merge_path_segments};
pub(crate) use misc::tool_process::{
    detected_tool_command, is_safe_tool_command_arg, locate_detected_tool,
    run_detected_tool_command_with_timeout, terminate_child_tree,
};
pub(crate) use misc::tool_version::local_tool_version;
