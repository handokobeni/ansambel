// Phase 3a-1 — Lark Open Platform API client.
//
// Typed wrapper around the subset of Lark/Feishu Open Platform we need
// for Phase 3a: tenant_access_token auth, Bitable CRUD, attachment
// upload/download, IM send. Pure HTTP — no Tauri command surface here
// (those live in `commands/lark_auth.rs` and the future kanban-sync
// layer).
//
// Design notes:
//   - `LarkConfig.base_url` is configurable so wiremock can stand in
//     for the real endpoint during unit tests.
//   - `tenant_access_token` cached with an explicit `Instant`-based
//     TTL. The refresh check uses a margin (default 10 min) so we
//     proactively refresh before the token actually expires.
//   - All methods are `async`. The caller (Tauri command) wraps them.
//   - NEVER log `app_secret` or returned `tenant_access_token`. The
//     `Debug` impl on LarkConfig redacts the secret.

use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Default international Lark base URL. China-mainland deployments use
/// `https://open.feishu.cn` — set via `LarkConfig.base_url`.
pub const DEFAULT_BASE_URL: &str = "https://open.larksuite.com";

/// Refresh tokens this many seconds before they actually expire so we
/// never serve a stale token to a request that's about to fly.
pub const REFRESH_MARGIN_SECS: u64 = 10 * 60;

#[derive(Clone, Serialize, Deserialize)]
pub struct LarkConfig {
    pub app_id: String,
    pub app_secret: String,
    pub app_token: String,
    pub table_id: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}

impl std::fmt::Debug for LarkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LarkConfig")
            .field("app_id", &self.app_id)
            .field("app_secret", &"<redacted>")
            .field("app_token", &self.app_token)
            .field("table_id", &self.table_id)
            .field("base_url", &self.base_url)
            .finish()
    }
}

#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    expires_at: Instant,
}

#[derive(Debug)]
pub struct LarkClient {
    config: LarkConfig,
    http: reqwest::Client,
    token_cache: Arc<Mutex<Option<CachedToken>>>,
}

