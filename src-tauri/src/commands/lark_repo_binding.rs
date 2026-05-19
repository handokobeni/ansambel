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

pub(crate) async fn detect_lark_schema_inner(
    app_token: &str,
    table_id: &str,
    data_dir: &std::path::Path,
    store: &dyn crate::commands::lark_auth::SecretStore,
) -> Result<ProposedMapping> {
    let mut cfg = crate::commands::lark_auth::load_lark_config_inner(data_dir, store)
        .map_err(|e| AppError::InvalidState(format!("global Lark credentials missing: {e}")))?;
    cfg.app_token = app_token.to_string();
    cfg.table_id = table_id.to_string();
    let client = Arc::new(crate::platform::lark_client::LarkClient::new(cfg));
    let detector = crate::task_provider::lark_field_resolver::BitableSchemaDetector::new(client);
    detector.propose_mapping(app_token, table_id).await
}

#[tauri::command]
pub async fn detect_lark_schema(
    app_token: String,
    table_id: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<ProposedMapping, String> {
    let data_dir = data_dir_from(&app_handle)?;
    let store = crate::commands::lark_auth::KeyringStore;
    detect_lark_schema_inner(&app_token, &table_id, &data_dir, &store)
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn list_lark_fields_inner(
    app_token: &str,
    table_id: &str,
    data_dir: &std::path::Path,
    store: &dyn crate::commands::lark_auth::SecretStore,
) -> Result<Vec<crate::platform::lark_client::BitableField>> {
    let mut cfg = crate::commands::lark_auth::load_lark_config_inner(data_dir, store)
        .map_err(|e| AppError::InvalidState(format!("global Lark credentials missing: {e}")))?;
    cfg.app_token = app_token.to_string();
    cfg.table_id = table_id.to_string();
    let client = Arc::new(crate::platform::lark_client::LarkClient::new(cfg));
    client.bitable_list_fields(app_token, table_id).await
}

#[tauri::command]
pub async fn list_lark_fields(
    app_token: String,
    table_id: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<Vec<crate::platform::lark_client::BitableField>, String> {
    let data_dir = data_dir_from(&app_handle)?;
    let store = crate::commands::lark_auth::KeyringStore;
    list_lark_fields_inner(&app_token, &table_id, &data_dir, &store)
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn list_lark_person_options_inner(
    app_token: &str,
    table_id: &str,
    field_name: &str,
    data_dir: &std::path::Path,
    store: &dyn crate::commands::lark_auth::SecretStore,
) -> Result<Vec<crate::state::PersonOption>> {
    let mut cfg = crate::commands::lark_auth::load_lark_config_inner(data_dir, store)
        .map_err(|e| AppError::InvalidState(format!("global Lark credentials missing: {e}")))?;
    cfg.app_token = app_token.to_string();
    cfg.table_id = table_id.to_string();
    let client = Arc::new(crate::platform::lark_client::LarkClient::new(cfg));

    // Fetch all records to enumerate person values present in the field.
    let records = client
        .bitable_list_records(app_token, table_id, None)
        .await?;

    let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for rec in &records {
        let Some(arr) = rec.fields.get(field_name).and_then(|v| v.as_array()) else {
            continue;
        };
        for person_val in arr {
            // Lark represents Person values as objects. Accept both
            // `{"id": "...", "name": "..."}` and `{"open_id": "...", "name": "..."}` shapes.
            let open_id = person_val
                .get("open_id")
                .or_else(|| person_val.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let name = person_val
                .get("name")
                .or_else(|| person_val.get("english_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !open_id.is_empty() && !name.is_empty() {
                seen.entry(open_id.to_string())
                    .or_insert_with(|| name.to_string());
            }
        }
    }

    let mut options: Vec<crate::state::PersonOption> = seen
        .into_iter()
        .map(|(open_id, name)| crate::state::PersonOption { open_id, name })
        .collect();
    options.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(options)
}

#[tauri::command]
pub async fn list_lark_person_options(
    app_token: String,
    table_id: String,
    field_name: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<Vec<crate::state::PersonOption>, String> {
    let data_dir = data_dir_from(&app_handle)?;
    let store = crate::commands::lark_auth::KeyringStore;
    list_lark_person_options_inner(&app_token, &table_id, &field_name, &data_dir, &store)
        .await
        .map_err(|e| e.to_string())
}

/// Follow a Lookup field's chain to its source SingleSelect and return its options.
///
/// Lark Lookup fields store their source reference in `property`. We've seen
/// these key spellings in the wild:
///   • `target_table` + `target_field` (formula-based Lookup — what Lark v1 uses
///     for the property shape that includes `formula`, `filter_info`, `roll_up`)
///   • `filter_info.target_table` + top-level `target_field` (asymmetric shape —
///     confirmed real-world case where `target_table` is nested inside the
///     `filter_info` object while `target_field` stays at the top level)
///   • `table_id` + `field_id` (simpler Lookup)
///   • `target_table_id` + `link_field_id` (older variant)
/// Try them in that priority order. Fall back to `Ok(vec![])` if none match.
///
/// If the chain is broken at any step (field not found, source not a
/// SingleSelect, options absent) we return `Ok(vec![])` and let the
/// frontend fall back to a text input.
pub(crate) async fn list_lark_lookup_options_inner(
    app_token: &str,
    table_id: &str,
    field_id: &str,
    data_dir: &std::path::Path,
    store: &dyn crate::commands::lark_auth::SecretStore,
) -> Result<Vec<crate::state::SingleSelectOption>> {
    let mut cfg = crate::commands::lark_auth::load_lark_config_inner(data_dir, store)
        .map_err(|e| AppError::InvalidState(format!("global Lark credentials missing: {e}")))?;
    cfg.app_token = app_token.to_string();
    cfg.table_id = table_id.to_string();
    let client = Arc::new(crate::platform::lark_client::LarkClient::new(cfg));

    // Step 1: find the Lookup field in the binding table.
    let fields = client.bitable_list_fields(app_token, table_id).await?;
    let lookup_field = match fields.iter().find(|f| f.field_id == field_id) {
        Some(f) => f,
        None => {
            tracing::warn!(
                field_id = %field_id,
                table_id = %table_id,
                "Phase 3a-3.1 lookup chain: field not found in table"
            );
            return Ok(vec![]);
        }
    };
    // Bitable type 19 = Lookup.
    if lookup_field.field_type != 19 {
        tracing::warn!(
            field_id = %field_id,
            field_type = lookup_field.field_type,
            "Phase 3a-3.1 lookup chain: field is not a Lookup (type 19)"
        );
        return Ok(vec![]);
    }

    // Step 2: extract source table_id and source field_id from property.
    let prop = match &lookup_field.property {
        Some(p) => p,
        None => {
            tracing::warn!(
                field_id = %field_id,
                "Phase 3a-3.1 lookup chain: Lookup field has no property"
            );
            return Ok(vec![]);
        }
    };
    // Accept all known property key spellings, newest/confirmed shape first.
    let src_table_id = prop
        .get("target_table") // top-level formula-based Lookup (confirmed shape)
        .or_else(|| {
            prop.get("filter_info")
                .and_then(|fi| fi.get("target_table"))
        }) // NEW: nested under filter_info
        .or_else(|| prop.get("table_id")) // simpler Lookup
        .or_else(|| prop.get("target_table_id")) // older variant
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let src_field_id = prop
        .get("target_field") // top-level
        .or_else(|| {
            prop.get("filter_info")
                .and_then(|fi| fi.get("target_field"))
        }) // NEW: nested under filter_info (in case it's there too)
        .or_else(|| prop.get("field_id")) // simpler Lookup
        .or_else(|| prop.get("link_field_id")) // older variant
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if src_table_id.is_empty() || src_field_id.is_empty() {
        tracing::warn!(
            field_id = %field_id,
            property = ?prop,
            "Phase 3a-3.1 lookup chain: could not extract src_table_id/src_field_id from property; check property keys"
        );
        return Ok(vec![]);
    }

    // Step 3: fetch fields of the source table.
    let src_fields = client.bitable_list_fields(app_token, src_table_id).await?;
    let src_field = match src_fields.iter().find(|f| f.field_id == src_field_id) {
        Some(f) => f,
        None => {
            tracing::warn!(
                src_table_id = %src_table_id,
                src_field_id = %src_field_id,
                "Phase 3a-3.1 lookup chain: source field not found in source table"
            );
            return Ok(vec![]);
        }
    };
    // Source must be a SingleSelect (type 3).
    if src_field.field_type != 3 {
        tracing::warn!(
            src_field_id = %src_field_id,
            field_type = src_field.field_type,
            "Phase 3a-3.1 lookup chain: source field is not a SingleSelect (type 3)"
        );
        return Ok(vec![]);
    }

    // Step 4: read options from the source field's property.
    let options = src_field.options();
    if options.is_empty() {
        tracing::warn!(
            src_field_id = %src_field_id,
            "Phase 3a-3.1 lookup chain: source SingleSelect field has no options"
        );
    }
    let result = options
        .into_iter()
        .map(|opt| crate::state::SingleSelectOption {
            option_id: opt.id,
            name: opt.name,
        })
        .collect();
    Ok(result)
}

#[tauri::command]
pub async fn list_lark_lookup_options(
    app_token: String,
    table_id: String,
    field_id: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<Vec<crate::state::SingleSelectOption>, String> {
    let data_dir = data_dir_from(&app_handle)?;
    let store = crate::commands::lark_auth::KeyringStore;
    list_lark_lookup_options_inner(&app_token, &table_id, &field_id, &data_dir, &store)
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
            filters: crate::state::FilterSpec::default(),
            field_mapping: FieldMapping {
                title: FieldRef {
                    field_id: "fld_t".into(),
                    field_name: "Task name".into(),
                },
                description: None,
                status: None,
                order: None,
                pic: None,
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

    #[tokio::test]
    async fn list_lark_fields_inner_returns_fields_via_lark_client() {
        use wiremock::matchers::{method, path as wm_path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // Token endpoint.
        Mock::given(method("POST"))
            .and(wm_path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "tenant_access_token": "t_test", "expire": 7200,
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(wm_path(
                "/open-apis/bitable/v1/apps/appA/tables/tblA/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "data": { "items": [
                    { "field_id": "fld1", "field_name": "Title", "type": 1,
                      "is_primary": true, "property": null },
                    { "field_id": "fld2", "field_name": "Status", "type": 3,
                      "is_primary": false, "property": {
                        "options": [{"id":"o1","name":"Todo"},{"id":"o2","name":"Done"}]
                      }
                    }
                ], "has_more": false, "page_token": null }
            })))
            .mount(&server)
            .await;

        let tmp = tempdir().unwrap();
        let store = crate::commands::lark_auth::InMemorySecretStore::new();
        let args = crate::commands::lark_auth::SetLarkCredentialsArgs {
            app_id: "cli_test".into(),
            app_secret: "sec".into(),
            base_url: Some(server.uri()),
        };
        crate::commands::lark_auth::set_lark_credentials_inner(args, tmp.path(), &store).unwrap();

        let fields = list_lark_fields_inner("appA", "tblA", tmp.path(), &store)
            .await
            .expect("ok");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].field_name, "Title");
        assert_eq!(fields[1].field_name, "Status");
    }

    #[tokio::test]
    async fn list_lark_person_options_inner_dedupes_persons_by_open_id() {
        use wiremock::matchers::{method, path as wm_path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(wm_path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "tenant_access_token": "t_test", "expire": 7200,
            })))
            .mount(&server)
            .await;
        // Three records — person "ou_alice" appears in records 1 and 3; "ou_bob" in records 2 and 3.
        Mock::given(method("GET"))
            .and(wm_path(
                "/open-apis/bitable/v1/apps/appA/tables/tblA/records",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "data": {
                    "items": [
                        {
                            "record_id": "rec1",
                            "fields": {
                                "PIC": [{"id": "ou_alice", "name": "Alice"}]
                            }
                        },
                        {
                            "record_id": "rec2",
                            "fields": {
                                "PIC": [{"id": "ou_bob", "name": "Bob"}]
                            }
                        },
                        {
                            "record_id": "rec3",
                            "fields": {
                                "PIC": [
                                    {"id": "ou_alice", "name": "Alice"},
                                    {"id": "ou_bob", "name": "Bob"}
                                ]
                            }
                        }
                    ],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;

        let tmp = tempdir().unwrap();
        let store = crate::commands::lark_auth::InMemorySecretStore::new();
        let args = crate::commands::lark_auth::SetLarkCredentialsArgs {
            app_id: "cli_test".into(),
            app_secret: "sec".into(),
            base_url: Some(server.uri()),
        };
        crate::commands::lark_auth::set_lark_credentials_inner(args, tmp.path(), &store).unwrap();

        let options = list_lark_person_options_inner("appA", "tblA", "PIC", tmp.path(), &store)
            .await
            .expect("ok");

        // Deduped: only Alice and Bob (sorted by name ascending)
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].name, "Alice");
        assert_eq!(options[0].open_id, "ou_alice");
        assert_eq!(options[1].name, "Bob");
        assert_eq!(options[1].open_id, "ou_bob");
    }

    #[tokio::test]
    async fn list_lark_person_options_inner_returns_empty_when_field_absent() {
        use wiremock::matchers::{method, path as wm_path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(wm_path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "tenant_access_token": "t_test", "expire": 7200,
            })))
            .mount(&server)
            .await;
        // Records have no PIC field
        Mock::given(method("GET"))
            .and(wm_path(
                "/open-apis/bitable/v1/apps/appA/tables/tblA/records",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "data": {
                    "items": [
                        { "record_id": "rec1", "fields": { "Title": "Task 1" } },
                        { "record_id": "rec2", "fields": { "Title": "Task 2" } }
                    ],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;

        let tmp = tempdir().unwrap();
        let store = crate::commands::lark_auth::InMemorySecretStore::new();
        let args = crate::commands::lark_auth::SetLarkCredentialsArgs {
            app_id: "cli_test".into(),
            app_secret: "sec".into(),
            base_url: Some(server.uri()),
        };
        crate::commands::lark_auth::set_lark_credentials_inner(args, tmp.path(), &store).unwrap();

        let options = list_lark_person_options_inner("appA", "tblA", "PIC", tmp.path(), &store)
            .await
            .expect("ok");
        assert!(options.is_empty(), "no PIC field → empty Vec");
    }

    #[tokio::test]
    async fn list_lark_lookup_options_inner_follows_chain_to_single_select() {
        use wiremock::matchers::{method, path as wm_path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // Token endpoint.
        Mock::given(method("POST"))
            .and(wm_path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "tenant_access_token": "t_test", "expire": 7200,
            })))
            .mount(&server)
            .await;
        // First call: binding table has a Lookup field (type 19) pointing to
        // the linked table + field.
        Mock::given(method("GET"))
            .and(wm_path(
                "/open-apis/bitable/v1/apps/appA/tables/tblA/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "data": { "items": [
                    {
                        "field_id": "fldLookup",
                        "field_name": "Sprint Status",
                        "type": 19,
                        "is_primary": false,
                        "property": {
                            "table_id": "tblLinked",
                            "field_id": "fldSrc",
                            "rollup_method": "ARRAY"
                        }
                    }
                ], "has_more": false, "page_token": null }
            })))
            .mount(&server)
            .await;
        // Second call: linked table has a SingleSelect field (type 3) with options.
        Mock::given(method("GET"))
            .and(wm_path(
                "/open-apis/bitable/v1/apps/appA/tables/tblLinked/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "data": { "items": [
                    {
                        "field_id": "fldSrc",
                        "field_name": "Status",
                        "type": 3,
                        "is_primary": false,
                        "property": {
                            "options": [
                                {"id": "optDone", "name": "Done"},
                                {"id": "optInProg", "name": "In Progress"},
                                {"id": "optUpcoming", "name": "Upcoming +1"}
                            ]
                        }
                    }
                ], "has_more": false, "page_token": null }
            })))
            .mount(&server)
            .await;

        let tmp = tempdir().unwrap();
        let store = crate::commands::lark_auth::InMemorySecretStore::new();
        let args = crate::commands::lark_auth::SetLarkCredentialsArgs {
            app_id: "cli_test".into(),
            app_secret: "sec".into(),
            base_url: Some(server.uri()),
        };
        crate::commands::lark_auth::set_lark_credentials_inner(args, tmp.path(), &store).unwrap();

        let options =
            list_lark_lookup_options_inner("appA", "tblA", "fldLookup", tmp.path(), &store)
                .await
                .expect("ok");

        assert_eq!(options.len(), 3);
        assert_eq!(options[0].option_id, "optDone");
        assert_eq!(options[0].name, "Done");
        assert_eq!(options[1].option_id, "optInProg");
        assert_eq!(options[1].name, "In Progress");
        assert_eq!(options[2].option_id, "optUpcoming");
        assert_eq!(options[2].name, "Upcoming +1");
    }

    #[tokio::test]
    async fn list_lark_lookup_options_inner_follows_chain_via_target_table_and_target_field() {
        // Lookup property uses target_table + target_field (formula-based Lookup shape).
        // Mocks:
        //  - bitable_list_fields(app_token, "tblBindings") returns the Lookup field
        //    with property = { "target_table": "tblSrc", "target_field": "fldSrcStat",
        //                      "formula": "...", "filter_info": {...}, "roll_up": 0 }
        //  - bitable_list_fields(app_token, "tblSrc") returns a SingleSelect field
        //    with id fldSrcStat and 3 options
        // Asserts: command returns those 3 options.
        use wiremock::matchers::{method, path as wm_path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // Token endpoint.
        Mock::given(method("POST"))
            .and(wm_path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "tenant_access_token": "t_test", "expire": 7200,
            })))
            .mount(&server)
            .await;
        // First call: binding table has a formula-based Lookup field (type 19)
        // using the confirmed property shape: target_table + target_field.
        Mock::given(method("GET"))
            .and(wm_path(
                "/open-apis/bitable/v1/apps/appA/tables/tblBindings/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "data": { "items": [
                    {
                        "field_id": "fldLookupFormula",
                        "field_name": "Sprint Status",
                        "type": 19,
                        "is_primary": false,
                        "property": {
                            "filter_info": {
                                "conditions": {
                                    "children": [{ "field_id": "fldtuN4Zou", "field_type": 1, "operator": "is", "value": null }],
                                    "conjunction": "and"
                                },
                                "target_table": "tblSrc"
                            },
                            "formatter": "",
                            "formula": "bitable::$table[tblSrc].FILTER(CurrentValue.$column[fldtuN4Zou]=bitable::$table[tblBindings].$field[fldwZnRp6y]).$column[fldSrcStat].LISTCOMBINE()",
                            "roll_up": 0,
                            "target_field": "fldSrcStat",
                            "target_table": "tblSrc"
                        }
                    }
                ], "has_more": false, "page_token": null }
            })))
            .mount(&server)
            .await;
        // Second call: source table has a SingleSelect field (type 3) with 3 options.
        Mock::given(method("GET"))
            .and(wm_path(
                "/open-apis/bitable/v1/apps/appA/tables/tblSrc/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "data": { "items": [
                    {
                        "field_id": "fldSrcStat",
                        "field_name": "Status",
                        "type": 3,
                        "is_primary": false,
                        "property": {
                            "options": [
                                {"id": "optNotStarted", "name": "Not Started"},
                                {"id": "optInProgress", "name": "In Progress"},
                                {"id": "optDone", "name": "Done"}
                            ]
                        }
                    }
                ], "has_more": false, "page_token": null }
            })))
            .mount(&server)
            .await;

        let tmp = tempdir().unwrap();
        let store = crate::commands::lark_auth::InMemorySecretStore::new();
        let args = crate::commands::lark_auth::SetLarkCredentialsArgs {
            app_id: "cli_test".into(),
            app_secret: "sec".into(),
            base_url: Some(server.uri()),
        };
        crate::commands::lark_auth::set_lark_credentials_inner(args, tmp.path(), &store).unwrap();

        let options = list_lark_lookup_options_inner(
            "appA",
            "tblBindings",
            "fldLookupFormula",
            tmp.path(),
            &store,
        )
        .await
        .expect("ok");

        assert_eq!(options.len(), 3);
        assert_eq!(options[0].option_id, "optNotStarted");
        assert_eq!(options[0].name, "Not Started");
        assert_eq!(options[1].option_id, "optInProgress");
        assert_eq!(options[1].name, "In Progress");
        assert_eq!(options[2].option_id, "optDone");
        assert_eq!(options[2].name, "Done");
    }

    #[tokio::test]
    async fn list_lark_lookup_options_inner_handles_target_table_nested_in_filter_info() {
        // Lookup property shape where `target_table` is nested under `filter_info`
        // (asymmetric — `target_field` stays top-level). Confirmed real-world shape.
        //
        // Mocks:
        //  - bitable_list_fields(app_token, "tblBindings") returns Lookup field whose
        //    property = {
        //      "filter_info": { "target_table": "tblSrc", "conditions": {...} },
        //      "target_field": "fldSrc",
        //      "formula": "...",
        //      "roll_up": 0
        //    }
        //  - bitable_list_fields(app_token, "tblSrc") returns a SingleSelect field
        //    with id "fldSrc" and 3 options.
        // Asserts: command returns those 3 options.
        use wiremock::matchers::{method, path as wm_path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // Token endpoint.
        Mock::given(method("POST"))
            .and(wm_path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "tenant_access_token": "t_test", "expire": 7200,
            })))
            .mount(&server)
            .await;
        // First call: binding table has a Lookup field (type 19) whose property uses the
        // asymmetric shape: target_table is nested inside filter_info, target_field is top-level.
        Mock::given(method("GET"))
            .and(wm_path(
                "/open-apis/bitable/v1/apps/appA/tables/tblBindings/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "data": { "items": [
                    {
                        "field_id": "fldLookupAsym",
                        "field_name": "Sprint Status",
                        "type": 19,
                        "is_primary": false,
                        "property": {
                            "filter_info": {
                                "conditions": {
                                    "children": [{ "field_id": "fldLink", "field_type": 1, "operator": "is", "value": null }],
                                    "conjunction": "and"
                                },
                                "target_table": "tblSrc"
                            },
                            "target_field": "fldSrc",
                            "formula": "bitable::$table[tblSrc].FILTER(...).$column[fldSrc].LISTCOMBINE()",
                            "roll_up": 0
                        }
                    }
                ], "has_more": false, "page_token": null }
            })))
            .mount(&server)
            .await;
        // Second call: source table has a SingleSelect field (type 3) with 3 options.
        Mock::given(method("GET"))
            .and(wm_path(
                "/open-apis/bitable/v1/apps/appA/tables/tblSrc/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "data": { "items": [
                    {
                        "field_id": "fldSrc",
                        "field_name": "Status",
                        "type": 3,
                        "is_primary": false,
                        "property": {
                            "options": [
                                {"id": "optTodo", "name": "Todo"},
                                {"id": "optInProg", "name": "In Progress"},
                                {"id": "optDone", "name": "Done"}
                            ]
                        }
                    }
                ], "has_more": false, "page_token": null }
            })))
            .mount(&server)
            .await;

        let tmp = tempdir().unwrap();
        let store = crate::commands::lark_auth::InMemorySecretStore::new();
        let args = crate::commands::lark_auth::SetLarkCredentialsArgs {
            app_id: "cli_test".into(),
            app_secret: "sec".into(),
            base_url: Some(server.uri()),
        };
        crate::commands::lark_auth::set_lark_credentials_inner(args, tmp.path(), &store).unwrap();

        let options = list_lark_lookup_options_inner(
            "appA",
            "tblBindings",
            "fldLookupAsym",
            tmp.path(),
            &store,
        )
        .await
        .expect("ok");

        assert_eq!(options.len(), 3);
        assert_eq!(options[0].option_id, "optTodo");
        assert_eq!(options[0].name, "Todo");
        assert_eq!(options[1].option_id, "optInProg");
        assert_eq!(options[1].name, "In Progress");
        assert_eq!(options[2].option_id, "optDone");
        assert_eq!(options[2].name, "Done");
    }

    #[tokio::test]
    async fn list_lark_lookup_options_inner_returns_empty_when_not_a_lookup() {
        use wiremock::matchers::{method, path as wm_path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(wm_path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "tenant_access_token": "t_test", "expire": 7200,
            })))
            .mount(&server)
            .await;
        // Field is type 3 (SingleSelect), NOT type 19 (Lookup).
        Mock::given(method("GET"))
            .and(wm_path(
                "/open-apis/bitable/v1/apps/appA/tables/tblA/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "data": { "items": [
                    {
                        "field_id": "fldNotLookup",
                        "field_name": "Status",
                        "type": 3,
                        "is_primary": false,
                        "property": {
                            "options": [{"id": "o1", "name": "Done"}]
                        }
                    }
                ], "has_more": false, "page_token": null }
            })))
            .mount(&server)
            .await;

        let tmp = tempdir().unwrap();
        let store = crate::commands::lark_auth::InMemorySecretStore::new();
        let args = crate::commands::lark_auth::SetLarkCredentialsArgs {
            app_id: "cli_test".into(),
            app_secret: "sec".into(),
            base_url: Some(server.uri()),
        };
        crate::commands::lark_auth::set_lark_credentials_inner(args, tmp.path(), &store).unwrap();

        let options =
            list_lark_lookup_options_inner("appA", "tblA", "fldNotLookup", tmp.path(), &store)
                .await
                .expect("ok");
        assert!(options.is_empty(), "non-Lookup field must return empty Vec");
    }

    #[tokio::test]
    async fn list_lark_fields_inner_errors_when_creds_missing() {
        let tmp = tempdir().unwrap();
        let store = crate::commands::lark_auth::InMemorySecretStore::new();
        // No settings file + empty store → load_lark_config_inner fails.
        let err = list_lark_fields_inner("appA", "tblA", tmp.path(), &store)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.to_lowercase().contains("config")
                || err.to_lowercase().contains("creds")
                || err.contains("global Lark credentials missing"),
            "expected creds/config error, got: {err}"
        );
    }

    #[tokio::test]
    async fn detect_lark_schema_inner_errors_when_creds_missing() {
        let tmp = tempdir().unwrap();
        let store = crate::commands::lark_auth::InMemorySecretStore::new();
        // No settings file + empty store → load_lark_config_inner fails;
        // detect_lark_schema_inner must wrap that as InvalidState with
        // the "global Lark credentials missing" prefix the wizard expects.
        let err = detect_lark_schema_inner("bascntest", "tbltest", tmp.path(), &store)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("global Lark credentials missing"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn detect_lark_schema_inner_injects_app_token_and_table_id_into_request() {
        use wiremock::matchers::{method, path as wm_path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // Token endpoint — required before any Bitable call.
        Mock::given(method("POST"))
            .and(wm_path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "msg": "ok",
                "tenant_access_token": "t_test", "expire": 7200,
            })))
            .mount(&server)
            .await;
        // bitable_list_fields path — wiremock URL match proves the inner
        // wired the caller's `app_token` + `table_id` into the request,
        // not some leftover from the persisted LarkConfig.
        Mock::given(method("GET"))
            .and(wm_path(
                "/open-apis/bitable/v1/apps/bascn_wizard_token/tables/tbl_wizard_table/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "items": [
                    { "field_id": "fld_p", "field_name": "Task name",
                      "type": 1, "is_primary": true }
                ] }
            })))
            .mount(&server)
            .await;

        // Persist creds with a DIFFERENT app_token/table_id than what
        // detect_lark_schema_inner will be called with — proves the
        // override path is exercised.
        let tmp = tempdir().unwrap();
        let store = crate::commands::lark_auth::InMemorySecretStore::new();
        let args = crate::commands::lark_auth::SetLarkCredentialsArgs {
            app_id: "cli_test".into(),
            app_secret: "shh".into(),
            base_url: Some(server.uri()),
        };
        crate::commands::lark_auth::set_lark_credentials_inner(args, tmp.path(), &store).unwrap();

        let proposed =
            detect_lark_schema_inner("bascn_wizard_token", "tbl_wizard_table", tmp.path(), &store)
                .await
                .unwrap();
        assert_eq!(proposed.fields.len(), 1);
        assert_eq!(proposed.suggested.title.field_id, "fld_p");
    }
}
