//! Storage selected only by the independent Web entry point.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

use crate::{database::SCHEMA_VERSION, AppError};
use rusqlite::{backup::Backup, Connection, OpenFlags};

pub const DATABASE_NAME: &str = "cc-switch-web.db";
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn data_dir() -> Option<&'static Path> {
    DATA_DIR.get().map(PathBuf::as_path)
}

fn database_error(error: impl std::fmt::Display) -> AppError {
    AppError::Database(error.to_string())
}

/// SQLite's backup API includes committed WAL contents without opening the source
/// for writing. Publish only a complete, verified snapshot, never replace a file.
pub fn copy_database(source: &Path, destination: &Path) -> Result<(), AppError> {
    let source_conn = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(database_error)?;
    source_conn
        .busy_timeout(Duration::from_secs(3))
        .map_err(database_error)?;
    let version: i32 = source_conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(database_error)?;
    if !(1..=SCHEMA_VERSION).contains(&version) {
        return Err(AppError::Database(format!(
            "不能导入数据库 v{version}，Web 支持至 v{SCHEMA_VERSION}。原库未修改。"
        )));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| AppError::Config("无效的 Web 数据路径".into()))?;
    fs::create_dir_all(parent).map_err(|error| AppError::io(parent, error))?;
    let temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| AppError::io(parent, error))?;
    let mut destination_conn = Connection::open(temporary.path()).map_err(database_error)?;
    {
        let backup = Backup::new(&source_conn, &mut destination_conn).map_err(database_error)?;
        backup
            .run_to_completion(128, Duration::from_millis(10), None)
            .map_err(database_error)?;
    }
    let integrity: String = destination_conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(database_error)?;
    if integrity != "ok" {
        return Err(AppError::Database(
            "导入副本完整性检查失败，原库未修改".into(),
        ));
    }
    drop(destination_conn);
    temporary
        .persist_noclobber(destination)
        .map_err(|error| AppError::io(destination, error.error))?;
    Ok(())
}

