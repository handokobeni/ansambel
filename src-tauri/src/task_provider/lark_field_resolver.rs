//! Pure resolver functions that map a Bitable record + a `FieldMapping`
//! to the parts of a `Task`. No I/O — easy to unit-test against arbitrary
//! mappings. Lives separate from `LarkProvider` so the runtime path is
//! independent of network access.

use crate::error::{AppError, Result};
use crate::platform::lark_client::{BitableField, BitableOption, BitableRecord, LarkClient};
use crate::state::{FieldMapping, FieldRef, KanbanColumn, ProposedMapping, StatusValueMapping};
use crate::task_provider::lark::parse_kanban_column;
use std::sync::Arc;

/// Extract text from a Bitable field value, accepting both shapes:
/// - Plain string (returned by the list endpoint): `"text content"`
/// - Segmented array (returned by the search endpoint): `[{"text": "...", "type": "text"}, ...]`
///
/// Segments are concatenated in order. Returns None if the value is neither shape
/// or yields an empty string.
pub(crate) fn extract_text(value: &serde_json::Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
        return None;
    }
    if let Some(arr) = value.as_array() {
        let mut buf = String::new();
        for seg in arr {
            if let Some(t) = seg.get("text").and_then(serde_json::Value::as_str) {
                buf.push_str(t);
            }
        }
        let trimmed = buf.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    None
}

/// Reads a field's string value off a record by field name. Bitable
/// returns record fields keyed by name in the JSON payload, so we look
/// it up by name; the resolver passes the `field_name` cached on the
/// `FieldRef`. Returns `None` if the field is missing/null/empty.
///
/// Handles both shapes returned by Lark Bitable:
/// - Plain string (list endpoint)
/// - Segmented array `[{"text": "...", "type": "text"}, ...]` (search endpoint)
fn read_string_by_name(record: &BitableRecord, field_name: &str) -> Option<String> {
    let value = record.fields.as_object()?.get(field_name)?;
    extract_text(value)
}

