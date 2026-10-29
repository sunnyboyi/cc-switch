//! Headless transport adapter. The desktop command signatures and entry point stay intact.
//! Only the operations explicitly listed here are exposed by the web server.

use std::{fmt::Display, str::FromStr};

use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

use crate::{
    commands,
    services::{model_pricing, profile::ProfileScope, skill::SkillService},
    AppError, AppState, AppType, McpService, ProfileService, PromptService, ProviderService,
};

pub fn initialize_outbound_proxy(db: &crate::Database) -> Result<(), AppError> {
    crate::proxy::http_client::init(db.get_global_proxy_url()?.as_deref()).map_err(message)
}

fn message(error: impl Display + 'static) -> AppError {
    let text = error.to_string();
    match (Box::new(error) as Box<dyn std::any::Any>).downcast::<AppError>() {
        Ok(error) => *error,
        Err(_) => AppError::Message(text),
    }
}

fn arg<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T, AppError> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null))
        .map_err(|_| AppError::InvalidInput(format!("参数无效: {name}")))
}

fn text<'a>(args: &'a Value, name: &str) -> Result<&'a str, AppError> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::InvalidInput(format!("参数无效: {name}")))
}

fn app(args: &Value, name: &str) -> Result<AppType, AppError> {
    AppType::from_str(text(args, name)?)
}

fn output<T: Serialize>(value: T) -> Result<Value, AppError> {
    serde_json::to_value(value).map_err(message)
}

macro_rules! reply {
    ($value:expr) => {
        output($value.map_err(message)?)
    };
}

macro_rules! wait {
    ($value:expr) => {
        reply!(crate::runtime::block_on($value))
    };
}

