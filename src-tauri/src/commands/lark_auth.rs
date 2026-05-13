// Phase 3a-1 — Tauri command surface for managing Lark credentials.
//
// Splits the four fields across two stores:
//   * `app_id`, `app_token`, `table_id`, `base_url`  → JSON on disk
//     (`lark_settings.json` in the Tauri app data dir).
//   * `app_secret`                                   → OS keyring under
//     service `ansambel.lark`, account `default`.
//
// Hard rule (project): NEVER log the secret or the resulting
// `tenant_access_token`. The status returned to the frontend reports
// only whether a secret is present; the secret itself never crosses
// the IPC boundary outbound.
//
// Testability: keyring access is abstracted behind `SecretStore` so unit
// tests can use an in-memory map (`InMemorySecretStore`). CI runners
// frequently have no usable OS keyring, so every command-level test in
// this module uses the in-memory implementation.

use crate::error::{AppError, Result};
use crate::persistence::atomic::{load_or_default, write_atomic};
use crate::platform::lark_client::{LarkClient, LarkConfig, DEFAULT_BASE_URL};
use crate::platform::paths::lark_settings_file;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use tauri::Manager;

/// Keyring service identifier — picked to be unambiguous across other
/// apps that might use the same OS keyring.
pub const LARK_KEYRING_SERVICE: &str = "ansambel.lark";

/// We store a single tenant credential per app install — the
/// per-account separation that a desktop keyring allows is not useful
/// here. Pinning to "default" keeps the lookup deterministic.
pub const LARK_KEYRING_ACCOUNT: &str = "default";

/// Persisted (non-secret) part of the Lark credential set.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LarkSettings {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub app_id: Option<String>,
    #[serde(default)]
    pub app_token: Option<String>,
    #[serde(default)]
    pub table_id: Option<String>,
    #[serde(default = "default_base_url_string")]
    pub base_url: String,
}

impl Default for LarkSettings {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            app_id: None,
            app_token: None,
            table_id: None,
            base_url: default_base_url_string(),
        }
    }
}

fn default_schema_version() -> u32 {
    1
}

fn default_base_url_string() -> String {
    DEFAULT_BASE_URL.to_string()
}

/// What the frontend sees when it asks "are we configured?". The
/// secret is never returned — only a boolean for whether one is in the
/// keyring. `configured` is true only when all four required fields are
/// populated (the three plain-text fields plus the secret).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LarkStatus {
    pub configured: bool,
    pub app_id: Option<String>,
    pub app_token: Option<String>,
    pub table_id: Option<String>,
    pub base_url: String,
    pub has_secret: bool,
}

/// What the frontend posts when configuring. `base_url` is optional —
/// callers leave it unset for the international endpoint.
#[derive(Deserialize, Debug, Clone)]
pub struct SetLarkCredentialsArgs {
    pub app_id: String,
    pub app_secret: String,
    pub app_token: String,
    pub table_id: String,
    #[serde(default)]
    pub base_url: Option<String>,
}

