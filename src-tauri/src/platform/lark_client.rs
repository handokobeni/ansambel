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

/// Lark's published rate limit for tenant-level API: 200 requests per
/// minute per app. We model this as a token bucket with capacity 200
/// and refill rate 200/60 ≈ 3.33 tokens/sec. The bucket starts full.
pub const RATE_LIMIT_CAPACITY: f64 = 200.0;
pub const RATE_LIMIT_REFILL_PER_SEC: f64 = 200.0 / 60.0;

/// Default delay before retrying after a 429 response when the
/// `Retry-After` header is absent or unparseable. Picked to give the
/// server a chance to recover without making the user wait long.
pub const DEFAULT_429_RETRY_SECS: u64 = 1;

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

/// Token-bucket rate limiter. Pure async — `acquire()` resolves when a
/// token is available, sleeping if needed. Capacity & refill rate are
/// fixed at construction. The bucket internals are public-by-fn for
/// testability; tests drive `try_acquire`/`refill` against an injected
/// `Instant` so we never have to actually wait wall-clock during a
/// unit test.
#[derive(Debug)]
pub struct RateLimiter {
    inner: Arc<Mutex<TokenBucket>>,
}

#[derive(Debug, Clone)]
pub struct TokenBucket {
    pub capacity: f64,
    pub tokens: f64,
    pub refill_per_sec: f64,
    pub last_refill: Instant,
}

impl RateLimiter {
    pub fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(TokenBucket {
                capacity,
                tokens: capacity, // start full
                refill_per_sec,
                last_refill: Instant::now(),
            })),
        }
    }

    /// Lark-defaults factory.
    pub fn lark_default() -> Self {
        Self::new(RATE_LIMIT_CAPACITY, RATE_LIMIT_REFILL_PER_SEC)
    }

    /// Acquire one token, sleeping if the bucket is empty. Multiple
    /// concurrent acquirers serialize via the inner mutex.
    pub async fn acquire(&self) {
        loop {
            let wait = {
                let mut bucket = self.inner.lock().await;
                match try_acquire(&mut bucket, Instant::now()) {
                    Ok(()) => return,
                    Err(d) => d,
                }
            };
            tokio::time::sleep(wait).await;
        }
    }
}

/// Refill the bucket up to `capacity` based on elapsed time since the
/// last refill. Pure — no clock side effects.
pub fn refill(bucket: &mut TokenBucket, now: Instant) {
    if now <= bucket.last_refill {
        return;
    }
    let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
    let added = elapsed * bucket.refill_per_sec;
    bucket.tokens = (bucket.tokens + added).min(bucket.capacity);
    bucket.last_refill = now;
}

/// Try to take one token from the bucket. On success, decrement and
/// return Ok. On failure, return the `Duration` the caller must sleep
/// before the bucket would have one token available. Pure.
pub fn try_acquire(bucket: &mut TokenBucket, now: Instant) -> std::result::Result<(), Duration> {
    refill(bucket, now);
    if bucket.tokens >= 1.0 {
        bucket.tokens -= 1.0;
        Ok(())
    } else {
        let needed = 1.0 - bucket.tokens;
        let wait_secs = needed / bucket.refill_per_sec;
        // Floor at 1ms so a degenerate caller can't busy-spin even on
        // a misconfigured bucket.
        let wait = Duration::from_secs_f64(wait_secs.max(0.001));
        Err(wait)
    }
}

