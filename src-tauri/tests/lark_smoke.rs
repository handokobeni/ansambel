// Phase 3a-1b — End-to-end smoke tests against the real Lark API.
//
// All tests in this file are `#[ignore]` by default so the standard
// `cargo test` / CI run never touches the network or a real tenant. Run
// them locally with:
//
//   export LARK_APP_ID=cli_xxxxxxxxxxxxxxxx
//   export LARK_APP_SECRET=...        # never commit
//   export LARK_APP_TOKEN=bascn...    # Bitable app token
//   export LARK_TABLE_ID=tbl...       # Bitable table id
//   export LARK_BASE_URL=https://open.larksuite.com   # optional
//   cargo test --test lark_smoke -- --ignored --nocapture
//
// Each test reads its env at runtime; if a required variable is missing
// it prints a hint and returns `Ok` rather than failing, so partial-env
// runs don't drown the output in red. NEVER log `LARK_APP_SECRET` or
// the returned `tenant_access_token` from these tests — the assertions
// are deliberately phrased so a failure surfaces enough to debug
// without leaking credentials.

use ansambel_lib::platform::lark_client::{LarkClient, LarkConfig, DEFAULT_BASE_URL};

/// Pull a `LarkConfig` from env. Returns `None` (with an informative
/// stderr message) when any required variable is missing.
fn lark_config_from_env() -> Option<LarkConfig> {
    let app_id = match std::env::var("LARK_APP_ID") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("LARK_APP_ID not set; skipping smoke test");
            return None;
        }
    };
    let app_secret = match std::env::var("LARK_APP_SECRET") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("LARK_APP_SECRET not set; skipping smoke test");
            return None;
        }
    };
    let app_token = match std::env::var("LARK_APP_TOKEN") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("LARK_APP_TOKEN not set; skipping smoke test");
            return None;
        }
    };
    let table_id = match std::env::var("LARK_TABLE_ID") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("LARK_TABLE_ID not set; skipping smoke test");
            return None;
        }
    };
    let base_url = std::env::var("LARK_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    Some(LarkConfig {
        app_id,
        app_secret,
        app_token,
        table_id,
        base_url,
    })
}

#[tokio::test]
#[ignore = "requires real Lark credentials in env"]
async fn smoke_tenant_access_token() {
    let Some(cfg) = lark_config_from_env() else {
        return;
    };
    let client = LarkClient::new(cfg);
    let token = client
        .tenant_access_token()
        .await
        .expect("tenant_access_token should succeed against real Lark");
    // Don't print the token. Assert structure only — Lark tokens are
    // opaque strings, but the API has always returned > 20 chars.
    assert!(
        token.len() > 20,
        "tenant_access_token unexpectedly short ({} chars)",
        token.len()
    );
}

#[tokio::test]
#[ignore = "requires real Lark credentials in env"]
async fn smoke_bitable_list_records() {
    let Some(cfg) = lark_config_from_env() else {
        return;
    };
    let app_token = cfg.app_token.clone();
    let table_id = cfg.table_id.clone();
    let client = LarkClient::new(cfg);
    let records = client
        .bitable_list_records(&app_token, &table_id, None)
        .await
        .expect("bitable_list_records should succeed against real Lark");
    eprintln!("bitable_list_records returned {} record(s)", records.len());
    // Don't assert on count — the target table may legitimately be empty.
}

#[tokio::test]
#[ignore = "requires real Lark credentials in env"]
async fn smoke_bitable_create_then_delete() {
    let Some(cfg) = lark_config_from_env() else {
        return;
    };
    let app_token = cfg.app_token.clone();
    let table_id = cfg.table_id.clone();
    let client = LarkClient::new(cfg);

    // Create a probe record. We can't assume the schema, so we attempt
    // to write a single text-like field named per the LARK_SMOKE_FIELD
    // env var; if missing we skip the create+delete test rather than
    // pollute a real table with rows the user didn't sanction.
    let field_name = match std::env::var("LARK_SMOKE_FIELD") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("LARK_SMOKE_FIELD not set; skipping create+delete smoke test");
            return;
        }
    };
    let value = format!(
        "ansambel-smoke-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
    let fields = serde_json::json!({ &field_name: value });
    let created = client
        .bitable_create_record(&app_token, &table_id, fields)
        .await
        .expect("bitable_create_record should succeed");
    eprintln!("created record_id={}", created.record_id);

    // Delete the probe so smoke runs don't accumulate rows.
    client
        .bitable_delete_record(&app_token, &table_id, &created.record_id)
        .await
        .expect("bitable_delete_record should succeed for the row we just created");
}
