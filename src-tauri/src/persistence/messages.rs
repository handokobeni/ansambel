use crate::error::Result;
use crate::persistence::atomic::{load_or_default, write_atomic};
use crate::platform::paths::messages_file;
use crate::state::Message;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Default)]
struct MessagesFile {
    schema_version: u32,
    messages: Vec<Message>,
}

pub fn load_messages(data_dir: &Path, workspace_id: &str) -> Result<Vec<Message>> {
    let file: MessagesFile = load_or_default(&messages_file(data_dir, workspace_id))?;
    Ok(file.messages)
}

pub fn save_messages(data_dir: &Path, workspace_id: &str, messages: &[Message]) -> Result<()> {
    let file = MessagesFile {
        schema_version: 1,
        messages: messages.to_vec(),
    };
    write_atomic(&messages_file(data_dir, workspace_id), &file)
}

/// Loads existing messages, appends one, and saves. Skips the append when a
/// message with the same id is already present so duplicate streaming events
/// (e.g. assistant text deduplication on the frontend echo path) don't
/// produce duplicate disk rows.
pub fn append_message(data_dir: &Path, workspace_id: &str, msg: &Message) -> Result<()> {
    let mut current = load_messages(data_dir, workspace_id).unwrap_or_default();
    if current.iter().any(|m| m.id == msg.id) {
        return Ok(());
    }
    current.push(msg.clone());
    save_messages(data_dir, workspace_id, &current)
}

/// Default page size for `list_messages` when no limit is supplied.
pub const DEFAULT_MESSAGE_PAGE: usize = 50;

