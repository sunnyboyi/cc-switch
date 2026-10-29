use rusqlite::Connection;

#[test]
fn web_import_migrates_only_its_copy_and_keeps_settings_and_backups_separate() {
    use cc_switch_lib::{web_storage, web_support, AppState, Database};
    use serde_json::json;
    use std::{fs, sync::Arc};

    let home = tempfile::tempdir().unwrap();
    std::env::set_var("CC_SWITCH_TEST_HOME", home.path());
    let desktop_dir = home.path().join(".cc-switch");
    drop(Database::init().unwrap());
    let desktop_path = desktop_dir.join("cc-switch.db");
    let desktop = Connection::open(&desktop_path).unwrap();
    desktop.execute_batch("PRAGMA journal_mode=WAL; PRAGMA user_version=23; CREATE TABLE import_fixture(value TEXT); INSERT INTO import_fixture VALUES ('committed-in-wal'); UPDATE proxy_config SET enabled=1, proxy_enabled=1, live_takeover_active=1;").unwrap();
    let settings_bytes = br#"{"language":"zh","showInTray":true,"minimizeToTrayOnClose":true,"sessionAutoSyncEnabled":false}"#;
    fs::write(desktop_dir.join("settings.json"), settings_bytes).unwrap();
    fs::write(
        desktop_dir.join("live-state.json"),
        "desktop runtime state must stay here",
    )
    .unwrap();
    fs::create_dir_all(desktop_dir.join("skills/example")).unwrap();
    fs::write(desktop_dir.join("skills/example/SKILL.md"), "skill content").unwrap();
    let before = fs::read(&desktop_path).unwrap();
    let wal_path = desktop_dir.join("cc-switch.db-wal");
    let wal_before = fs::read(&wal_path).unwrap();

    assert!(web_storage::initialize().unwrap());
    let web_dir = home.path().join(".cc-switch-web");
    let web_path = web_dir.join("cc-switch-web.db");
    assert!(web_path.exists());
    assert!(!web_dir.join("cc-switch.db").exists());
    let web_runtime: serde_json::Value =
        serde_json::from_slice(&fs::read(web_dir.join("live-state.json")).unwrap()).unwrap();
    assert_eq!(web_runtime["apps"]["claude"]["mode"], "direct");
    assert_ne!(
        fs::read(web_dir.join("live-state.json")).unwrap(),
        fs::read(desktop_dir.join("live-state.json")).unwrap()
    );
    assert_eq!(
        fs::read(web_dir.join("skills/example/SKILL.md")).unwrap(),
        b"skill content"
    );
    let state = AppState::new(Arc::new(Database::init().unwrap()));
    let web = Connection::open(&web_path).unwrap();
    assert_eq!(
        web.query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
            .unwrap(),
        24
    );
    assert_eq!(
        web.query_row("SELECT value FROM import_fixture", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "committed-in-wal"
    );
    assert_eq!(
        web.query_row(
            "SELECT SUM(enabled + proxy_enabled + live_takeover_active) FROM proxy_config",
            [],
            |row| row.get::<_, i32>(0)
        )
        .unwrap(),
        0
    );
    let skills = cc_switch_lib::SkillService::new();
    let mut settings = web_support::dispatch(&state, &skills, "get_settings", json!({})).unwrap();
    assert_eq!(settings["visibleApps"]["opencode"], false);
    let mut invalid = settings.clone();
    invalid["claudeConfigDir"] = json!("relative/path");
    let unchanged = fs::read(web_dir.join("settings.json")).unwrap();
    assert!(web_support::dispatch(
        &state,
        &skills,
        "save_settings",
        json!({"settings":invalid})
    )
    .is_err());
    assert_eq!(fs::read(web_dir.join("settings.json")).unwrap(), unchanged);
    settings["language"] = json!("en");
    settings["showInTray"] = json!(false); // Desktop-only fields remain preserved.
    settings["claudeConfigDir"] = json!(home.path().join("custom-claude"));
    web_support::dispatch(
        &state,
        &skills,
        "save_settings",
        json!({"settings":settings}),
    )
    .unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(web_dir.join("settings.json")).unwrap()).unwrap();
    assert_eq!(saved["language"], "en");
    assert_eq!(saved["showInTray"], true);
    state
        .db
        .set_setting("web_guard_marker", "before-backup")
        .unwrap();
    let backup = web_support::dispatch(&state, &skills, "create_db_backup", json!({})).unwrap();
    assert!(web_dir
        .join("backups")
        .join(backup.as_str().unwrap())
        .exists());
    state
        .db
        .set_setting("web_guard_marker", "after-backup")
        .unwrap();
    let renamed = web_support::dispatch(
        &state,
        &skills,
        "rename_db_backup",
        json!({"oldFilename":backup,"newName":"web-isolation"}),
    )
    .unwrap();
    web_support::dispatch(
        &state,
        &skills,
        "restore_db_backup",
        json!({"filename":renamed}),
    )
    .unwrap();
    assert_eq!(
        state.db.get_setting("web_guard_marker").unwrap().as_deref(),
        Some("before-backup")
    );
    web_support::dispatch(
        &state,
        &skills,
        "delete_db_backup",
        json!({"filename":renamed}),
    )
    .unwrap();
    assert!(!web_dir
        .join("backups")
        .join(renamed.as_str().unwrap())
        .exists());
    let saved_bytes = fs::read(web_dir.join("settings.json")).unwrap();
    assert!(!web_storage::prepare_storage_at(&desktop_dir, &web_dir).unwrap());
    assert_eq!(
        fs::read(web_dir.join("settings.json")).unwrap(),
        saved_bytes
    );
    assert_eq!(
        desktop
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
            .unwrap(),
        23
    );
    assert_eq!(fs::read(&desktop_path).unwrap(), before);
    assert_eq!(fs::read(&wal_path).unwrap(), wal_before);
    assert_eq!(
        fs::read(desktop_dir.join("settings.json")).unwrap(),
        settings_bytes
    );
    assert_eq!(
        web.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(web);
    drop(desktop);
    drop(state);

    let future_path = home.path().join("future.db");
    let future = Connection::open(&future_path).unwrap();
    future.execute_batch("PRAGMA user_version=25").unwrap();
    drop(future);
    let rejected = home.path().join("rejected.db");
    assert!(web_storage::copy_database(&future_path, &rejected).is_err());
    assert!(!rejected.exists());
    assert!(web_storage::prepare_storage_at(&desktop_dir, &desktop_dir).is_err());
}

#[test]
fn storage_preflight_checks_versions_without_changing_existing_databases() {
    use cc_switch_lib::web_storage::{prepare_storage_at, DATABASE_NAME};

    let directory = tempfile::tempdir().unwrap();
    for version in [0, 21, 23, 24, 25] {
        let source = directory.path().join(format!("source-v{version}"));
        let destination = directory.path().join(format!("web-v{version}"));
        std::fs::create_dir_all(&source).unwrap();
        let path = source.join("cc-switch.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(&format!("CREATE TABLE fixture(id TEXT); INSERT INTO fixture VALUES ('preserve'); PRAGMA user_version={version};")).unwrap();
        drop(conn);
        let before = std::fs::read(&path).unwrap();
        let result = prepare_storage_at(&source, &destination);
        let supported = (1..=24).contains(&version);
        assert_eq!(result.is_ok(), supported, "v{version}: {result:?}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let web_path = destination.join(DATABASE_NAME);
        assert_eq!(web_path.exists(), supported);

        if !supported {
            std::fs::copy(&path, &web_path).unwrap();
        }
        let web_before = std::fs::read(&web_path).unwrap();
        let result = prepare_storage_at(&source, &destination);
        assert_eq!(result.is_ok(), supported, "Web v{version}: {result:?}");
        assert_eq!(std::fs::read(&web_path).unwrap(), web_before);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}
