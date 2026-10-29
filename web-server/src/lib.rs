use std::sync::{Arc, Mutex};

use axum::{
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use cc_switch_lib::{
    AppError, AppState, AppType, EditorSave, Provider, ProviderService, SkillService,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct WebState {
    app: AppState,
    token_hash: [u8; 32],
    // Keep the web snapshot check and the existing service mutation together.
    // The original service still owns native-file locking and conflict detection.
    gate: Arc<Mutex<()>>,
    skills: Arc<SkillService>,
    changes: tokio::sync::broadcast::Sender<Value>,
    shutdown: tokio::sync::watch::Sender<bool>,
}

impl WebState {
    pub fn validate_token(token: &str) -> Result<(), &'static str> {
        if token.len() < 32 || !token.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err("CC_SWITCH_WEB_TOKEN must contain at least 32 printable ASCII characters, without whitespace");
        }
        Ok(())
    }

    pub fn new(app: AppState, token: &str) -> Result<Self, &'static str> {
        Self::validate_token(token)?;
        Ok(Self {
            app,
            token_hash: Sha256::digest(token.as_bytes()).into(),
            gate: Arc::new(Mutex::new(())),
            skills: Arc::new(SkillService::new()),
            changes: tokio::sync::broadcast::channel(128).0,
            shutdown: tokio::sync::watch::channel(false).0,
        })
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
    }
}

pub fn router(state: WebState, web_root: std::path::PathBuf) -> Router {
    let api = Router::new()
        .route(
            "/session",
            get(|| async {
                Json(json!({"authenticated": true, "platform": std::env::consts::OS}))
            }),
        )
        .route("/apps/:app/providers", get(list).post(create))
        .route("/apps/:app/editor", get(new_editor))
        .route("/apps/:app/providers/:id", get(detail).put(update))
        .route("/apps/:app/providers/:id/switch", post(switch))
        .route("/command/:command", post(command))
        .route("/events", get(events))
        .route(
            "/skills/upload/:app",
            post(upload_skill).layer(DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        .fallback(|| async { StatusCode::NOT_FOUND })
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .layer(DefaultBodyLimit::max(1024 * 1024));

    Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new(web_root))
        .layer(middleware::from_fn(response_headers))
        .with_state(state)
}

async fn authenticate(State(state): State<WebState>, request: Request, next: Next) -> Response {
    let supplied = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    let hash: [u8; 32] = Sha256::digest(supplied.as_bytes()).into();
    if !bool::from(hash.ct_eq(&state.token_hash)) {
        return ApiError(StatusCode::UNAUTHORIZED, "Access token required".into()).into_response();
    }
    next.run(request).await
}

async fn response_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    for (name, value) in [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("content-security-policy", "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"),
    ] {
        response.headers_mut().insert(name, HeaderValue::from_static(value));
    }
    response
}

#[derive(Debug)]
struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        if let AppError::Message(message) = &error {
            if serde_json::from_str::<Value>(message)
                .ok()
                .is_some_and(|v| v["code"] == cc_switch_lib::live::patch::EDIT_CONFLICT_CODE)
            {
                return Self(StatusCode::CONFLICT, message.clone());
            }
        }
        let status = match &error {
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::InvalidInput(_) | AppError::Config(_) => StatusCode::UNPROCESSABLE_ENTITY,
            _ => StatusCode::BAD_REQUEST,
        };
        Self(status, error.to_string())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandRequest {
    #[serde(default = "empty_args")]
    args: Value,
    #[serde(default)]
    expected_revision: Option<String>,
}

fn empty_args() -> Value {
    json!({})
}