/// Returns the most recent `limit` messages older than `before_id`, in
/// chronological order (oldest first). When `before_id` is `None`, returns
/// the latest page. When `before_id` is unknown, returns an empty slice so
/// the frontend can stop paginating.
pub fn list_messages_paginated(
    data_dir: &Path,
    workspace_id: &str,
    limit: Option<usize>,
    before_id: Option<&str>,
) -> Result<Vec<Message>> {
    let limit = limit.unwrap_or(DEFAULT_MESSAGE_PAGE).max(1);
    let all = load_messages(data_dir, workspace_id).unwrap_or_default();
    let upto = match before_id {
        Some(id) => match all.iter().position(|m| m.id == id) {
            Some(i) => i,
            None => return Ok(Vec::new()),
        },
        None => all.len(),
    };
    let head = &all[..upto];
    let start = head.len().saturating_sub(limit);
    Ok(head[start..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Message, MessageRole};
    use tempfile::TempDir;

    fn make_msg(id: &str, ws: &str) -> Message {
        Message {
            id: id.into(),
            workspace_id: ws.into(),
            role: MessageRole::User,
            text: format!("Body for {id}"),
            is_partial: false,
            tool_use: None,
            tool_result: None,
            created_at: 1_776_000_000,
        }
    }

    #[test]
    fn load_messages_missing_file_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let msgs = load_messages(tmp.path(), "ws_none").unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn save_and_load_messages_round_trip() {
        let tmp = TempDir::new().unwrap();
        let m1 = make_msg("msg_a", "ws_x");
        let m2 = make_msg("msg_b", "ws_x");
        save_messages(tmp.path(), "ws_x", &[m1.clone(), m2.clone()]).unwrap();
        let loaded = load_messages(tmp.path(), "ws_x").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0], m1);
        assert_eq!(loaded[1], m2);
    }

    #[test]
    fn save_messages_writes_schema_version() {
        let tmp = TempDir::new().unwrap();
        save_messages(tmp.path(), "ws_x", &[make_msg("msg_a", "ws_x")]).unwrap();
        let path = crate::platform::paths::messages_file(tmp.path(), "ws_x");
        let raw = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["schema_version"], 1);
    }

    #[test]
    fn save_messages_creates_parent_dir() {
        let tmp = TempDir::new().unwrap();
        save_messages(tmp.path(), "ws_y", &[make_msg("msg_a", "ws_y")]).unwrap();
        assert!(tmp.path().join("messages").is_dir());
    }

    #[test]
    fn load_messages_handles_per_workspace_isolation() {
        let tmp = TempDir::new().unwrap();
        save_messages(tmp.path(), "ws_a", &[make_msg("msg_x", "ws_a")]).unwrap();
        save_messages(tmp.path(), "ws_b", &[make_msg("msg_y", "ws_b")]).unwrap();
        let a = load_messages(tmp.path(), "ws_a").unwrap();
        let b = load_messages(tmp.path(), "ws_b").unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        assert_eq!(a[0].id, "msg_x");
        assert_eq!(b[0].id, "msg_y");
    }

    #[test]
    fn append_message_persists_in_order() {
        let tmp = TempDir::new().unwrap();
        save_messages(tmp.path(), "ws_z", &[make_msg("msg_1", "ws_z")]).unwrap();
        let mut current = load_messages(tmp.path(), "ws_z").unwrap();
        current.push(make_msg("msg_2", "ws_z"));
        save_messages(tmp.path(), "ws_z", &current).unwrap();
        let loaded = load_messages(tmp.path(), "ws_z").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, "msg_1");
        assert_eq!(loaded[1].id, "msg_2");
    }

    #[test]
    fn append_message_appends_to_empty_workspace() {
        let tmp = TempDir::new().unwrap();
        let m = make_msg("msg_a", "ws_new");
        append_message(tmp.path(), "ws_new", &m).unwrap();
        let loaded = load_messages(tmp.path(), "ws_new").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], m);
    }

    #[test]
    fn append_message_appends_to_existing_history() {
        let tmp = TempDir::new().unwrap();
        save_messages(tmp.path(), "ws_q", &[make_msg("msg_a", "ws_q")]).unwrap();
        append_message(tmp.path(), "ws_q", &make_msg("msg_b", "ws_q")).unwrap();
        let loaded = load_messages(tmp.path(), "ws_q").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, "msg_a");
        assert_eq!(loaded[1].id, "msg_b");
    }

    #[test]
    fn append_message_skips_duplicate_id() {
        let tmp = TempDir::new().unwrap();
        let m = make_msg("msg_dup", "ws_d");
        append_message(tmp.path(), "ws_d", &m).unwrap();
        append_message(tmp.path(), "ws_d", &m).unwrap();
        let loaded = load_messages(tmp.path(), "ws_d").unwrap();
        assert_eq!(loaded.len(), 1);
    }

    fn write_history(tmp: &TempDir, ws: &str, count: usize) {
        let msgs: Vec<Message> = (1..=count)
            .map(|i| make_msg(&format!("msg_{i:03}"), ws))
            .collect();
        save_messages(tmp.path(), ws, &msgs).unwrap();
    }

    #[test]
    fn list_messages_paginated_empty_history_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let out = list_messages_paginated(tmp.path(), "ws_empty", None, None).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn list_messages_paginated_returns_latest_page_by_default() {
        let tmp = TempDir::new().unwrap();
        write_history(&tmp, "ws_full", 200);
        let out = list_messages_paginated(tmp.path(), "ws_full", None, None).unwrap();
        assert_eq!(out.len(), DEFAULT_MESSAGE_PAGE);
        assert_eq!(out.first().unwrap().id, "msg_151");
        assert_eq!(out.last().unwrap().id, "msg_200");
    }

    #[test]
    fn list_messages_paginated_respects_explicit_limit() {
        let tmp = TempDir::new().unwrap();
        write_history(&tmp, "ws_limit", 30);
        let out = list_messages_paginated(tmp.path(), "ws_limit", Some(10), None).unwrap();
        assert_eq!(out.len(), 10);
        assert_eq!(out.first().unwrap().id, "msg_021");
        assert_eq!(out.last().unwrap().id, "msg_030");
    }

    #[test]
    fn list_messages_paginated_returns_all_when_history_smaller_than_limit() {
        let tmp = TempDir::new().unwrap();
        write_history(&tmp, "ws_small", 3);
        let out = list_messages_paginated(tmp.path(), "ws_small", Some(50), None).unwrap();
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn list_messages_paginated_with_before_id_returns_older_page() {
        let tmp = TempDir::new().unwrap();
        write_history(&tmp, "ws_page", 100);
        let out =
            list_messages_paginated(tmp.path(), "ws_page", Some(20), Some("msg_050")).unwrap();
        assert_eq!(out.len(), 20);
        assert_eq!(out.first().unwrap().id, "msg_030");
        assert_eq!(out.last().unwrap().id, "msg_049");
    }

    #[test]
    fn list_messages_paginated_with_unknown_before_id_returns_empty() {
        let tmp = TempDir::new().unwrap();
        write_history(&tmp, "ws_x", 5);
        let out = list_messages_paginated(tmp.path(), "ws_x", None, Some("nope")).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn list_messages_paginated_at_start_of_history_clamps_to_remaining() {
        let tmp = TempDir::new().unwrap();
        write_history(&tmp, "ws_start", 10);
        let out =
            list_messages_paginated(tmp.path(), "ws_start", Some(20), Some("msg_003")).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "msg_001");
        assert_eq!(out[1].id, "msg_002");
    }
}
