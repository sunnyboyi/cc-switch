use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use cc_switch_lib::{web_storage, AppState, Database};
use cc_switch_web::{router, WebState};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

const TOKEN: &str = "session-test-token-isolated-32-characters";

async fn post(app: &Router, path: &str, body: Value, authenticated: bool) -> (StatusCode, Vec<u8>) {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if authenticated {
        request = request.header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert!(response.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("blob:"));
    let status = response.status();
    (
        status,
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
}

async fn rpc(app: &Router, name: &str, args: Value) -> Value {
    let (status, bytes) = post(
        app,
        &format!("/api/command/{name}"),
        json!({"args":args}),
        true,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{name}: {}",
        String::from_utf8_lossy(&bytes)
    );
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn sessions_reuse_native_readers_and_keep_files_and_index_scoped() {
    let home = tempfile::tempdir().unwrap();
    std::env::set_var("CC_SWITCH_TEST_HOME", home.path());
    web_storage::initialize().unwrap();
    let state = AppState::new(Arc::new(Database::init().unwrap()));
    let assets = tempfile::tempdir().unwrap();
    let app = router(
        WebState::new(state.clone(), TOKEN).unwrap(),
        assets.path().into(),
    );
    let project = home.path().join(".claude/projects/fixture");
    std::fs::create_dir_all(&project).unwrap();
    let path = project.join("fixture-session.jsonl");
    let first = json!({"type":"user", "sessionId":"fixture-session", "cwd":home.path(), "uuid":"u1", "timestamp":"2026-11-26T00:00:00Z", "message":{"role":"user", "content":[{"type":"text","text":"会话 needle 搜索"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"aW1hZ2U="}}]}}).to_string()+"\n";
    let mut transcript = first.clone();
    for i in 0..80 {
        transcript += &(json!({"type":"assistant","sessionId":"fixture-session","cwd":home.path(),"uuid":format!("a{i}"),"timestamp":"2026-11-26T00:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":format!("Reply {i} {}", "x".repeat(4000))}]}}).to_string()+"\n");
    }
    std::fs::write(&path, &transcript).unwrap();
    let mut source = json!({"providerId":"claude","sourcePath":path});
    for route in [
        "/api/command/list_sessions",
        "/api/sessions/stream",
        "/api/sessions/image",
    ] {
        assert_eq!(
            post(&app, route, source.clone(), false).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    let listed = rpc(&app, "list_sessions", json!({})).await;
    assert!(listed
        .as_array()
        .unwrap()
        .iter()
        .any(|session| session["sessionId"] == "fixture-session"
            && session["resumeCommand"].is_string()));
    // Use the scanner's opaque source path exactly as the shared UI does.
    source["sourcePath"] = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["sessionId"] == "fixture-session")
        .unwrap()["sourcePath"]
        .clone();
    let messages = rpc(&app, "get_session_messages", source.clone()).await;
    assert_eq!(messages.as_array().unwrap().len(), 81);
    let (status, bytes) = post(&app, "/api/sessions/stream", source.clone(), true).await;
    assert_eq!(status, StatusCode::OK);
    let chunks: Vec<Value> = std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(chunks.first().unwrap()["type"], "header");
    assert_eq!(chunks.last().unwrap()["type"], "done");
    let streamed: Vec<Value> = chunks
        .iter()
        .filter_map(|chunk| chunk["messages"].as_array())
        .flatten()
        .cloned()
        .collect();
    assert_eq!(json!(streamed), messages);
    assert!(
        chunks
            .iter()
            .filter(|chunk| chunk["type"] == "messages")
            .count()
            > 1
    );
    let content_ref =
        json!({"kind":"jsonl","offset":0,"len":first.len(),"pointer":"/message/content/0/text"});
    let mut block = source.clone();
    block["contentRef"] = content_ref;
    block["limit"] = json!(2);
    let page = rpc(&app, "get_session_block_content", block.clone()).await;
    assert_eq!(page["text"], "会话");
    assert_eq!(page["truncated"], true);
    block["offset"] = page["nextOffset"].clone();
    assert_ne!(
        rpc(&app, "get_session_block_content", block).await["text"],
        "会话"
    );
    let mut image = source.clone();
    image["image"] = json!({"mediaType":"image/png","size":5,"source":{"kind":"inline","content":{"kind":"jsonl","offset":0,"len":first.len(),"pointer":"/message/content/1/source/data"}}});
    let (status, bytes) = post(&app, "/api/sessions/image", image.clone(), true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"image");
    image["image"]["mediaType"] = json!("image/svg+xml");
    assert_eq!(
        post(&app, "/api/sessions/image", image, true).await.0,
        StatusCode::BAD_REQUEST
    );

    // The UI's search switch persists only in the Web settings; the index is rebuildable.
    let mut settings = rpc(&app, "get_settings", json!({})).await;
    settings["sessionContentSearchEnabled"] = json!(true);
    rpc(&app, "save_settings", json!({"settings":settings})).await;
    assert_eq!(
        rpc(&app, "get_settings", json!({})).await["sessionContentSearchEnabled"],
        true
    );
    rpc(&app, "sync_session_index", json!({})).await;
    let mut found = false;
    for _ in 0..100 {
        let result = rpc(
            &app,
            "search_session_content",
            json!({"query":"needle","providerIds":["claude"],"requestId":"search-fixture"}),
        )
        .await;
        if !result["hits"].as_array().unwrap().is_empty() {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(found);
    assert!(home
        .path()
        .join(".cc-switch-web/session-index.db")
        .is_file());
    assert!(!home.path().join(".cc-switch/session-index.db").exists());
    assert!(!home.path().join(".cc-switch/cc-switch.db").exists());

    let outside = home.path().join("private.jsonl");
    std::fs::write(&outside, &first).unwrap();
    let invalid = json!({"providerId":"claude","sourcePath":outside});
    assert_eq!(
        post(
            &app,
            "/api/command/get_session_messages",
            json!({"args":invalid}),
            true
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (_, bytes) = post(&app, "/api/sessions/stream", invalid.clone(), true).await;
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["type"],
        "error"
    );
    let mut delete = source.clone();
    delete["sessionId"] = json!("wrong-id");
    assert_eq!(
        post(
            &app,
            "/api/command/delete_session",
            json!({"args":delete}),
            true
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), transcript);
    let result = rpc(&app, "delete_sessions", json!({"items":[{"providerId":"claude","sessionId":"fixture-session","sourcePath":source["sourcePath"]},{"providerId":"claude","sessionId":"fixture-session","sourcePath":outside}]})).await;
    assert_eq!(result[0]["success"], true);
    assert_eq!(result[1]["success"], false);
    assert!(!path.exists());
    assert_eq!(std::fs::read_to_string(outside).unwrap(), first);
    assert!(rpc(
        &app,
        "search_session_content",
        json!({"query":"needle","providerIds":["claude"]})
    )
    .await["hits"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        post(
            &app,
            "/api/command/launch_session_terminal",
            json!({"args":{"command":"touch /tmp/not-allowed"}}),
            true
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    rpc(&app, "clear_session_index", json!({})).await;
    assert!(!home.path().join(".cc-switch-web/session-index.db").exists());
}
