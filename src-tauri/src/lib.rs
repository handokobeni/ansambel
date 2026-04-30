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
                settings,
            };

            app.manage(std::sync::Arc::new(std::sync::Mutex::new(state)));
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_state_construction_includes_tasks_field() {
        use crate::state::{AppSettings, AppState};
        use std::collections::HashMap;
        // Verify the struct literal compiles with all three entity maps.
        let state = AppState {
            repos: HashMap::new(),
            workspaces: HashMap::new(),
            tasks: HashMap::new(),
            agents: HashMap::new(),
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
            settings: AppSettings::default(),
        };
        assert!(state.agents.is_empty());
    }
}
