use crate::error::{AppError, Result};
use crate::persistence::atomic::load_or_default;
use crate::platform::paths::messages_file;
use crate::state::Message;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Schema versions this build can safely deserialize.
/// - v1: legacy `{ schema_version, messages }` single JSON object.
/// - v2: JSONL — first line is `{"schema_version":2}` header, each subsequent
///   line is a serialized `Message`. Lets `list_messages_paginated` seek from
///   the end and `append_message` append a single line in O(1).
///
/// Files in v1 still load via the slow path; the next append migrates them.
pub const KNOWN_SCHEMA_VERSIONS: &[u32] = &[1, 2];
/// Latest version emitted by `save_messages` and `append_message`.
pub const SCHEMA_VERSION: u32 = 2;

pub fn check_schema_version(v: u32) -> Result<()> {
    if KNOWN_SCHEMA_VERSIONS.contains(&v) {
        Ok(())
    } else {
        Err(AppError::Other(format!(
            "Unsupported message schema version {v}. Please update Ansambel to read this workspace's history."
        )))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileFormat {
    Empty,
    LegacyJson,
    Jsonl,
}

/// Sniffs the on-disk format by reading the first line. v1 files start with
/// `{"schema_version":1,"messages":[`; v2 files start with `{"schema_version":2}\n`.
/// A file present but unparseable is treated as Empty so the caller produces
/// a fresh JSONL on the next write.
fn detect_format(path: &Path) -> Result<FileFormat> {
    if !path.exists() {
        return Ok(FileFormat::Empty);
    }
    let f = File::open(path)?;
    let mut reader = BufReader::new(f);
    let mut first = String::new();
    reader.read_line(&mut first)?;
    let trimmed = first.trim();
    if trimmed.is_empty() {
        return Ok(FileFormat::Empty);
    }
    let v: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return Ok(FileFormat::Empty),
    };
    // v1 files contain the full message list inline — `messages` field present.
    // v2 headers contain only `schema_version`.
    if v.get("messages").is_some() {
        Ok(FileFormat::LegacyJson)
    } else {
        Ok(FileFormat::Jsonl)
    }
}

#[derive(Serialize, Deserialize, Default)]
struct LegacyMessagesFile {
    schema_version: u32,
    messages: Vec<Message>,
}

#[derive(Serialize, Deserialize)]
struct JsonlHeader {
    schema_version: u32,
}

pub fn load_messages(data_dir: &Path, workspace_id: &str) -> Result<Vec<Message>> {
    let path = messages_file(data_dir, workspace_id);
    match detect_format(&path)? {
        FileFormat::Empty => Ok(Vec::new()),
        FileFormat::LegacyJson => {
            let file: LegacyMessagesFile = load_or_default(&path)?;
            check_schema_version(file.schema_version)?;
            Ok(file.messages)
        }
        FileFormat::Jsonl => read_jsonl_all(&path),
    }
}

fn read_jsonl_all(path: &Path) -> Result<Vec<Message>> {
    let f = File::open(path)?;
    let reader = BufReader::new(f);
    let mut lines = reader.lines();
    let header_line = lines.next().ok_or_else(|| AppError::ParseFailed {
        what: "messages.jsonl".into(),
        msg: "missing header".into(),
    })??;
    let header: JsonlHeader = serde_json::from_str(&header_line)?;
    check_schema_version(header.schema_version)?;
    let mut messages = Vec::new();
    for line in lines {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        messages.push(serde_json::from_str::<Message>(&line)?);
    }
    Ok(messages)
}

pub fn save_messages(data_dir: &Path, workspace_id: &str, messages: &[Message]) -> Result<()> {
    write_jsonl(&messages_file(data_dir, workspace_id), messages)
}

fn write_jsonl(path: &Path, messages: &[Message]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = File::create(&tmp)?;
        let header = JsonlHeader {
            schema_version: SCHEMA_VERSION,
        };
        writeln!(f, "{}", serde_json::to_string(&header)?)?;
        for m in messages {
            writeln!(f, "{}", serde_json::to_string(m)?)?;
        }
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Appends a single message. Empty workspaces get a fresh JSONL file; v1
/// files migrate to JSONL transparently on first append after upgrade; v2
/// files take the fast O(1) append path with a substring + parse dedup pass.
///
/// Propagates errors from `detect_format` / `load_messages` so an
/// unknown-schema file is surfaced to the caller instead of being silently
/// overwritten.
pub fn append_message(data_dir: &Path, workspace_id: &str, msg: &Message) -> Result<()> {
    let path = messages_file(data_dir, workspace_id);
    match detect_format(&path)? {
        FileFormat::Empty => write_jsonl(&path, std::slice::from_ref(msg)),
        FileFormat::LegacyJson => {
            // Migrate v1 → v2 on first append after upgrade.
            let mut current = load_messages(data_dir, workspace_id)?;
            if current.iter().any(|m| m.id == msg.id) {
                return Ok(());
            }
            current.push(msg.clone());
            write_jsonl(&path, &current)
        }
        FileFormat::Jsonl => {
            if jsonl_contains_id(&path, &msg.id)? {
                return Ok(());
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut f = OpenOptions::new().append(true).open(&path)?;
            writeln!(f, "{}", serde_json::to_string(msg)?)?;
            Ok(())
        }
    }
}

fn jsonl_contains_id(path: &Path, id: &str) -> Result<bool> {
    let f = File::open(path)?;
    let reader = BufReader::new(f);
    for line in reader.lines() {
        let line = line?;
        // Cheap pre-check before paying for serde_json::from_str.
        if !line.contains(id) {
            continue;
        }
        if let Ok(m) = serde_json::from_str::<Message>(&line) {
            if m.id == id {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Reads the last `n` complete message lines of a JSONL file by seeking
/// backward in 8 KiB chunks. Returns at most `n` messages in chronological
/// (file-insertion) order.
fn read_jsonl_tail(path: &Path, n: usize) -> Result<Vec<Message>> {
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    if len == 0 {
        return Ok(Vec::new());
    }
    let chunk_size: u64 = 8 * 1024;
    let mut buf: Vec<u8> = Vec::new();
    let mut pos = len;
    // n message lines + 1 header line + 1 leading partial line = n+2 newlines
    // is the worst case when our seek lands mid-line. Stop reading once the
    // buffer holds enough newlines or we've read the whole file.
    while pos > 0 {
        let read = chunk_size.min(pos);
        pos -= read;
        f.seek(SeekFrom::Start(pos))?;
        let mut chunk = vec![0u8; read as usize];
        f.read_exact(&mut chunk)?;
        chunk.extend_from_slice(&buf);
        buf = chunk;
        let newlines = buf.iter().filter(|&&b| b == b'\n').count();
        if newlines > n + 1 {
            break;
        }
    }
    let from_start = pos == 0;
    let raw = String::from_utf8_lossy(&buf);
    let mut iter = raw.lines();
    if from_start {
        // Skip the schema-version header on the very first line.
        let header_line = iter.next().unwrap_or("");
        let header: JsonlHeader = serde_json::from_str(header_line)?;
        check_schema_version(header.schema_version)?;
    } else {
        // We landed mid-line; the first piece is a partial fragment.
        iter.next();
    }
    let mut messages: Vec<Message> = Vec::new();
    for line in iter {
        if line.trim().is_empty() {
            continue;
        }
        // Defensively skip any header-shaped line; messages always have an id.
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if value.get("id").is_none() {
            continue;
        }
        messages.push(serde_json::from_value::<Message>(value)?);
    }
    let start = messages.len().saturating_sub(n);
    Ok(messages[start..].to_vec())
}

/// Default page size for `list_messages` when no limit is supplied.
pub const DEFAULT_MESSAGE_PAGE: usize = 50;

/// Returns the most recent `limit` messages older than `before_id`, in
/// chronological order (oldest first). When `before_id` is `None`, returns
/// the latest page. When `before_id` is unknown, returns an empty slice so
/// the frontend can stop paginating.
///
/// JSONL files take the tail-read fast path when `before_id` is `None`;
/// older-page requests still walk the whole file (rare in practice — users
/// rarely paginate past 5 pages).
pub fn list_messages_paginated(
    data_dir: &Path,
    workspace_id: &str,
    limit: Option<usize>,
    before_id: Option<&str>,
) -> Result<Vec<Message>> {
    let limit = limit.unwrap_or(DEFAULT_MESSAGE_PAGE).max(1);
    let path = messages_file(data_dir, workspace_id);
    match detect_format(&path)? {
        FileFormat::Empty => Ok(Vec::new()),
        FileFormat::LegacyJson => {
            slice_before(&load_messages(data_dir, workspace_id)?, limit, before_id)
        }
        FileFormat::Jsonl => {
            if before_id.is_some() {
                slice_before(&load_messages(data_dir, workspace_id)?, limit, before_id)
            } else {
                read_jsonl_tail(&path, limit)
            }
        }
    }
}

fn slice_before(all: &[Message], limit: usize, before_id: Option<&str>) -> Result<Vec<Message>> {
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
            attachments: Vec::new(),
        }
    }

    /// Test-only helper that writes a v1 (legacy) `{schema_version, messages}`
    /// fixture so we can verify load + migration paths without leaking the
    /// legacy writer into production code.
    fn save_messages_legacy_v1(data_dir: &Path, workspace_id: &str, messages: &[Message]) {
        let path = messages_file(data_dir, workspace_id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let body = serde_json::json!({
            "schema_version": 1,
            "messages": messages,
        });
        std::fs::write(&path, serde_json::to_string(&body).unwrap()).unwrap();
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
    fn save_messages_writes_jsonl_with_schema_version_header() {
        let tmp = TempDir::new().unwrap();
        save_messages(tmp.path(), "ws_x", &[make_msg("msg_a", "ws_x")]).unwrap();
        let path = messages_file(tmp.path(), "ws_x");
        let raw = std::fs::read_to_string(&path).unwrap();
        let mut lines = raw.lines();
        let header_line = lines.next().unwrap();
        let header: serde_json::Value = serde_json::from_str(header_line).unwrap();
        assert_eq!(header["schema_version"], SCHEMA_VERSION);
        let msg_line = lines.next().unwrap();
        let msg: serde_json::Value = serde_json::from_str(msg_line).unwrap();
        assert_eq!(msg["id"], "msg_a");
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

    #[test]
    fn append_message_writes_jsonl_when_file_is_empty() {
        let tmp = TempDir::new().unwrap();
        append_message(tmp.path(), "ws_new", &make_msg("msg_a", "ws_new")).unwrap();
        let raw = std::fs::read_to_string(messages_file(tmp.path(), "ws_new")).unwrap();
        let mut lines = raw.lines();
        let header: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(header["schema_version"], SCHEMA_VERSION);
        let msg: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(msg["id"], "msg_a");
        assert!(lines.next().is_none());
    }

    #[test]
    fn append_message_takes_fast_path_on_jsonl_files() {
        let tmp = TempDir::new().unwrap();
        // Two appends — second must not rewrite the file from scratch.
        append_message(tmp.path(), "ws_fast", &make_msg("msg_1", "ws_fast")).unwrap();
        append_message(tmp.path(), "ws_fast", &make_msg("msg_2", "ws_fast")).unwrap();
        let raw = std::fs::read_to_string(messages_file(tmp.path(), "ws_fast")).unwrap();
        let lines: Vec<&str> = raw.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 messages
        let m1: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        let m2: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(m1["id"], "msg_1");
        assert_eq!(m2["id"], "msg_2");
    }

    #[test]
    fn append_message_migrates_legacy_v1_to_jsonl() {
        let tmp = TempDir::new().unwrap();
        save_messages_legacy_v1(
            tmp.path(),
            "ws_legacy",
            &[
                make_msg("msg_a", "ws_legacy"),
                make_msg("msg_b", "ws_legacy"),
            ],
        );
        // Sanity check: starts as legacy.
        let path = messages_file(tmp.path(), "ws_legacy");
        assert_eq!(detect_format(&path).unwrap(), FileFormat::LegacyJson);

        append_message(tmp.path(), "ws_legacy", &make_msg("msg_c", "ws_legacy")).unwrap();

        // After append, file must be JSONL.
        assert_eq!(detect_format(&path).unwrap(), FileFormat::Jsonl);
        let loaded = load_messages(tmp.path(), "ws_legacy").unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].id, "msg_a");
        assert_eq!(loaded[1].id, "msg_b");
        assert_eq!(loaded[2].id, "msg_c");
    }

    #[test]
    fn load_messages_handles_legacy_v1_format() {
        let tmp = TempDir::new().unwrap();
        save_messages_legacy_v1(tmp.path(), "ws_legacy", &[make_msg("msg_a", "ws_legacy")]);
        let loaded = load_messages(tmp.path(), "ws_legacy").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "msg_a");
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

    #[test]
    fn list_messages_paginated_handles_legacy_v1_format() {
        let tmp = TempDir::new().unwrap();
        let msgs: Vec<Message> = (1..=5)
            .map(|i| make_msg(&format!("msg_{i:03}"), "ws_legacy"))
            .collect();
        save_messages_legacy_v1(tmp.path(), "ws_legacy", &msgs);
        let out = list_messages_paginated(tmp.path(), "ws_legacy", None, None).unwrap();
        assert_eq!(out.len(), 5);
        assert_eq!(out[0].id, "msg_001");
        assert_eq!(out[4].id, "msg_005");
    }

    #[test]
    fn list_messages_paginated_tail_reads_large_jsonl_file() {
        let tmp = TempDir::new().unwrap();
        // 1000 messages keeps the file well above the 8 KiB chunk boundary.
        write_history(&tmp, "ws_jsonl", 1000);
        let path = messages_file(tmp.path(), "ws_jsonl");
        assert_eq!(detect_format(&path).unwrap(), FileFormat::Jsonl);

        let out = list_messages_paginated(tmp.path(), "ws_jsonl", Some(10), None).unwrap();
        assert_eq!(out.len(), 10);
        assert_eq!(out.first().unwrap().id, "msg_991");
        assert_eq!(out.last().unwrap().id, "msg_1000");
    }

    #[test]
    fn check_schema_version_accepts_known() {
        for v in KNOWN_SCHEMA_VERSIONS {
            assert!(check_schema_version(*v).is_ok());
        }
    }

    #[test]
    fn check_schema_version_rejects_unknown() {
        let err = check_schema_version(999).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("schema"), "msg should mention schema: {msg}");
        assert!(msg.contains("999"), "msg should mention version: {msg}");
    }

    #[test]
    fn load_messages_returns_err_when_schema_version_is_unknown() {
        let tmp = TempDir::new().unwrap();
        let path = messages_file(tmp.path(), "ws_future");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Fabricate a v999 file using the legacy JSON layout.
        let bogus = serde_json::json!({
            "schema_version": 999,
            "messages": []
        });
        std::fs::write(&path, serde_json::to_string(&bogus).unwrap()).unwrap();

        let result = load_messages(tmp.path(), "ws_future");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("schema"));
    }

    #[test]
    fn load_messages_returns_err_when_jsonl_header_is_unknown_version() {
        let tmp = TempDir::new().unwrap();
        let path = messages_file(tmp.path(), "ws_future_jsonl");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = File::create(&path).unwrap();
        writeln!(f, "{{\"schema_version\":999}}").unwrap();
        let m = make_msg("msg_a", "ws_future_jsonl");
        writeln!(f, "{}", serde_json::to_string(&m).unwrap()).unwrap();
        drop(f);

        let result = load_messages(tmp.path(), "ws_future_jsonl");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("schema"));
    }

    #[test]
    fn list_messages_paginated_returns_err_when_schema_version_is_unknown() {
        let tmp = TempDir::new().unwrap();
        let path = messages_file(tmp.path(), "ws_future");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bogus = serde_json::json!({
            "schema_version": 999,
            "messages": []
        });
        std::fs::write(&path, serde_json::to_string(&bogus).unwrap()).unwrap();

        let result = list_messages_paginated(tmp.path(), "ws_future", None, None);
        assert!(result.is_err());
    }

    #[test]
    fn load_messages_accepts_known_versions_after_save() {
        let tmp = TempDir::new().unwrap();
        save_messages(tmp.path(), "ws_v2", &[make_msg("msg_a", "ws_v2")]).unwrap();
        let loaded = load_messages(tmp.path(), "ws_v2").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "msg_a");
    }

    #[test]
    fn detect_format_classifies_each_kind() {
        let tmp = TempDir::new().unwrap();
        let path = messages_file(tmp.path(), "ws_fmt");
        // missing → Empty
        assert_eq!(detect_format(&path).unwrap(), FileFormat::Empty);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // empty file → Empty
        std::fs::write(&path, "").unwrap();
        assert_eq!(detect_format(&path).unwrap(), FileFormat::Empty);
        // legacy v1
        save_messages_legacy_v1(tmp.path(), "ws_fmt", &[make_msg("msg_a", "ws_fmt")]);
        assert_eq!(detect_format(&path).unwrap(), FileFormat::LegacyJson);
        // jsonl v2
        save_messages(tmp.path(), "ws_fmt", &[make_msg("msg_a", "ws_fmt")]).unwrap();
        assert_eq!(detect_format(&path).unwrap(), FileFormat::Jsonl);
    }
}
