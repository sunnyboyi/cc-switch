#![allow(non_snake_case)]

#[cfg(feature = "desktop")]
mod agy_account;
mod auth;
#[cfg(feature = "desktop")]
mod balance;
#[cfg(feature = "desktop")]
mod claude_account;
#[cfg(feature = "desktop")]
mod codex_oauth;
#[cfg(feature = "desktop")]
mod coding_plan;
mod config;
#[cfg(feature = "desktop")]
mod copilot;
#[cfg(feature = "desktop")]
mod custom_icon;
#[cfg(feature = "desktop")]
mod deeplink;
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
mod xai_oauth;

#[cfg(feature = "desktop")]
mod lightweight;
#[cfg(feature = "desktop")]
mod s3_sync;
mod usage;
#[cfg(feature = "desktop")]
mod webdav_sync;
#[cfg(feature = "desktop")]
mod workspace;

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
pub use codex_oauth::*;
#[cfg(feature = "desktop")]
pub use coding_plan::*;
pub use config::*;
#[cfg(feature = "desktop")]
pub use copilot::*;
#[cfg(feature = "desktop")]
pub use custom_icon::*;
#[cfg(feature = "desktop")]
pub use deeplink::*;
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
pub use xai_oauth::*;

#[cfg(feature = "desktop")]
pub use lightweight::*;
#[cfg(feature = "desktop")]
pub use s3_sync::*;
pub use usage::*;
#[cfg(feature = "desktop")]
pub use webdav_sync::*;
#[cfg(feature = "desktop")]
pub use workspace::*;
