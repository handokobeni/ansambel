//! Pure resolver functions that map a Bitable record + a `FieldMapping`
//! to the parts of a `Task`. No I/O — easy to unit-test against arbitrary
//! mappings. Lives separate from `LarkProvider` so the runtime path is
//! independent of network access.

use crate::error::{AppError, Result};
use crate::platform::lark_client::BitableRecord;
use crate::state::{FieldMapping, KanbanColumn, StatusValueMapping};
use crate::task_provider::lark::parse_kanban_column;

/// Reads a field's string value off a record by `field_id`. Bitable
/// returns record fields keyed by name in the JSON payload, so we look
/// it up by name; the resolver passes the `field_name` cached on the
/// `FieldRef`. Returns `None` if the field is missing/null/empty.
fn read_string_by_name<'a>(record: &'a BitableRecord, field_name: &str) -> Option<&'a str> {
    record
        .fields
        .as_object()
        .and_then(|m| m.get(field_name))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
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
        return Ok(v.to_string());
    }
    if let Some(p) = primary_field_name {
        if let Some(v) = read_string_by_name(record, p) {
            return Ok(v.to_string());
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
pub fn resolve_description(record: &BitableRecord, mapping: &FieldMapping) -> String {
    mapping
        .description
        .as_ref()
        .and_then(|f| read_string_by_name(record, &f.field_name))
        .map(|s| s.to_string())
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
}