async fn command(
    State(state): State<WebState>,
    Path(command): Path<String>,
    Json(request): Json<CommandRequest>,
) -> Result<Json<Value>, ApiError> {
    if !request.args.is_object() {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "参数必须是对象".into(),
        ));
    }
    let services = state.skills.clone();
    if matches!(
        command.as_str(),
        "web_models_dev"
            | "get_tool_versions"
            | "list_tool_installations"
            | "probe_tool_installations"
            | "prepare_tool_uninstall"
    ) {
        // These reads do not modify the database or live configuration. Slow
        // version registries and tool probes must not block management writes.
        return tokio::task::spawn_blocking(move || {
            cc_switch_lib::web_support::dispatch(&state.app, &services, &command, request.args)
                .map(Json)
                .map_err(ApiError::from)
        })
        .await
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "读取任务失败".into()))?;
    }
    let changes = state.changes.clone();
    let is_read = [
        "get_",
        "read_",
        "list_",
        "scan_",
        "fetch_",
        "search_",
        "check_",
        "validate_",
        "test_",
        "query",
        "web_",
        "is_",
    ]
    .iter()
    .any(|prefix| command.starts_with(prefix));
    blocking(state, move |state| {
        if matches!(command.as_str(), "add_provider" | "update_provider") {
            let provider: Provider = serde_json::from_value(request.args["provider"].clone())
                .map_err(|_| ApiError(StatusCode::UNPROCESSABLE_ENTITY, "供应商字段无效".into()))?;
            if provider.id.trim().is_empty() || provider.name.trim().is_empty() || !provider.settings_config.is_object() {
                return Err(ApiError(StatusCode::UNPROCESSABLE_ENTITY, "供应商 ID、名称不能为空，配置必须为对象".into()));
            }
        }
        if matches!(command.as_str(), "update_provider" | "delete_provider") {
            let app_name = request.args["app"].as_str().unwrap_or("");
            let app = app_type(app_name)?;
            let id = if command == "update_provider" {
                request.args["originalId"].as_str().or(request.args["provider"]["id"].as_str())
            } else { request.args["id"].as_str() }.unwrap_or("");
            let stored = find(state, &app, id)?;
            if request.expected_revision.as_deref() != Some(revision(&stored).as_str()) {
                return Err(ApiError(StatusCode::CONFLICT, "供应商已被其他页面修改。当前草稿已保留，请重新打开最新配置后编辑。".into()));
            }
        }
        let result = cc_switch_lib::web_support::dispatch(state, &services, &command, request.args.clone());
        // A failed write can still have a partial outcome. Let other browsers reload
        // authoritative state without ever broadcasting the configuration or secrets.
        if !is_read {
            let _ = changes.send(json!({"command": command, "app": request.args["app"], "scope": request.args["scope"]}));
        }
        let mut data = result?;
        if command == "get_providers" {
            if let Some(providers) = data.as_object_mut() {
                for provider in providers.values_mut() {
                    let typed: Provider = serde_json::from_value(provider.clone()).map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "无法读取供应商版本".into()))?;
                    provider["_webRevision"] = Value::String(revision(&typed));
                }
            }
        }
        Ok(data)
    }).await
}

async fn events(
    State(state): State<WebState>,
) -> axum::response::Sse<
    impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
> {
    let mut changes = state.changes.subscribe();
    let mut shutdown = state.shutdown.subscribe();
    let stream = async_stream::stream! {
        yield Ok(axum::response::sse::Event::default().data("{\"command\":\"connected\"}"));
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                event = changes.recv() => {
                    match event {
                        Ok(data) => yield Ok(axum::response::sse::Event::default().data(data.to_string())),
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => yield Ok(axum::response::sse::Event::default().data("{\"command\":\"resync\"}")),
                        Err(_) => break,
                    }
                }
            }
        }
    };
    axum::response::Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

async fn upload_skill(
    State(state): State<WebState>,
    Path(app): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, ApiError> {
    let app = app_type(&app)?;
    let changes = state.changes.clone();
    blocking(state, move |state| {
        use std::io::Write;
        let mut file = tempfile::Builder::new()
            .suffix(".zip")
            .tempfile()
            .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "无法创建上传文件".into()))?;
        file.write_all(&body)
            .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "上传文件写入失败".into()))?;
        let result = cc_switch_lib::web_support::install_uploaded_zip(state, file.path(), app);
        let _ = changes.send(json!({"command": "install_skills_from_zip"}));
        result.map_err(ApiError::from)
    })
    .await
}

fn app_type(app: &str) -> Result<AppType, ApiError> {
    match app {
        "claude" => Ok(AppType::Claude),
        "codex" => Ok(AppType::Codex),
        "gemini" => Ok(AppType::Gemini),
        _ => Err(ApiError(
            StatusCode::NOT_FOUND,
            "Unsupported application".into(),
        )),
    }
}

async fn blocking<F>(state: WebState, work: F) -> Result<Json<Value>, ApiError>
where
    F: FnOnce(&AppState) -> Result<Value, ApiError> + Send + 'static,
{
    // Existing methods may block_on the shared async runtime; keep them off
    // Axum's async workers, just as the desktop commands do.
    tokio::task::spawn_blocking(move || {
        let _guard = state.gate.lock().map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Service lock unavailable".into(),
            )
        })?;
        work(&state.app).map(Json)
    })
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Service task failed".into(),
        )
    })?
}

fn revision(provider: &Provider) -> String {
    // Provider metadata contains HashMaps. Their iteration order changes on read,
    // so source ordering must not turn an unchanged row into a false conflict.
    let mut value = serde_json::to_value(provider).expect("provider is serializable");
    value.sort_all_objects();
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&value).expect("provider is serializable"))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_ignores_object_order_but_detects_actual_changes() {
        let first = Provider::with_id(
            "id".into(),
            "Provider".into(),
            serde_json::from_str(r#"{"env":{"a":1,"b":2},"config":{}}"#).unwrap(),
            None,
        );
        let mut reordered = first.clone();
        reordered.settings_config =
            serde_json::from_str(r#"{"config":{},"env":{"b":2,"a":1}}"#).unwrap();
        assert_eq!(revision(&first), revision(&reordered));
        reordered.name = "Changed".into();
        assert_ne!(revision(&first), revision(&reordered));
    }
}

fn find(state: &AppState, app: &AppType, id: &str) -> Result<Provider, ApiError> {
    state
        .db
        // Match the list snapshot, including metadata merged from endpoints.
        .get_all_providers(app.as_str())?
        .shift_remove(id)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "Provider not found".into()))
}