#[derive(Debug)]
pub struct LarkClient {
    config: LarkConfig,
    http: reqwest::Client,
    token_cache: Arc<Mutex<Option<CachedToken>>>,
    limiter: Arc<RateLimiter>,
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
            limiter: Arc::new(RateLimiter::lark_default()),
        }
    }

    pub fn config(&self) -> &LarkConfig {
        &self.config
    }

    /// Acquire one rate-limit token, send the request, and on a 429
    /// response sleep then retry exactly once. `build` is invoked
    /// per attempt (so each call constructs a fresh RequestBuilder)
    /// — keep it cheap. `label` is a short string prefixed to error
    /// messages so callers don't have to wrap. The returned Response
    /// has not had its body consumed; callers extract text/bytes as
    /// needed.
    async fn send_with_retry<F>(&self, label: &str, build: F) -> Result<reqwest::Response>
    where
        F: Fn() -> reqwest::RequestBuilder,
    {
        self.limiter.acquire().await;
        let first = build()
            .send()
            .await
            .map_err(|e| AppError::Lark(format!("{label} request: {e}")))?;
        if first.status() != reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Ok(first);
        }
        let wait = parse_retry_after(first.headers())
            .unwrap_or(Duration::from_secs(DEFAULT_429_RETRY_SECS));
        drop(first);
        tokio::time::sleep(wait).await;
        self.limiter.acquire().await;
        build()
            .send()
            .await
            .map_err(|e| AppError::Lark(format!("{label} retry request: {e}")))
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
            .send_with_retry("tenant_access_token", || self.http.post(&url).json(&body))
            .await?;
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

/// Parse the `Retry-After` header into a `Duration`. Lark's docs say
/// it'll be sent as a non-negative integer "seconds" (no HTTP-date
/// form). We accept any parseable integer and clamp the result to a
/// reasonable upper bound — a misbehaving server returning
/// "Retry-After: 3600" should not pause the whole app for an hour.
pub fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let v = headers.get(reqwest::header::RETRY_AFTER)?;
    let s = v.to_str().ok()?;
    let secs: u64 = s.trim().parse().ok()?;
    // Cap at 60s — anything longer is server confusion, not signal.
    Some(Duration::from_secs(secs.min(60)))
}

// ── Bitable CRUD ─────────────────────────────────────────────────────

/// One row from a Bitable table. `fields` is a free-form JSON object;
/// the schema is enforced at the table level in Lark, not here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BitableRecord {
    pub record_id: String,
    pub fields: serde_json::Value,
    /// Lark returns these as top-level fields on the record object.
    /// Stored as opaque JSON so we can pull `created_time` /
    /// `last_modified_time` without explicit fields here.
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

impl BitableRecord {
    pub fn extra_i64(&self, key: &str) -> Option<i64> {
        self.extra.get(key).and_then(|v| v.as_i64())
    }
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

/// Field metadata as returned by Bitable. `field_type` is the numeric
/// code Lark uses (1=Text, 2=Number, 3=SingleSelect, 5=DateTime,
/// 7=Checkbox, 15=URL, 17=Attachment). Property is free-form JSON
/// whose shape depends on the type.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BitableField {
    pub field_id: String,
    pub field_name: String,
    #[serde(rename = "type")]
    pub field_type: u32,
    #[serde(default)]
    pub property: Option<serde_json::Value>,
    /// Marks the table's primary column (the locked first field every Bitable
    /// has). Used as a read-only fallback source for `title` so existing
    /// tables whose data lives in the primary column render in Ansambel even
    /// before the user populates the wizard-created `title` field.
    #[serde(default)]
    pub is_primary: bool,
}

/// One option of a Bitable single-select field. Lives inside
/// `BitableField.property.options`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BitableOption {
    pub id: String,
    pub name: String,
}

