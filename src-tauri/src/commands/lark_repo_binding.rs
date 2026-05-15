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

pub(crate) fn get_lark_repo_binding_inner(
    data_dir: &std::path::Path,
    repo_id: &str,
) -> Result<Option<BitableBinding>> {
    crate::persistence::lark_repo_bindings::get_binding(data_dir, repo_id)
}

#[tauri::command]
pub async fn get_lark_repo_binding(
    repo_id: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<Option<BitableBinding>, String> {
    let data_dir = data_dir_from(&app_handle)?;
    get_lark_repo_binding_inner(&data_dir, &repo_id).map_err(|e| e.to_string())
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

pub(crate) async fn delete_lark_repo_binding_inner(
    data_dir: &std::path::Path,
    repo_id: &str,
    handle: TaskProviderHandle,
) -> Result<bool> {
    let removed = crate::persistence::lark_repo_bindings::delete_binding(data_dir, repo_id)?;
    let mut guard = handle.write().await;
    guard.remove(repo_id);
    Ok(removed)
}

#[tauri::command]
pub async fn delete_lark_repo_binding(
    repo_id: String,
    app_handle: tauri::AppHandle,
    provider_handle: State<'_, TaskProviderHandle>,
) -> std::result::Result<(), String> {
    let data_dir = data_dir_from(&app_handle)?;
    delete_lark_repo_binding_inner(&data_dir, &repo_id, provider_handle.inner().clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn list_lark_repo_bindings_inner(
    data_dir: &std::path::Path,
) -> Result<std::collections::HashMap<String, BitableBinding>> {
    let file = crate::persistence::lark_repo_bindings::load_bindings(data_dir)?;
    Ok(file.bindings)
}

#[tauri::command]
pub async fn list_lark_repo_bindings(
    app_handle: tauri::AppHandle,
) -> std::result::Result<std::collections::HashMap<String, BitableBinding>, String> {
    let data_dir = data_dir_from(&app_handle)?;
    list_lark_repo_bindings_inner(&data_dir).map_err(|e| e.to_string())
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

    #[test]
    fn get_binding_returns_none_for_unknown_repo() {
        let tmp = tempdir().unwrap();
        let result = get_lark_repo_binding_inner(tmp.path(), "unknown_repo").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn get_binding_returns_some_for_existing_repo() {
        let tmp = tempdir().unwrap();
        crate::persistence::lark_repo_bindings::set_binding(tmp.path(), "repo_x", sample_binding())
            .unwrap();
        let result = get_lark_repo_binding_inner(tmp.path(), "repo_x").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().app_token, "bascntest");
    }

    #[test]
    fn list_bindings_returns_empty_when_no_file() {
        let tmp = tempdir().unwrap();
        let bindings = list_lark_repo_bindings_inner(tmp.path()).unwrap();
        assert!(bindings.is_empty());
    }

    #[test]
    fn list_bindings_returns_all_persisted() {
        let tmp = tempdir().unwrap();
        crate::persistence::lark_repo_bindings::set_binding(tmp.path(), "repo_a", sample_binding())
            .unwrap();
        let mut b2 = sample_binding();
        b2.app_token = "another_token".into();
        crate::persistence::lark_repo_bindings::set_binding(tmp.path(), "repo_b", b2).unwrap();
        let bindings = list_lark_repo_bindings_inner(tmp.path()).unwrap();
        assert_eq!(bindings.len(), 2);
        assert!(bindings.contains_key("repo_a"));
        assert!(bindings.contains_key("repo_b"));
    }

    #[tokio::test]
    async fn delete_binding_removes_from_disk_and_handle() {
        let tmp = tempdir().unwrap();
        crate::persistence::lark_repo_bindings::set_binding(tmp.path(), "repo_x", sample_binding())
            .unwrap();
        let handle = empty_handle();
        {
            let mut guard = handle.write().await;
            guard.insert(
                "repo_x".to_string(),
                Arc::new(crate::task_provider::local::LocalProvider::new(
                    tmp.path().to_path_buf(),
                )),
            );
        }

        let removed = delete_lark_repo_binding_inner(tmp.path(), "repo_x", handle.clone())
            .await
            .unwrap();
        assert!(removed);

        assert!(get_lark_repo_binding_inner(tmp.path(), "repo_x")
            .unwrap()
            .is_none());
        assert!(!handle.read().await.contains_key("repo_x"));
    }

    #[tokio::test]
    async fn delete_binding_returns_false_when_absent() {
        let tmp = tempdir().unwrap();
        let handle = empty_handle();
        let removed = delete_lark_repo_binding_inner(tmp.path(), "missing_repo", handle)
            .await
            .unwrap();
        assert!(!removed);
    }
}