/// Reads a field as plain text, tolerating the several shapes Lark
/// Bitable returns:
///   - Text field (`type` 1): plain JSON string → returned as-is.
///   - Rich-text / long-text: array of segments
///     `[{"type":"text","text":"..."}, {"type":"url","text":"...","link":"..."}, ...]`.
///     We concatenate each segment's `text` (link / mention / formula
///     segments all carry a human-readable `text` key). Note: links
///     surface as the link's `text` only — the URL is dropped, which is
///     intentional for system-prompt context.
///   - Lookup-field arrays: `["AC1 text", "AC2 text"]` (linked field is
///     plain text) or `[[{...segments}], [{...segments}]]` (linked field
///     is rich-text). Each item becomes one entry; entries joined with
///     blank lines so multi-row aggregations like "AC items linked to
///     this task" render as a readable list in Claude's system prompt.
///   - Single-select read shape `{"id": "...", "text": "..."}`: extracts
///     `text`. Lets users map a single-select as description if they
///     repurpose one for free-form notes.
///   - Object with `markdown_text` or `value`: extracts that key.
///     Catches a few non-standard shapes seen in the wild.
///
/// Whitespace-trimmed; empty result → None.
fn read_field_as_text(record: &BitableRecord, field_name: &str) -> Option<String> {
    let value = record.fields.as_object()?.get(field_name)?;
    let text = match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => array_to_text(arr),
        serde_json::Value::Object(o) => object_to_text(o).unwrap_or_default(),
        _ => return None,
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Renders an array value to text. Two array shapes are distinguished by
/// the items' structure:
///   - All items are segment objects (each has `type` + `text`): this is
///     a single rich-text field broken into typed runs. Concat all
///     segments' `text` with no separator — they're one prose.
///   - Otherwise: treat each item as one independent entry (Lookup-field
///     pulling N rows from a linked table). Render each item to text and
///     join with blank lines.
fn array_to_text(arr: &[serde_json::Value]) -> String {
    let all_segments = !arr.is_empty()
        && arr.iter().all(|v| {
            v.as_object()
                .map(|o| o.contains_key("type") && o.contains_key("text"))
                .unwrap_or(false)
        });
    if all_segments {
        return arr
            .iter()
            .filter_map(|seg| seg.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("");
    }
    arr.iter()
        .filter_map(item_to_text)
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn item_to_text(item: &serde_json::Value) -> Option<String> {
    match item {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(inner) => Some(array_to_text(inner)),
        serde_json::Value::Object(o) => object_to_text(o),
        _ => None,
    }
}

fn object_to_text(o: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    o.get("text")
        .or_else(|| o.get("markdown_text"))
        .or_else(|| o.get("value"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Resolves the title for a record. Tries the mapped `title` field
/// first; falls back to `primary_field_name` if both are non-empty.
/// Returns an error message if neither has a value.
pub fn resolve_title(
    record: &BitableRecord,
    mapping: &FieldMapping,
    primary_field_name: Option<&str>,
) -> Result<String> {
    if let Some(v) = read_string_by_name(record, &mapping.title.field_name) {
        return Ok(v);
    }
    if let Some(p) = primary_field_name {
        if let Some(v) = read_string_by_name(record, p) {
            return Ok(v);
        }
    }
    Err(AppError::Lark(format!(
        "record {} missing title (mapped field '{}' empty; primary '{}' empty)",
        record.record_id,
        mapping.title.field_name,
        primary_field_name.unwrap_or("<unknown>"),
    )))
}

/// Resolves the description for a record. Returns empty string when
/// the mapping has no description field set or the value is missing.
/// Uses `read_field_as_text` so users can map rich-text fields (e.g.
/// "Acceptance Criteria") without the value silently coming back blank.
pub fn resolve_description(record: &BitableRecord, mapping: &FieldMapping) -> String {
    mapping
        .description
        .as_ref()
        .and_then(|f| read_field_as_text(record, &f.field_name))
        .unwrap_or_default()
}

/// Resolves the kanban column for a record using the mapped status
/// field + value mapping. Layered fallback:
///   1. If status field unmapped → `default_column`
///   2. Single-select object `{id, text}`: look up `id` in `entries`;
///      if absent, run `text` through the fuzzy parser
///   3. Plain text value: look up lowercased text in `entries`; if
///      absent, run raw text through the fuzzy parser
///   4. Any fuzzy miss → `default_column`
pub fn resolve_status(
    record: &BitableRecord,
    mapping: &FieldMapping,
    values: &StatusValueMapping,
) -> KanbanColumn {
    let Some(status_field) = &mapping.status else {
        return values.default_column.clone();
    };
    let fields = match record.fields.as_object() {
        Some(o) => o,
        None => return values.default_column.clone(),
    };
    let raw = fields.get(&status_field.field_name);
    let Some(raw) = raw else {
        return values.default_column.clone();
    };
    if let Some(obj) = raw.as_object() {
        if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
            if let Some(col) = values.entries.get(id) {
                return col.clone();
            }
        }
        if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
            if let Some(col) = parse_kanban_column(text) {
                return col;
            }
        }
    }
    if let Some(s) = raw.as_str().filter(|s| !s.is_empty()) {
        if let Some(col) = values.entries.get(&s.to_lowercase()) {
            return col.clone();
        }
        if let Some(col) = parse_kanban_column(s) {
            return col;
        }
    }
    values.default_column.clone()
}

/// Resolves the order value for sorting. Mapped `order` field wins;
/// otherwise falls back to negative `created_time` (so newer rows sort
/// first when sorted ASC by this number).
pub fn resolve_order(record: &BitableRecord, mapping: &FieldMapping) -> i32 {
    if let Some(order_ref) = &mapping.order {
        let fields = match record.fields.as_object() {
            Some(o) => o,
            None => return 0,
        };
        if let Some(n) = fields.get(&order_ref.field_name).and_then(|v| v.as_i64()) {
            return n as i32;
        }
    }
    let created_secs = record.extra_i64("created_time").unwrap_or(0) / 1000;
    // Saturate to i32 range. Post-2038 timestamps would overflow; we'd rather
    // collapse to a constant than wrap and produce nonsense sort order.
    let clamped = created_secs.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    -clamped
}

pub(crate) struct BitableSchemaDetector {
    client: Arc<LarkClient>,
}

impl BitableSchemaDetector {
    pub(crate) fn new(client: Arc<LarkClient>) -> Self {
        Self { client }
    }

    /// Fetches Bitable fields and proposes a mapping using deterministic
    /// auto-detection rules:
    ///   - title: the `is_primary: true` field
    ///   - status: first field whose normalised name contains
    ///     "status", "stage", "phase", or "kanban" (alphabetic order)
    ///   - description / order: left as None (user opts in)
    pub(crate) async fn propose_mapping(
        &self,
        app_token: &str,
        table_id: &str,
    ) -> crate::error::Result<ProposedMapping> {
        let fields = self.client.bitable_list_fields(app_token, table_id).await?;
        let primary = fields
            .iter()
            .find(|f| f.is_primary)
            .ok_or_else(|| {
                crate::error::AppError::Lark(format!(
                    "Bitable {app_token}/{table_id} has no primary field"
                ))
            })?
            .clone();
        let suggested_title = FieldRef {
            field_id: primary.field_id.clone(),
            field_name: primary.field_name.clone(),
        };
        let mut status_candidates: Vec<&BitableField> = fields
            .iter()
            .filter(|f| {
                let normalised: String = f
                    .field_name
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect();
                ["status", "stage", "phase", "kanban"]
                    .iter()
                    .any(|kw| normalised.contains(kw))
            })
            .collect();
        status_candidates.sort_by(|a, b| a.field_name.cmp(&b.field_name));
        let status_field = status_candidates.into_iter().next().cloned();

        let (status_options, suggested_status_values) = if let Some(sf) = &status_field {
            let opts: Vec<BitableOption> = sf.options();
            let mut entries = std::collections::HashMap::new();
            for opt in &opts {
                if let Some(col) = crate::task_provider::lark::parse_kanban_column(&opt.name) {
                    entries.insert(opt.id.clone(), col);
                }
            }
            (
                Some(opts),
                crate::state::StatusValueMapping {
                    entries,
                    default_column: KanbanColumn::Todo,
                },
            )
        } else {
            (None, crate::state::StatusValueMapping::default())
        };

        let suggested = FieldMapping {
            title: suggested_title,
            description: None,
            status: status_field.map(|f| FieldRef {
                field_id: f.field_id.clone(),
                field_name: f.field_name.clone(),
            }),
            order: None,
        };
        Ok(ProposedMapping {
            fields,
            suggested,
            status_options,
            suggested_status_values,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::FieldRef;

    fn rec(record_id: &str, fields: serde_json::Value) -> BitableRecord {
        serde_json::from_value(serde_json::json!({
            "record_id": record_id,
            "fields": fields,
        }))
        .unwrap()
    }

    fn title_mapping(name: &str) -> FieldMapping {
        FieldMapping {
            title: FieldRef {
                field_id: "fld_t".into(),
                field_name: name.into(),
            },
            ..Default::default()
        }
    }

    #[test]
    fn resolve_title_uses_explicit_field() {
        let r = rec("r1", serde_json::json!({"title": "Hello"}));
        let m = title_mapping("title");
        assert_eq!(resolve_title(&r, &m, None).unwrap(), "Hello");
    }

    #[test]
    fn resolve_title_falls_back_to_primary() {
        let r = rec(
            "r1",
            serde_json::json!({"Task name": "From primary", "title": ""}),
        );
        let m = title_mapping("title");
        assert_eq!(
            resolve_title(&r, &m, Some("Task name")).unwrap(),
            "From primary"
        );
    }

    #[test]
    fn resolve_title_errors_when_both_empty() {
        let r = rec("r1", serde_json::json!({"title": "", "Task name": ""}));
        let m = title_mapping("title");
        let err = resolve_title(&r, &m, Some("Task name")).unwrap_err();
        assert!(err.to_string().contains("missing title"));
        assert!(err.to_string().contains("r1"));
    }

    // ── extract_text unit tests ─────────────────────────────────────────────

    #[test]
    fn extract_text_handles_plain_string_from_list_endpoint() {
        let value = serde_json::json!("Hello world");
        assert_eq!(extract_text(&value), Some("Hello world".to_string()));
    }

    #[test]
    fn extract_text_handles_segmented_array_from_search_endpoint() {
        let value = serde_json::json!([
            { "text": "Hello ", "type": "text" },
            { "text": "world", "type": "text" }
        ]);
        assert_eq!(extract_text(&value), Some("Hello world".to_string()));
    }

    #[test]
    fn extract_text_returns_none_for_null() {
        let value = serde_json::Value::Null;
        assert_eq!(extract_text(&value), None);
    }

    #[test]
    fn extract_text_returns_none_for_empty_array() {
        let value = serde_json::json!([]);
        assert_eq!(extract_text(&value), None);
    }

    #[test]
    fn extract_text_returns_none_for_array_without_text_keys() {
        let value = serde_json::json!([{ "type": "user", "id": "ou_x" }]);
        assert_eq!(extract_text(&value), None);
    }

    #[test]
    fn extract_text_returns_none_for_empty_string() {
        let value = serde_json::json!("   ");
        assert_eq!(extract_text(&value), None);
    }

    // ── resolve_title with search-endpoint segmented array shape ────────────

    #[test]
    fn resolve_title_handles_segmented_array_from_search_endpoint() {
        // The search endpoint returns text fields as segmented arrays.
        // Before the fix, read_string_by_name called .as_str() which returned
        // None for arrays, causing every search-result record to be skipped.
        let r = rec(
            "r1",
            serde_json::json!({
                "Task name": [{ "text": "User bisa filter", "type": "text" }]
            }),
        );
        let m = title_mapping("Task name");
        assert_eq!(resolve_title(&r, &m, None).unwrap(), "User bisa filter");
    }

    #[test]
    fn resolve_title_falls_back_to_segmented_primary_field() {
        // Primary field also returns segmented shape from search endpoint.
        let r = rec(
            "r1",
            serde_json::json!({
                "title": "",
                "Task name": [
                    { "text": "User bisa mendapatkan narasi", "type": "text" },
                    { "text": " lebih lanjut", "type": "text" }
                ]
            }),
        );
        let m = title_mapping("title");
        assert_eq!(
            resolve_title(&r, &m, Some("Task name")).unwrap(),
            "User bisa mendapatkan narasi lebih lanjut"
        );
    }

    #[test]
    fn resolve_title_handles_both_plain_string_and_segmented_parity() {
        // Both shapes should yield the same title string — confirming list and
        // search endpoint records are parsed identically.
        let plain = rec("r1", serde_json::json!({"Task name": "User bisa filter"}));
        let segmented = rec(
            "r2",
            serde_json::json!({"Task name": [{"text": "User bisa filter", "type": "text"}]}),
        );
        let m = title_mapping("Task name");
        assert_eq!(
            resolve_title(&plain, &m, None).unwrap(),
            resolve_title(&segmented, &m, None).unwrap()
        );
    }

    #[test]
    fn resolve_description_returns_empty_when_unmapped() {
        let r = rec("r1", serde_json::json!({"description": "ignored"}));
        let m = title_mapping("title");
        assert_eq!(resolve_description(&r, &m), "");
    }

    #[test]
    fn resolve_description_uses_mapped_field() {
        let r = rec("r1", serde_json::json!({"desc": "hello"}));
        let mut m = title_mapping("title");
        m.description = Some(FieldRef {
            field_id: "fld_d".into(),
            field_name: "desc".into(),
        });
        assert_eq!(resolve_description(&r, &m), "hello");
    }

    fn ac_mapping() -> FieldMapping {
        let mut m = title_mapping("title");
        m.description = Some(FieldRef {
            field_id: "fld_ac".into(),
            field_name: "Acceptance Criteria".into(),
        });
        m
    }

    #[test]
    fn resolve_description_concatenates_rich_text_segments() {
        // Real-world Bitable rich-text shape: array of text/url/mention
        // segments. We concat each segment's `text` so the agent's system
        // prompt receives readable prose, not JSON.
        let r = rec(
            "r1",
            serde_json::json!({
                "Acceptance Criteria": [
                    {"type": "text", "text": "Given user logs in, "},
                    {"type": "url", "text": "see spec", "link": "https://example.com"},
                    {"type": "text", "text": " then session is bound to device."}
                ]
            }),
        );
        assert_eq!(
            resolve_description(&r, &ac_mapping()),
            "Given user logs in, see spec then session is bound to device."
        );
    }

    #[test]
    fn resolve_description_reads_single_select_text() {
        // Single-select fields read as `{id, text}`. Repurposing one as a
        // tiny "category" description is rare but possible; we surface
        // the visible text instead of returning blank.
        let r = rec(
            "r1",
            serde_json::json!({
                "Acceptance Criteria": {"id": "opt_x", "text": "Must pass QA"}
            }),
        );
        assert_eq!(resolve_description(&r, &ac_mapping()), "Must pass QA");
    }

    #[test]
    fn resolve_description_reads_markdown_text_object() {
        // Some Lark API surfaces wrap rich-text in `{markdown_text: "..."}`.
        let r = rec(
            "r1",
            serde_json::json!({
                "Acceptance Criteria": {"markdown_text": "- Login\n- Logout"}
            }),
        );
        assert_eq!(resolve_description(&r, &ac_mapping()), "- Login\n- Logout");
    }

    #[test]
    fn resolve_description_trims_whitespace() {
        let r = rec(
            "r1",
            serde_json::json!({
                "Acceptance Criteria": "  \n\n  Trim me  \n\n  "
            }),
        );
        assert_eq!(resolve_description(&r, &ac_mapping()), "Trim me");
    }

    #[test]
    fn resolve_description_returns_empty_for_whitespace_only_rich_text() {
        let r = rec(
            "r1",
            serde_json::json!({
                "Acceptance Criteria": [
                    {"type": "text", "text": "   "},
                    {"type": "text", "text": "\n\n"}
                ]
            }),
        );
        assert_eq!(resolve_description(&r, &ac_mapping()), "");
    }

    #[test]
    fn resolve_description_joins_lookup_array_of_plain_strings() {
        // Lookup field pulling a plain-text linked field returns a flat
        // array of strings (one per linked record). Each item is one AC
        // entry; we separate with blank lines so Claude reads them as a
        // list rather than one wall of text.
        let r = rec(
            "r1",
            serde_json::json!({
                "Acceptance Criteria": [
                    "User Superadmin login hingga 3 sesi berbeda",
                    "User Employee login pada 1 sesi aktif",
                    "User Superadmin mencapai batas maksimal sesi ke-4"
                ]
            }),
        );
        let result = resolve_description(&r, &ac_mapping());
        assert!(result.contains("Superadmin login hingga 3 sesi"));
        assert!(result.contains("Employee login pada 1 sesi"));
        assert!(result.contains("mencapai batas maksimal sesi ke-4"));
        // Entries separated by blank line for readability.
        assert!(result.contains("\n\n"));
    }

    #[test]
    fn resolve_description_joins_lookup_array_of_rich_text() {
        // Lookup pulling a rich-text linked field returns an array of
        // arrays (one segment-array per linked record).
        let r = rec(
            "r1",
            serde_json::json!({
                "Acceptance Criteria": [
                    [{"type": "text", "text": "Given pengguna superadmin, "},
                     {"type": "text", "text": "session ke-4 ditolak."}],
                    [{"type": "text", "text": "Given employee, "},
                     {"type": "text", "text": "session ke-2 ditolak."}]
                ]
            }),
        );
        let result = resolve_description(&r, &ac_mapping());
        assert!(result.contains("Given pengguna superadmin, session ke-4 ditolak."));
        assert!(result.contains("Given employee, session ke-2 ditolak."));
        assert!(result.contains("\n\n"));
    }

    #[test]
    fn resolve_description_skips_empty_entries_in_lookup_array() {
        let r = rec(
            "r1",
            serde_json::json!({
                "Acceptance Criteria": ["First AC", "", "   ", "Second AC"]
            }),
        );
        let result = resolve_description(&r, &ac_mapping());
        assert_eq!(result, "First AC\n\nSecond AC");
    }

    #[test]
    fn resolve_description_returns_empty_for_unsupported_value_types() {
        // Boolean / number / null values don't make sense as descriptions
        // and we don't want to leak `"true"` or `"42"` into Claude's
        // context. Fall through to empty.
        let r = rec("r1", serde_json::json!({"Acceptance Criteria": 42}));
        assert_eq!(resolve_description(&r, &ac_mapping()), "");
        let r2 = rec("r1", serde_json::json!({"Acceptance Criteria": true}));
        assert_eq!(resolve_description(&r2, &ac_mapping()), "");
        let r3 = rec("r1", serde_json::json!({"Acceptance Criteria": null}));
        assert_eq!(resolve_description(&r3, &ac_mapping()), "");
    }

    fn status_mapping() -> FieldMapping {
        FieldMapping {
            title: FieldRef {
                field_id: "fld_t".into(),
                field_name: "title".into(),
            },
            status: Some(FieldRef {
                field_id: "fld_s".into(),
                field_name: "Task Status".into(),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn resolve_status_returns_default_when_unmapped() {
        let r = rec("r1", serde_json::json!({"title": "x"}));
        let m = title_mapping("title");
        let v = StatusValueMapping {
            default_column: KanbanColumn::Review,
            ..Default::default()
        };
        assert_eq!(resolve_status(&r, &m, &v), KanbanColumn::Review);
    }

    #[test]
    fn resolve_status_uses_option_id_via_entries() {
        let r = rec(
            "r1",
            serde_json::json!({"Task Status": {"id": "opt_a", "text": "Hai"}}),
        );
        let mut entries = std::collections::HashMap::new();
        entries.insert("opt_a".into(), KanbanColumn::Done);
        let v = StatusValueMapping {
            entries,
            default_column: KanbanColumn::Todo,
        };
        let m = status_mapping();
        assert_eq!(resolve_status(&r, &m, &v), KanbanColumn::Done);
    }

    #[test]
    fn resolve_status_falls_back_to_fuzzy_for_text_value() {
        let r = rec("r1", serde_json::json!({"Task Status": "In Progress"}));
        let v = StatusValueMapping::default();
        let m = status_mapping();
        assert_eq!(resolve_status(&r, &m, &v), KanbanColumn::InProgress);
    }

    #[test]
    fn resolve_status_uses_default_for_unmapped_unknown_text() {
        let r = rec("r1", serde_json::json!({"Task Status": "xyz"}));
        let v = StatusValueMapping {
            default_column: KanbanColumn::Review,
            ..Default::default()
        };
        let m = status_mapping();
        assert_eq!(resolve_status(&r, &m, &v), KanbanColumn::Review);
    }

    #[test]
    fn resolve_status_falls_through_to_fuzzy_when_option_id_not_in_entries() {
        let r = rec(
            "r1",
            serde_json::json!({"Task Status": {"id": "opt_unknown", "text": "Done"}}),
        );
        let mut entries = std::collections::HashMap::new();
        entries.insert("opt_other".into(), KanbanColumn::Todo);
        let v = StatusValueMapping {
            entries,
            default_column: KanbanColumn::Todo,
        };
        let m = status_mapping();
        assert_eq!(resolve_status(&r, &m, &v), KanbanColumn::Done);
    }

    #[test]
    fn resolve_status_uses_default_when_field_missing_on_record() {
        let r = rec("r1", serde_json::json!({"title": "x"}));
        let v = StatusValueMapping {
            default_column: KanbanColumn::InProgress,
            ..Default::default()
        };
        let m = status_mapping();
        assert_eq!(resolve_status(&r, &m, &v), KanbanColumn::InProgress);
    }

    #[test]
    fn resolve_order_uses_mapped_field_when_present() {
        let r = rec("r1", serde_json::json!({"order": 42}));
        let mut m = title_mapping("title");
        m.order = Some(FieldRef {
            field_id: "fld_o".into(),
            field_name: "order".into(),
        });
        assert_eq!(resolve_order(&r, &m), 42);
    }

    #[test]
    fn resolve_order_falls_back_to_negative_created_time() {
        let r = serde_json::from_value::<BitableRecord>(serde_json::json!({
            "record_id": "r1",
            "fields": {"title": "x"},
            "created_time": 1700000000000_i64,
        }))
        .unwrap();
        let m = title_mapping("title");
        assert_eq!(resolve_order(&r, &m), -1700000000);
    }

    #[test]
    fn resolve_order_returns_zero_when_no_mapping_and_no_created_time() {
        let r = rec("r1", serde_json::json!({"title": "x"}));
        let m = title_mapping("title");
        assert_eq!(resolve_order(&r, &m), 0);
    }

    // ── BitableSchemaDetector tests ──────────────────────────────────────────

    use crate::platform::lark_client::LarkConfig;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

    fn detector(uri: &str) -> BitableSchemaDetector {
        BitableSchemaDetector::new(Arc::new(LarkClient::new(LarkConfig {
            app_id: "cli_t".into(),
            app_secret: "s".into(),
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            base_url: uri.into(),
        })))
    }

    async fn mount_fields(server: &MockServer, fields: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path(
                "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "items": fields }
            })))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn propose_picks_primary_as_title() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_fields(
            &server,
            serde_json::json!([
                {"field_id": "fld_pri", "field_name": "Task name", "type": 1, "is_primary": true},
                {"field_id": "fld_other", "field_name": "Notes", "type": 1, "is_primary": false}
            ]),
        )
        .await;
        let p = detector(&server.uri())
            .propose_mapping("bascntest", "tbltest")
            .await
            .unwrap();
        assert_eq!(p.suggested.title.field_id, "fld_pri");
        assert_eq!(p.suggested.title.field_name, "Task name");
    }

    #[tokio::test]
    async fn propose_detects_status_field_by_name_keyword() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_fields(
            &server,
            serde_json::json!([
                {"field_id": "fld_pri", "field_name": "Task name", "type": 1, "is_primary": true},
                {"field_id": "fld_s", "field_name": "Task Status", "type": 3, "is_primary": false,
                 "property": {"options": [
                     {"id": "opt_a", "name": "To Do"},
                     {"id": "opt_b", "name": "Done"}
                 ]}}
            ]),
        )
        .await;
        let p = detector(&server.uri())
            .propose_mapping("bascntest", "tbltest")
            .await
            .unwrap();
        let status = p.suggested.status.expect("status should be detected");
        assert_eq!(status.field_id, "fld_s");
        let opts = p.status_options.unwrap();
        assert_eq!(opts.len(), 2);
        assert_eq!(
            p.suggested_status_values.entries.get("opt_a"),
            Some(&KanbanColumn::Todo)
        );
        assert_eq!(
            p.suggested_status_values.entries.get("opt_b"),
            Some(&KanbanColumn::Done)
        );
    }

    #[tokio::test]
    async fn propose_returns_none_status_when_no_match() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_fields(
            &server,
            serde_json::json!([
                {"field_id": "fld_pri", "field_name": "Task name", "type": 1, "is_primary": true},
                {"field_id": "fld_n", "field_name": "Notes", "type": 1, "is_primary": false}
            ]),
        )
        .await;
        let p = detector(&server.uri())
            .propose_mapping("bascntest", "tbltest")
            .await
            .unwrap();
        assert!(p.suggested.status.is_none());
        assert!(p.status_options.is_none());
    }

    #[tokio::test]
    async fn propose_picks_alphabetically_first_when_multiple_status_fields() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_fields(
            &server,
            serde_json::json!([
                {"field_id": "fld_pri", "field_name": "Task name", "type": 1, "is_primary": true},
                {"field_id": "fld_sprint", "field_name": "Sprint Stage", "type": 3, "is_primary": false},
                {"field_id": "fld_status", "field_name": "Task Status", "type": 3, "is_primary": false}
            ]),
        )
        .await;
        let p = detector(&server.uri())
            .propose_mapping("bascntest", "tbltest")
            .await
            .unwrap();
        assert_eq!(p.suggested.status.unwrap().field_id, "fld_sprint");
    }

    #[tokio::test]
    async fn propose_errors_when_no_primary() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_fields(
            &server,
            serde_json::json!([
                {"field_id": "fld_x", "field_name": "Notes", "type": 1, "is_primary": false}
            ]),
        )
        .await;
        let err = detector(&server.uri())
            .propose_mapping("bascntest", "tbltest")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no primary field"));
    }
}