async fn list(
    State(state): State<WebState>,
    Path(app): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let app = app_type(&app)?;
    blocking(state, move |state| {
        let providers = ProviderService::list(state, app.clone())?;
        let mode = cc_switch_lib::mode::controller::app_mode_view(state, &app)
            .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
        let current = ProviderService::current(state, app)?;
        let mut rows: Vec<_> = providers.values().collect();
        rows.sort_by_key(|p| (p.sort_index.unwrap_or(usize::MAX), p.created_at.unwrap_or(0)));
        Ok(json!({
            "providers": rows.iter().map(|p| json!({"id": p.id, "name": p.name, "notes": p.notes})).collect::<Vec<_>>(),
            "currentId": current,
            "mode": mode,
        }))
    }).await
}

fn editor_detail(state: &AppState, app: AppType, provider: Provider) -> Result<Value, ApiError> {
    let category = ProviderService::editor_category(
        state,
        &app,
        Some(&provider.id),
        provider.category.clone(),
    )?;
    let view = ProviderService::editor_view_with_meta(
        state,
        app,
        &provider.settings_config,
        category.as_deref(),
        Some(&provider.id),
        provider.meta.as_ref(),
    )?;
    Ok(json!({"revision": revision(&provider), "provider": provider, "editor": view}))
}

async fn detail(
    State(state): State<WebState>,
    Path((app, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let app = app_type(&app)?;
    blocking(state, move |state| {
        editor_detail(state, app.clone(), find(state, &app, &id)?)
    })
    .await
}

async fn new_editor(
    State(state): State<WebState>,
    Path(app): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let app = app_type(&app)?;
    blocking(state, move |state| {
        let draft = if app == AppType::Codex {
            json!({"auth": {}, "config": ""})
        } else {
            json!({})
        };
        let view = ProviderService::editor_view(state, app, &draft, None)?;
        Ok(json!({"editor": view, "draft": draft}))
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SaveRequest {
    name: String,
    notes: Option<String>,
    settings_config: Value,
    editor_save: EditorSave,
    expected_revision: Option<String>,
}

fn validate(save: &SaveRequest) -> Result<(), ApiError> {
    if save.name.trim().is_empty()
        || save.name.chars().count() > 120
        || !save.settings_config.is_object()
        || !save.editor_save.base.is_object()
    {
        return Err(ApiError(StatusCode::UNPROCESSABLE_ENTITY, "Name is required (maximum 120 characters); settings and editor base must be JSON objects".into()));
    }
    Ok(())
}

async fn create(
    State(state): State<WebState>,
    Path(app): Path<String>,
    Json(save): Json<SaveRequest>,
) -> Result<Json<Value>, ApiError> {
    let app = app_type(&app)?;
    validate(&save)?;
    blocking(state, move |state| {
        let mut provider = Provider::with_id(
            uuid::Uuid::new_v4().to_string(),
            save.name.trim().into(),
            save.settings_config,
            None,
        );
        provider.notes = save.notes;
        let id = provider.id.clone();
        ProviderService::add_from_editor(state, app, provider, true, Some(save.editor_save))?;
        Ok(json!({"id": id}))
    })
    .await
}

async fn update(
    State(state): State<WebState>,
    Path((app, id)): Path<(String, String)>,
    Json(save): Json<SaveRequest>,
) -> Result<Json<Value>, ApiError> {
    let app = app_type(&app)?;
    validate(&save)?;
    blocking(state, move |state| {
        let mut provider = find(state, &app, &id)?;
        if save.expected_revision.as_deref() != Some(revision(&provider).as_str()) {
            return Err(ApiError(StatusCode::CONFLICT, "Provider changed. Reopen the editor to load the latest version; your draft has been kept.".into()));
        }
        provider.name = save.name.trim().into();
        provider.notes = save.notes;
        provider.settings_config = save.settings_config;
        ProviderService::update_from_editor(state, app, Some(&id), provider, Some(save.editor_save))?;
        Ok(json!({"id": id}))
    }).await
}

async fn switch(
    State(state): State<WebState>,
    Path((app, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let app = app_type(&app)?;
    blocking(state, move |state| {
        find(state, &app, &id)?;
        let result = ProviderService::switch(state, app, &id)?;
        Ok(json!({"result": result}))
    })
    .await
}