impl BitableField {
    /// Extracts the single-select options list from `property.options`.
    /// Returns an empty Vec for fields that aren't single-select.
    pub fn options(&self) -> Vec<BitableOption> {
        self.property
            .as_ref()
            .and_then(|p| p.get("options"))
            .and_then(|o| o.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[derive(Deserialize)]
struct BitableFieldListResponse {
    code: i64,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    data: Option<BitableFieldListData>,
}

#[derive(Deserialize)]
struct BitableFieldListData {
    #[serde(default)]
    items: Vec<BitableField>,
}

#[derive(Deserialize)]
struct BitableCreateFieldResponse {
    code: i64,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    data: Option<BitableCreateFieldData>,
}

#[derive(Deserialize)]
struct BitableCreateFieldData {
    #[serde(default)]
    field: Option<BitableField>,
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
            let resp = self
                .send_with_retry("bitable_list", || {
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
                    req
                })
                .await?;
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
        let body = serde_json::json!({ "fields": fields });
        let resp = self
            .send_with_retry("bitable_create", || {
                self.http.post(&url).bearer_auth(&token).json(&body)
            })
            .await?;
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

    /// Fetch a single record by its `record_id`. Used by callers that
    /// need canonical row state after a mutation (Bitable's PUT/PATCH
    /// endpoints don't include row metadata in their responses).
    pub async fn bitable_get_record(
        &self,
        app_token: &str,
        table_id: &str,
        record_id: &str,
    ) -> Result<BitableRecord> {
        let token = self.tenant_access_token().await?;
        let url = format!(
            "{}/open-apis/bitable/v1/apps/{}/tables/{}/records/{}",
            self.config.base_url, app_token, table_id, record_id
        );
        let resp = self
            .send_with_retry("bitable_get_record", || {
                self.http.get(&url).bearer_auth(&token)
            })
            .await?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::Lark(format!("bitable_get_record body: {e}")))?;
        if !status.is_success() {
            return Err(AppError::Lark(format!(
                "bitable_get_record http {status}: {}",
                truncate(&text, 200)
            )));
        }
        let parsed: BitableSingleResponse = serde_json::from_str(&text).map_err(|e| {
            AppError::Lark(format!(
                "bitable_get_record parse: {e}; body={}",
                truncate(&text, 200)
            ))
        })?;
        if parsed.code != 0 {
            return Err(AppError::Lark(format!(
                "bitable_get_record code {}: {}",
                parsed.code, parsed.msg
            )));
        }
        parsed
            .data
            .and_then(|d| d.record)
            .ok_or_else(|| AppError::Lark("bitable_get_record missing record in response".into()))
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
        let body = serde_json::json!({ "fields": fields });
        let resp = self
            .send_with_retry("bitable_update", || {
                self.http.put(&url).bearer_auth(&token).json(&body)
            })
            .await?;
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
            .send_with_retry("bitable_delete", || {
                self.http.delete(&url).bearer_auth(&token)
            })
            .await?;
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

    /// List all fields in a Bitable table. Used by the schema wizard.
    pub async fn bitable_list_fields(
        &self,
        app_token: &str,
        table_id: &str,
    ) -> Result<Vec<BitableField>> {
        let token = self.tenant_access_token().await?;
        let url = format!(
            "{}/open-apis/bitable/v1/apps/{}/tables/{}/fields",
            self.config.base_url, app_token, table_id
        );
        let resp = self
            .send_with_retry("bitable_list_fields", || {
                self.http.get(&url).bearer_auth(&token)
            })
            .await?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::Lark(format!("bitable_list_fields body: {e}")))?;
        if !status.is_success() {
            return Err(AppError::Lark(format!(
                "bitable_list_fields http {status}: {}",
                truncate(&text, 200)
            )));
        }
        let parsed: BitableFieldListResponse = serde_json::from_str(&text).map_err(|e| {
            AppError::Lark(format!(
                "bitable_list_fields parse: {e}; body={}",
                truncate(&text, 200)
            ))
        })?;
        if parsed.code != 0 {
            return Err(AppError::Lark(format!(
                "bitable_list_fields code {}: {}",
                parsed.code, parsed.msg
            )));
        }
        Ok(parsed.data.map(|d| d.items).unwrap_or_default())
    }

    /// Create one field in a Bitable table. `property` shape depends on
    /// `field_type` — see Lark Open Platform docs.
    pub async fn bitable_create_field(
        &self,
        app_token: &str,
        table_id: &str,
        field_name: &str,
        field_type: u32,
        property: Option<serde_json::Value>,
    ) -> Result<BitableField> {
        let token = self.tenant_access_token().await?;
        let url = format!(
            "{}/open-apis/bitable/v1/apps/{}/tables/{}/fields",
            self.config.base_url, app_token, table_id
        );
        let mut body = serde_json::json!({
            "field_name": field_name,
            "type": field_type,
        });
        if let Some(prop) = &property {
            body["property"] = prop.clone();
        }
        let resp = self
            .send_with_retry("bitable_create_field", || {
                self.http.post(&url).bearer_auth(&token).json(&body)
            })
            .await?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::Lark(format!("bitable_create_field body: {e}")))?;
        if !status.is_success() {
            return Err(AppError::Lark(format!(
                "bitable_create_field http {status}: {}",
                truncate(&text, 200)
            )));
        }
        let parsed: BitableCreateFieldResponse = serde_json::from_str(&text).map_err(|e| {
            AppError::Lark(format!(
                "bitable_create_field parse: {e}; body={}",
                truncate(&text, 200)
            ))
        })?;
        if parsed.code != 0 {
            return Err(AppError::Lark(format!(
                "bitable_create_field code {}: {}",
                parsed.code, parsed.msg
            )));
        }
        parsed
            .data
            .and_then(|d| d.field)
            .ok_or_else(|| AppError::Lark("bitable_create_field missing field in response".into()))
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
        // Multipart form bodies own their bytes and are consumed by send().
        // Rebuilding inside an Fn closure would force us to clone potentially
        // large blobs on every attempt, so we honour the rate limit but skip
        // the 429-retry helper for this one endpoint.
        self.limiter.acquire().await;
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
            .send_with_retry("attachment_download", || {
                self.http.get(&url).bearer_auth(&token)
            })
            .await?;
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
        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": msg_type,
            "content": content,
        });
        let resp = self
            .send_with_retry("im_send_message", || {
                self.http
                    .post(&url)
                    .bearer_auth(&token)
                    .query(&[("receive_id_type", receive_id_type)])
                    .json(&body)
            })
            .await?;
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

