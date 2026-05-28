use crate::error::Result;
use crate::persistence::settings::save_settings;
use crate::state::AppState;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

#[tauri::command]
pub async fn set_selected_repo(
    repo_id: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    set_selected_repo_inner(repo_id, data_dir, state.inner().clone()).map_err(|e| {
        tracing::error!(error = %e, "set_selected_repo failed");
        e.to_string()
    })
}

#[tauri::command]
pub async fn get_selected_repo(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<Option<String>, String> {
    get_selected_repo_inner(state.inner().clone()).map_err(|e| e.to_string())
}

pub(crate) fn set_selected_repo_inner(
    repo_id: Option<String>,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<()> {
    let settings_snapshot = {
        let mut st = state
            .lock()
            .map_err(|e| crate::error::AppError::Other(e.to_string()))?;
        st.settings.selected_repo_id = repo_id;
        st.settings.clone()
    };
    save_settings(&data_dir, &settings_snapshot)
}

pub(crate) fn get_selected_repo_inner(state: Arc<Mutex<AppState>>) -> Result<Option<String>> {
    let st = state
        .lock()
        .map_err(|e| crate::error::AppError::Other(e.to_string()))?;
    Ok(st.settings.selected_repo_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::settings::load_settings;
    use crate::state::AppSettings;

    fn make_state(initial: AppSettings) -> Arc<Mutex<AppState>> {
        Arc::new(Mutex::new(AppState {
            settings: initial,
            ..AppState::default()
        }))
    }

    #[test]
    fn set_selected_repo_inner_updates_state_and_persists() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();
        let state = make_state(AppSettings::default());

        set_selected_repo_inner(Some("repo_kelola".into()), data_dir.clone(), state.clone())
            .unwrap();

        // In-memory updated
        assert_eq!(
            state.lock().unwrap().settings.selected_repo_id.as_deref(),
            Some("repo_kelola")
        );
        // Round-tripped through disk
        let loaded = load_settings(&data_dir).unwrap();
        assert_eq!(loaded.selected_repo_id.as_deref(), Some("repo_kelola"));
    }

    #[test]
    fn set_selected_repo_inner_with_none_clears_persisted_selection() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();
        let state = make_state(AppSettings {
            selected_repo_id: Some("repo_old".into()),
            ..AppSettings::default()
        });

        set_selected_repo_inner(None, data_dir.clone(), state.clone()).unwrap();

        assert!(state.lock().unwrap().settings.selected_repo_id.is_none());
        let loaded = load_settings(&data_dir).unwrap();
        assert!(loaded.selected_repo_id.is_none());
    }

    #[test]
    fn get_selected_repo_inner_returns_current_settings_value() {
        let state = make_state(AppSettings {
            selected_repo_id: Some("repo_active".into()),
            ..AppSettings::default()
        });

        let got = get_selected_repo_inner(state).unwrap();
        assert_eq!(got.as_deref(), Some("repo_active"));
    }

    #[test]
    fn get_selected_repo_inner_returns_none_for_default_settings() {
        let state = make_state(AppSettings::default());
        assert!(get_selected_repo_inner(state).unwrap().is_none());
    }
}
