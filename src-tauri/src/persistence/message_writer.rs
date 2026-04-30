//! Debounced message-append writer.
//!
//! `DebouncedWriter` has last-write-wins-per-path semantics, which is the
//! right fit for whole-file snapshots (workspaces.json, sessions.json) but
//! the wrong fit for "accumulate appends": each queue snapshot would
//! capture stale on-disk state and the closure that ultimately runs would
//! contain only the latest message.
//!
//! `MessageWriter` adds an in-memory pending list per workspace that is
//! drained at flush time via the JSONL fast path of `append_message`. The
//! wrapped `DebouncedWriter` is used purely as a debounced timer that
//! triggers the drain — burst events collapse to a single drain call per
//! 500 ms window, keeping disk write count low without losing messages.

use crate::error::Result;
use crate::persistence::debounce::DebouncedWriter;
use crate::persistence::messages::append_message;
use crate::platform::paths::messages_file;
use crate::state::Message;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Per-workspace pending-message map keyed by (data_dir, workspace_id).
type PendingMap = Arc<Mutex<HashMap<(PathBuf, String), Vec<Message>>>>;

#[derive(Clone)]
pub struct MessageWriter {
    debouncer: DebouncedWriter,
    /// Per-(data_dir, workspace_id) queue of messages waiting to be
    /// flushed. The `data_dir` lives in the key so unit tests sharing one
    /// process can target distinct tempdirs without colliding.
    pending: PendingMap,
}

impl MessageWriter {
    pub fn new(debounce: Duration) -> Self {
        Self {
            debouncer: DebouncedWriter::new(debounce),
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Enqueues a message for debounced persistence. Duplicate ids in the
    /// same pending batch are skipped so streaming events that re-emit the
    /// same id don't produce duplicate rows. On-disk dedup is owned by
    /// `append_message`.
    pub fn queue(&self, data_dir: &Path, workspace_id: &str, msg: Message) -> Result<()> {
        let key = (data_dir.to_path_buf(), workspace_id.to_string());
        {
            let mut pending = self
                .pending
                .lock()
                .map_err(|e| crate::error::AppError::Other(format!("pending lock: {e}")))?;
            let bucket = pending.entry(key.clone()).or_default();
            if bucket.iter().any(|m| m.id == msg.id) {
                return Ok(());
            }
            bucket.push(msg);
        }
        // The closure replaces any pending closure for the same path; only
        // the latest survives in the debouncer queue. That's fine — every
        // closure performs the same drain operation, so collapsing them to
        // one is exactly the desired behaviour.
        let path = messages_file(data_dir, workspace_id);
        let pending = self.pending.clone();
        let data_dir_owned = data_dir.to_path_buf();
        let workspace_id_owned = workspace_id.to_string();
        self.debouncer.queue_with(path, move || {
            drain_and_persist(&pending, &data_dir_owned, &workspace_id_owned);
        })
    }

    /// Flushes any pending writes synchronously. Call on app shutdown so
    /// in-flight messages aren't lost.
    pub async fn flush_all(&self) {
        self.debouncer.flush_all().await;
    }
}

fn drain_and_persist(pending: &PendingMap, data_dir: &Path, workspace_id: &str) {
    let key = (data_dir.to_path_buf(), workspace_id.to_string());
    let drained = match pending.lock() {
        Ok(mut p) => p.remove(&key).unwrap_or_default(),
        Err(e) => {
            tracing::warn!(error = %e, "MessageWriter: pending lock poisoned");
            return;
        }
    };
    for msg in drained {
        if let Err(e) = append_message(data_dir, workspace_id, &msg) {
            tracing::warn!(
                workspace_id = %workspace_id,
                error = %e,
                "MessageWriter: append_message failed during drain"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::messages::load_messages;
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

    #[tokio::test]
    async fn burst_writes_persist_all_messages_after_flush() {
        let tmp = TempDir::new().unwrap();
        let writer = MessageWriter::new(Duration::from_millis(50));

        for i in 0..10 {
            writer
                .queue(
                    tmp.path(),
                    "ws_burst",
                    make_msg(&format!("msg_{i}"), "ws_burst"),
                )
                .unwrap();
        }
        writer.flush_all().await;

        let on_disk = load_messages(tmp.path(), "ws_burst").unwrap();
        assert_eq!(on_disk.len(), 10);
        for (i, m) in on_disk.iter().enumerate() {
            assert_eq!(m.id, format!("msg_{i}"));
        }
    }

    #[tokio::test]
    async fn duplicate_ids_in_same_burst_are_deduped() {
        let tmp = TempDir::new().unwrap();
        let writer = MessageWriter::new(Duration::from_millis(50));

        let m = make_msg("msg_dup", "ws_dup");
        writer.queue(tmp.path(), "ws_dup", m.clone()).unwrap();
        writer.queue(tmp.path(), "ws_dup", m.clone()).unwrap();
        writer.queue(tmp.path(), "ws_dup", m.clone()).unwrap();
        writer.flush_all().await;

        let on_disk = load_messages(tmp.path(), "ws_dup").unwrap();
        assert_eq!(on_disk.len(), 1);
    }

    #[tokio::test]
    async fn duplicate_ids_across_separate_flushes_are_deduped() {
        let tmp = TempDir::new().unwrap();
        let writer = MessageWriter::new(Duration::from_millis(50));

        let m = make_msg("msg_dup_xf", "ws_dup_xf");
        // First flush persists the message.
        writer.queue(tmp.path(), "ws_dup_xf", m.clone()).unwrap();
        writer.flush_all().await;
        // Second flush queues the same id again; on-disk dedup in
        // append_message must keep the file at length 1.
        writer.queue(tmp.path(), "ws_dup_xf", m.clone()).unwrap();
        writer.flush_all().await;

        let on_disk = load_messages(tmp.path(), "ws_dup_xf").unwrap();
        assert_eq!(on_disk.len(), 1);
    }

    #[tokio::test]
    async fn distinct_workspaces_drain_independently() {
        let tmp = TempDir::new().unwrap();
        let writer = MessageWriter::new(Duration::from_millis(50));

        writer
            .queue(tmp.path(), "ws_a", make_msg("msg_a1", "ws_a"))
            .unwrap();
        writer
            .queue(tmp.path(), "ws_b", make_msg("msg_b1", "ws_b"))
            .unwrap();
        writer.flush_all().await;

        let a = load_messages(tmp.path(), "ws_a").unwrap();
        let b = load_messages(tmp.path(), "ws_b").unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        assert_eq!(a[0].id, "msg_a1");
        assert_eq!(b[0].id, "msg_b1");
    }

    #[tokio::test]
    async fn flush_drains_pending_immediately() {
        let tmp = TempDir::new().unwrap();
        // Long debounce — without flush, nothing should land on disk.
        let writer = MessageWriter::new(Duration::from_secs(60));
        writer
            .queue(tmp.path(), "ws_flush", make_msg("msg_a", "ws_flush"))
            .unwrap();
        writer.flush_all().await;

        let on_disk = load_messages(tmp.path(), "ws_flush").unwrap();
        assert_eq!(on_disk.len(), 1);
    }
}