/// Optional credentials passed by the frontend when the user clicks
/// Test Connection BEFORE Save — so they can verify form input without
/// committing to keyring/disk. When any required field is missing or
/// empty, we fall back to the stored config; that way the saved-state
/// "click Test on the loaded form" UX keeps working unchanged.
#[derive(Deserialize, Debug, Clone, Default)]
pub struct TestConnectionOverride {
    #[serde(default)]
    pub app_id: Option<String>,
    #[serde(default)]
    pub app_secret: Option<String>,
    #[serde(default)]
    pub app_token: Option<String>,
    #[serde(default)]
    pub table_id: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

impl TestConnectionOverride {
    /// Build a `LarkConfig` from override fields when all four required
    /// values are present and non-empty. Returns `None` to signal "fall
    /// back to stored credentials".
    pub fn to_config(&self) -> Option<LarkConfig> {
        let app_id = self.app_id.as_deref()?.trim();
        let app_secret = self.app_secret.as_deref()?.trim();
        let app_token = self.app_token.as_deref()?.trim();
        let table_id = self.table_id.as_deref()?.trim();
        if app_id.is_empty() || app_secret.is_empty() || app_token.is_empty() || table_id.is_empty()
        {
            return None;
        }
        let base_url = self
            .base_url
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(default_base_url_string);
        Some(LarkConfig {
            app_id: app_id.to_string(),
            app_secret: app_secret.to_string(),
            app_token: app_token.to_string(),
            table_id: table_id.to_string(),
            base_url,
        })
    }
}

/// Pluggable secret store so unit tests can run without a real OS
/// keyring. The real impl wraps `keyring::Entry`.
pub trait SecretStore: Send + Sync {
    fn set(&self, service: &str, account: &str, value: &str) -> Result<()>;
    fn get(&self, service: &str, account: &str) -> Result<Option<String>>;
    fn delete(&self, service: &str, account: &str) -> Result<()>;
}

/// Production secret store backed by the OS keyring (libsecret on
/// Linux, Keychain on macOS, Credential Manager on Windows).
pub struct KeyringStore;

impl SecretStore for KeyringStore {
    fn set(&self, service: &str, account: &str, value: &str) -> Result<()> {
        let entry = keyring::Entry::new(service, account)
            .map_err(|e| AppError::Lark(format!("keyring entry: {e}")))?;
        entry
            .set_password(value)
            .map_err(|e| AppError::Lark(format!("keyring set: {e}")))?;
        Ok(())
    }
    fn get(&self, service: &str, account: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(service, account)
            .map_err(|e| AppError::Lark(format!("keyring entry: {e}")))?;
        match entry.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::Lark(format!("keyring get: {e}"))),
        }
    }
    fn delete(&self, service: &str, account: &str) -> Result<()> {
        let entry = keyring::Entry::new(service, account)
            .map_err(|e| AppError::Lark(format!("keyring entry: {e}")))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            // delete is idempotent — clearing twice must not surface
            // as an error to the user.
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::Lark(format!("keyring delete: {e}"))),
        }
    }
}

/// In-memory secret store for tests. Thread-safe so it can be shared
/// across async tasks; entries are keyed by `(service, account)`.
pub struct InMemorySecretStore {
    inner: Mutex<HashMap<(String, String), String>>,
}

impl InMemorySecretStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemorySecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for InMemorySecretStore {
    fn set(&self, service: &str, account: &str, value: &str) -> Result<()> {
        let mut g = self
            .inner
            .lock()
            .map_err(|e| AppError::Lark(format!("in-memory store poisoned: {e}")))?;
        g.insert(
            (service.to_string(), account.to_string()),
            value.to_string(),
        );
        Ok(())
    }
    fn get(&self, service: &str, account: &str) -> Result<Option<String>> {
        let g = self
            .inner
            .lock()
            .map_err(|e| AppError::Lark(format!("in-memory store poisoned: {e}")))?;
        Ok(g.get(&(service.to_string(), account.to_string())).cloned())
    }
    fn delete(&self, service: &str, account: &str) -> Result<()> {
        let mut g = self
            .inner
            .lock()
            .map_err(|e| AppError::Lark(format!("in-memory store poisoned: {e}")))?;
        g.remove(&(service.to_string(), account.to_string()));
        Ok(())
    }
}

fn require_nonempty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(AppError::Lark(format!("{field} must not be empty")))
    } else {
        Ok(())
    }
}

// ── Inner (pure) helpers ─────────────────────────────────────────────

pub fn set_lark_credentials_inner(
    args: SetLarkCredentialsArgs,
    data_dir: &Path,
    store: &dyn SecretStore,
) -> Result<LarkStatus> {
    require_nonempty("app_id", &args.app_id)?;
    require_nonempty("app_secret", &args.app_secret)?;
    require_nonempty("app_token", &args.app_token)?;
    require_nonempty("table_id", &args.table_id)?;
    let base_url = args
        .base_url
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(default_base_url_string);

    let settings = LarkSettings {
        schema_version: default_schema_version(),
        app_id: Some(args.app_id),
        app_token: Some(args.app_token),
        table_id: Some(args.table_id),
        base_url,
    };
    write_atomic(&lark_settings_file(data_dir), &settings)?;
    store.set(LARK_KEYRING_SERVICE, LARK_KEYRING_ACCOUNT, &args.app_secret)?;

    Ok(status_from_settings(&settings, true))
}

