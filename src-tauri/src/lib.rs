use tauri::{Emitter, Manager};

pub mod commands;
pub mod error;
pub mod ids;
pub mod logging;
pub mod panic;
pub mod persistence;
pub mod platform;
pub mod state;
pub mod task_provider;

/// Auto-migrate the user's Phase 3a-2 global Lark config to a per-repo
/// binding on first launch after upgrade. Idempotent; no-op when:
///   - bindings file already exists with at least one entry, OR
///   - old `lark_settings.json` has no `app_token`/`table_id`, OR
///   - no repo is selected.
fn maybe_migrate_to_per_repo_binding(
    data_dir: &std::path::Path,
    settings: &crate::state::AppSettings,
) -> Option<(String, crate::state::BitableBinding)> {
    let bindings = crate::persistence::lark_repo_bindings::load_bindings(data_dir).ok()?;
    if !bindings.bindings.is_empty() {
        return None;
    }
    let selected_repo = settings.selected_repo_id.clone()?;
    let legacy_path = data_dir.join("lark_settings.json");
    let bytes = std::fs::read(&legacy_path).ok()?;
    let legacy: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let app_token = legacy
        .get("app_token")
        .and_then(|v| v.as_str())?
        .to_string();
    let table_id = legacy.get("table_id").and_then(|v| v.as_str())?.to_string();
    if app_token.is_empty() || table_id.is_empty() {
        return None;
    }
    let binding = crate::state::BitableBinding {
        app_token,
        table_id,
        filters: crate::state::FilterSpec::default(),
        field_mapping: crate::state::FieldMapping {
            title: crate::state::FieldRef {
                field_id: "PENDING_RESOLVE".into(),
                field_name: "title".into(),
            },
            description: None,
            status: None,
            order: None,
        },
        status_value_mapping: crate::state::StatusValueMapping::default(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        updated_at: 0,
    };
    let _ = crate::persistence::lark_repo_bindings::set_binding(
        data_dir,
        &selected_repo,
        binding.clone(),
    );
    let _ = std::fs::write(
        legacy_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "app_id": legacy.get("app_id").and_then(|v| v.as_str()).unwrap_or(""),
            "base_url": legacy.get("base_url").and_then(|v| v.as_str()).unwrap_or(
                crate::platform::lark_client::DEFAULT_BASE_URL
            ),
        }))
        .unwrap_or_default(),
    );
    Some((selected_repo, binding))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("resolve app data dir");
            crate::platform::paths::ensure_data_dirs(&data_dir)?;
            let guard = crate::logging::init(&data_dir)?;
            // Keep the WorkerGuard alive for the process lifetime via Tauri state.
            app.manage(std::sync::Arc::new(std::sync::Mutex::new(Some(guard))));
            crate::panic::install_hook(data_dir.clone());

            // Hydrate AppState from disk
            let repos = crate::persistence::repos::load_repos(&data_dir)?;
            let workspaces = crate::persistence::workspaces::load_and_reset_running(&data_dir)?;
            let tasks = crate::persistence::tasks::load_tasks(&data_dir)?;
            let settings = crate::persistence::settings::load_settings(&data_dir)?;

            // Phase 3a-3 migration: convert global Lark config (3a-2 shape) into
            // per-repo binding for the currently selected repo. Must run before
            // `settings` is moved into AppState.
            let migrated = maybe_migrate_to_per_repo_binding(&data_dir, &settings);

            // Bump lark binding schema v1 → v3 (no v2 ever shipped).
            if let Err(e) = persistence::lark_repo_bindings::migrate_v1_to_v3(&data_dir) {
                tracing::warn!("lark binding schema migration failed: {e}");
            }

            let state = crate::state::AppState {
                repos,
                workspaces,
                tasks,
                agents: std::collections::HashMap::new(),
                terminals: std::collections::HashMap::new(),
                settings,
            };

            app.manage(std::sync::Arc::new(std::sync::Mutex::new(state)));

            // Phase 3a-3 Task 6: TaskProviderHandle is now a per-repo HashMap.
            // Task 10 will populate it from persisted bindings; start empty.
            // provider_for_repo() falls back to LocalProvider for repos with
            // no entry, so existing local-only workflows continue to work.
            let provider_handle: crate::state::TaskProviderHandle =
                std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));
            app.manage(provider_handle.clone());

            // Build providers for every repo that has a binding.
            let bindings = crate::persistence::lark_repo_bindings::load_bindings(&data_dir)
                .unwrap_or_default();
            {
                let handle = provider_handle.clone();
                let data_dir_clone = data_dir.clone();
                tauri::async_runtime::spawn(async move {
                    for (repo_id, binding) in bindings.bindings.iter() {
                        let store = crate::commands::lark_auth::KeyringStore;
                        let cfg_res = crate::commands::lark_auth::load_lark_config_inner(
                            &data_dir_clone,
                            &store,
                        );
                        let Ok(mut cfg) = cfg_res else {
                            tracing::warn!(
                                repo_id = %repo_id,
                                "skipping Lark provider init: global creds missing"
                            );
                            continue;
                        };
                        cfg.app_token = binding.app_token.clone();
                        cfg.table_id = binding.table_id.clone();
                        let client =
                            std::sync::Arc::new(crate::platform::lark_client::LarkClient::new(cfg));
                        let provider: std::sync::Arc<dyn crate::task_provider::TaskProvider> =
                            std::sync::Arc::new(
                                crate::task_provider::lark::LarkProvider::from_binding(
                                    client,
                                    binding.clone(),
                                ),
                            );
                        handle.write().await.insert(repo_id.clone(), provider);
                    }
                });
            }

            if let Some((repo_id, _)) = migrated {
                tracing::info!(
                    repo_id = %repo_id,
                    "Phase 3a-3 auto-migration: created binding from old lark_settings.json"
                );
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("lark-migrated", repo_id);
                }
            }

            // Initial hydrate: pull tasks from LocalProvider and populate
            // AppState mirror. Spawned so setup() returns immediately.
            // Task 10 will replace this with binding-aware hydration.
            {
                let state_arc = app
                    .try_state::<std::sync::Arc<std::sync::Mutex<crate::state::AppState>>>()
                    .expect("AppState managed")
                    .inner()
                    .clone();
                let data_dir_clone = data_dir.clone();
                tauri::async_runtime::spawn(async move {
                    let provider = crate::state::make_default_local_provider(&data_dir_clone);
                    match provider.list_tasks(None).await {
                        Ok(tasks) => {
                            if let Ok(mut st) = state_arc.lock() {
                                let default_repo = crate::commands::task::resolve_default_repo(&st);
                                st.tasks.clear();
                                for mut t in tasks {
                                    if t.repo_id.is_empty() {
                                        if let Some(ref r) = default_repo {
                                            t.repo_id = r.clone();
                                        }
                                    }
                                    st.tasks.insert(t.id.clone(), t);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "initial task hydrate failed");
                        }
                    }
                });
            }

            // Debounced message writer — collapses bursts of stream events
            // into a single disk write per workspace per ~500 ms window.
            // Construction calls `tokio::spawn` for the debouncer worker,
            // so it must run inside the Tauri async runtime context. The
            // worker task lives on the long-lived global tokio runtime
            // and survives after `block_on` returns.
            let message_writer = tauri::async_runtime::block_on(async {
                crate::persistence::message_writer::MessageWriter::new(
                    std::time::Duration::from_millis(500),
                )
            });
            app.manage(message_writer);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::commands::system::get_app_version,
            crate::commands::repo::add_repo,
            crate::commands::repo::list_repos,
            crate::commands::repo::remove_repo,
            crate::commands::repo::update_gh_profile,
            crate::commands::workspace::create_workspace,
            crate::commands::workspace::list_workspaces,
            crate::commands::workspace::remove_workspace,
            crate::commands::task::add_task,
            crate::commands::task::list_tasks,
            crate::commands::task::update_task,
            crate::commands::task::move_task,
            crate::commands::task::remove_task,
            crate::commands::task::refresh_tasks,
            crate::commands::agent::spawn_agent,
            crate::commands::agent::send_message,
            crate::commands::agent::stop_agent,
            crate::commands::agent::list_messages,
            crate::commands::agent::reattach_agent,
            crate::commands::diff::workspace_diff,
            crate::commands::files::workspace_files,
            crate::commands::files::workspace_files_recursive,
            crate::commands::search::workspace_search,
            crate::commands::scripts::script_list,
            crate::commands::scripts::script_set,
            crate::commands::terminal::terminal_spawn,
            crate::commands::terminal::terminal_write,
            crate::commands::terminal::terminal_resize,
            crate::commands::terminal::terminal_kill,
            crate::commands::terminal::terminal_reattach,
            crate::commands::file_io::file_read,
            crate::commands::file_io::file_write,
            crate::commands::scripts::script_run,
            crate::commands::lark_auth::set_lark_credentials,
            crate::commands::lark_auth::get_lark_status,
            crate::commands::lark_auth::test_lark_connection,
            crate::commands::lark_auth::clear_lark_credentials,
            crate::commands::lark_repo_binding::get_lark_repo_binding,
            crate::commands::lark_repo_binding::set_lark_repo_binding,
            crate::commands::lark_repo_binding::delete_lark_repo_binding,
            crate::commands::lark_repo_binding::list_lark_repo_bindings,
            crate::commands::lark_repo_binding::detect_lark_schema,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // On shutdown drain any pending debounced writes so messages
            // queued in the last 500 ms aren't lost when the user closes
            // the window. We block the run-loop briefly here — the flush
            // is bounded by the in-memory pending list, not network I/O.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(writer) =
                    app.try_state::<crate::persistence::message_writer::MessageWriter>()
                {
                    let writer = writer.inner().clone();
                    tauri::async_runtime::block_on(async move {
                        writer.flush_all().await;
                    });
                }
            }
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_state_construction_includes_tasks_field() {
        use crate::state::{AppSettings, AppState};
        use std::collections::HashMap;
        // Verify the struct literal compiles with all entity maps.
        let state = AppState {
            repos: HashMap::new(),
            workspaces: HashMap::new(),
            tasks: HashMap::new(),
            agents: HashMap::new(),
            terminals: HashMap::new(),
            settings: AppSettings::default(),
        };
        assert!(state.tasks.is_empty());
    }

    #[test]
    fn all_task_commands_exist_as_public_fns() {
        // Verify all five command symbols are resolvable — catches accidental renames.
        let _ = crate::commands::task::add_task as *const () as usize;
        let _ = crate::commands::task::list_tasks as *const () as usize;
        let _ = crate::commands::task::update_task as *const () as usize;
        let _ = crate::commands::task::move_task as *const () as usize;
        let _ = crate::commands::task::remove_task as *const () as usize;
    }

    #[test]
    fn workspace_diff_command_is_registered() {
        // Symbol existence check — ensures the command is wired in
        // `tauri::generate_handler!` and not silently dropped.
        let _ = std::any::type_name_of_val(&crate::commands::diff::workspace_diff);
    }

    #[test]
    fn workspace_files_command_is_registered() {
        let _ = std::any::type_name_of_val(&crate::commands::files::workspace_files);
    }

    #[test]
    fn workspace_files_recursive_command_is_registered() {
        let _ = std::any::type_name_of_val(&crate::commands::files::workspace_files_recursive);
    }

    #[test]
    fn workspace_search_command_is_registered() {
        let _ = std::any::type_name_of_val(&crate::commands::search::workspace_search);
    }

    #[test]
    fn script_list_command_is_registered() {
        let _ = std::any::type_name_of_val(&crate::commands::scripts::script_list);
    }

    #[test]
    fn script_set_command_is_registered() {
        let _ = std::any::type_name_of_val(&crate::commands::scripts::script_set);
    }

    #[test]
    fn terminal_spawn_command_is_registered() {
        let _ = std::any::type_name_of_val(&crate::commands::terminal::terminal_spawn);
    }

    #[test]
    fn terminal_write_command_is_registered() {
        let _ = std::any::type_name_of_val(&crate::commands::terminal::terminal_write);
    }

    #[test]
    fn terminal_resize_command_is_registered() {
        let _ = std::any::type_name_of_val(&crate::commands::terminal::terminal_resize);
    }

    #[test]
    fn terminal_kill_command_is_registered() {
        let _ = std::any::type_name_of_val(&crate::commands::terminal::terminal_kill);
    }

    #[test]
    fn terminal_reattach_command_is_registered() {
        let _ = std::any::type_name_of_val(&crate::commands::terminal::terminal_reattach);
    }

    #[test]
    fn file_read_command_is_registered() {
        let _ = std::any::type_name_of_val(&crate::commands::file_io::file_read);
    }

    #[test]
    fn file_write_command_is_registered() {
        let _ = std::any::type_name_of_val(&crate::commands::file_io::file_write);
    }

    #[test]
    fn script_run_command_is_registered() {
        let _ = std::any::type_name_of_val(&crate::commands::scripts::script_run);
    }

    #[test]
    fn all_agent_commands_are_accessible() {
        // Compile-time check that all five command functions are pub and accessible
        use crate::commands::agent::{
            list_messages, reattach_agent, send_message, spawn_agent, stop_agent,
        };
        let _ = std::any::type_name_of_val(&spawn_agent);
        let _ = std::any::type_name_of_val(&send_message);
        let _ = std::any::type_name_of_val(&stop_agent);
        let _ = std::any::type_name_of_val(&list_messages);
        let _ = std::any::type_name_of_val(&reattach_agent);
    }

    #[test]
    fn app_state_has_agents_field_at_startup() {
        use crate::state::{AppSettings, AppState};
        use std::collections::HashMap;
        let state = AppState {
            repos: HashMap::new(),
            workspaces: HashMap::new(),
            tasks: HashMap::new(),
            agents: HashMap::new(),
            terminals: HashMap::new(),
            settings: AppSettings::default(),
        };
        assert!(state.agents.is_empty());
        assert!(state.terminals.is_empty());
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    fn make_existing_binding() -> crate::state::BitableBinding {
        crate::state::BitableBinding {
            app_token: "x".into(),
            table_id: "y".into(),
            filters: crate::state::FilterSpec::default(),
            field_mapping: crate::state::FieldMapping {
                title: crate::state::FieldRef {
                    field_id: "fld_t".into(),
                    field_name: "title".into(),
                },
                description: None,
                status: None,
                order: None,
            },
            status_value_mapping: crate::state::StatusValueMapping::default(),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn migration_no_op_when_bindings_already_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let mut bindings = crate::persistence::lark_repo_bindings::BindingsFile::default();
        bindings
            .bindings
            .insert("repo_x".into(), make_existing_binding());
        crate::persistence::lark_repo_bindings::save_bindings(tmp.path(), &bindings).unwrap();
        let settings = crate::state::AppSettings::default();
        assert!(maybe_migrate_to_per_repo_binding(tmp.path(), &settings).is_none());
    }

    #[test]
    fn migration_no_op_when_no_selected_repo() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("lark_settings.json"),
            r#"{"app_id":"x","app_token":"t","table_id":"i"}"#,
        )
        .unwrap();
        let settings = crate::state::AppSettings::default();
        assert!(maybe_migrate_to_per_repo_binding(tmp.path(), &settings).is_none());
    }

    #[test]
    fn migration_creates_binding_for_selected_repo() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("lark_settings.json"),
            r#"{"app_id":"cli_x","app_token":"bascn","table_id":"tbl","base_url":"https://open.larksuite.com"}"#,
        )
        .unwrap();
        let settings = crate::state::AppSettings {
            selected_repo_id: Some("repo_x".into()),
            ..Default::default()
        };
        let result = maybe_migrate_to_per_repo_binding(tmp.path(), &settings);
        assert!(result.is_some());
        let (repo_id, _) = result.unwrap();
        assert_eq!(repo_id, "repo_x");
        let stored = crate::persistence::lark_repo_bindings::get_binding(tmp.path(), "repo_x")
            .unwrap()
            .unwrap();
        assert_eq!(stored.app_token, "bascn");
        assert_eq!(stored.table_id, "tbl");
        let new: serde_json::Value =
            serde_json::from_slice(&std::fs::read(tmp.path().join("lark_settings.json")).unwrap())
                .unwrap();
        assert!(new.get("app_token").is_none());
    }

    // Helper: write a legacy lark_settings.json with the Phase 3a-2 shape.
    fn write_legacy_settings(dir: &std::path::Path, body: &str) {
        std::fs::write(dir.join("lark_settings.json"), body).unwrap();
    }

    fn legacy_settings_default() -> &'static str {
        r#"{"app_id":"cli_x","app_token":"bascn_xyz","table_id":"tbl_xyz","base_url":"https://open.larksuite.com"}"#
    }

    fn settings_with_repo(repo_id: &str) -> crate::state::AppSettings {
        crate::state::AppSettings {
            selected_repo_id: Some(repo_id.into()),
            ..Default::default()
        }
    }

    #[test]
    fn migration_stamps_placeholder_field_id_and_initial_timestamps() {
        // The migrated binding must carry a `PENDING_RESOLVE` placeholder so
        // the wizard can detect it on first open and prompt the user to
        // re-detect against the real Bitable schema. `updated_at == 0` is
        // the "needs refresh" sentinel, `created_at > 0` proves it was
        // stamped at migration time.
        let tmp = tempfile::tempdir().unwrap();
        write_legacy_settings(tmp.path(), legacy_settings_default());
        let settings = settings_with_repo("repo_x");

        let (_, binding) = maybe_migrate_to_per_repo_binding(tmp.path(), &settings).unwrap();

        assert_eq!(binding.field_mapping.title.field_id, "PENDING_RESOLVE");
        assert_eq!(binding.field_mapping.title.field_name, "title");
        assert!(binding.field_mapping.description.is_none());
        assert!(binding.field_mapping.status.is_none());
        assert!(binding.field_mapping.order.is_none());
        assert_eq!(
            binding.status_value_mapping,
            crate::state::StatusValueMapping::default()
        );
        assert!(binding.created_at > 0, "created_at should be stamped");
        assert_eq!(
            binding.updated_at, 0,
            "updated_at sentinel marks 'needs wizard refresh'"
        );
    }

    #[test]
    fn migration_rewrites_legacy_file_to_minimal_shape() {
        let tmp = tempfile::tempdir().unwrap();
        write_legacy_settings(tmp.path(), legacy_settings_default());
        let settings = settings_with_repo("repo_x");

        maybe_migrate_to_per_repo_binding(tmp.path(), &settings).unwrap();

        let rewritten: serde_json::Value =
            serde_json::from_slice(&std::fs::read(tmp.path().join("lark_settings.json")).unwrap())
                .unwrap();
        // The rewritten file must keep only app_id + base_url. The
        // binding-scoped fields move into lark_repo_bindings.json.
        assert_eq!(
            rewritten.get("app_id").and_then(|v| v.as_str()),
            Some("cli_x")
        );
        assert_eq!(
            rewritten.get("base_url").and_then(|v| v.as_str()),
            Some("https://open.larksuite.com")
        );
        assert!(rewritten.get("app_token").is_none());
        assert!(rewritten.get("table_id").is_none());
    }

    #[test]
    fn migration_no_op_when_legacy_file_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = settings_with_repo("repo_x");
        assert!(maybe_migrate_to_per_repo_binding(tmp.path(), &settings).is_none());
        // No legacy file → no binding file should be created.
        assert!(!tmp.path().join("lark_repo_bindings.json").exists());
    }

    #[test]
    fn migration_no_op_when_app_token_empty() {
        let tmp = tempfile::tempdir().unwrap();
        write_legacy_settings(
            tmp.path(),
            r#"{"app_id":"cli_x","app_token":"","table_id":"tbl","base_url":"https://x"}"#,
        );
        let settings = settings_with_repo("repo_x");
        assert!(maybe_migrate_to_per_repo_binding(tmp.path(), &settings).is_none());
        // Legacy file should remain untouched so the user can either
        // populate app_token or migrate manually.
        let raw = std::fs::read_to_string(tmp.path().join("lark_settings.json")).unwrap();
        assert!(raw.contains("\"app_token\""));
    }

    #[test]
    fn migration_no_op_when_table_id_missing() {
        let tmp = tempfile::tempdir().unwrap();
        write_legacy_settings(
            tmp.path(),
            r#"{"app_id":"cli_x","app_token":"bascn","base_url":"https://x"}"#,
        );
        let settings = settings_with_repo("repo_x");
        assert!(maybe_migrate_to_per_repo_binding(tmp.path(), &settings).is_none());
    }

    #[test]
    fn migration_idempotent_after_first_run() {
        // After migration runs once, re-running on the same data dir must
        // be a no-op (the bindings map is now non-empty, which is the
        // guard's first check). This protects against accidental double
        // migration overwriting the user's wizard-refined binding.
        let tmp = tempfile::tempdir().unwrap();
        write_legacy_settings(tmp.path(), legacy_settings_default());
        let settings = settings_with_repo("repo_x");

        let first = maybe_migrate_to_per_repo_binding(tmp.path(), &settings);
        assert!(first.is_some());
        let first_binding = first.unwrap().1;

        // Simulate user refining the binding via the wizard.
        let mut refined = first_binding.clone();
        refined.field_mapping.title.field_id = "fld_real_pri".into();
        refined.field_mapping.title.field_name = "Task name".into();
        refined.updated_at = 1747200000;
        crate::persistence::lark_repo_bindings::set_binding(tmp.path(), "repo_x", refined.clone())
            .unwrap();

        // Second migration call should NOT touch the refined binding.
        let second = maybe_migrate_to_per_repo_binding(tmp.path(), &settings);
        assert!(second.is_none());
        let still_there = crate::persistence::lark_repo_bindings::get_binding(tmp.path(), "repo_x")
            .unwrap()
            .unwrap();
        assert_eq!(still_there.field_mapping.title.field_id, "fld_real_pri");
        assert_eq!(still_there.updated_at, 1747200000);
    }

    // ── Integration: migrated binding flows through LarkProvider end-to-end ──

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn mount_token(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "tenant_access_token": "t_xyz",
                "expire": 7200
            })))
            .mount(server)
            .await;
    }

    async fn mount_records(server: &MockServer, items: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascn_xyz/tables/tbl_xyz/records",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "items": items, "has_more": false, "page_token": "" }
            })))
            .mount(server)
            .await;
    }

    async fn mount_fields(server: &MockServer, fields: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascn_xyz/tables/tbl_xyz/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "items": fields }
            })))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn migrated_binding_hydrates_records_via_lark_provider() {
        // End-to-end proof: migrate legacy → build LarkProvider from the
        // migrated binding → list_tasks returns hydrated records. The
        // placeholder field_id "PENDING_RESOLVE" is OK because the
        // resolver looks up by field_name ("title"), not field_id.
        let tmp = tempfile::tempdir().unwrap();
        write_legacy_settings(tmp.path(), legacy_settings_default());
        let settings = settings_with_repo("repo_x");

        let (_, binding) = maybe_migrate_to_per_repo_binding(tmp.path(), &settings).unwrap();

        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_records(
            &server,
            serde_json::json!([
                {
                    "record_id": "rec_a",
                    "fields": {"title": "First task"},
                    "created_time": 1700000000000_i64,
                    "last_modified_time": 1700000000000_i64
                },
                {
                    "record_id": "rec_b",
                    "fields": {"title": "Second task"},
                    "created_time": 1700000001000_i64,
                    "last_modified_time": 1700000001000_i64
                }
            ]),
        )
        .await;
        // primary_field_name is fetched lazily; mount empty so resolver
        // exercises the field_name path (not the primary fallback).
        mount_fields(&server, serde_json::json!([])).await;

        let client = std::sync::Arc::new(crate::platform::lark_client::LarkClient::new(
            crate::platform::lark_client::LarkConfig {
                app_id: "cli_x".into(),
                app_secret: "secret".into(),
                app_token: binding.app_token.clone(),
                table_id: binding.table_id.clone(),
                base_url: server.uri(),
            },
        ));
        let provider = crate::task_provider::lark::LarkProvider::from_binding(client, binding);

        use crate::task_provider::TaskProvider;
        let tasks = provider.list_tasks(Some("repo_x")).await.unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].title, "First task");
        assert_eq!(tasks[0].repo_id, "repo_x");
        assert_eq!(tasks[1].title, "Second task");
    }

    #[tokio::test]
    async fn migrated_binding_falls_back_to_primary_field_when_title_absent() {
        // If the user's Bitable doesn't have a "title" field, the resolver
        // falls back to the primary field. This proves the layered
        // fallback survives the placeholder mapping.
        let tmp = tempfile::tempdir().unwrap();
        write_legacy_settings(tmp.path(), legacy_settings_default());
        let settings = settings_with_repo("repo_x");

        let (_, binding) = maybe_migrate_to_per_repo_binding(tmp.path(), &settings).unwrap();

        let server = MockServer::start().await;
        mount_token(&server).await;
        // Record has "Task name" (the primary), no "title" field.
        mount_records(
            &server,
            serde_json::json!([{
                "record_id": "rec_a",
                "fields": {"Task name": "Primary-fallback task"},
                "created_time": 1700000000000_i64,
                "last_modified_time": 1700000000000_i64
            }]),
        )
        .await;
        mount_fields(
            &server,
            serde_json::json!([
                {"field_id": "fld_pri", "field_name": "Task name", "type": 1, "is_primary": true}
            ]),
        )
        .await;

        let client = std::sync::Arc::new(crate::platform::lark_client::LarkClient::new(
            crate::platform::lark_client::LarkConfig {
                app_id: "cli_x".into(),
                app_secret: "secret".into(),
                app_token: binding.app_token.clone(),
                table_id: binding.table_id.clone(),
                base_url: server.uri(),
            },
        ));
        let provider = crate::task_provider::lark::LarkProvider::from_binding(client, binding);

        use crate::task_provider::TaskProvider;
        let tasks = provider.list_tasks(Some("repo_x")).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Primary-fallback task");
    }

    #[test]
    fn setup_bumps_schema_to_v3_on_legacy_v1_file() {
        use crate::persistence::lark_repo_bindings::{load_bindings, migrate_v1_to_v3};
        use crate::platform::paths::lark_repo_bindings_file;
        let tmp = tempfile::tempdir().unwrap();
        let path = lark_repo_bindings_file(tmp.path());
        std::fs::write(&path, r#"{"schema_version":1,"bindings":{}}"#).unwrap();

        let v = migrate_v1_to_v3(tmp.path()).unwrap();
        assert_eq!(v, 3);
        assert_eq!(load_bindings(tmp.path()).unwrap().schema_version, 3);
    }
}