    // ── bitable get record ───────────────────────────────────────

    #[tokio::test]
    async fn bitable_get_record_returns_single_record_by_id() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records/rec_xyz",
            ))
            .and(header("authorization", "Bearer t_xyz"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "record": {
                        "record_id": "rec_xyz",
                        "fields": { "title": "Hello" },
                        "created_time": 1700000000000_i64,
                        "last_modified_time": 1700000001000_i64
                    }
                }
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let rec = client
            .bitable_get_record("bascntest", "tbltest", "rec_xyz")
            .await
            .unwrap();
        assert_eq!(rec.record_id, "rec_xyz");
        assert_eq!(rec.extra_i64("created_time"), Some(1_700_000_000_000));
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

    // ── rate limiter (pure) ──────────────────────────────────────

    #[test]
    fn rate_limiter_try_acquire_succeeds_when_bucket_has_tokens() {
        let now = Instant::now();
        let mut bucket = TokenBucket {
            capacity: 5.0,
            tokens: 3.0,
            refill_per_sec: 1.0,
            last_refill: now,
        };
        assert!(try_acquire(&mut bucket, now).is_ok());
        assert!((bucket.tokens - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rate_limiter_try_acquire_returns_wait_when_empty() {
        let now = Instant::now();
        let mut bucket = TokenBucket {
            capacity: 5.0,
            tokens: 0.0,
            refill_per_sec: 2.0, // 2 tokens/sec → 1 token in 0.5s
            last_refill: now,
        };
        let wait = try_acquire(&mut bucket, now).unwrap_err();
        // 0.5s ± floor, comfortably above the 1ms floor.
        assert!(wait >= Duration::from_millis(450));
        assert!(wait <= Duration::from_millis(550));
    }

    #[test]
    fn rate_limiter_refill_caps_at_capacity() {
        let earlier = Instant::now();
        let later = earlier + Duration::from_secs(1000);
        let mut bucket = TokenBucket {
            capacity: 5.0,
            tokens: 0.0,
            refill_per_sec: 1.0,
            last_refill: earlier,
        };
        refill(&mut bucket, later);
        // 1000 s × 1 tok/s = 1000, capped at capacity 5.
        assert!((bucket.tokens - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rate_limiter_refill_noop_when_now_is_not_after_last() {
        let now = Instant::now();
        let mut bucket = TokenBucket {
            capacity: 5.0,
            tokens: 2.0,
            refill_per_sec: 1.0,
            last_refill: now,
        };
        refill(&mut bucket, now);
        assert!((bucket.tokens - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rate_limiter_try_acquire_uses_at_least_1ms_floor() {
        let now = Instant::now();
        let mut bucket = TokenBucket {
            capacity: 5.0,
            tokens: 0.999_999,           // almost full
            refill_per_sec: 1_000_000.0, // would compute < 1µs naively
            last_refill: now,
        };
        let wait = try_acquire(&mut bucket, now).unwrap_err();
        assert!(wait >= Duration::from_millis(1), "got {wait:?}");
    }

    #[tokio::test]
    async fn rate_limiter_acquire_returns_immediately_when_tokens_available() {
        let limiter = RateLimiter::new(5.0, 1.0);
        let start = Instant::now();
        limiter.acquire().await;
        // Should be effectively instant; allow up to 50ms of scheduler jitter.
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn rate_limiter_acquire_sleeps_when_bucket_empty() {
        // Capacity 1, refill 10 tok/s → after taking the initial token,
        // next acquire must wait ~100ms.
        let limiter = RateLimiter::new(1.0, 10.0);
        limiter.acquire().await; // drain
        let start = Instant::now();
        limiter.acquire().await;
        let elapsed = start.elapsed();
        // Allow generous bounds — schedulers are noisy in CI.
        assert!(
            elapsed >= Duration::from_millis(50),
            "expected ≥50ms wait, got {elapsed:?}"
        );
    }

    #[test]
    fn lark_default_rate_limiter_matches_constants() {
        let limiter = RateLimiter::lark_default();
        // Inspect via Debug — we don't expose the bucket directly.
        let s = format!("{limiter:?}");
        assert!(s.contains("200"), "{s}");
    }

    // ── parse_retry_after ────────────────────────────────────────

    #[test]
    fn parse_retry_after_parses_integer_seconds() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "5".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(5)));
    }

    #[test]
    fn parse_retry_after_trims_whitespace() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "  7 ".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(7)));
    }

    #[test]
    fn parse_retry_after_caps_at_60_seconds() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "3600".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(60)));
    }

    #[test]
    fn parse_retry_after_returns_none_when_missing() {
        let headers = reqwest::header::HeaderMap::new();
        assert_eq!(parse_retry_after(&headers), None);
    }

    #[test]
    fn parse_retry_after_returns_none_for_http_date() {
        // HTTP-date form is valid per RFC 7231 but Lark sends integers
        // only; we accept None and fall back to DEFAULT_429_RETRY_SECS.
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            "Wed, 21 Oct 2026 07:28:00 GMT".parse().unwrap(),
        );
        assert_eq!(parse_retry_after(&headers), None);
    }

    #[test]
    fn parse_retry_after_returns_none_for_negative() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "-3".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), None);
    }

    // ── 429 retry through send_with_retry ────────────────────────

    #[tokio::test]
    async fn send_with_retry_retries_429_then_succeeds() {
        let server = MockServer::start().await;
        // First response: 429 with Retry-After: 1. The client must wait
        // (we'll cap to <2s wall-clock for the test) and retry, getting
        // the success body on attempt #2.
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", "1")
                    .set_body_string("rate limited"),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "msg": "ok",
                "tenant_access_token": "t_after_retry",
                "expire": 7200,
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let start = Instant::now();
        let token = client.tenant_access_token().await.unwrap();
        let elapsed = start.elapsed();
        assert_eq!(token, "t_after_retry");
        // Confirms the retry path actually slept.
        assert!(
            elapsed >= Duration::from_millis(900),
            "expected ≥900ms wait, got {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn send_with_retry_surfaces_429_when_retry_also_429() {
        let server = MockServer::start().await;
        // Both attempts get 429 → final response surfaced as error.
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", "1")
                    .set_body_string("still rate limited"),
            )
            .expect(2)
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let err = client.tenant_access_token().await.unwrap_err();
        let s = err.to_string();
        assert!(s.contains("429"), "{s}");
    }

    #[tokio::test]
    async fn send_with_retry_uses_default_when_retry_after_missing() {
        let server = MockServer::start().await;
        // 429 with NO Retry-After header → falls back to
        // DEFAULT_429_RETRY_SECS (1s).
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(429).set_body_string("limited"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "tenant_access_token": "t_default_retry",
                "expire": 7200,
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let start = Instant::now();
        let token = client.tenant_access_token().await.unwrap();
        let elapsed = start.elapsed();
        assert_eq!(token, "t_default_retry");
        // Default is 1s — confirm we waited at least most of it.
        assert!(
            elapsed >= Duration::from_millis(900),
            "expected ≥900ms wait, got {elapsed:?}"
        );
    }

    // ── bitable list fields ──────────────────────────────────────

    #[tokio::test]
    async fn bitable_list_fields_returns_field_metadata() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
            ))
            .and(header("authorization", "Bearer t_xyz"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {
                            "field_id": "fld_a",
                            "field_name": "title",
                            "type": 1,
                            "property": null
                        },
                        {
                            "field_id": "fld_b",
                            "field_name": "kanban_column",
                            "type": 3,
                            "property": {
                                "options": [{"name": "todo", "id": "opt_1"}]
                            }
                        }
                    ],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let fields = client
            .bitable_list_fields("bascntest", "tbltest")
            .await
            .unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].field_name, "title");
        assert_eq!(fields[0].field_type, 1);
        assert_eq!(fields[1].field_name, "kanban_column");
        assert_eq!(fields[1].field_type, 3);
    }

    // ── bitable create field ─────────────────────────────────────

    #[tokio::test]
    async fn bitable_create_field_posts_field_definition() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
            ))
            .and(header("authorization", "Bearer t_xyz"))
            .and(body_json(serde_json::json!({
                "field_name": "kanban_column",
                "type": 3,
                "property": {
                    "options": [
                        {"name": "todo"},
                        {"name": "in_progress"}
                    ]
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "field": {
                        "field_id": "fld_new",
                        "field_name": "kanban_column",
                        "type": 3,
                        "property": {
                            "options": [
                                {"name": "todo", "id": "opt_1"},
                                {"name": "in_progress", "id": "opt_2"}
                            ]
                        }
                    }
                }
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let field = client
            .bitable_create_field(
                "bascntest",
                "tbltest",
                "kanban_column",
                3,
                Some(serde_json::json!({
                    "options": [
                        {"name": "todo"},
                        {"name": "in_progress"}
                    ]
                })),
            )
            .await
            .unwrap();
        assert_eq!(field.field_id, "fld_new");
        assert_eq!(field.field_name, "kanban_column");
        assert_eq!(field.field_type, 3);
    }

    #[tokio::test]
    async fn bitable_create_field_omits_property_when_none() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
            ))
            .and(body_json(serde_json::json!({
                "field_name": "repo_id",
                "type": 1
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "field": {
                        "field_id": "fld_repo",
                        "field_name": "repo_id",
                        "type": 1
                    }
                }
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let field = client
            .bitable_create_field("bascntest", "tbltest", "repo_id", 1, None)
            .await
            .unwrap();
        assert_eq!(field.field_id, "fld_repo");
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

    #[test]
    fn bitable_field_options_extracts_from_property() {
        let f = BitableField {
            field_id: "fld_x".into(),
            field_name: "Status".into(),
            field_type: 3,
            property: Some(serde_json::json!({
                "options": [
                    {"id": "opt_1", "name": "To Do"},
                    {"id": "opt_2", "name": "Done"}
                ]
            })),
            is_primary: false,
        };
        let opts = f.options();
        assert_eq!(opts.len(), 2);
        assert_eq!(opts[0].id, "opt_1");
        assert_eq!(opts[1].name, "Done");
    }

    #[test]
    fn bitable_field_options_empty_when_no_property() {
        let f = BitableField {
            field_id: "fld_x".into(),
            field_name: "Text".into(),
            field_type: 1,
            property: None,
            is_primary: false,
        };
        assert!(f.options().is_empty());
    }
}