pub fn get_lark_status_inner(data_dir: &Path, store: &dyn SecretStore) -> Result<LarkStatus> {
    let settings: LarkSettings = load_or_default(&lark_settings_file(data_dir))?;
    let has_secret = store
        .get(LARK_KEYRING_SERVICE, LARK_KEYRING_ACCOUNT)?
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    Ok(status_from_settings(&settings, has_secret))
}

pub fn clear_lark_credentials_inner(data_dir: &Path, store: &dyn SecretStore) -> Result<()> {
    store.delete(LARK_KEYRING_SERVICE, LARK_KEYRING_ACCOUNT)?;
    // Write a default (empty) settings file rather than deleting it —
    // keeps the on-disk shape predictable and lets `load_or_default`
    // round-trip via the same path.
    write_atomic(&lark_settings_file(data_dir), &LarkSettings::default())?;
    Ok(())
}

/// Load the full config (settings + secret) into a `LarkConfig` ready
/// to construct a `LarkClient`. Returns `NotFound` if any required
/// field is missing — callers should surface this as "not configured"
/// in the UI, not as a hard error.
pub fn load_lark_config_inner(data_dir: &Path, store: &dyn SecretStore) -> Result<LarkConfig> {
    let settings: LarkSettings = load_or_default(&lark_settings_file(data_dir))?;
    let secret = store
        .get(LARK_KEYRING_SERVICE, LARK_KEYRING_ACCOUNT)?
        .ok_or_else(|| AppError::NotFound("Lark app_secret".into()))?;
    let app_id = settings
        .app_id
        .ok_or_else(|| AppError::NotFound("Lark app_id".into()))?;
    let app_token = settings
        .app_token
        .ok_or_else(|| AppError::NotFound("Lark app_token".into()))?;
    let table_id = settings
        .table_id
        .ok_or_else(|| AppError::NotFound("Lark table_id".into()))?;
    Ok(LarkConfig {
        app_id,
        app_secret: secret,
        app_token,
        table_id,
        base_url: settings.base_url,
    })
}

pub async fn test_lark_connection_inner(
    override_cfg: TestConnectionOverride,
    data_dir: &Path,
    store: &dyn SecretStore,
) -> Result<()> {
    let cfg = match override_cfg.to_config() {
        Some(c) => c,
        None => load_lark_config_inner(data_dir, store)?,
    };
    let client = LarkClient::new(cfg);
    // tenant_access_token() round-trips the credentials against the
    // real Lark API. Discarding the returned String here is deliberate
    // — we never log or surface it.
    let _token = client.tenant_access_token().await?;
    Ok(())
}

fn status_from_settings(s: &LarkSettings, has_secret: bool) -> LarkStatus {
    let configured = has_secret
        && s.app_id.as_deref().is_some_and(|v| !v.is_empty())
        && s.app_token.as_deref().is_some_and(|v| !v.is_empty())
        && s.table_id.as_deref().is_some_and(|v| !v.is_empty());
    LarkStatus {
        configured,
        app_id: s.app_id.clone(),
        app_token: s.app_token.clone(),
        table_id: s.table_id.clone(),
        base_url: s.base_url.clone(),
        has_secret,
    }
}

// ── Tauri command surface ────────────────────────────────────────────

