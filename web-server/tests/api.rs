use std::sync::Arc;

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use cc_switch_lib::{AppState, Database};
use cc_switch_web::{router, WebState};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

const TOKEN: &str = "test-token-32-characters-long-and-private";

async fn call(
    app: &Router,
    method: Method,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let payload = if let Some(body) = body {
        request = request.header(header::CONTENT_TYPE, "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };
    let response = app
        .clone()
        .oneshot(request.body(payload).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert!(response.headers().contains_key("content-security-policy"));
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn save(name: &str, settings: Value, base: Value, revision: Option<Value>) -> Value {
    json!({"name":name, "notes":"web test", "settingsConfig":settings,
        "expectedRevision":revision, "editorSave":{"base":base,"onConflict":"refuse"}})
}

// One isolated process-wide home: existing device settings caches are global.
// Never allow these integration tests to touch the developer's CLI configuration.
#[tokio::test(flavor = "multi_thread")]
async fn authenticated_provider_lifecycle_preserves_files_and_rejects_conflicts() {
    let home = tempfile::tempdir().unwrap();
    std::env::set_var("CC_SWITCH_TEST_HOME", home.path());
    let assets = tempfile::tempdir().unwrap();
    std::fs::write(assets.path().join("index.html"), "web assets").unwrap();
    assert!(cc_switch_lib::web_storage::initialize().unwrap());
    let app_state = AppState::new(Arc::new(Database::init().unwrap()));
    assert!(WebState::new(app_state.clone(), "short").is_err());
    assert!(WebState::validate_token("valid-length-but-internal\ncontrol-character").is_err());
    cc_switch_lib::mode::operation::recover_on_startup(&app_state.db);
    cc_switch_lib::mode::controller::startup(&app_state).await;
    let app = router(
        WebState::new(app_state.clone(), TOKEN).unwrap(),
        assets.path().into(),
    );
    let root = "/api/apps/claude/providers";

    assert_eq!(
        call(&app, Method::GET, root, None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&app, Method::POST, root, Some("wrong"), Some(json!({})))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &app,
            Method::GET,
            "/api/apps/unknown/providers",
            Some(TOKEN),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (status, empty) = call(&app, Method::GET, root, Some(TOKEN), None).await;
    assert_eq!(status, StatusCode::OK, "{empty}");
    assert_eq!(empty["providers"], json!([]));

    let (_, preview) = call(
        &app,
        Method::GET,
        "/api/apps/claude/editor",
        Some(TOKEN),
        None,
    )
    .await;
    let base = preview["editor"]["settings"].clone();
    let bad = save(" ", json!([]), base.clone(), None);
    assert_eq!(
        call(&app, Method::POST, root, Some(TOKEN), Some(bad))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let settings = json!({"env":{"ANTHROPIC_AUTH_TOKEN":"private-api-key-a","ANTHROPIC_BASE_URL":"https://example.invalid"}});
    let (status, created) = call(
        &app,
        Method::POST,
        root,
        Some(TOKEN),
        Some(save("First", settings.clone(), base, None)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let first_id = created["id"].as_str().unwrap();
    let first_path = format!("{root}/{first_id}");
    let (_, snapshot) = call(&app, Method::GET, &first_path, Some(TOKEN), None).await;
    assert_eq!(snapshot["provider"]["name"], "First");
    assert!(snapshot["editor"]["settings"]
        .to_string()
        .contains("private-api-key-a"));
    let (_, listed) = call(&app, Method::GET, root, Some(TOKEN), None).await;
    assert!(!listed.to_string().contains("private-api-key-a"));
    assert!(!listed.to_string().contains("settingsConfig"));
    assert_eq!(listed["currentId"], first_id);

    let live_path = cc_switch_lib::get_claude_settings_path();
    assert!(
        live_path.starts_with(home.path()),
        "test must stay isolated"
    );
    let before = std::fs::read(&live_path).unwrap();
    // Two browser tabs saving the same version: only one can succeed.
    let body = save(
        "Renamed",
        snapshot["editor"]["settings"].clone(),
        snapshot["editor"]["settings"].clone(),
        Some(snapshot["revision"].clone()),
    );
    let (a, b) = tokio::join!(
        call(
            &app,
            Method::PUT,
            &first_path,
            Some(TOKEN),
            Some(body.clone())
        ),
        call(&app, Method::PUT, &first_path, Some(TOKEN), Some(body)),
    );
    let statuses = [a.0, b.0];
    assert!(statuses.contains(&StatusCode::OK), "{a:?} {b:?}");
    assert!(statuses.contains(&StatusCode::CONFLICT), "{a:?} {b:?}");
    assert_eq!(
        std::fs::read(&live_path).unwrap(),
        before,
        "renaming must preserve client bytes"
    );

    let (_, latest) = call(&app, Method::GET, &first_path, Some(TOKEN), None).await;
    let mut external: Value = serde_json::from_slice(&before).unwrap();
    external["permissions"] = json!({"allow":["external"]});
    std::fs::write(&live_path, serde_json::to_vec_pretty(&external).unwrap()).unwrap();
    let mut draft = latest["editor"]["settings"].clone();
    draft["permissions"] = json!({"allow":["browser"]});
    let (status, conflict) = call(
        &app,
        Method::PUT,
        &first_path,
        Some(TOKEN),
        Some(save(
            "Renamed",
            draft,
            latest["editor"]["settings"].clone(),
            Some(latest["revision"].clone()),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
    let actual: Value = serde_json::from_slice(&std::fs::read(&live_path).unwrap()).unwrap();
    assert_eq!(actual["permissions"]["allow"], json!(["external"]));

    let (_, preview) = call(
        &app,
        Method::GET,
        "/api/apps/claude/editor",
        Some(TOKEN),
        None,
    )
    .await;
    let mut second_settings = preview["editor"]["settings"].clone();
    second_settings["env"] = json!({"ANTHROPIC_AUTH_TOKEN":"private-api-key-b","ANTHROPIC_BASE_URL":"https://second.invalid"});
    let (status, second) = call(
        &app,
        Method::POST,
        root,
        Some(TOKEN),
        Some(save(
            "Second",
            second_settings,
            preview["editor"]["settings"].clone(),
            None,
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    let second_id = second["id"].as_str().unwrap();
    let (status, switched) = call(
        &app,
        Method::POST,
        &format!("{root}/{second_id}/switch"),
        Some(TOKEN),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{switched}");
    let (_, after) = call(&app, Method::GET, root, Some(TOKEN), None).await;
    assert_eq!(after["currentId"], second_id);
    let actual: Value = serde_json::from_slice(&std::fs::read(&live_path).unwrap()).unwrap();
    assert_eq!(actual["env"]["ANTHROPIC_AUTH_TOKEN"], "private-api-key-b");
    assert_eq!(actual["permissions"]["allow"], json!(["external"]));
    assert_eq!(
        call(
            &app,
            Method::POST,
            &format!("{root}/missing/switch"),
            Some(TOKEN),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    // All three adapters must save, read, edit and switch using their native projections.
    for app_id in ["codex", "gemini"] {
        let (status, preview) = call(
            &app,
            Method::GET,
            &format!("/api/apps/{app_id}/editor"),
            Some(TOKEN),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{app_id}: {preview}");
        assert!(preview["editor"]["settings"].is_object());
        let mut settings = preview["editor"]["settings"].clone();
        if app_id == "codex" {
            settings["auth"] = json!({"OPENAI_API_KEY":"codex-test-key"});
            settings["config"] = json!(format!("{}\nmodel_provider = \"custom\"\nmodel = \"test-model\"\n[model_providers.custom]\nname = \"Web test\"\nbase_url = \"https://example.invalid/v1\"\nwire_api = \"responses\"\nrequires_openai_auth = true\n", settings["config"].as_str().unwrap_or("")));
        } else {
            settings["env"] = json!({"GEMINI_API_KEY":"gemini-test-key","GOOGLE_GEMINI_BASE_URL":"https://example.invalid"});
        }
        let collection = format!("/api/apps/{app_id}/providers");
        let mut body = save(
            "Native adapter",
            settings,
            preview["editor"]["settings"].clone(),
            None,
        );
        body["editorSave"]["draft"] = preview["draft"].clone();
        let (status, created) =
            call(&app, Method::POST, &collection, Some(TOKEN), Some(body)).await;
        assert_eq!(status, StatusCode::OK, "{app_id}: {created}");
        let path = format!("{collection}/{}", created["id"].as_str().unwrap());
        let (status, detail) = call(&app, Method::GET, &path, Some(TOKEN), None).await;
        assert_eq!(status, StatusCode::OK, "{app_id}: {detail}");
        let body = save(
            "Native adapter updated",
            detail["editor"]["settings"].clone(),
            detail["editor"]["settings"].clone(),
            Some(detail["revision"].clone()),
        );
        let (status, updated) = call(&app, Method::PUT, &path, Some(TOKEN), Some(body)).await;
        assert_eq!(status, StatusCode::OK, "{app_id}: {updated}");
        let (status, switched) = call(
            &app,
            Method::POST,
            &format!("{path}/switch"),
            Some(TOKEN),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{app_id}: {switched}");
        let (_, listed) = call(&app, Method::GET, &collection, Some(TOKEN), None).await;
        assert_eq!(listed["currentId"], created["id"]);
    }
    assert!(home
        .path()
        .join(".cc-switch-web/cc-switch-web.db")
        .is_file());
    assert!(!home.path().join(".cc-switch/cc-switch.db").exists());
    let reopened = Database::init().unwrap();
    assert_eq!(reopened.get_all_providers("claude").unwrap().len(), 2);
    management_lifecycle(&app, home.path()).await;
    cc_switch_lib::mode::controller::detach_all(&app_state).await;
    if app_state.proxy_service.is_running().await {
        app_state.proxy_service.stop().await.unwrap();
    }
}

async fn rpc(app: &Router, name: &str, args: Value) -> Value {
    let (status, value) = call(
        app,
        Method::POST,
        &format!("/api/command/{name}"),
        Some(TOKEN),
        Some(json!({"args": args})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{name}: {value}");
    value
}

async fn management_lifecycle(app: &Router, home: &std::path::Path) {
    assert_eq!(
        call(
            app,
            Method::POST,
            "/api/command/get_settings",
            None,
            Some(json!({"args":{}}))
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    for (name, args) in [
        ("open_provider_terminal", json!({})),
        ("upsert_mcp_server", json!({"server":[]})),
        ("web_directory", json!({"kind":"../../etc"})),
    ] {
        assert_eq!(
            call(
                app,
                Method::POST,
                &format!("/api/command/{name}"),
                Some(TOKEN),
                Some(json!({"args":args}))
            )
            .await
            .0,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{name}"
        );
    }
    let settings = rpc(app, "get_settings", json!({})).await;
    assert_eq!(settings["visibleApps"]["claude"], true);
    assert_eq!(settings["visibleApps"]["pi"], false);
    let mut hidden = settings.clone();
    hidden["visibleApps"]["gemini"] = json!(false);
    rpc(app, "save_settings", json!({"settings":hidden})).await;
    assert_eq!(
        rpc(app, "get_settings", json!({})).await["visibleApps"]["gemini"],
        false,
        "the HTTP response must preserve saved application visibility"
    );
    let mut all_hidden = settings.clone();
    for name in ["claude", "codex", "gemini"] {
        all_hidden["visibleApps"][name] = json!(false);
    }
    rpc(app, "save_settings", json!({"settings":all_hidden})).await;
    let fallback = rpc(app, "get_settings", json!({})).await;
    assert_eq!(fallback["visibleApps"]["claude"], true);
    assert_eq!(fallback["visibleApps"]["codex"], false);
    assert_eq!(fallback["visibleApps"]["gemini"], false);
    rpc(app, "save_settings", json!({"settings":settings})).await;

    let provider = json!({"id":"rpc-provider", "name":"Web full form", "settingsConfig":{"env":{"ANTHROPIC_AUTH_TOKEN":"web-test-private","ANTHROPIC_BASE_URL":"https://example.invalid"}}, "meta":{"customEndpoints":{},"costMultiplier":"1.25"},"notes":"retain metadata"});
    let before = rpc(app, "get_current_provider", json!({"app":"claude"})).await;
    for bad in [
        json!({"id":"bad","name":" ","settingsConfig":{}}),
        json!({"id":"bad","name":"Invalid","settingsConfig":[]}),
    ] {
        assert_eq!(
            call(
                app,
                Method::POST,
                "/api/command/add_provider",
                Some(TOKEN),
                Some(json!({"args":{"app":"claude","provider":bad}}))
            )
            .await
            .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    rpc(
        app,
        "add_provider",
        json!({"app":"claude","provider":provider,"addToLive":false}),
    )
    .await;
    assert_eq!(
        before,
        rpc(app, "get_current_provider", json!({"app":"claude"})).await,
        "adding a non-live provider must not activate it"
    );
    rpc(
        app,
        "add_custom_endpoint",
        json!({"app":"claude","providerId":"rpc-provider","url":"https://second.example.invalid"}),
    )
    .await;
    let listed = rpc(app, "get_providers", json!({"app":"claude"})).await;
    let revision = listed["rpc-provider"]["_webRevision"].clone();
    let mut updated = listed["rpc-provider"].clone();
    updated["name"] = json!("Updated web form");
    let body = json!({"args":{"app":"claude","provider":updated},"expected_revision":revision});
    assert_eq!(
        call(
            app,
            Method::POST,
            "/api/command/update_provider",
            Some(TOKEN),
            Some(body.clone())
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            app,
            Method::POST,
            "/api/command/update_provider",
            Some(TOKEN),
            Some(body)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let listed = rpc(app, "get_providers", json!({"app":"claude"})).await;
    assert_eq!(listed["rpc-provider"]["notes"], "retain metadata");

    let server = json!({"id":"web-mcp", "name":"Web MCP", "server":{"command":"test-mcp-binary","args":["--read-only"]}, "apps":{},"tags":[]});
    rpc(app, "upsert_mcp_server", json!({"server":server})).await;
    rpc(
        app,
        "toggle_mcp_app",
        json!({"serverId":"web-mcp","app":"claude","enabled":true}),
    )
    .await;
    assert_eq!(
        rpc(app, "get_mcp_servers", json!({})).await["web-mcp"]["apps"]["claude"],
        true
    );
    let native: Value =
        serde_json::from_slice(&std::fs::read(home.join(".claude.json")).unwrap()).unwrap();
    assert_eq!(
        native["mcpServers"]["web-mcp"]["command"],
        "test-mcp-binary"
    );
    rpc(
        app,
        "toggle_mcp_app",
        json!({"serverId":"web-mcp","app":"claude","enabled":false}),
    )
    .await;
    rpc(app, "delete_mcp_server", json!({"id":"web-mcp"})).await;
    assert!(rpc(app, "get_mcp_servers", json!({}))
        .await
        .get("web-mcp")
        .is_none());

    // SSE carries operation metadata, never provider keys or prompt contents.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/events")
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut events = response.into_body();
    events.frame().await.unwrap().unwrap();
    let prompt = json!({"id":"web-prompt","name":"Web prompt","content":"private prompt contents","enabled":false});
    rpc(
        app,
        "upsert_prompt",
        json!({"app":"claude","id":"web-prompt","prompt":prompt}),
    )
    .await;
    let frame = tokio::time::timeout(std::time::Duration::from_secs(2), events.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    let event = String::from_utf8(frame.to_vec()).unwrap();
    assert!(event.contains("upsert_prompt"));
    assert!(!event.contains("private prompt contents") && !event.contains(TOKEN));
    drop(events);
    rpc(
        app,
        "enable_prompt",
        json!({"app":"claude","id":"web-prompt"}),
    )
    .await;
    assert_eq!(
        rpc(
            app,
            "get_current_prompt_file_content",
            json!({"app":"claude"})
        )
        .await,
        "private prompt contents"
    );

    let profile = rpc(
        app,
        "create_profile",
        json!({"name":"Web snapshot","scope":"claude"}),
    )
    .await;
    assert_eq!(profile["payload"]["prompts"]["claude"], "web-prompt");
    let mut disabled = prompt.clone();
    disabled["enabled"] = json!(false);
    rpc(
        app,
        "upsert_prompt",
        json!({"app":"claude","id":"web-prompt","prompt":disabled}),
    )
    .await;
    assert_eq!(
        rpc(
            app,
            "get_current_prompt_file_content",
            json!({"app":"claude"})
        )
        .await,
        ""
    );
    rpc(
        app,
        "apply_profile",
        json!({"id":profile["id"],"scope":"claude"}),
    )
    .await;
    assert_eq!(
        rpc(
            app,
            "get_current_prompt_file_content",
            json!({"app":"claude"})
        )
        .await,
        "private prompt contents"
    );
    assert_eq!(
        rpc(app, "list_profiles", json!({})).await["currentIds"]["claude"],
        profile["id"]
    );
    rpc(
        app,
        "update_profile",
        json!({"id":profile["id"],"name":"Renamed snapshot"}),
    )
    .await;
    rpc(app, "clear_current_profile", json!({"scope":"claude"})).await;
    rpc(app, "delete_profile", json!({"id":profile["id"]})).await;
    rpc(
        app,
        "upsert_prompt",
        json!({"app":"claude","id":"web-prompt","prompt":disabled}),
    )
    .await;
    rpc(
        app,
        "delete_prompt",
        json!({"app":"claude","id":"web-prompt"}),
    )
    .await;

    let skill_dir = home.join(".claude/skills/web-fixture");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: web-fixture\ndescription: Web test fixture\n---\n# Fixture\n",
    )
    .unwrap();
    let unmanaged = rpc(app, "scan_unmanaged_skills", json!({})).await;
    assert!(unmanaged
        .as_array()
        .unwrap()
        .iter()
        .any(|skill| skill["directory"] == "web-fixture"));
    let imported = rpc(
        app,
        "import_skills_from_apps",
        json!({"imports":[{"directory":"web-fixture","apps":{"claude":true}}]}),
    )
    .await;
    let skill_id = imported[0]["id"].clone();
    rpc(
        app,
        "toggle_skill_app",
        json!({"id":skill_id,"app":"codex","enabled":true}),
    )
    .await;
    assert!(home.join(".codex/skills/web-fixture/SKILL.md").is_file());
    rpc(
        app,
        "toggle_skill_app",
        json!({"id":skill_id,"app":"codex","enabled":false}),
    )
    .await;
    rpc(app, "uninstall_skill_unified", json!({"id":skill_id})).await;
    let backups = rpc(app, "get_skill_backups", json!({})).await;
    assert!(!backups.as_array().unwrap().is_empty());
    rpc(
        app,
        "restore_skill_backup",
        json!({"backupId":backups[0]["backupId"],"currentApp":"claude"}),
    )
    .await;
    assert!(!rpc(app, "get_installed_skills", json!({}))
        .await
        .as_array()
        .unwrap()
        .is_empty());

    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file(
        "uploaded/SKILL.md",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    std::io::Write::write_all(
        &mut zip,
        b"---\nname: uploaded\ndescription: Uploaded fixture\n---\n# Uploaded\n",
    )
    .unwrap();
    zip.start_file(
        "uploaded/reference.txt",
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    std::io::Write::write_all(&mut zip, &vec![b'x'; 1100 * 1024]).unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    assert!(
        bytes.len() > 1024 * 1024,
        "ZIP upload must bypass the normal 1 MB JSON limit"
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/skills/upload/claude")
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .header(header::CONTENT_TYPE, "application/zip")
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let uploaded: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(status, StatusCode::OK, "{uploaded}");
    assert!(uploaded.to_string().contains("uploaded"));
    assert_eq!(
        call(
            app,
            Method::POST,
            "/api/command/install_skills_from_zip",
            Some(TOKEN),
            Some(json!({"args":{"filePath":"/etc/passwd","currentApp":"claude"}}))
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let projects = home.join(".claude/projects/web-fixture");
    std::fs::create_dir_all(&projects).unwrap();
    std::fs::write(projects.join("web-session.jsonl"), "{\"type\":\"assistant\",\"message\":{\"id\":\"web-msg\",\"model\":\"claude-sonnet-4-6\",\"usage\":{\"input_tokens\":10,\"output_tokens\":20},\"stop_reason\":\"end_turn\"},\"timestamp\":\"2026-10-28T12:00:00Z\",\"sessionId\":\"web-session\"}\n").unwrap();
    rpc(app, "sync_session_usage", json!({})).await;
    let summary = rpc(app, "get_usage_summary", json!({"appType":"claude"})).await;
    assert!(summary["totalRequests"].as_u64().unwrap() >= 1, "{summary}");
    assert!(
        summary["totalOutputTokens"].as_u64().unwrap() >= 20,
        "{summary}"
    );
    for name in [
        "get_usage_summary_by_app",
        "get_usage_trends",
        "get_provider_stats",
        "get_model_stats",
        "get_usage_first_date",
        "get_usage_data_sources",
        "get_model_pricing",
        "get_models_dev_sync_config",
    ] {
        rpc(app, name, json!({})).await;
    }
    let logs = rpc(
        app,
        "get_request_logs",
        json!({"filters":{},"page":0,"pageSize":20}),
    )
    .await;
    assert!(logs["total"].as_u64().unwrap() >= 1, "{logs}");

    let port_probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = port_probe.local_addr().unwrap().port();
    drop(port_probe);
    let mut config = rpc(app, "get_global_proxy_config", json!({})).await;
    config["listenAddress"] = json!("127.0.0.1");
    config["listenPort"] = json!(port);
    rpc(app, "update_global_proxy_config", json!({"config":config})).await;
    rpc(
        app,
        "set_proxy_takeover_for_app",
        json!({"appType":"claude","enabled":true,"stack":false,"route":"rpc-provider"}),
    )
    .await;
    assert_eq!(
        rpc(app, "get_app_mode", json!({"appType":"claude"})).await["mode"],
        "route"
    );
    assert_eq!(
        rpc(app, "get_proxy_status", json!({})).await["running"],
        true
    );
    rpc(
        app,
        "set_auto_failover_enabled",
        json!({"appType":"claude","enabled":true}),
    )
    .await;
    assert_eq!(
        rpc(
            app,
            "get_auto_failover_enabled",
            json!({"appType":"claude"})
        )
        .await,
        true
    );
    rpc(
        app,
        "set_auto_failover_enabled",
        json!({"appType":"claude","enabled":false}),
    )
    .await;
    rpc(app, "stop_proxy_with_restore", json!({})).await;
    assert_eq!(
        rpc(app, "get_app_mode", json!({"appType":"claude"})).await["mode"],
        "direct"
    );
    let live: Value =
        serde_json::from_slice(&std::fs::read(home.join(".claude/settings.json")).unwrap())
            .unwrap();
    assert!(
        !live.to_string().contains(&format!("127.0.0.1:{port}")),
        "stop must restore direct client settings"
    );
}