/// Called on the HTTP server's blocking pool. Some existing synchronous services
/// themselves block_on the runtime, so this must not run on an async worker.
pub fn dispatch(
    state: &AppState,
    skills: &SkillService,
    command: &str,
    args: Value,
) -> Result<Value, AppError> {
    let db = &state.db;
    match command {
        "get_providers" => reply!(ProviderService::list(state, app(&args, "app")?)),
        "get_current_provider" => reply!(ProviderService::current(state, app(&args, "app")?)),
        "get_provider_editor_view" => {
            let app = app(&args, "app")?;
            let id: Option<String> = arg(&args, "providerId")?;
            let category = ProviderService::editor_category(
                state,
                &app,
                id.as_deref(),
                arg(&args, "category")?,
            )?;
            let meta: Option<crate::ProviderMeta> = arg(&args, "meta")?;
            reply!(ProviderService::editor_view_with_meta(
                state,
                app,
                &arg(&args, "settingsConfig")?,
                category.as_deref(),
                id.as_deref(),
                meta.as_ref(),
            ))
        }
        "add_provider" => reply!(ProviderService::add_from_editor(
            state,
            app(&args, "app")?,
            arg(&args, "provider")?,
            arg::<Option<bool>>(&args, "addToLive")?.unwrap_or(true),
            arg(&args, "editorSave")?,
        )),
        "update_provider" => reply!(ProviderService::update_from_editor(
            state,
            app(&args, "app")?,
            arg::<Option<String>>(&args, "originalId")?.as_deref(),
            arg(&args, "provider")?,
            arg(&args, "editorSave")?,
        )),
        "delete_provider" => {
            ProviderService::delete(state, app(&args, "app")?, text(&args, "id")?)?;
            output(true)
        }
        "switch_provider" => reply!(ProviderService::switch(
            state,
            app(&args, "app")?,
            text(&args, "id")?
        )),
        "add_provider_to_live_config" | "remove_provider_from_live_config" => {
            let app = app(&args, "app")?;
            let id = text(&args, "id")?;
            if command == "add_provider_to_live_config" {
                ProviderService::add_to_live_config(state, app, id)?;
            } else {
                ProviderService::remove_from_live_config(state, app, id)?;
            }
            output(true)
        }
        "import_default_config" => reply!(ProviderService::import_default_config(
            state,
            app(&args, "app")?
        )),
        "read_live_provider_settings" => {
            reply!(ProviderService::read_live_settings(app(&args, "app")?))
        }
        "update_providers_sort_order" => {
            ProviderService::update_sort_order(state, app(&args, "app")?, arg(&args, "updates")?)?;
            output(true)
        }
        "get_custom_endpoints" => reply!(ProviderService::get_custom_endpoints(
            state,
            app(&args, "app")?,
            text(&args, "providerId")?
        )),
        "add_custom_endpoint" => reply!(ProviderService::add_custom_endpoint(
            state,
            app(&args, "app")?,
            text(&args, "providerId")?,
            arg(&args, "url")?
        )),
        "remove_custom_endpoint" => reply!(ProviderService::remove_custom_endpoint(
            state,
            app(&args, "app")?,
            text(&args, "providerId")?,
            arg(&args, "url")?
        )),
        "update_endpoint_last_used" => reply!(ProviderService::update_endpoint_last_used(
            state,
            app(&args, "app")?,
            text(&args, "providerId")?,
            arg(&args, "url")?
        )),
        "get_universal_providers" => reply!(ProviderService::list_universal(state)),
        "get_universal_provider" => {
            reply!(ProviderService::get_universal(state, text(&args, "id")?))
        }
        "upsert_universal_provider" => reply!(ProviderService::upsert_universal(
            state,
            arg(&args, "provider")?
        )),
        "delete_universal_provider" => {
            reply!(ProviderService::delete_universal(state, text(&args, "id")?))
        }
        "sync_universal_provider" => reply!(ProviderService::sync_universal_to_apps(
            state,
            text(&args, "id")?
        )),

        "get_settings" => {
            let mut settings = output(crate::settings::get_settings_for_frontend())?;
            let mut visible = output(
                crate::settings::get_settings()
                    .visible_apps
                    .unwrap_or_default(),
            )?;
            if let Some(apps) = visible.as_object_mut() {
                for (app, enabled) in apps.iter_mut() {
                    if !matches!(app.as_str(), "claude" | "codex" | "gemini") {
                        *enabled = json!(false);
                    }
                }
                if !["claude", "codex", "gemini"]
                    .iter()
                    .any(|app| apps.get(*app) == Some(&json!(true)))
                {
                    apps.insert("claude".into(), json!(true));
                }
            }
            settings["visibleApps"] = visible;
            Ok(settings)
        }
        // Only the shared page preferences are writable here. Desktop-only settings
        // have side effects and must not be changed by a stale full-settings form.
        "save_settings" => {
            let incoming = arg::<crate::settings::AppSettings>(&args, "settings")?;
            for directory in [
                &incoming.claude_config_dir,
                &incoming.codex_config_dir,
                &incoming.gemini_config_dir,
            ]
            .into_iter()
            .flatten()
            {
                if !directory.trim().is_empty()
                    && !std::path::Path::new(directory.trim()).is_absolute()
                {
                    return Err(AppError::InvalidInput(
                        "CLI 配置目录必须为服务器上的绝对路径".into(),
                    ));
                }
            }
            let mut settings = crate::settings::get_settings();
            settings.usage_dashboard_refresh_interval_ms =
                incoming.usage_dashboard_refresh_interval_ms;
            settings.session_auto_sync_enabled = incoming.session_auto_sync_enabled;
            settings.usage_display = incoming.usage_display;
            settings.skill_sync_method = incoming.skill_sync_method;
            settings.failover_confirmed = incoming.failover_confirmed;
            settings.language = incoming.language;
            settings.quota_display = incoming.quota_display;
            settings.visible_apps = incoming.visible_apps;
            settings.app_order = incoming.app_order;
            settings.show_profile_switcher = incoming.show_profile_switcher;
            settings.show_provider_search = incoming.show_provider_search;
            settings.check_tool_updates_on_startup = incoming.check_tool_updates_on_startup;
            settings.backup_interval_hours = incoming.backup_interval_hours;
            settings.backup_retain_count = incoming.backup_retain_count;
            settings.claude_config_dir = incoming.claude_config_dir;
            settings.codex_config_dir = incoming.codex_config_dir;
            settings.gemini_config_dir = incoming.gemini_config_dir;
            crate::settings::update_settings(settings)?;
            output(true)
        }

        "get_tool_versions" => wait!(commands::get_tool_versions(
            arg(&args, "tools")?,
            arg(&args, "wslShellByTool")?
        )),
        "list_tool_installations" => wait!(commands::list_tool_installations(arg(&args, "tools")?)),
        "probe_tool_installations" => {
            wait!(commands::probe_tool_installations(arg(&args, "tools")?))
        }
        "prepare_tool_uninstall" => wait!(commands::prepare_tool_uninstall(
            arg(&args, "tool")?,
            arg(&args, "wslShellByTool")?
        )),
        "run_tool_lifecycle_action" => wait!(commands::run_tool_lifecycle_action(
            arg(&args, "tools")?,
            arg(&args, "action")?,
            arg(&args, "wslShellByTool")?,
            arg(&args, "uninstallPlan")?
        )),
        "get_claude_desktop_status" => reply!(crate::claude_desktop_config::get_status(
            db,
            crate::runtime::block_on(state.proxy_service.is_running())
        )),
        "get_global_proxy_url" => reply!(db.get_global_proxy_url()),
        "set_global_proxy_url" => {
            let url = text(&args, "url")?;
            let url = (!url.trim().is_empty()).then_some(url);
            crate::proxy::http_client::validate_proxy(url).map_err(message)?;
            db.set_global_proxy_url(url)?;
            crate::proxy::http_client::apply_proxy(url).map_err(message)?;
            output(())
        }
        "test_proxy_url" => wait!(commands::test_proxy_url(arg(&args, "url")?)),
        "scan_local_proxies" => output(crate::runtime::block_on(commands::scan_local_proxies())),
        "get_upstream_proxy_status" => output(commands::get_upstream_proxy_status()),
        "list_db_backups" => reply!(commands::list_db_backups()),
        "create_db_backup" => {
            let path = db
                .backup_database_file()?
                .ok_or_else(|| AppError::Database("Web 数据库尚未创建".into()))?;
            output(path.file_name().unwrap_or_default().to_string_lossy())
        }
        "rename_db_backup" => reply!(commands::rename_db_backup(
            arg(&args, "oldFilename")?,
            arg(&args, "newName")?
        )),
        "delete_db_backup" => reply!(commands::delete_db_backup(arg(&args, "filename")?)),
        "restore_db_backup" => wait!(commands::restore_db_backup_for_state(
            state,
            arg(&args, "filename")?
        )),
        "web_storage_info" => output(json!({
            "directory": crate::config::get_app_config_dir(),
            "database": crate::config::get_database_path(),
            "schemaVersion": crate::database::SCHEMA_VERSION,
            "platform": std::env::consts::OS,
        })),

        "get_mcp_servers" => reply!(McpService::get_all_servers(state)),
        "upsert_mcp_server" => reply!(McpService::upsert_server(state, arg(&args, "server")?)),
        "delete_mcp_server" => reply!(McpService::delete_server(state, text(&args, "id")?)),
        "toggle_mcp_app" => wait!(McpService::toggle_app_locked(
            state,
            text(&args, "serverId")?,
            app(&args, "app")?,
            arg(&args, "enabled")?
        )),
        "import_mcp_from_apps" => reply!(McpService::import_from_all_apps(state)),
        "resync_mcp_to_apps" => {
            let apps: Option<Vec<String>> = arg(&args, "apps")?;
            let targets = McpService::resync_targets(apps.as_deref())?;
            output(crate::runtime::block_on(async {
                let mut outcomes = Vec::new();
                for app in targets {
                    let _guard = state.proxy_service.lock_switch_for_app(app.as_str()).await;
                    outcomes.push(McpService::resync_app(state, &app));
                }
                outcomes
            }))
        }
        "validate_mcp_command" => wait!(commands::validate_mcp_command(arg(&args, "cmd")?)),

        "get_prompts" => reply!(PromptService::get_prompts(state, app(&args, "app")?)),
        "upsert_prompt" => reply!(PromptService::upsert_prompt(
            state,
            app(&args, "app")?,
            text(&args, "id")?,
            arg(&args, "prompt")?
        )),
        "delete_prompt" => reply!(PromptService::delete_prompt(
            state,
            app(&args, "app")?,
            text(&args, "id")?
        )),
        "enable_prompt" => reply!(PromptService::enable_prompt(
            state,
            app(&args, "app")?,
            text(&args, "id")?
        )),
        "import_prompt_from_file" => {
            reply!(PromptService::import_from_file(state, app(&args, "app")?))
        }
        "get_current_prompt_file_content" => {
            reply!(PromptService::get_current_file_content(app(&args, "app")?))
        }
        "get_prompt_file_location" => wait!(commands::get_prompt_file_location(arg(&args, "app")?)),
        "get_pi_prompt_file" => wait!(commands::get_pi_prompt_file(arg(&args, "kind")?)),
        "replace_pi_prompt_file" => wait!(commands::replace_pi_prompt_file(
            arg(&args, "kind")?,
            arg(&args, "expectedRevision")?,
            arg(&args, "content")?
        )),
        "delete_pi_prompt_file" => wait!(commands::delete_pi_prompt_file(
            arg(&args, "kind")?,
            arg(&args, "expectedRevision")?
        )),
        "list_pi_prompt_templates" => wait!(commands::list_pi_prompt_templates()),
        "upsert_pi_prompt_template" => wait!(commands::upsert_pi_prompt_template(
            arg(&args, "slug")?,
            arg(&args, "originalSlug")?,
            arg(&args, "expectedRevision")?,
            arg(&args, "content")?
        )),
        "delete_pi_prompt_template" => wait!(commands::delete_pi_prompt_template(
            arg(&args, "slug")?,
            arg(&args, "expectedRevision")?
        )),

        "list_profiles" => {
            let profiles = ProfileService::list(state)?
                .into_iter()
                .map(commands::ProfileDto::from)
                .collect::<Vec<_>>();
            output(json!({"profiles": profiles, "currentIds": {
                "claude": db.get_current_profile_id("claude")?,
                "claudeDesktop": db.get_current_profile_id("claude-desktop")?,
                "codex": db.get_current_profile_id("codex")?,
            }}))
        }
        "create_profile" => output(commands::ProfileDto::from(ProfileService::create(
            state,
            text(&args, "name")?,
            ProfileScope::parse(text(&args, "scope")?)?,
        )?)),
        "update_profile" => {
            let scope: Option<String> = arg(&args, "scope")?;
            output(commands::ProfileDto::from(ProfileService::update(
                state,
                text(&args, "id")?,
                arg(&args, "name")?,
                arg::<Option<bool>>(&args, "resnapshot")?.unwrap_or(false),
                scope.as_deref().map(ProfileScope::parse).transpose()?,
            )?))
        }
        "delete_profile" => reply!(ProfileService::delete(state, text(&args, "id")?)),
        "apply_profile" => reply!(ProfileService::apply(
            state,
            text(&args, "id")?,
            ProfileScope::parse(text(&args, "scope")?)?
        )),
        "clear_current_profile" => {
            reply!(db
                .set_current_profile_id(ProfileScope::parse(text(&args, "scope")?)?.as_str(), None))
        }

        "get_installed_skills" => reply!(SkillService::get_all_installed(db)),
        "get_skill_backups" => reply!(SkillService::list_backups()),
        "delete_skill_backup" => {
            SkillService::delete_backup(text(&args, "backupId")?).map_err(message)?;
            output(true)
        }
        "install_skill_unified" => {
            wait!(skills.install(db, &arg(&args, "skill")?, &app(&args, "currentApp")?))
        }
        "uninstall_skill_unified" => reply!(SkillService::uninstall(db, text(&args, "id")?)),
        "restore_skill_backup" => reply!(SkillService::restore_from_backup(
            db,
            text(&args, "backupId")?,
            &app(&args, "currentApp")?
        )),
        "toggle_skill_app" => {
            SkillService::toggle_app(
                db,
                text(&args, "id")?,
                &app(&args, "app")?,
                arg(&args, "enabled")?,
            )
            .map_err(message)?;
            output(true)
        }
        "scan_unmanaged_skills" => reply!(SkillService::scan_unmanaged(db)),
        "import_skills_from_apps" => {
            reply!(SkillService::import_from_apps(db, arg(&args, "imports")?))
        }
        "discover_available_skills" => wait!(skills.discover_available_report(
            db.get_skill_repos()?,
            arg::<Option<bool>>(&args, "forceRefresh")?.unwrap_or(false)
        )),
        "discover_repo_skills" => {
            let owner = text(&args, "owner")?;
            let name = text(&args, "name")?;
            if let Some(repo) = db
                .get_skill_repos()?
                .into_iter()
                .find(|r| r.owner == owner && r.name == name)
            {
                output(crate::runtime::block_on(skills.discover_repo(&repo)))
            } else {
                output(crate::services::skill::SkillDiscoveryResult::default())
            }
        }
        "check_skill_updates" => wait!(skills.check_updates_report(db)),
        "resync_skills_to_apps" => output(SkillService::resync_all_apps(db)),
        "update_skill" => wait!(skills.update_skill(db, text(&args, "id")?)),
        "migrate_skill_storage" => reply!(SkillService::migrate_storage(db, arg(&args, "target")?)),
        "get_cc_switch_skills_dir" => wait!(commands::get_cc_switch_skills_dir()),
        "get_skill_repos" => reply!(db.get_skill_repos()),
        "add_skill_repo" => {
            let repo: crate::services::skill::SkillRepo = arg(&args, "repo")?;
            SkillService::validate_repo_ref(&repo.owner, &repo.name, &repo.branch)
                .map_err(message)?;
            db.save_skill_repo(&repo)?;
            output(true)
        }
        "remove_skill_repo" => {
            db.delete_skill_repo(text(&args, "owner")?, text(&args, "name")?)?;
            output(true)
        }
        "search_skills_sh" => wait!(commands::search_skills_sh(
            arg(&args, "query")?,
            arg::<usize>(&args, "limit")?.min(100),
            arg(&args, "offset")?
        )),

        "get_app_mode" => reply!(crate::mode::controller::app_mode_view(
            state,
            &app(&args, "appType")?
        )),
        "get_direct_provider" => reply!(crate::mode::controller::direct_provider_id(
            state,
            &app(&args, "appType")?
        )),
        "take_startup_attach_failures" => {
            output(crate::mode::controller::startup_attach_failures_snapshot())
        }
        "get_proxy_status" => wait!(state.proxy_service.get_status()),
        "get_proxy_takeover_status" => wait!(state.proxy_service.get_takeover_status()),
        "start_proxy_server" => wait!(state.proxy_service.start()),
        // Restoring clients is mandatory for a remote stop operation.
        "stop_proxy_with_restore" | "stop_proxy_server" => {
            wait!(crate::mode::controller::exit_all(state))
        }
        "set_proxy_takeover_for_app" => {
            let app = app(&args, "appType")?;
            if arg::<bool>(&args, "enabled")? {
                wait!(crate::mode::controller::enter_with_route(
                    state,
                    &app,
                    arg::<Option<bool>>(&args, "stack")?.unwrap_or(false),
                    arg::<Option<String>>(&args, "route")?.as_deref()
                ))
            } else {
                wait!(crate::mode::controller::exit(state, &app))
            }
        }
        "set_proxy_route" => wait!(crate::mode::controller::set_route(
            state,
            &app(&args, "appType")?,
            text(&args, "providerId")?
        )),
        "get_proxy_stack" => wait!(crate::mode::controller::stack_view_with_clients(
            state,
            &app(&args, "appType")?
        )),
        "set_proxy_stack_member" => {
            let result = crate::runtime::block_on(crate::mode::controller::set_stack_member(
                state,
                &app(&args, "appType")?,
                text(&args, "providerId")?,
                arg(&args, "enabled")?,
            ))
            .map_err(|error| message(serde_json::to_string(&error).unwrap_or(error.message)))?;
            output(result)
        }
        "adopt_codex_stack_catalog" => {
            wait!(crate::mode::controller::adopt_codex_stack_catalog(state))
        }
        "get_global_proxy_config" => wait!(db.get_global_proxy_config()),
        "update_global_proxy_config" => {
            let config = arg(&args, "config")?;
            if crate::runtime::block_on(state.proxy_service.update_global_config(&config))
                .map_err(message)?
            {
                crate::runtime::block_on(crate::mode::controller::resync_routes(state))
                    .map_err(message)?;
            }
            output(())
        }
        "get_proxy_client_host" => {
            output(crate::settings::get_proxy_client_host().unwrap_or_default())
        }
        "set_proxy_client_host" => wait!(commands::save_proxy_client_host(
            state,
            text(&args, "host")?
        )),
        "get_proxy_config_for_app" => wait!(db.get_proxy_config_for_app(text(&args, "appType")?)),
        "update_proxy_config_for_app" => {
            let mut config: crate::proxy::types::AppProxyConfig = arg(&args, "config")?;
            let app = AppType::from_str(&config.app_type)?;
            if !app.supports_local_proxy() {
                return Err(AppError::InvalidInput("该应用不支持本地路由".into()));
            }
            config.enabled = crate::mode::current::is_proxy(&app);
            let circuit = crate::proxy::CircuitBreakerConfig::from(&config);
            let app_type = config.app_type.clone();
            crate::runtime::block_on(db.update_proxy_config_for_app(config))?;
            wait!(state
                .proxy_service
                .update_circuit_breaker_config_for_app(&app_type, circuit))
        }
        "get_pricing_model_source" => wait!(db.get_pricing_model_source(text(&args, "appType")?)),
        "set_pricing_model_source" => {
            wait!(db.set_pricing_model_source(text(&args, "appType")?, text(&args, "value")?))
        }
        "get_auto_failover_enabled" => output(
            crate::runtime::block_on(db.get_proxy_config_for_app(text(&args, "appType")?))?
                .auto_failover_enabled,
        ),
        "get_failover_queue" => {
            let app = text(&args, "appType")?;
            commands::require_failover_app(app).map_err(message)?;
            let providers = db.get_all_providers(app)?;
            output(
                db.get_failover_queue(app)?
                    .into_iter()
                    .filter(|item| {
                        providers.get(&item.provider_id).is_some_and(|provider| {
                            crate::proxy::provider_router::provider_supports_failover(app, provider)
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "get_available_providers_for_failover" => {
            let app = text(&args, "appType")?;
            commands::require_failover_app(app).map_err(message)?;
            output(
                db.get_available_providers_for_failover(app)?
                    .into_iter()
                    .filter(|provider| {
                        crate::proxy::provider_router::provider_supports_failover(app, provider)
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "add_to_failover_queue" | "remove_from_failover_queue" => {
            let app = text(&args, "appType")?;
            let id = text(&args, "providerId")?;
            commands::require_failover_app(app).map_err(message)?;
            if command == "add_to_failover_queue" {
                commands::require_failover_provider(db, app, id).map_err(message)?;
                db.add_to_failover_queue(app, id)?;
            } else {
                db.remove_from_failover_queue(app, id)?;
            }
            output(())
        }
        "set_auto_failover_enabled" => wait!(commands::set_auto_failover_for_state(
            None,
            state,
            arg(&args, "appType")?,
            arg(&args, "enabled")?
        )),
        "get_provider_health" => {
            wait!(db.get_provider_health(text(&args, "providerId")?, text(&args, "appType")?))
        }
        // The desktop command currently has no in-memory stats interface either.
        "get_circuit_breaker_stats" => output(Value::Null),
        "reset_circuit_breaker" => wait!(commands::reset_circuit_breaker_for_state(
            None,
            state,
            arg(&args, "providerId")?,
            arg(&args, "appType")?
        )),

        "get_claude_common_config_snippet" => reply!(db.get_config_snippet("claude")),
        "get_common_config_snippet" => reply!(db.get_config_snippet(text(&args, "appType")?)),
        "get_model_pricing_file_path" => {
            output(model_pricing::model_pricing_file_path().to_string_lossy())
        }
        "reload_model_pricing" => reply!(model_pricing::sync_local_model_pricing(db)),
        "web_models_dev" => wait!(async {
            let response = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(message)?
                .get("https://models.dev/api.json")
                .send()
                .await
                .map_err(message)?
                .error_for_status()
                .map_err(message)?;
            response.json::<Value>().await.map_err(message)
        }),
        "ensure_codex_official_provider" => reply!(db.ensure_official_seed_by_id(
            crate::database::CODEX_OFFICIAL_PROVIDER_ID,
            AppType::Codex
        )),
        "web_home_dir" => output(crate::config::get_home_dir().to_string_lossy()),
        "web_join_path" => output(
            arg::<Vec<String>>(&args, "parts")?
                .iter()
                .collect::<std::path::PathBuf>()
                .to_string_lossy(),
        ),
        "get_app_config_dir_override" => output(crate::web_storage::data_dir()),
        "web_directory" => {
            let path = match text(&args, "kind")? {
                "open_skill_dir" => {
                    SkillService::get_installed_skill_dir(text(&args, "directory")?)
                        .map_err(message)?
                }
                "open_cc_switch_skills_dir" => crate::config::get_app_config_dir().join("skills"),
                "open_model_pricing_folder" | "open_app_config_folder" => {
                    crate::config::get_app_config_dir()
                }
                "open_config_folder" => std::path::PathBuf::from(
                    crate::runtime::block_on(commands::get_config_dir(arg(&args, "app")?))
                        .map_err(message)?,
                ),
                _ => return Err(AppError::InvalidInput("不支持此目录类型".into())),
            };
            output(path.to_string_lossy())
        }
        "stream_check_provider" => {
            let app = app(&args, "appType")?;
            let provider = db
                .get_provider_by_id(text(&args, "providerId")?, app.as_str())?
                .ok_or_else(|| AppError::InvalidInput("供应商不存在".into()))?;
            wait!(
                crate::services::stream_check::StreamCheckService::check_with_retry(
                    &app,
                    &provider,
                    &crate::services::stream_check::StreamCheckConfig::default(),
                    None
                )
            )
        }
        "queryProviderUsage" => wait!(ProviderService::query_usage(
            state,
            app(&args, "app")?,
            text(&args, "providerId")?
        )),

        "get_usage_summary" | "get_usage_trends" | "get_provider_stats" | "get_model_stats" => {
            let start = arg(&args, "startDate")?;
            let end = arg(&args, "endDate")?;
            let app: Option<String> = arg(&args, "appType")?;
            let provider: Option<String> = arg(&args, "providerName")?;
            let model: Option<String> = arg(&args, "model")?;
            match command {
                "get_usage_summary" => reply!(db.get_usage_summary(
                    start,
                    end,
                    app.as_deref(),
                    provider.as_deref(),
                    model.as_deref()
                )),
                "get_usage_trends" => reply!(db.get_daily_trends(
                    start,
                    end,
                    app.as_deref(),
                    provider.as_deref(),
                    model.as_deref()
                )),
                "get_provider_stats" => reply!(db.get_provider_stats(
                    start,
                    end,
                    app.as_deref(),
                    provider.as_deref(),
                    model.as_deref()
                )),
                _ => reply!(db.get_model_stats(
                    start,
                    end,
                    app.as_deref(),
                    provider.as_deref(),
                    model.as_deref()
                )),
            }
        }
        "get_usage_summary_by_app" => reply!(db.get_usage_summary_by_app(
            arg(&args, "startDate")?,
            arg(&args, "endDate")?,
            arg::<Option<String>>(&args, "providerName")?.as_deref(),
            arg::<Option<String>>(&args, "model")?.as_deref()
        )),
        "get_usage_first_date" => reply!(db.get_first_usage_date(
            arg::<Option<String>>(&args, "appType")?.as_deref(),
            arg::<Option<String>>(&args, "providerName")?.as_deref(),
            arg::<Option<String>>(&args, "model")?.as_deref()
        )),
        "get_request_logs" => reply!(db.get_request_logs(
            &arg(&args, "filters")?,
            arg(&args, "page")?,
            arg::<u32>(&args, "pageSize")?.clamp(1, 200)
        )),
        "get_request_detail" => reply!(db.get_request_detail(text(&args, "requestId")?)),
        "get_models_dev_sync_config" => reply!(model_pricing::get_models_dev_sync_state(db)),
        "get_model_pricing" => reply!(commands::get_model_pricing_for_db(db)),
        "save_models_dev_sync_config" => reply!(model_pricing::save_models_dev_sync_config(
            db,
            arg(&args, "config")?
        )),
        "record_models_dev_sync_result" => reply!(model_pricing::record_models_dev_sync_result(
            db,
            arg(&args, "syncedAt")?,
            arg(&args, "error")?
        )),
        "update_model_pricing" => reply!(model_pricing::update_model_pricing(
            db,
            model_pricing::ModelPricingInfo {
                model_id: arg(&args, "modelId")?,
                display_name: arg(&args, "displayName")?,
                input_cost_per_million: arg(&args, "inputCost")?,
                output_cost_per_million: arg(&args, "outputCost")?,
                cache_read_cost_per_million: arg(&args, "cacheReadCost")?,
                cache_creation_cost_per_million: arg(&args, "cacheCreationCost")?,
            }
        )),
        "update_model_pricing_batch" => reply!(model_pricing::update_model_pricing_batch(
            db,
            arg(&args, "entries")?
        )),
        "delete_model_pricing" => reply!(model_pricing::delete_model_pricing(
            db,
            text(&args, "modelId")?
        )),
        "check_provider_limits" => {
            reply!(db.check_provider_limits(text(&args, "providerId")?, text(&args, "appType")?))
        }
        "get_session_usage_last_sync" => {
            output(crate::services::session_usage::last_sync_completed_at())
        }
        "sync_session_usage" => {
            let _guard = crate::runtime::block_on(
                crate::services::session_usage::session_sync_mutex().lock(),
            );
            output(crate::services::session_usage::sync_all_unlocked(db))
        }
        "get_usage_data_sources" => reply!(
            crate::services::session_usage::get_data_source_breakdown(db)
        ),
        "get_custom_icons" => reply!(crate::services::provider::icon::list_library_icons(db)),
        "add_custom_icon" => reply!(crate::services::provider::icon::add_library_icon(
            db,
            text(&args, "name")?,
            text(&args, "data")?
        )),
        "rename_custom_icon" => reply!(crate::services::provider::icon::rename_library_icon(
            db,
            text(&args, "id")?,
            text(&args, "name")?
        )),
        "delete_custom_icon" => reply!(crate::services::provider::icon::delete_library_icon(
            db,
            text(&args, "id")?
        )),
        "get_claude_code_config_path" => wait!(commands::get_claude_code_config_path()),
        "get_config_dir" => wait!(commands::get_config_dir(arg(&args, "app")?)),
        "get_app_config_path" => wait!(commands::get_app_config_path()),
        "check_env_conflicts" => reply!(commands::check_env_conflicts(arg(&args, "app")?)),
        "get_hermes_live_provider_ids" => reply!(commands::get_hermes_live_provider_ids()),
        "get_hermes_live_provider" => reply!(commands::get_hermes_live_provider(arg(
            &args,
            "providerId"
        )?)),
        "get_hermes_model_config" => reply!(commands::get_hermes_model_config()),
        "is_portable_mode" => wait!(commands::is_portable_mode()),
        "get_agy_models" => wait!(commands::get_agy_models(
            arg(&args, "apiKey")?,
            arg(&args, "baseUrl")?
        )),
        "get_opencode_models" => wait!(commands::get_opencode_models()),
        "fetch_models_for_config" => wait!(commands::fetch_models_for_config(
            arg(&args, "baseUrl")?,
            arg(&args, "apiKey")?,
            arg(&args, "isFullUrl")?,
            arg(&args, "modelsUrl")?,
            arg(&args, "customUserAgent")?,
            arg(&args, "apiFormat")?,
            arg(&args, "requestHeaders")?
        )),
        "read_omo_local_file" => wait!(commands::read_omo_local_file()),
        "read_omo_slim_local_file" => wait!(commands::read_omo_slim_local_file()),
        "get_openclaw_live_provider_ids" => reply!(commands::get_openclaw_live_provider_ids()),
        "get_openclaw_live_provider" => reply!(commands::get_openclaw_live_provider(arg(
            &args,
            "providerId"
        )?)),
        "scan_openclaw_config_health" => reply!(commands::scan_openclaw_config_health()),
        "get_openclaw_default_model" => reply!(commands::get_openclaw_default_model()),
        "set_openclaw_default_model" => {
            reply!(commands::set_openclaw_default_model(arg(&args, "model")?))
        }
        "get_openclaw_model_catalog" => reply!(commands::get_openclaw_model_catalog()),
        "get_claude_desktop_default_routes" => {
            output(commands::get_claude_desktop_default_routes())
        }
        "test_api_endpoints" => wait!(commands::test_api_endpoints(
            arg(&args, "urls")?,
            arg(&args, "timeoutSecs")?
        )),
        "get_opencode_live_provider_ids" => reply!(commands::get_opencode_live_provider_ids()),
        "get_opencode_default_model" => reply!(commands::get_opencode_default_model()),
        "set_opencode_default_model" => {
            reply!(commands::set_opencode_default_model(arg(&args, "model")?))
        }
        _ => Err(AppError::InvalidInput(format!(
            "Web 入口尚不支持此操作: {command}"
        ))),
    }
}

/// Start the same session-log synchronizer used by desktop, without a window.
pub fn start_background_tasks(state: &AppState) {
    let db = state.db.clone();
    crate::runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let maintenance_db = db.clone();
            let _ =
                crate::runtime::spawn_blocking(move || maintenance_db.periodic_backup_if_needed())
                    .await;
            if !crate::settings::get_settings().session_auto_sync_enabled {
                continue;
            }
            let _guard = crate::services::session_usage::session_sync_mutex()
                .lock()
                .await;
            let db = db.clone();
            let _ = crate::runtime::spawn_blocking(move || {
                crate::services::session_usage::sync_all_unlocked(&db)
            })
            .await;
        }
    });
}

pub fn install_uploaded_zip(
    state: &AppState,
    path: &std::path::Path,
    app: AppType,
) -> Result<Value, AppError> {
    reply!(SkillService::install_from_zip(&state.db, path, &app))
}