#[tauri::command]
pub async fn set_lark_credentials(
    app_id: String,
    app_secret: String,
    app_token: String,
    table_id: String,
    base_url: Option<String>,
    app: tauri::AppHandle,
) -> std::result::Result<LarkStatus, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let store = KeyringStore;
    let args = SetLarkCredentialsArgs {
        app_id,
        app_secret,
        app_token,
        table_id,
        base_url,
    };
    set_lark_credentials_inner(args, &data_dir, &store).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_lark_status(app: tauri::AppHandle) -> std::result::Result<LarkStatus, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let store = KeyringStore;
    get_lark_status_inner(&data_dir, &store).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_lark_connection(
    app_id: Option<String>,
    app_secret: Option<String>,
    app_token: Option<String>,
    table_id: Option<String>,
    base_url: Option<String>,
    app: tauri::AppHandle,
) -> std::result::Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let store = KeyringStore;
    let override_cfg = TestConnectionOverride {
        app_id,
        app_secret,
        app_token,
        table_id,
        base_url,
    };
    test_lark_connection_inner(override_cfg, &data_dir, &store)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_lark_credentials(app: tauri::AppHandle) -> std::result::Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let store = KeyringStore;
    clear_lark_credentials_inner(&data_dir, &store).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn verify_lark_schema(
    app: tauri::AppHandle,
) -> std::result::Result<crate::task_provider::schema::SchemaCheckResult, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let store = KeyringStore;
    verify_lark_schema_inner(&data_dir, &store)
        .await
        .map_err(|e| e.to_string())
}