fn copy_resource(source: &Path, destination: &Path) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(source).map_err(|error| AppError::io(source, error))?;
    if metadata.file_type().is_symlink() {
        return Err(AppError::Config(format!(
            "导入资源包含符号链接，请手动迁移：{}",
            source.display()
        )));
    }
    if metadata.is_dir() {
        fs::create_dir_all(destination).map_err(|error| AppError::io(destination, error))?;
        for entry in fs::read_dir(source).map_err(|error| AppError::io(source, error))? {
            let entry = entry.map_err(|error| AppError::io(source, error))?;
            copy_resource(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else if !destination.exists() {
        let bytes = fs::read(source).map_err(|error| AppError::io(source, error))?;
        crate::config::atomic_write_private(destination, &bytes)?;
    }
    Ok(())
}

/// First import only: subsequent launches use the Web copy and never refresh it
/// from desktop. Device recovery state and credentials are deliberately excluded.
pub fn prepare_storage_at(source: &Path, destination: &Path) -> Result<bool, AppError> {
    fs::create_dir_all(destination).map_err(|error| AppError::io(destination, error))?;
    let destination = destination
        .canonicalize()
        .map_err(|error| AppError::io(destination, error))?;
    if source.exists()
        && source
            .canonicalize()
            .map_err(|error| AppError::io(source, error))?
            == destination
    {
        return Err(AppError::Config(
            "Web 数据目录不能与官方数据目录相同".into(),
        ));
    }
    let database = destination.join(DATABASE_NAME);
    if database.exists() {
        let conn = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(database_error)?;
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(database_error)?;
        drop(conn);
        if !(1..=SCHEMA_VERSION).contains(&version) {
            return Err(AppError::Database(format!(
                "Web 数据库版本 v{version} 不兼容，支持至 v{SCHEMA_VERSION}"
            )));
        }
        if version < SCHEMA_VERSION {
            let backup = destination.join("backups").join(format!(
                "before-upgrade-v{version}-{}.db",
                uuid::Uuid::new_v4()
            ));
            copy_database(&database, &backup)?;
        }
        return Ok(false);
    }
    let source_database = source.join("cc-switch.db");
    // Stage the entire first import. Failed imports never leave a usable partial DB.
    let staging =
        tempfile::tempdir_in(&destination).map_err(|error| AppError::io(&destination, error))?;
    if source_database.exists() {
        copy_database(&source_database, &staging.path().join(DATABASE_NAME))?;
        for resource in ["settings.json", "skills", "model-pricing.json"] {
            if source.join(resource).exists() {
                copy_resource(&source.join(resource), &staging.path().join(resource))?;
            }
        }
        let settings_path = staging.path().join("settings.json");
        if settings_path.exists() {
            let bytes =
                fs::read(&settings_path).map_err(|error| AppError::io(&settings_path, error))?;
            let mut settings: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(database_error)?;
            if settings["skillStorageLocation"] == "unified" {
                let unified = crate::config::get_home_dir().join(".agents").join("skills");
                if unified.exists() {
                    copy_resource(&unified, &staging.path().join("skills"))?;
                }
                settings["skillStorageLocation"] = serde_json::json!("cc_switch");
                crate::config::atomic_write_private(
                    &settings_path,
                    &serde_json::to_vec_pretty(&settings).map_err(database_error)?,
                )?;
            }
        }
        // Keep a pristine imported snapshot before migration or runtime cleanup.
        copy_database(
            &staging.path().join(DATABASE_NAME),
            &destination
                .join("backups")
                .join(format!("desktop-import-{}.db", uuid::Uuid::new_v4())),
        )?;
        let conn = Connection::open(staging.path().join(DATABASE_NAME)).map_err(database_error)?;
        // Import configuration, never another process's running proxy takeover.
        let has_proxy: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='proxy_config')", [], |row| row.get(0)).map_err(database_error)?;
        if has_proxy {
            let columns = conn
                .prepare("PRAGMA table_info(proxy_config)")
                .map_err(database_error)?
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(database_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(database_error)?;
            for column in ["enabled", "proxy_enabled", "live_takeover_active"] {
                if columns.iter().any(|existing| existing == column) {
                    conn.execute(&format!("UPDATE proxy_config SET {column}=0"), [])
                        .map_err(database_error)?;
                }
            }
        }
        // Old takeover backups are runtime recovery artifacts, not configuration.
        let has_backups: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='proxy_live_backup')", [], |row| row.get(0)).map_err(database_error)?;
        if has_backups {
            conn.execute("DELETE FROM proxy_live_backup", [])
                .map_err(database_error)?;
        }
        drop(conn);
    }
    for entry in
        fs::read_dir(staging.path()).map_err(|error| AppError::io(staging.path(), error))?
    {
        let entry = entry.map_err(|error| AppError::io(staging.path(), error))?;
        let target = destination.join(entry.file_name());
        if entry.file_name() != DATABASE_NAME {
            copy_resource(&entry.path(), &target)?;
        }
    }
    if source_database.exists() {
        copy_database(&staging.path().join(DATABASE_NAME), &database)?;
    }
    Ok(true)
}

pub fn initialize() -> Result<bool, AppError> {
    let source = crate::config::get_app_config_dir();
    let destination = crate::config::get_home_dir().join(".cc-switch-web");
    let first_start = prepare_storage_at(&source, &destination)?;
    DATA_DIR
        .set(destination)
        .map_err(|_| AppError::Config("Web 数据路径已初始化".into()))?;
    crate::settings::reload_settings()?;
    if first_start {
        crate::mode::state::update(&crate::live::engine::DeviceStore::for_device(), |state| {
            for app in ["claude", "claude-desktop", "codex", "gemini", "grokbuild"] {
                state.apps.entry(app.into()).or_default().set_mode_state(
                    crate::mode::state::ModeState {
                        mode: Some(crate::mode::state::Mode::Direct),
                        ..Default::default()
                    },
                );
            }
        })?;
    }
    Ok(first_start)
}
