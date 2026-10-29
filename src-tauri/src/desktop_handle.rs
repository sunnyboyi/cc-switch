//! Optional desktop capability. A headless build cannot construct a desktop handle.

#[cfg(not(feature = "desktop"))]
#[derive(Clone)]
pub enum DesktopHandle {}
#[cfg(feature = "desktop")]
pub use tauri::AppHandle as DesktopHandle;
