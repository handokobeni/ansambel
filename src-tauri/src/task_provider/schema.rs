// Phase 3a-2 — Bitable schema wizard.
//
// Defines the set of fields that must exist for the LarkProvider to
// function, and provides a verify-and-create function the Tauri
// command surface delegates to. Idempotent: re-running after fields
// exist is a no-op.

use crate::error::{AppError, Result};
use crate::platform::lark_client::LarkClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Lark numeric field type codes used by 3a-2 fields. See
/// `bitable_create_field` docs for the full enum.
pub const FIELD_TYPE_TEXT: u32 = 1;
pub const FIELD_TYPE_NUMBER: u32 = 2;
pub const FIELD_TYPE_SINGLE_SELECT: u32 = 3;

#[derive(Debug, Clone)]
pub struct RequiredField {
    pub name: &'static str,
    pub field_type: u32,
    pub property: Option<serde_json::Value>,
}

/// The 5 fields required for Phase 3a-2 kanban sync. Later phases
/// extend this with their own `required_fields_phase_3a_X()` and the
/// wizard runs the union.
pub fn required_fields_phase_3a2() -> Vec<RequiredField> {
    vec![
        RequiredField {
            name: "title",
            field_type: FIELD_TYPE_TEXT,
            property: None,
        },
        RequiredField {
            name: "description",
            field_type: FIELD_TYPE_TEXT,
            property: None,
        },
        RequiredField {
            name: "kanban_column",
            field_type: FIELD_TYPE_SINGLE_SELECT,
            property: Some(serde_json::json!({
                "options": [
                    {"name": "todo"},
                    {"name": "in_progress"},
                    {"name": "review"},
                    {"name": "done"}
                ]
            })),
        },
        RequiredField {
            name: "repo_id",
            field_type: FIELD_TYPE_TEXT,
            property: None,
        },
        RequiredField {
            name: "order_within_column",
            field_type: FIELD_TYPE_NUMBER,
            property: None,
        },
    ]
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SchemaCheckResult {
    pub ok: bool,
    pub created: Vec<String>,
    pub already_present: Vec<String>,
    pub type_mismatches: Vec<String>,
}

/// Diff the configured table against the required schema, creating
/// missing fields. Type mismatches are reported, not auto-fixed.
pub async fn verify_schema(
    client: &LarkClient,
    app_token: &str,
    table_id: &str,
    required: &[RequiredField],
) -> Result<SchemaCheckResult> {
    let existing = client.bitable_list_fields(app_token, table_id).await?;
    let existing_by_name: HashMap<&str, u32> = existing
        .iter()
        .map(|f| (f.field_name.as_str(), f.field_type))
        .collect();

    let mut created = Vec::new();
    let mut already_present = Vec::new();
    let mut type_mismatches = Vec::new();

    for req in required {
        match existing_by_name.get(req.name) {
            Some(t) if *t == req.field_type => {
                already_present.push(req.name.to_string());
            }
            Some(_wrong_type) => {
                type_mismatches.push(req.name.to_string());
            }
            None => {
                client
                    .bitable_create_field(
                        app_token,
                        table_id,
                        req.name,
                        req.field_type,
                        req.property.clone(),
                    )
                    .await
                    .map_err(|e| AppError::Lark(format!("create field {}: {e}", req.name)))?;
                created.push(req.name.to_string());
            }
        }
    }

    Ok(SchemaCheckResult {
        ok: type_mismatches.is_empty(),
        created,
        already_present,
        type_mismatches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::lark_client::{LarkClient, LarkConfig};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_config(base: &str) -> LarkConfig {
        LarkConfig {
            app_id: "cli_t".into(),
            app_secret: "s".into(),
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
                "tenant_access_token": "t_xyz",
                "expire": 7200
            })))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn verify_schema_creates_all_missing() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "items": [], "has_more": false, "page_token": "" }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "field": {"field_id": "fld_x", "field_name": "x", "type": 1}
                }
            })))
            .expect(5)
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let result = verify_schema(
            &client,
            "bascntest",
            "tbltest",
            &required_fields_phase_3a2(),
        )
        .await
        .unwrap();
        assert!(result.ok);
        assert_eq!(result.created.len(), 5);
        assert!(result.already_present.is_empty());
        assert!(result.type_mismatches.is_empty());
    }

    #[tokio::test]
    async fn verify_schema_skips_present_fields() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {"field_id": "f1", "field_name": "title", "type": 1},
                        {"field_id": "f2", "field_name": "repo_id", "type": 1},
                        {"field_id": "f3", "field_name": "description", "type": 1}
                    ]
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "field": {"field_id": "fld_x", "field_name": "x", "type": 1}
                }
            })))
            .expect(2)
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let result = verify_schema(
            &client,
            "bascntest",
            "tbltest",
            &required_fields_phase_3a2(),
        )
        .await
        .unwrap();
        assert!(result.ok);
        assert_eq!(result.created.len(), 2);
        assert_eq!(result.already_present.len(), 3);
        assert!(result.type_mismatches.is_empty());
    }

    #[tokio::test]
    async fn verify_schema_surfaces_type_mismatch() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {"field_id": "f1", "field_name": "kanban_column", "type": 1}
                    ]
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "field": {"field_id": "fld_x", "field_name": "x", "type": 1}
                }
            })))
            .expect(4)
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let result = verify_schema(
            &client,
            "bascntest",
            "tbltest",
            &required_fields_phase_3a2(),
        )
        .await
        .unwrap();
        assert!(!result.ok);
        assert_eq!(result.type_mismatches, vec!["kanban_column".to_string()]);
        assert_eq!(result.created.len(), 4);
    }

    #[tokio::test]
    async fn verify_schema_idempotent_on_rerun() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {"field_id": "f1", "field_name": "title", "type": 1},
                        {"field_id": "f2", "field_name": "description", "type": 1},
                        {"field_id": "f3", "field_name": "kanban_column", "type": 3},
                        {"field_id": "f4", "field_name": "repo_id", "type": 1},
                        {"field_id": "f5", "field_name": "order_within_column", "type": 2}
                    ]
                }
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let result = verify_schema(
            &client,
            "bascntest",
            "tbltest",
            &required_fields_phase_3a2(),
        )
        .await
        .unwrap();
        assert!(result.ok);
        assert!(result.created.is_empty());
        assert_eq!(result.already_present.len(), 5);
    }
}
