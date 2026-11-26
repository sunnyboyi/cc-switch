use cc_switch_lib::{web_storage, Database};
use rusqlite::Connection;

#[test]
fn existing_web_v24_upgrades_with_backup_without_touching_desktop() {
    let home = tempfile::tempdir().unwrap();
    std::env::set_var("CC_SWITCH_TEST_HOME", home.path());
    drop(Database::init().unwrap());
    let desktop = home.path().join(".cc-switch/cc-switch.db");
    let desktop_before = std::fs::read(&desktop).unwrap();
    let web_dir = home.path().join(".cc-switch-web");
    std::fs::create_dir_all(&web_dir).unwrap();
    let web = web_dir.join(web_storage::DATABASE_NAME);
    web_storage::copy_database(&desktop, &web).unwrap();
    let conn = Connection::open(&web).unwrap();
    // Reproduce the schema shipped by the previous Web build, including its
    // pre-release v24 without session_log_sync.parser_state.
    conn.execute_batch("PRAGMA user_version=24; ALTER TABLE session_log_sync DROP COLUMN parser_state; ALTER TABLE skills DROP COLUMN repo_host; ALTER TABLE skill_repos DROP COLUMN host; CREATE TABLE legacy_fixture(value TEXT); INSERT INTO legacy_fixture VALUES ('keep');").unwrap();
    for table in ["skills", "mcp_servers"] {
        for column in ["enabled_copilot", "enabled_workbuddy", "enabled_codebuddy"] {
            conn.execute_batch(&format!("ALTER TABLE {table} DROP COLUMN {column}"))
                .unwrap();
        }
    }
    drop(conn);
    assert!(!web_storage::initialize().unwrap());
    drop(Database::init().unwrap());
    let conn = Connection::open(&web).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
            .unwrap(),
        28
    );
    assert_eq!(
        conn.query_row("SELECT value FROM legacy_fixture", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    conn.prepare("SELECT parser_state FROM session_log_sync")
        .unwrap();
    conn.prepare(
        "SELECT enabled_copilot, enabled_workbuddy, enabled_codebuddy, repo_host FROM skills",
    )
    .unwrap();
    let backup = std::fs::read_dir(web_dir.join("backups"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("before-upgrade-v24-")
        })
        .expect("backup before migration");
    let backup = Connection::open(backup).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
            .unwrap(),
        24
    );
    assert_eq!(std::fs::read(desktop).unwrap(), desktop_before);
}