pub async fn verify_lark_schema_inner(
    data_dir: &std::path::Path,
    store: &dyn SecretStore,
) -> crate::error::Result<crate::task_provider::schema::SchemaCheckResult> {
    let cfg = load_lark_config_inner(data_dir, store)?;
    let app_token = cfg.app_token.clone();
    let table_id = cfg.table_id.clone();
    let client = crate::platform::lark_client::LarkClient::new(cfg);
    crate::task_provider::schema::verify_schema(
        &client,
        &app_token,
        &table_id,
        &crate::task_provider::schema::required_fields_phase_3a2(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_args() -> SetLarkCredentialsArgs {
        SetLarkCredentialsArgs {
            app_id: "cli_test".into(),
            app_secret: "shh_secret".into(),
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            base_url: None,
        }
    }

    #[test]
    fn lark_settings_default_uses_international_base_url() {
        let s = LarkSettings::default();
        assert_eq!(s.schema_version, 1);
        assert_eq!(s.base_url, DEFAULT_BASE_URL);
        assert!(s.app_id.is_none());
        assert!(s.app_token.is_none());
        assert!(s.table_id.is_none());
    }

    #[test]
    fn lark_settings_round_trips_json_without_secret_field() {
        let s = LarkSettings {
            schema_version: 1,
            app_id: Some("cli_x".into()),
            app_token: Some("bascn_y".into()),
            table_id: Some("tbl_z".into()),
            base_url: "https://open.feishu.cn".into(),
        };
        let j = serde_json::to_string(&s).unwrap();
        assert!(!j.contains("app_secret"), "secret leaked to JSON: {j}");
        let back: LarkSettings = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn in_memory_store_round_trip() {
        let store = InMemorySecretStore::new();
        assert!(store.get("svc", "acct").unwrap().is_none());
        store.set("svc", "acct", "value").unwrap();
        assert_eq!(store.get("svc", "acct").unwrap().as_deref(), Some("value"));
        store.delete("svc", "acct").unwrap();
        assert!(store.get("svc", "acct").unwrap().is_none());
    }

    #[test]
    fn in_memory_store_delete_is_idempotent() {
        let store = InMemorySecretStore::new();
        store.delete("svc", "missing").unwrap();
        store.delete("svc", "missing").unwrap();
    }

    #[test]
    fn set_persists_settings_and_secret() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        let status = set_lark_credentials_inner(make_args(), tmp.path(), &store).unwrap();
        assert!(status.configured);
        assert!(status.has_secret);
        assert_eq!(status.app_id.as_deref(), Some("cli_test"));
        // Secret is in the store, not the settings file.
        let on_disk = std::fs::read_to_string(lark_settings_file(tmp.path())).unwrap();
        assert!(
            !on_disk.contains("shh_secret"),
            "secret leaked to disk: {on_disk}"
        );
        assert_eq!(
            store
                .get(LARK_KEYRING_SERVICE, LARK_KEYRING_ACCOUNT)
                .unwrap()
                .as_deref(),
            Some("shh_secret")
        );
    }

    #[test]
    fn set_uses_default_base_url_when_none_provided() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        let status = set_lark_credentials_inner(make_args(), tmp.path(), &store).unwrap();
        assert_eq!(status.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn set_honours_custom_base_url() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        let mut args = make_args();
        args.base_url = Some("https://open.feishu.cn".into());
        let status = set_lark_credentials_inner(args, tmp.path(), &store).unwrap();
        assert_eq!(status.base_url, "https://open.feishu.cn");
    }

    #[test]
    fn set_treats_blank_base_url_as_unset() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        let mut args = make_args();
        args.base_url = Some("   ".into());
        let status = set_lark_credentials_inner(args, tmp.path(), &store).unwrap();
        assert_eq!(status.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn set_rejects_empty_app_id() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        let mut args = make_args();
        args.app_id = "  ".into();
        let err = set_lark_credentials_inner(args, tmp.path(), &store).unwrap_err();
        assert!(err.to_string().contains("app_id"), "{err}");
    }

    #[test]
    fn set_rejects_empty_app_secret() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        let mut args = make_args();
        args.app_secret = "".into();
        let err = set_lark_credentials_inner(args, tmp.path(), &store).unwrap_err();
        assert!(err.to_string().contains("app_secret"), "{err}");
    }

    #[test]
    fn set_rejects_empty_app_token() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        let mut args = make_args();
        args.app_token = "".into();
        let err = set_lark_credentials_inner(args, tmp.path(), &store).unwrap_err();
        assert!(err.to_string().contains("app_token"), "{err}");
    }

    #[test]
    fn set_rejects_empty_table_id() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        let mut args = make_args();
        args.table_id = "".into();
        let err = set_lark_credentials_inner(args, tmp.path(), &store).unwrap_err();
        assert!(err.to_string().contains("table_id"), "{err}");
    }

    #[test]
    fn get_status_when_nothing_configured() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        let status = get_lark_status_inner(tmp.path(), &store).unwrap();
        assert!(!status.configured);
        assert!(!status.has_secret);
        assert_eq!(status.app_id, None);
        assert_eq!(status.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn get_status_after_set_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        set_lark_credentials_inner(make_args(), tmp.path(), &store).unwrap();
        let status = get_lark_status_inner(tmp.path(), &store).unwrap();
        assert!(status.configured);
        assert!(status.has_secret);
        assert_eq!(status.app_id.as_deref(), Some("cli_test"));
    }

    #[test]
    fn get_status_reports_unconfigured_when_secret_missing_but_settings_present() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        set_lark_credentials_inner(make_args(), tmp.path(), &store).unwrap();
        // Manually delete the secret — simulates an OS keyring eviction
        // or a clear() that races with disk persistence.
        store
            .delete(LARK_KEYRING_SERVICE, LARK_KEYRING_ACCOUNT)
            .unwrap();
        let status = get_lark_status_inner(tmp.path(), &store).unwrap();
        assert!(!status.configured);
        assert!(!status.has_secret);
        // The non-secret fields are still visible — the UI shows the
        // form populated but flags that the secret needs re-entering.
        assert_eq!(status.app_id.as_deref(), Some("cli_test"));
    }

    #[test]
    fn clear_removes_settings_and_secret() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        set_lark_credentials_inner(make_args(), tmp.path(), &store).unwrap();
        clear_lark_credentials_inner(tmp.path(), &store).unwrap();
        let status = get_lark_status_inner(tmp.path(), &store).unwrap();
        assert!(!status.configured);
        assert!(!status.has_secret);
        assert_eq!(status.app_id, None);
    }

    #[test]
    fn clear_is_idempotent_when_nothing_persisted() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        clear_lark_credentials_inner(tmp.path(), &store).unwrap();
        clear_lark_credentials_inner(tmp.path(), &store).unwrap();
    }

    #[test]
    fn load_lark_config_errors_when_secret_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        // Write a settings file but skip the secret.
        let s = LarkSettings {
            schema_version: 1,
            app_id: Some("cli".into()),
            app_token: Some("bascn".into()),
            table_id: Some("tbl".into()),
            base_url: DEFAULT_BASE_URL.to_string(),
        };
        write_atomic(&lark_settings_file(tmp.path()), &s).unwrap();
        let err = load_lark_config_inner(tmp.path(), &store).unwrap_err();
        assert!(err.to_string().contains("app_secret"), "{err}");
    }

    #[test]
    fn load_lark_config_errors_when_app_id_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        store
            .set(LARK_KEYRING_SERVICE, LARK_KEYRING_ACCOUNT, "shh")
            .unwrap();
        // Settings file missing app_id but present for others.
        let s = LarkSettings {
            schema_version: 1,
            app_id: None,
            app_token: Some("bascn".into()),
            table_id: Some("tbl".into()),
            base_url: DEFAULT_BASE_URL.to_string(),
        };
        write_atomic(&lark_settings_file(tmp.path()), &s).unwrap();
        let err = load_lark_config_inner(tmp.path(), &store).unwrap_err();
        assert!(err.to_string().contains("app_id"), "{err}");
    }

    #[test]
    fn load_lark_config_errors_when_app_token_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        store
            .set(LARK_KEYRING_SERVICE, LARK_KEYRING_ACCOUNT, "shh")
            .unwrap();
        let s = LarkSettings {
            schema_version: 1,
            app_id: Some("cli".into()),
            app_token: None,
            table_id: Some("tbl".into()),
            base_url: DEFAULT_BASE_URL.to_string(),
        };
        write_atomic(&lark_settings_file(tmp.path()), &s).unwrap();
        let err = load_lark_config_inner(tmp.path(), &store).unwrap_err();
        assert!(err.to_string().contains("app_token"), "{err}");
    }

    #[test]
    fn load_lark_config_errors_when_table_id_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        store
            .set(LARK_KEYRING_SERVICE, LARK_KEYRING_ACCOUNT, "shh")
            .unwrap();
        let s = LarkSettings {
            schema_version: 1,
            app_id: Some("cli".into()),
            app_token: Some("bascn".into()),
            table_id: None,
            base_url: DEFAULT_BASE_URL.to_string(),
        };
        write_atomic(&lark_settings_file(tmp.path()), &s).unwrap();
        let err = load_lark_config_inner(tmp.path(), &store).unwrap_err();
        assert!(err.to_string().contains("table_id"), "{err}");
    }

    #[test]
    fn load_lark_config_returns_full_config_when_present() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        set_lark_credentials_inner(make_args(), tmp.path(), &store).unwrap();
        let cfg = load_lark_config_inner(tmp.path(), &store).unwrap();
        assert_eq!(cfg.app_id, "cli_test");
        assert_eq!(cfg.app_secret, "shh_secret");
        assert_eq!(cfg.app_token, "bascntest");
        assert_eq!(cfg.table_id, "tbltest");
        assert_eq!(cfg.base_url, DEFAULT_BASE_URL);
    }

    #[tokio::test]
    async fn test_connection_succeeds_against_mock_lark() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok",
                "tenant_access_token": "t_real",
                "expire": 7200,
            })))
            .mount(&server)
            .await;
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        let mut args = make_args();
        args.base_url = Some(server.uri());
        set_lark_credentials_inner(args, tmp.path(), &store).unwrap();
        test_lark_connection_inner(TestConnectionOverride::default(), tmp.path(), &store)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_connection_surfaces_error_when_unconfigured() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        let err = test_lark_connection_inner(TestConnectionOverride::default(), tmp.path(), &store)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("app_secret"), "{err}");
    }

    #[tokio::test]
    async fn test_connection_surfaces_bad_credentials() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 99991663,
                "msg": "app not found",
            })))
            .mount(&server)
            .await;
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        let mut args = make_args();
        args.base_url = Some(server.uri());
        set_lark_credentials_inner(args, tmp.path(), &store).unwrap();
        let err = test_lark_connection_inner(TestConnectionOverride::default(), tmp.path(), &store)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("app not found"), "{err}");
    }

    #[tokio::test]
    async fn test_connection_uses_override_when_provided() {
        // Nothing in keyring/disk. Override passed in directly succeeds.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "tenant_access_token": "t_override",
                "expire": 7200,
            })))
            .mount(&server)
            .await;
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        let override_cfg = TestConnectionOverride {
            app_id: Some("cli_override".into()),
            app_secret: Some("secret_override".into()),
            app_token: Some("bascn_override".into()),
            table_id: Some("tbl_override".into()),
            base_url: Some(server.uri()),
        };
        test_lark_connection_inner(override_cfg, tmp.path(), &store)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_connection_falls_back_to_stored_when_override_incomplete() {
        // Partial override (missing app_secret) → fall back to stored.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "tenant_access_token": "t_stored",
                "expire": 7200,
            })))
            .mount(&server)
            .await;
        let tmp = tempfile::tempdir().unwrap();
        let store = InMemorySecretStore::new();
        let mut args = make_args();
        args.base_url = Some(server.uri());
        set_lark_credentials_inner(args, tmp.path(), &store).unwrap();
        let partial = TestConnectionOverride {
            app_id: Some("cli_x".into()),
            app_secret: None,
            app_token: Some("bascn".into()),
            table_id: Some("tbl".into()),
            base_url: None,
        };
        test_lark_connection_inner(partial, tmp.path(), &store)
            .await
            .unwrap();
    }

    #[test]
    fn test_connection_override_to_config_returns_none_when_any_field_missing() {
        let mut o = TestConnectionOverride {
            app_id: Some("cli".into()),
            app_secret: Some("s".into()),
            app_token: Some("b".into()),
            table_id: Some("t".into()),
            base_url: None,
        };
        assert!(o.to_config().is_some());
        for clear in [
            |o: &mut TestConnectionOverride| o.app_id = None,
            |o: &mut TestConnectionOverride| o.app_secret = None,
            |o: &mut TestConnectionOverride| o.app_token = None,
            |o: &mut TestConnectionOverride| o.table_id = None,
        ] {
            let mut snapshot = o.clone();
            clear(&mut snapshot);
            assert!(
                snapshot.to_config().is_none(),
                "expected None: {snapshot:?}"
            );
        }
        // Blank string for any field also counts as missing.
        o.app_id = Some("   ".into());
        assert!(o.to_config().is_none());
    }

    #[test]
    fn test_connection_override_to_config_trims_and_defaults_base_url() {
        let o = TestConnectionOverride {
            app_id: Some("  cli  ".into()),
            app_secret: Some("  s  ".into()),
            app_token: Some(" b ".into()),
            table_id: Some(" t ".into()),
            base_url: Some("   ".into()),
        };
        let cfg = o.to_config().unwrap();
        assert_eq!(cfg.app_id, "cli");
        assert_eq!(cfg.app_secret, "s");
        assert_eq!(cfg.app_token, "b");
        assert_eq!(cfg.table_id, "t");
        assert_eq!(cfg.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn lark_status_round_trips_json() {
        let s = LarkStatus {
            configured: true,
            app_id: Some("cli".into()),
            app_token: Some("bascn".into()),
            table_id: Some("tbl".into()),
            base_url: DEFAULT_BASE_URL.to_string(),
            has_secret: true,
        };
        let j = serde_json::to_string(&s).unwrap();
        assert!(!j.contains("secret_value"), "{j}");
        let back: LarkStatus = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn status_from_settings_marks_unconfigured_when_app_id_blank() {
        let s = LarkSettings {
            schema_version: 1,
            app_id: Some(String::new()),
            app_token: Some("bascn".into()),
            table_id: Some("tbl".into()),
            base_url: DEFAULT_BASE_URL.to_string(),
        };
        let status = status_from_settings(&s, true);
        assert!(!status.configured);
    }

    #[test]
    fn keyring_store_is_constructable() {
        // Construction only — calling set/get on a real OS keyring is
        // unreliable in CI runners. The InMemorySecretStore tests above
        // cover behavior, so this just pins that the type compiles and
        // its trait impl is wired correctly.
        let _store: Box<dyn SecretStore> = Box::new(KeyringStore);
    }
}
