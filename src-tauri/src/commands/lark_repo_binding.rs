use crate::error::{AppError, Result};
use crate::state::{BitableBinding, ProposedMapping, TaskProviderHandle};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[tauri::command]
pub async fn get_lark_repo_binding(
    repo_id: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<Option<BitableBinding>, String> {
    let data_dir = data_dir_from(&app_handle)?;
    crate::persistence::lark_repo_bindings::get_binding(&data_dir, &repo_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_lark_repo_binding(
    repo_id: String,
    binding: BitableBinding,
    app_handle: tauri::AppHandle,
    provider_handle: State<'_, TaskProviderHandle>,
) -> std::result::Result<(), String> {
    let data_dir = data_dir_from(&app_handle)?;
    set_lark_repo_binding_inner(
        &repo_id,
        binding,
        &data_dir,
        provider_handle.inner().clone(),
    )
    .await
    .map_err(|e| e.to_string())
}

pub(crate) async fn set_lark_repo_binding_inner(
    repo_id: &str,
    mut binding: BitableBinding,
    data_dir: &std::path::Path,
    handle: TaskProviderHandle,
) -> Result<()> {
    if binding.field_mapping.title.field_id.is_empty() {
        return Err(AppError::InvalidState("title field is required".into()));
    }
    let now = now_unix();
    if binding.created_at == 0 {
        binding.created_at = now;
    }
    binding.updated_at = now;

    crate::persistence::lark_repo_bindings::set_binding(data_dir, repo_id, binding.clone())?;

    let store = crate::commands::lark_auth::KeyringStore;
    let mut cfg = crate::commands::lark_auth::load_lark_config_inner(data_dir, &store)
        .map_err(|e| AppError::InvalidState(format!("global Lark credentials missing: {e}")))?;
    cfg.app_token = binding.app_token.clone();
    cfg.table_id = binding.table_id.clone();
    let client = Arc::new(crate::platform::lark_client::LarkClient::new(cfg));
    let provider: Arc<dyn crate::task_provider::TaskProvider> = Arc::new(
        crate::task_provider::lark::LarkProvider::from_binding(client, binding),
    );

    {
        let mut guard = handle.write().await;
        guard.insert(repo_id.to_string(), provider);
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_lark_repo_binding(
    repo_id: String,
    app_handle: tauri::AppHandle,
    provider_handle: State<'_, TaskProviderHandle>,
) -> std::result::Result<(), String> {
    let data_dir = data_dir_from(&app_handle)?;
    crate::persistence::lark_repo_bindings::delete_binding(&data_dir, &repo_id)
        .map_err(|e| e.to_string())?;
    let mut guard = provider_handle.write().await;
    guard.remove(&repo_id);
    Ok(())
}

#[tauri::command]
pub async fn list_lark_repo_bindings(
    app_handle: tauri::AppHandle,
) -> std::result::Result<std::collections::HashMap<String, BitableBinding>, String> {
    let data_dir = data_dir_from(&app_handle)?;
    let file = crate::persistence::lark_repo_bindings::load_bindings(&data_dir)
        .map_err(|e| e.to_string())?;
    Ok(file.bindings)
}

#[tauri::command]
pub async fn detect_lark_schema(
    app_token: String,
    table_id: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<ProposedMapping, String> {
    let data_dir = data_dir_from(&app_handle)?;
    let store = crate::commands::lark_auth::KeyringStore;
    let mut cfg = crate::commands::lark_auth::load_lark_config_inner(&data_dir, &store)
        .map_err(|e| format!("global Lark credentials missing: {e}"))?;
    cfg.app_token = app_token.clone();
    cfg.table_id = table_id.clone();
    let client = Arc::new(crate::platform::lark_client::LarkClient::new(cfg));
    let detector = crate::task_provider::lark_field_resolver::BitableSchemaDetector::new(client);
    detector
        .propose_mapping(&app_token, &table_id)
        .await
        .map_err(|e| e.to_string())
}

fn data_dir_from(app_handle: &tauri::AppHandle) -> std::result::Result<PathBuf, String> {
    use tauri::Manager;
    app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{FieldMapping, FieldRef, StatusValueMapping};
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn sample_binding() -> BitableBinding {
        BitableBinding {
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            field_mapping: FieldMapping {
                title: FieldRef {
                    field_id: "fld_t".into(),
                    field_name: "Task name".into(),
                },
                description: None,
                status: None,
                order: None,
            },
            status_value_mapping: StatusValueMapping::default(),
            created_at: 0,
            updated_at: 0,
        }
    }

    fn empty_handle() -> TaskProviderHandle {
        Arc::new(tokio::sync::RwLock::new(HashMap::new()))
    }

    #[tokio::test]
    async fn set_binding_rejects_missing_title_field() {
        let tmp = tempdir().unwrap();
        let mut b = sample_binding();
        b.field_mapping.title.field_id = String::new();
        let handle = empty_handle();
        let err = set_lark_repo_binding_inner("repo_x", b, tmp.path(), handle)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("title field is required"));
    }
}