impl LarkClient {
    pub fn new(config: LarkConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client builds with default config"),
            token_cache: Arc::new(Mutex::new(None)),
        }
    }

    pub fn config(&self) -> &LarkConfig {
        &self.config
    }

    /// Returns a valid tenant_access_token, fetching a new one only
    /// when the cached value is missing or within `REFRESH_MARGIN_SECS`
    /// of its expiry. Concurrent callers serialize through the cache
    /// mutex; only one HTTP call ever fires per refresh.
    pub async fn tenant_access_token(&self) -> Result<String> {
        let mut guard = self.token_cache.lock().await;
        let now = Instant::now();
        if let Some(t) = guard.as_ref() {
            if should_use_cached(t.expires_at, now, REFRESH_MARGIN_SECS) {
                return Ok(t.token.clone());
            }
        }
        let fresh = self.fetch_tenant_token().await?;
        let expires_at = now + Duration::from_secs(fresh.expire_secs.max(REFRESH_MARGIN_SECS));
        let token = fresh.token.clone();
        *guard = Some(CachedToken {
            token: fresh.token,
            expires_at,
        });
        Ok(token)
    }

    async fn fetch_tenant_token(&self) -> Result<FreshToken> {
        let url = format!(
            "{}/open-apis/auth/v3/tenant_access_token/internal",
            self.config.base_url
        );
        let body = serde_json::json!({
            "app_id": self.config.app_id,
            "app_secret": self.config.app_secret,
        });
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Lark(format!("tenant_access_token request: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::Lark(format!("tenant_access_token body: {e}")))?;
        if !status.is_success() {
            return Err(AppError::Lark(format!(
                "tenant_access_token http {status}: {}",
                truncate(&text, 200)
            )));
        }
        let parsed: TenantTokenResponse = serde_json::from_str(&text).map_err(|e| {
            AppError::Lark(format!(
                "tenant_access_token parse: {e}; body={}",
                truncate(&text, 200)
            ))
        })?;
        if parsed.code != 0 {
            return Err(AppError::Lark(format!(
                "tenant_access_token code {}: {}",
                parsed.code, parsed.msg
            )));
        }
        Ok(FreshToken {
            token: parsed.tenant_access_token,
            expire_secs: parsed.expire,
        })
    }
}

/// Returns true when the cached token is still safely usable — i.e.,
/// the cached expiry is far enough in the future that `now` + margin
/// hasn't crossed it. Pure function for unit-testing the cache logic
/// without spinning up an HTTP server.
pub fn should_use_cached(expires_at: Instant, now: Instant, margin_secs: u64) -> bool {
    let margin = Duration::from_secs(margin_secs);
    expires_at > now && (expires_at - now) > margin
}

#[derive(Debug)]
struct FreshToken {
    token: String,
    expire_secs: u64,
}

#[derive(Deserialize)]
struct TenantTokenResponse {
    code: i64,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    tenant_access_token: String,
    #[serde(default)]
    expire: u64,
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

// ── Bitable CRUD ─────────────────────────────────────────────────────

/// One row from a Bitable table. `fields` is a free-form JSON object;
/// the schema is enforced at the table level in Lark, not here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BitableRecord {
    pub record_id: String,
    pub fields: serde_json::Value,
}

/// Hard pagination cap so a runaway loop can't keep fetching forever.
/// Bitable returns up to 500 records per page; 100 pages = 50k records
/// — well past anything we'd reasonably want to paginate through
/// without batching at the caller level.
const MAX_LIST_PAGES: usize = 100;
const DEFAULT_PAGE_SIZE: u32 = 500;

#[derive(Deserialize)]
struct BitableListResponse {
    code: i64,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    data: Option<BitableListData>,
}

#[derive(Deserialize)]
struct BitableListData {
    #[serde(default)]
    items: Vec<BitableRecord>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    page_token: String,
}

#[derive(Deserialize)]
struct BitableSingleResponse {
    code: i64,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    data: Option<BitableSingleData>,
}

#[derive(Deserialize)]
struct BitableSingleData {
    #[serde(default)]
    record: Option<BitableRecord>,
}

#[derive(Deserialize)]
struct BitableEmptyResponse {
    code: i64,
    #[serde(default)]
    msg: String,
}

impl LarkClient {
    /// List every record in a Bitable table, optionally filtered. The
    /// filter is passed through verbatim — see Lark Bitable docs for
    /// the expression grammar (`CurrentValue.[field]=value`).
    /// Auto-paginates up to `MAX_LIST_PAGES` pages.
    pub async fn bitable_list_records(
        &self,
        app_token: &str,
        table_id: &str,
        filter: Option<&str>,
    ) -> Result<Vec<BitableRecord>> {
        let token = self.tenant_access_token().await?;
        let mut out: Vec<BitableRecord> = Vec::new();
        let mut page_token: Option<String> = None;
        for _ in 0..MAX_LIST_PAGES {
            let url = format!(
                "{}/open-apis/bitable/v1/apps/{}/tables/{}/records",
                self.config.base_url, app_token, table_id
            );
            let mut req = self
                .http
                .get(&url)
                .bearer_auth(&token)
                .query(&[("page_size", DEFAULT_PAGE_SIZE.to_string())]);
            if let Some(f) = filter {
                req = req.query(&[("filter", f)]);
            }
            if let Some(pt) = page_token.as_ref() {
                req = req.query(&[("page_token", pt.as_str())]);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| AppError::Lark(format!("bitable_list request: {e}")))?;
            let status = resp.status();
            let text = resp
                .text()
                .await
                .map_err(|e| AppError::Lark(format!("bitable_list body: {e}")))?;
            if !status.is_success() {
                return Err(AppError::Lark(format!(
                    "bitable_list http {status}: {}",
                    truncate(&text, 200)
                )));
            }
            let parsed: BitableListResponse = serde_json::from_str(&text).map_err(|e| {
                AppError::Lark(format!(
                    "bitable_list parse: {e}; body={}",
                    truncate(&text, 200)
                ))
            })?;
            if parsed.code != 0 {
                return Err(AppError::Lark(format!(
                    "bitable_list code {}: {}",
                    parsed.code, parsed.msg
                )));
            }
            let data = parsed
                .data
                .ok_or_else(|| AppError::Lark("bitable_list missing data".into()))?;
            out.extend(data.items);
            if !data.has_more || data.page_token.is_empty() {
                return Ok(out);
            }
            page_token = Some(data.page_token);
        }
        Err(AppError::Lark(format!(
            "bitable_list pagination exceeded {MAX_LIST_PAGES} pages"
        )))
    }

    /// Create a single record. `fields` is a JSON object — the keys
    /// must match the table's column names, values match the declared
    /// types (e.g. number, text, single-select).
    pub async fn bitable_create_record(
        &self,
        app_token: &str,
        table_id: &str,
        fields: serde_json::Value,
    ) -> Result<BitableRecord> {
        let token = self.tenant_access_token().await?;
        let url = format!(
            "{}/open-apis/bitable/v1/apps/{}/tables/{}/records",
            self.config.base_url, app_token, table_id
        );
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&token)
            .json(&serde_json::json!({ "fields": fields }))
            .send()
            .await
            .map_err(|e| AppError::Lark(format!("bitable_create request: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::Lark(format!("bitable_create body: {e}")))?;
        if !status.is_success() {
            return Err(AppError::Lark(format!(
                "bitable_create http {status}: {}",
                truncate(&text, 200)
            )));
        }
        let parsed: BitableSingleResponse = serde_json::from_str(&text).map_err(|e| {
            AppError::Lark(format!(
                "bitable_create parse: {e}; body={}",
                truncate(&text, 200)
            ))
        })?;
        if parsed.code != 0 {
            return Err(AppError::Lark(format!(
                "bitable_create code {}: {}",
                parsed.code, parsed.msg
            )));
        }
        parsed
            .data
            .and_then(|d| d.record)
            .ok_or_else(|| AppError::Lark("bitable_create missing record in response".into()))
    }

    /// Patch one record's fields. The body matches Bitable's
    /// partial-update semantics — only the keys you include are
    /// modified. Returns Ok on 200 + code 0.
    pub async fn bitable_update_record(
        &self,
        app_token: &str,
        table_id: &str,
        record_id: &str,
        fields: serde_json::Value,
    ) -> Result<()> {
        let token = self.tenant_access_token().await?;
        let url = format!(
            "{}/open-apis/bitable/v1/apps/{}/tables/{}/records/{}",
            self.config.base_url, app_token, table_id, record_id
        );
        let resp = self
            .http
            .put(&url)
            .bearer_auth(&token)
            .json(&serde_json::json!({ "fields": fields }))
            .send()
            .await
            .map_err(|e| AppError::Lark(format!("bitable_update request: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::Lark(format!("bitable_update body: {e}")))?;
        if !status.is_success() {
            return Err(AppError::Lark(format!(
                "bitable_update http {status}: {}",
                truncate(&text, 200)
            )));
        }
        let parsed: BitableEmptyResponse = serde_json::from_str(&text).map_err(|e| {
            AppError::Lark(format!(
                "bitable_update parse: {e}; body={}",
                truncate(&text, 200)
            ))
        })?;
        if parsed.code != 0 {
            return Err(AppError::Lark(format!(
                "bitable_update code {}: {}",
                parsed.code, parsed.msg
            )));
        }
        Ok(())
    }

    pub async fn bitable_delete_record(
        &self,
        app_token: &str,
        table_id: &str,
        record_id: &str,
    ) -> Result<()> {
        let token = self.tenant_access_token().await?;
        let url = format!(
            "{}/open-apis/bitable/v1/apps/{}/tables/{}/records/{}",
            self.config.base_url, app_token, table_id, record_id
        );
        let resp = self
            .http
            .delete(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| AppError::Lark(format!("bitable_delete request: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::Lark(format!("bitable_delete body: {e}")))?;
        if !status.is_success() {
            return Err(AppError::Lark(format!(
                "bitable_delete http {status}: {}",
                truncate(&text, 200)
            )));
        }
        let parsed: BitableEmptyResponse = serde_json::from_str(&text).map_err(|e| {
            AppError::Lark(format!(
                "bitable_delete parse: {e}; body={}",
                truncate(&text, 200)
            ))
        })?;
        if parsed.code != 0 {
            return Err(AppError::Lark(format!(
                "bitable_delete code {}: {}",
                parsed.code, parsed.msg
            )));
        }
        Ok(())
    }
}

// ── Drive: attachment upload + download ─────────────────────────────

#[derive(Deserialize)]
struct AttachmentUploadResponse {
    code: i64,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    data: Option<AttachmentUploadData>,
}

#[derive(Deserialize)]
struct AttachmentUploadData {
    #[serde(default)]
    file_token: String,
}

impl LarkClient {
    /// Upload bytes as a media file. Returns the `file_token` Lark
    /// assigns; we persist that in the Bitable attachment field so the
    /// row references the file. `parent_node` is the destination
    /// (typically the Bitable app_token) and `parent_type` is the
    /// Lark resource kind (e.g. `bitable_file`). `file_name` is what
    /// users see in the file viewer.
    pub async fn attachment_upload(
        &self,
        parent_node: &str,
        parent_type: &str,
        file_name: &str,
        bytes: Vec<u8>,
    ) -> Result<String> {
        let token = self.tenant_access_token().await?;
        let url = format!(
            "{}/open-apis/drive/v1/medias/upload_all",
            self.config.base_url
        );
        let size = bytes.len();
        let form = reqwest::multipart::Form::new()
            .text("file_name", file_name.to_string())
            .text("parent_type", parent_type.to_string())
            .text("parent_node", parent_node.to_string())
            .text("size", size.to_string())
            .part(
                "file",
                reqwest::multipart::Part::bytes(bytes).file_name(file_name.to_string()),
            );
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| AppError::Lark(format!("attachment_upload request: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::Lark(format!("attachment_upload body: {e}")))?;
        if !status.is_success() {
            return Err(AppError::Lark(format!(
                "attachment_upload http {status}: {}",
                truncate(&text, 200)
            )));
        }
        let parsed: AttachmentUploadResponse = serde_json::from_str(&text).map_err(|e| {
            AppError::Lark(format!(
                "attachment_upload parse: {e}; body={}",
                truncate(&text, 200)
            ))
        })?;
        if parsed.code != 0 {
            return Err(AppError::Lark(format!(
                "attachment_upload code {}: {}",
                parsed.code, parsed.msg
            )));
        }
        let file_token = parsed.data.map(|d| d.file_token).unwrap_or_default();
        if file_token.is_empty() {
            return Err(AppError::Lark(
                "attachment_upload returned empty file_token".into(),
            ));
        }
        Ok(file_token)
    }

    /// Download a file by its Lark `file_token`. Returns raw bytes.
    /// Note: for Bitable attachments specifically, the caller may need
    /// to pass additional `extra` query params — this method exposes
    /// the simple form sufficient for non-Bitable media. The
    /// upcoming sync layer wraps this with the Bitable-specific shape.
    pub async fn attachment_download(&self, file_token: &str) -> Result<Vec<u8>> {
        let token = self.tenant_access_token().await?;
        let url = format!(
            "{}/open-apis/drive/v1/medias/{}/download",
            self.config.base_url, file_token
        );
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| AppError::Lark(format!("attachment_download request: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Lark(format!(
                "attachment_download http {status}: {}",
                truncate(&text, 200)
            )));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::Lark(format!("attachment_download body: {e}")))?;
        Ok(bytes.to_vec())
    }
}

// ── IM: send message ───────────────────────────────────────────────

#[derive(Deserialize)]
struct ImSendResponse {
    code: i64,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    data: Option<ImSendData>,
}

#[derive(Deserialize)]
struct ImSendData {
    #[serde(default)]
    message_id: String,
}

impl LarkClient {
    /// Send an IM message. `receive_id_type` is the kind of the
    /// receiver id (`open_id` / `chat_id` / `user_id` / `email`).
    /// `msg_type` matches Lark's message kinds (`text` / `post` /
    /// `interactive`, etc). `content` is JSON-stringified per the
    /// Lark spec — callers serialize. Returns the assigned
    /// `message_id`.
    pub async fn im_send_message(
        &self,
        receive_id_type: &str,
        receive_id: &str,
        msg_type: &str,
        content: &str,
    ) -> Result<String> {
        let token = self.tenant_access_token().await?;
        let url = format!("{}/open-apis/im/v1/messages", self.config.base_url);
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&token)
            .query(&[("receive_id_type", receive_id_type)])
            .json(&serde_json::json!({
                "receive_id": receive_id,
                "msg_type": msg_type,
                "content": content,
            }))
            .send()
            .await
            .map_err(|e| AppError::Lark(format!("im_send_message request: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::Lark(format!("im_send_message body: {e}")))?;
        if !status.is_success() {
            return Err(AppError::Lark(format!(
                "im_send_message http {status}: {}",
                truncate(&text, 200)
            )));
        }
        let parsed: ImSendResponse = serde_json::from_str(&text).map_err(|e| {
            AppError::Lark(format!(
                "im_send_message parse: {e}; body={}",
                truncate(&text, 200)
            ))
        })?;
        if parsed.code != 0 {
            return Err(AppError::Lark(format!(
                "im_send_message code {}: {}",
                parsed.code, parsed.msg
            )));
        }
        Ok(parsed.data.map(|d| d.message_id).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_config(base: &str) -> LarkConfig {
        LarkConfig {
            app_id: "app_test_id".into(),
            app_secret: "app_test_secret".into(),
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            base_url: base.into(),
        }
    }

    async fn mount_token(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok",
                "tenant_access_token": "t_xyz",
                "expire": 7200,
            })))
            .mount(server)
            .await;
    }

    #[test]
    fn should_use_cached_returns_true_when_far_from_expiry() {
        let now = Instant::now();
        let expires_at = now + Duration::from_secs(7200); // 2h from now
        assert!(should_use_cached(expires_at, now, 600));
    }

    #[test]
    fn should_use_cached_returns_false_within_margin() {
        let now = Instant::now();
        // 5 min until expiry, but margin is 10 min → must refresh.
        let expires_at = now + Duration::from_secs(300);
        assert!(!should_use_cached(expires_at, now, 600));
    }

    #[test]
    fn should_use_cached_returns_false_when_already_expired() {
        let now = Instant::now();
        let expires_at = now.checked_sub(Duration::from_secs(1)).unwrap_or(now);
        assert!(!should_use_cached(expires_at, now, 600));
    }

    #[test]
    fn lark_config_debug_redacts_app_secret() {
        let cfg = make_config("http://localhost");
        let s = format!("{cfg:?}");
        assert!(s.contains("<redacted>"), "expected redaction, got: {s}");
        assert!(
            !s.contains("app_test_secret"),
            "secret leaked into Debug: {s}"
        );
    }

    #[tokio::test]
    async fn tenant_token_fetches_and_returns_value() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .and(body_json(serde_json::json!({
                "app_id": "app_test_id",
                "app_secret": "app_test_secret",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok",
                "tenant_access_token": "t_abc",
                "expire": 7200,
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let token = client.tenant_access_token().await.unwrap();
        assert_eq!(token, "t_abc");
    }

    #[tokio::test]
    async fn tenant_token_cached_until_near_expiry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok",
                "tenant_access_token": "t_first",
                "expire": 7200,
            })))
            .expect(1) // exactly one HTTP call across both .tenant_access_token() calls
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let a = client.tenant_access_token().await.unwrap();
        let b = client.tenant_access_token().await.unwrap();
        assert_eq!(a, "t_first");
        assert_eq!(b, "t_first");
        // server.verify_all() runs on drop; .expect(1) asserts the
        // mock saw the right call count.
    }

    #[tokio::test]
    async fn tenant_token_refreshed_after_expiry() {
        let server = MockServer::start().await;
        // First mock: short-lived token (expires immediately past the
        // margin). Wiremock matches each request to ONE mock; since
        // both calls match the same URL/method, the second uses the
        // same body — but `expire: 1` makes the cache invalid right
        // away, so the client must re-issue the request.
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok",
                "tenant_access_token": "t_refreshed",
                "expire": 1,
            })))
            .expect(2)
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let _ = client.tenant_access_token().await.unwrap();
        let _ = client.tenant_access_token().await.unwrap();
    }

    #[tokio::test]
    async fn tenant_token_surfaces_non_zero_code_as_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 99991663,
                "msg": "app not found",
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let err = client.tenant_access_token().await.unwrap_err();
        let s = err.to_string();
        assert!(s.contains("99991663"), "{s}");
        assert!(s.contains("app not found"), "{s}");
    }

    #[tokio::test]
    async fn tenant_token_surfaces_http_5xx_as_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(503).set_body_string("upstream down"))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let err = client.tenant_access_token().await.unwrap_err();
        let s = err.to_string();
        assert!(s.contains("503"), "{s}");
    }

    // ── bitable list ─────────────────────────────────────────────

    #[tokio::test]
    async fn bitable_list_returns_records_in_one_page() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .and(header("authorization", "Bearer t_xyz"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok",
                "data": {
                    "items": [
                        { "record_id": "rec1", "fields": { "Title": "A" } },
                        { "record_id": "rec2", "fields": { "Title": "B" } }
                    ],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let records = client
            .bitable_list_records("bascntest", "tbltest", None)
            .await
            .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].record_id, "rec1");
        assert_eq!(records[0].fields["Title"], "A");
    }

    #[tokio::test]
    async fn bitable_list_auto_paginates() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        // First page: has_more=true, page_token=p2.
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .and(query_param("page_size", "500"))
            // No page_token query on the first call — wiremock matches
            // the first request that satisfies all `.and()` conditions
            // and the others fall through to the second mock.
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [{ "record_id": "r1", "fields": {} }],
                    "has_more": true,
                    "page_token": "p2"
                }
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        // Second page: has_more=false, no page_token.
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .and(query_param("page_token", "p2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [{ "record_id": "r2", "fields": {} }],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let records = client
            .bitable_list_records("bascntest", "tbltest", None)
            .await
            .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].record_id, "r1");
        assert_eq!(records[1].record_id, "r2");
    }

    #[tokio::test]
    async fn bitable_list_passes_filter_query() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .and(query_param("filter", "CurrentValue.[repo_id]=repo_abc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "items": [], "has_more": false, "page_token": "" }
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let records = client
            .bitable_list_records(
                "bascntest",
                "tbltest",
                Some("CurrentValue.[repo_id]=repo_abc"),
            )
            .await
            .unwrap();
        assert!(records.is_empty());
    }

    #[tokio::test]
    async fn bitable_list_surfaces_non_zero_code_as_error() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1254000,
                "msg": "app_token invalid"
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let err = client
            .bitable_list_records("bascntest", "tbltest", None)
            .await
            .unwrap_err();
        let s = err.to_string();
        assert!(
            s.contains("1254000") && s.contains("app_token invalid"),
            "{s}"
        );
    }

    // ── bitable create ───────────────────────────────────────────

    #[tokio::test]
    async fn bitable_create_assigns_record_id() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .and(header("authorization", "Bearer t_xyz"))
            .and(body_json(serde_json::json!({
                "fields": { "Title": "New" }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "record": {
                        "record_id": "rec_new",
                        "fields": { "Title": "New" }
                    }
                }
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let rec = client
            .bitable_create_record(
                "bascntest",
                "tbltest",
                serde_json::json!({ "Title": "New" }),
            )
            .await
            .unwrap();
        assert_eq!(rec.record_id, "rec_new");
        assert_eq!(rec.fields["Title"], "New");
    }

    #[tokio::test]
    async fn bitable_create_missing_record_in_response_is_error() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {}
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let err = client
            .bitable_create_record("bascntest", "tbltest", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("missing record"), "{err}");
    }

    // ── bitable update ───────────────────────────────────────────

    #[tokio::test]
    async fn bitable_update_succeeds_with_partial_fields() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("PUT"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records/rec_id",
            ))
            .and(header("authorization", "Bearer t_xyz"))
            .and(body_json(serde_json::json!({
                "fields": { "Status": "done" }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok"
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        client
            .bitable_update_record(
                "bascntest",
                "tbltest",
                "rec_id",
                serde_json::json!({ "Status": "done" }),
            )
            .await
            .unwrap();
    }

    // ── bitable delete ───────────────────────────────────────────

    #[tokio::test]
    async fn bitable_delete_succeeds() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("DELETE"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records/rec_to_delete",
            ))
            .and(header("authorization", "Bearer t_xyz"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok"
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        client
            .bitable_delete_record("bascntest", "tbltest", "rec_to_delete")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn bitable_delete_surfaces_non_zero_code() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("DELETE"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records/rec_nope",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1254043,
                "msg": "record_not_found"
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let err = client
            .bitable_delete_record("bascntest", "tbltest", "rec_nope")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("record_not_found"), "{err}");
    }

    // ── attachment upload ────────────────────────────────────────

    #[tokio::test]
    async fn attachment_upload_returns_file_token() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/open-apis/drive/v1/medias/upload_all"))
            .and(header("authorization", "Bearer t_xyz"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "file_token": "boxn_abcdef" }
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let token = client
            .attachment_upload("bascntest", "bitable_file", "bundle.tar.gz", vec![1, 2, 3])
            .await
            .unwrap();
        assert_eq!(token, "boxn_abcdef");
    }

    #[tokio::test]
    async fn attachment_upload_empty_file_token_is_error() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/open-apis/drive/v1/medias/upload_all"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {}
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let err = client
            .attachment_upload("bascntest", "bitable_file", "x.bin", vec![])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("empty file_token"), "{err}");
    }

    // ── attachment download ──────────────────────────────────────

    #[tokio::test]
    async fn attachment_download_returns_bytes() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path("/open-apis/drive/v1/medias/boxn_test/download"))
            .and(header("authorization", "Bearer t_xyz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello world".as_ref()))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let bytes = client.attachment_download("boxn_test").await.unwrap();
        assert_eq!(bytes, b"hello world");
    }

    #[tokio::test]
    async fn attachment_download_surfaces_404() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path("/open-apis/drive/v1/medias/boxn_nope/download"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let err = client.attachment_download("boxn_nope").await.unwrap_err();
        assert!(err.to_string().contains("404"), "{err}");
    }

    // ── im send ──────────────────────────────────────────────────

    #[tokio::test]
    async fn im_send_message_returns_message_id() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/open-apis/im/v1/messages"))
            .and(query_param("receive_id_type", "open_id"))
            .and(header("authorization", "Bearer t_xyz"))
            .and(body_json(serde_json::json!({
                "receive_id": "ou_abc",
                "msg_type": "text",
                "content": "{\"text\":\"hi\"}"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "message_id": "om_xyz" }
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let msg_id = client
            .im_send_message("open_id", "ou_abc", "text", "{\"text\":\"hi\"}")
            .await
            .unwrap();
        assert_eq!(msg_id, "om_xyz");
    }

    #[tokio::test]
    async fn im_send_message_surfaces_non_zero_code() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/open-apis/im/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 230002,
                "msg": "bot disabled"
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let err = client
            .im_send_message("open_id", "ou_x", "text", "{}")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("bot disabled"), "{err}");
    }

    #[tokio::test]
    async fn tenant_token_surfaces_network_error() {
        // Point the client at a port nothing is listening on — connect
        // should fail.
        let client = LarkClient::new(make_config("http://127.0.0.1:1"));
        let err = client.tenant_access_token().await.unwrap_err();
        let s = err.to_string();
        // Reqwest's error message is platform-dependent ("connection
        // refused" / "couldn't connect") so just sanity-check the
        // wrapping prefix.
        assert!(
            s.starts_with("Lark API: tenant_access_token request:"),
            "got: {s}"
        );
    }
}
