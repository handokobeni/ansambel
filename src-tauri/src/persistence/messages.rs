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
}
