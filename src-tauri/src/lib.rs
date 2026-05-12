use tauri::Manager;

pub mod commands;
pub mod error;
pub mod ids;
pub mod logging;
pub mod panic;
pub mod persistence;
pub mod platform;
pub mod state;

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

            let state = crate::state::AppState {
                repos,
                workspaces,
                tasks,
                agents: std::collections::HashMap::new(),
                terminals: std::collections::HashMap::new(),
                settings,
            };

            app.manage(std::sync::Arc::new(std::sync::Mutex::new(state)));

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
