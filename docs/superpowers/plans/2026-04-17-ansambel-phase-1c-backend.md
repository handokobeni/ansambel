# Ansambel — Phase 1c Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking. Execute **after** Phase 1b is merged.

**Goal:** Build the agent backend layer — spawn
`claude -p --output-format stream-json --verbose` inside a workspace worktree
via PTY, parse NDJSON stream-json output, persist messages, send user input
through stdin, and stop agents cleanly. Phase 1c-frontend will render the chat
UI on top of these primitives.

**Architecture:** Add a fourth runtime entity (`AgentHandle`) to `AppState`;
runtime-only (not persisted — agents are dead after restart). `Message` struct
is persisted in `messages/<workspace-id>.json` (debounced 500ms).
`platform/pty.rs` wraps `portable-pty`'s cross-platform PTY (ConPTY on Windows,
openpty on Unix). A reader thread parses Claude's stream-json NDJSON into typed
`AgentEvent`s and forwards them to the frontend through the Tauri Channel API.
Stdin writes go through a `tokio::sync::mpsc` channel to a writer thread,
decoupling the command handler from blocking PTY writes.

**Tech Stack:** Rust 1.82, Tauri v2 with `tauri::ipc::Channel`, **new deps:**
`portable-pty = "0.8"`, `tokio` (already pulled in by Tauri but add explicit
`sync` and `process` features). Reuses Phase 0/1a/1b infrastructure:
`platform::paths::messages_file`, `platform::binary::claude_binary`,
`persistence` debounce + atomic write, `ids` generator.

**Prerequisite:** Phase 1b-frontend merged (PR #9, kanban + Plan/Work mode).

---

## Table of Contents

1. [Task 1](#task-1-add-portable-pty--tokio-feature-deps) — `portable-pty` +
   `tokio` deps (compile check)
2. [Task 2](#task-2-add-message--messagerole-types-to-staters) — `Message` +
   `MessageRole` enum (8 tests)
3. [Task 3](#task-3-add-message_id-to-idsrs) — `message_id()` generator (3
   tests)
4. [Task 4](#task-4-persistencemessagesrs--load--save--debounce-helper) —
   `persistence/messages.rs` (6 tests)
5. [Task 5](#task-5-add-agentevent-types-to-staters) — `AgentEvent`,
   `AgentStatus`, `SessionInit` types (5 tests)
6. [Task 6](#task-6-add-agenthandle--agents-field-to-appstate) — `AgentHandle`
   - `agents` runtime field on `AppState` (3 tests)
7. [Task 7](#task-7-platformptyrs--cross-platform-pty-wrapper) —
   `platform/pty.rs` (6 tests)
8. [Task 8](#task-8-commandsagent_streamrs--ndjson-stream-json-parser) —
   `commands/agent_stream.rs` NDJSON parser (10 tests)
9. [Task 9](#task-9-commandsagentrsspawn_agent--context-injection) —
   `spawn_agent` command + context injection (7 tests)
10. [Task 10](#task-10-commandsagentrssend_message) — `send_message` command (4
    tests)
11. [Task 11](#task-11-commandsagentrsstop_agent) — `stop_agent` command (4
    tests)
12. [Task 12](#task-12-wire-commands-into-librs--capabilities) — register
    commands + permissions (2 integration tests)

---

## Task 1: Add `portable-pty` + `tokio` feature deps

**Files:**

- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1.1: Write failing test**

```rust
// Add to src-tauri/src/platform/mod.rs at the top level:
#[cfg(test)]
mod dep_tests {
    #[test]
    fn portable_pty_is_resolvable() {
        // Compile-only: ensure the crate is in scope.
        let _ = std::any::type_name::<portable_pty::PtySize>();
    }
}
```

- [ ] **Step 1.2: Run check to verify it fails**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo check --tests 2>&1 | tail -10
```

Expected: `error[E0432]: unresolved import 'portable_pty'` — crate not declared.

- [ ] **Step 1.3: Implement**

In `src-tauri/Cargo.toml` `[dependencies]` (alphabetical), add:

```toml
portable-pty = "0.8"
tokio = { version = "1", features = ["sync", "process", "io-util", "rt-multi-thread", "macros"] }
```

(`tokio` may already be pulled in transitively by Tauri; making it explicit
ensures the `sync` / `process` features we rely on are enabled.)

- [ ] **Step 1.4: Run check to verify it passes**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib platform::dep_tests 2>&1 | tail -10
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 1.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/platform/mod.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add portable-pty 0.8 and explicit tokio features for agent runtime

portable-pty is the cross-platform PTY abstraction (ConPTY on Windows,
openpty on Unix). Tokio sync/process features are needed for the agent
stdin mpsc channel and child process management.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Add `Message` + `MessageRole` types to `state.rs`

**Files:**

- Modify: `src-tauri/src/state.rs`

- [ ] **Step 2.1: Write failing tests**

```rust
// Add to src-tauri/src/state.rs  #[cfg(test)] mod tests:

#[test]
fn message_role_round_trips_json() {
    for (role, want) in [
        (MessageRole::User, "\"user\""),
        (MessageRole::Assistant, "\"assistant\""),
        (MessageRole::System, "\"system\""),
        (MessageRole::Tool, "\"tool\""),
    ] {
        let s = serde_json::to_string(&role).unwrap();
        assert_eq!(s, want, "role {role:?}");
    }
}

#[test]
fn message_role_default_is_user() {
    assert_eq!(MessageRole::default(), MessageRole::User);
}

#[test]
fn message_round_trips_json() {
    let m = Message {
        id: "msg_abc123".into(),
        workspace_id: "ws_xyz".into(),
        role: MessageRole::Assistant,
        text: "Hello world".into(),
        is_partial: false,
        tool_use: None,
        tool_result: None,
        created_at: 1_776_000_000,
    };
    let json = serde_json::to_string(&m).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(back, m);
}

#[test]
fn message_partial_flag_serializes() {
    let m = Message {
        id: "msg_p1".into(),
        workspace_id: "ws_a".into(),
        role: MessageRole::Assistant,
        text: "streaming...".into(),
        is_partial: true,
        tool_use: None,
        tool_result: None,
        created_at: 0,
    };
    let json = serde_json::to_string(&m).unwrap();
    assert!(json.contains("\"is_partial\":true"));
}

#[test]
fn message_tool_use_optional() {
    let plain = Message {
        id: "msg_x".into(),
        workspace_id: "ws_a".into(),
        role: MessageRole::Assistant,
        text: "no tools".into(),
        is_partial: false,
        tool_use: None,
        tool_result: None,
        created_at: 0,
    };
    let json = serde_json::to_string(&plain).unwrap();
    assert!(json.contains("\"tool_use\":null"));
}

#[test]
fn message_tool_use_round_trip() {
    let m = Message {
        id: "msg_t".into(),
        workspace_id: "ws_a".into(),
        role: MessageRole::Assistant,
        text: String::new(),
        is_partial: false,
        tool_use: Some(ToolUse {
            id: "toolu_01".into(),
            name: "Read".into(),
            input: serde_json::json!({"path": "/etc/hosts"}),
        }),
        tool_result: None,
        created_at: 0,
    };
    let json = serde_json::to_string(&m).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(back, m);
}

#[test]
fn message_tool_result_round_trip() {
    let m = Message {
        id: "msg_r".into(),
        workspace_id: "ws_a".into(),
        role: MessageRole::Tool,
        text: String::new(),
        is_partial: false,
        tool_use: None,
        tool_result: Some(ToolResult {
            tool_use_id: "toolu_01".into(),
            content: "127.0.0.1 localhost".into(),
            is_error: false,
        }),
        created_at: 0,
    };
    let json = serde_json::to_string(&m).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(back, m);
}

#[test]
fn message_role_lowercase_in_json() {
    let m = Message {
        id: "msg_r".into(),
        workspace_id: "ws_a".into(),
        role: MessageRole::User,
        text: "hi".into(),
        is_partial: false,
        tool_use: None,
        tool_result: None,
        created_at: 0,
    };
    let json = serde_json::to_string(&m).unwrap();
    assert!(json.contains("\"role\":\"user\""));
}
```

- [ ] **Step 2.2: Run tests to verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state::tests::message 2>&1 | tail -15
```

Expected: compile errors — `Message`, `MessageRole`, `ToolUse`, `ToolResult` not
found.

- [ ] **Step 2.3: Implement**

Add to `src-tauri/src/state.rs` after the `Task` struct, before
`fn app_version()`:

```rust
#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    #[default]
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub content: String,
    pub is_error: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Message {
    pub id: String,                    // prefix `msg_`
    pub workspace_id: String,
    pub role: MessageRole,
    pub text: String,
    pub is_partial: bool,              // true while streaming
    pub tool_use: Option<ToolUse>,
    pub tool_result: Option<ToolResult>,
    pub created_at: i64,
}
```

- [ ] **Step 2.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state::tests::message 2>&1 | tail -10
```

Expected: `test result: ok. 8 passed`.

- [ ] **Step 2.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/state.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add Message, MessageRole, ToolUse, ToolResult types

Message wraps user/assistant/tool exchanges. is_partial supports streaming
chunks during a single assistant turn. tool_use/tool_result are optional
side-channel structs for Claude tool invocations.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Add `message_id()` to `ids.rs`

**Files:**

- Modify: `src-tauri/src/ids.rs`

- [ ] **Step 3.1: Write failing tests**

```rust
// Append to src-tauri/src/ids.rs #[cfg(test)] mod tests:

#[test]
fn message_id_has_prefix_and_length() {
    let id = message_id();
    assert!(id.starts_with("msg_"), "expected msg_ prefix, got {id}");
    assert_eq!(id.len(), "msg_".len() + 6);
}

#[test]
fn message_id_uses_only_allowed_alphabet() {
    let id = message_id();
    let body = id.strip_prefix("msg_").unwrap();
    for c in body.chars() {
        assert!(
            (c.is_ascii_alphanumeric() && c.is_ascii_lowercase()) || c.is_ascii_digit(),
            "Unexpected char {c:?} in id {id}"
        );
    }
}

#[test]
fn message_id_no_collisions() {
    let set: std::collections::HashSet<String> = (0..1_000).map(|_| message_id()).collect();
    assert_eq!(set.len(), 1_000);
}
```

> Note: `message_id()` already exists in `ids.rs` from Phase 0 (look for the
> existing `pub fn message_id()` definition). If it's already present, this task
> only adds tests — skip Step 3.3.

- [ ] **Step 3.2: Run tests**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib ids::tests::message_id 2>&1 | tail -15
```

If tests fail with `function 'message_id' not found`, proceed to 3.3. If they
pass: `message_id()` already exists; commit only the new tests in 3.5.

- [ ] **Step 3.3: Implement (if needed)**

Add to `src-tauri/src/ids.rs` after the existing id functions:

```rust
pub fn message_id() -> String {
    format!("msg_{}", id_body())
}
```

- [ ] **Step 3.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib ids::tests::message_id 2>&1 | tail -10
```

Expected: `test result: ok. 3 passed`.

- [ ] **Step 3.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/ids.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add message_id() generator and tests

Follows the same nanoid(6, ALPHABET) pattern as task_id, repo_id, etc.
Prefix msg_ is short and visually distinct.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: `persistence/messages.rs` — load + save + debounce helper

**Files:**

- Create: `src-tauri/src/persistence/messages.rs`
- Modify: `src-tauri/src/persistence/mod.rs`

- [ ] **Step 4.1: Write failing tests**

```rust
// New file src-tauri/src/persistence/messages.rs:

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
        save_messages(tmp.path(), "ws_x", &vec![m1.clone(), m2.clone()]).unwrap();
        let loaded = load_messages(tmp.path(), "ws_x").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0], m1);
        assert_eq!(loaded[1], m2);
    }

    #[test]
    fn save_messages_writes_schema_version() {
        let tmp = TempDir::new().unwrap();
        save_messages(tmp.path(), "ws_x", &vec![make_msg("msg_a", "ws_x")]).unwrap();
        let path = crate::platform::paths::messages_file(tmp.path(), "ws_x");
        let raw = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["schema_version"], 1);
    }

    #[test]
    fn save_messages_creates_parent_dir() {
        let tmp = TempDir::new().unwrap();
        // Don't pre-create messages/ subdir; save_messages should mkdir -p.
        save_messages(tmp.path(), "ws_y", &vec![make_msg("msg_a", "ws_y")]).unwrap();
        assert!(tmp.path().join("messages").is_dir());
    }

    #[test]
    fn load_messages_handles_per_workspace_isolation() {
        let tmp = TempDir::new().unwrap();
        save_messages(tmp.path(), "ws_a", &vec![make_msg("msg_x", "ws_a")]).unwrap();
        save_messages(tmp.path(), "ws_b", &vec![make_msg("msg_y", "ws_b")]).unwrap();
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
        save_messages(tmp.path(), "ws_z", &vec![make_msg("msg_1", "ws_z")]).unwrap();
        let mut current = load_messages(tmp.path(), "ws_z").unwrap();
        current.push(make_msg("msg_2", "ws_z"));
        save_messages(tmp.path(), "ws_z", &current).unwrap();
        let loaded = load_messages(tmp.path(), "ws_z").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, "msg_1");
        assert_eq!(loaded[1].id, "msg_2");
    }
}
```

- [ ] **Step 4.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib persistence::messages 2>&1 | tail -15
```

Expected: compile error — `load_messages` / `save_messages` not found.

- [ ] **Step 4.3: Implement**

Replace the test scaffold in `src-tauri/src/persistence/messages.rs` with:

```rust
//! Per-workspace message persistence at `<data_dir>/messages/<ws-id>.json`.

use crate::error::AppResult;
use crate::persistence::atomic::{load_or_default, write_atomic};
use crate::state::Message;
use serde::{Deserialize, Serialize};
use std::path::Path;

const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Default)]
struct OnDisk {
    schema_version: u32,
    messages: Vec<Message>,
}

pub fn load_messages(data_dir: &Path, workspace_id: &str) -> AppResult<Vec<Message>> {
    let path = crate::platform::paths::messages_file(data_dir, workspace_id);
    let on_disk: OnDisk = load_or_default(&path)?;
    Ok(on_disk.messages)
}

pub fn save_messages(data_dir: &Path, workspace_id: &str, messages: &[Message]) -> AppResult<()> {
    let path = crate::platform::paths::messages_file(data_dir, workspace_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let on_disk = OnDisk {
        schema_version: SCHEMA_VERSION,
        messages: messages.to_vec(),
    };
    write_atomic(&path, &serde_json::to_vec_pretty(&on_disk)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    // … the tests written in Step 4.1 stay here unchanged
}
```

In `src-tauri/src/persistence/mod.rs` add the module:

```rust
pub mod messages;
```

(alphabetical order alongside `repos`, `settings`, `tasks`, `workspaces`).

- [ ] **Step 4.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib persistence::messages 2>&1 | tail -10
```

Expected: `test result: ok. 6 passed`.

- [ ] **Step 4.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/persistence/messages.rs src-tauri/src/persistence/mod.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add persistence/messages.rs with load and save helpers

Stores Vec<Message> at <data_dir>/messages/<ws-id>.json with schema_version
wrapper. Reuses atomic writer + load_or_default from Phase 0. Creates the
messages/ subdir on first save.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Add `AgentEvent` types to `state.rs`

**Files:**

- Modify: `src-tauri/src/state.rs`

- [ ] **Step 5.1: Write failing tests**

```rust
// Append to src-tauri/src/state.rs  #[cfg(test)] mod tests:

#[test]
fn agent_status_round_trips_json() {
    for (s, want) in [
        (AgentStatus::Running, "\"running\""),
        (AgentStatus::Waiting, "\"waiting\""),
        (AgentStatus::Error, "\"error\""),
        (AgentStatus::Stopped, "\"stopped\""),
    ] {
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(j, want);
    }
}

#[test]
fn agent_event_message_serializes_with_type_tag() {
    let ev = AgentEvent::Message {
        id: "msg_a".into(),
        role: MessageRole::Assistant,
        text: "Hi".into(),
        is_partial: true,
    };
    let j = serde_json::to_string(&ev).unwrap();
    assert!(j.contains("\"type\":\"message\""));
    assert!(j.contains("\"is_partial\":true"));
}

#[test]
fn agent_event_status_serializes_with_type_tag() {
    let ev = AgentEvent::Status {
        status: AgentStatus::Running,
    };
    let j = serde_json::to_string(&ev).unwrap();
    assert!(j.contains("\"type\":\"status\""));
    assert!(j.contains("\"status\":\"running\""));
}

#[test]
fn agent_event_error_serializes() {
    let ev = AgentEvent::Error {
        message: "spawn failed".into(),
    };
    let j = serde_json::to_string(&ev).unwrap();
    assert!(j.contains("\"type\":\"error\""));
    assert!(j.contains("\"message\":\"spawn failed\""));
}

#[test]
fn agent_event_init_carries_session_id() {
    let ev = AgentEvent::Init {
        session_id: "ses_xyz".into(),
        model: "claude-sonnet-4-6".into(),
    };
    let j = serde_json::to_string(&ev).unwrap();
    assert!(j.contains("\"type\":\"init\""));
    assert!(j.contains("\"session_id\":\"ses_xyz\""));
}
```

- [ ] **Step 5.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state::tests::agent 2>&1 | tail -15
```

Expected: compile errors — `AgentEvent`, `AgentStatus` not found.

- [ ] **Step 5.3: Implement**

Add to `src-tauri/src/state.rs` after `Message`/`ToolUse`/`ToolResult`:

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Running,
    Waiting,
    Error,
    Stopped,
}

/// Streaming event from a running agent, sent over the Tauri Channel API.
/// All variants use struct form (not tuple) so the JSON shape is uniform:
/// `{"type":"status","status":"running"}`, `{"type":"error","message":"…"}`.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AgentEvent {
    Init {
        session_id: String,
        model: String,
    },
    Message {
        id: String,
        role: MessageRole,
        text: String,
        is_partial: bool,
    },
    ToolUse {
        message_id: String,
        tool_use: ToolUse,
    },
    ToolResult {
        message_id: String,
        tool_result: ToolResult,
    },
    Status {
        status: AgentStatus,
    },
    Error {
        message: String,
    },
}
```

> The struct-variant form for `Status` and `Error` is intentional — it keeps the
> JSON internally tagged and discriminable from the TypeScript side. Tests in
> Step 5.1 already use the struct form (`AgentEvent::Status { status: ... }`).

- [ ] **Step 5.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state::tests::agent 2>&1 | tail -10
```

Expected: `test result: ok. 5 passed`.

- [ ] **Step 5.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/state.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add AgentEvent and AgentStatus types

AgentEvent is the typed message we send to the frontend over
tauri::ipc::Channel. It uses internally tagged enum (#[serde(tag = "type")])
so the TS side can discriminate cleanly.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Add `AgentHandle` + `agents` field to `AppState`

**Files:**

- Modify: `src-tauri/src/state.rs`

- [ ] **Step 6.1: Write failing tests**

```rust
// Append to src-tauri/src/state.rs  #[cfg(test)] mod tests:

#[test]
fn app_state_has_agents_field() {
    let state = AppState::default();
    assert!(state.agents.is_empty());
}

#[test]
fn app_state_construction_with_agents_compiles() {
    let _state = AppState {
        repos: std::collections::HashMap::new(),
        workspaces: std::collections::HashMap::new(),
        tasks: std::collections::HashMap::new(),
        agents: std::collections::HashMap::new(),
        settings: AppSettings::default(),
    };
}

#[test]
fn agent_handle_has_required_fields() {
    use tokio::sync::mpsc;
    let (tx, _rx) = mpsc::unbounded_channel::<String>();
    let h = AgentHandle {
        workspace_id: "ws_xyz".into(),
        stdin_tx: tx,
        session_id: None,
    };
    assert_eq!(h.workspace_id, "ws_xyz");
    assert!(h.session_id.is_none());
}
```

- [ ] **Step 6.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state::tests 2>&1 | tail -15
```

Expected: errors — `AppState.agents`, `AgentHandle` not found.

- [ ] **Step 6.3: Implement**

In `src-tauri/src/state.rs`, replace `AppState` and add `AgentHandle`:

```rust
/// Runtime-only handle to a spawned Claude agent process. Not persisted —
/// dies on app restart, so workspace status resets `Running → Waiting`.
#[derive(Debug)]
pub struct AgentHandle {
    pub workspace_id: String,
    pub stdin_tx: tokio::sync::mpsc::UnboundedSender<String>,
    pub session_id: Option<String>,
}

#[derive(Default, Debug)]
pub struct AppState {
    pub repos: std::collections::HashMap<String, RepoInfo>,
    pub workspaces: std::collections::HashMap<String, WorkspaceInfo>,
    pub tasks: std::collections::HashMap<String, Task>,
    pub agents: std::collections::HashMap<String, AgentHandle>, // NEW
    pub settings: AppSettings,
}
```

> `AgentHandle` does not derive `Clone` (the mpsc Sender is clonable but we
> don't want to accidentally hold two senders to the same agent).

Update `src-tauri/src/lib.rs` `setup` block to include the agents field:

```rust
let state = crate::state::AppState {
    repos,
    workspaces,
    tasks,
    agents: std::collections::HashMap::new(),  // always empty on startup
    settings,
};
```

- [ ] **Step 6.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib 2>&1 | tail -15
```

Expected: all existing tests + 3 new ones pass.

- [ ] **Step 6.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/state.rs src-tauri/src/lib.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add AgentHandle struct and AppState.agents runtime field

AgentHandle holds the workspace id, stdin mpsc sender, and Claude session
id (set after Init event arrives). Runtime-only — never persisted. The
agents map is always initialized empty on startup.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: `platform/pty.rs` — cross-platform PTY wrapper

**Files:**

- Create: `src-tauri/src/platform/pty.rs`
- Modify: `src-tauri/src/platform/mod.rs`

- [ ] **Step 7.1: Write failing tests**

```rust
// New file src-tauri/src/platform/pty.rs (write skeleton + tests first):

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::time::Duration;

    fn echo_command() -> portable_pty::CommandBuilder {
        let mut cmd = if cfg!(windows) {
            let mut c = portable_pty::CommandBuilder::new("cmd");
            c.args(["/C", "echo hello"]);
            c
        } else {
            let mut c = portable_pty::CommandBuilder::new("sh");
            c.args(["-c", "echo hello"]);
            c
        };
        cmd.cwd(std::env::temp_dir());
        cmd
    }

    #[test]
    fn spawn_pty_returns_session() {
        let session = spawn(echo_command()).expect("spawn echo");
        assert!(session.pid() > 0);
    }

    #[test]
    fn spawn_pty_reads_stdout() {
        let session = spawn(echo_command()).expect("spawn echo");
        let reader = session.reader();
        let mut buf = String::new();
        let mut br = BufReader::new(reader);
        br.read_line(&mut buf).expect("read line");
        assert!(buf.contains("hello"), "got: {buf:?}");
    }

    #[test]
    fn spawn_pty_writes_stdin() {
        // sh -c "read X; echo got=$X" reads from stdin then exits.
        let mut cmd = if cfg!(windows) {
            let mut c = portable_pty::CommandBuilder::new("cmd");
            c.args(["/C", "set /p X=&& echo got=%X%"]);
            c
        } else {
            let mut c = portable_pty::CommandBuilder::new("sh");
            c.args(["-c", "read X; echo got=$X"]);
            c
        };
        cmd.cwd(std::env::temp_dir());
        let session = spawn(cmd).expect("spawn read");
        let mut writer = session.writer();
        writeln!(writer, "world").expect("write line");
        drop(writer);
        let reader = session.reader();
        let mut br = BufReader::new(reader);
        let mut out = String::new();
        // pty echoes the input back; loop until we see "got=".
        for _ in 0..10 {
            let mut line = String::new();
            if br.read_line(&mut line).is_err() {
                break;
            }
            out.push_str(&line);
            if out.contains("got=world") {
                break;
            }
        }
        assert!(out.contains("got=world"), "expected got=world, saw {out:?}");
    }

    #[test]
    fn pty_session_pid_is_stable() {
        let session = spawn(echo_command()).expect("spawn echo");
        let pid_a = session.pid();
        let pid_b = session.pid();
        assert_eq!(pid_a, pid_b);
    }

    #[test]
    fn pty_session_kill_terminates_child() {
        // Long-running sleep; kill, then verify the reader hits EOF.
        let mut cmd = if cfg!(windows) {
            let mut c = portable_pty::CommandBuilder::new("cmd");
            c.args(["/C", "ping -n 60 127.0.0.1"]);
            c
        } else {
            let mut c = portable_pty::CommandBuilder::new("sh");
            c.args(["-c", "sleep 60"]);
            c
        };
        cmd.cwd(std::env::temp_dir());
        let mut session = spawn(cmd).expect("spawn sleep");
        std::thread::sleep(Duration::from_millis(100));
        session.kill().expect("kill");
        // Wait for exit (poll up to 2s).
        for _ in 0..40 {
            if session.try_wait().expect("try_wait").is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("child did not exit within 2s of kill");
    }

    #[test]
    fn spawn_pty_unknown_binary_returns_err() {
        let cmd = portable_pty::CommandBuilder::new("definitely-not-a-real-binary-xyz");
        let result = spawn(cmd);
        assert!(result.is_err());
    }
}
```

- [ ] **Step 7.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib platform::pty 2>&1 | tail -15
```

Expected: compile error — `spawn`, `PtySession` not defined.

- [ ] **Step 7.3: Implement**

Replace `src-tauri/src/platform/pty.rs` body with:

```rust
//! Cross-platform PTY wrapper around `portable-pty`.
//!
//! Unix uses `fork()` + `openpty`. Windows uses ConPTY (Win 10 1809+).
//! The slave end is closed in the parent immediately after spawn so the
//! child receives EOF on its stdin when the writer is dropped.

use crate::error::{AppError, AppResult};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};

const DEFAULT_ROWS: u16 = 30;
const DEFAULT_COLS: u16 = 120;

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    pid: u32,
}

pub fn spawn(cmd: CommandBuilder) -> AppResult<PtySession> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| AppError::CommandFailed {
            cmd: "openpty".into(),
            msg: e.to_string(),
        })?;

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| AppError::CommandFailed {
            cmd: "pty.spawn".into(),
            msg: e.to_string(),
        })?;

    // Close slave in parent so child gets EOF when we drop our writer.
    drop(pair.slave);

    let pid = child.process_id().unwrap_or(0);
    Ok(PtySession {
        master: pair.master,
        child,
        pid,
    })
}

impl PtySession {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Returns a fresh `Read` handle on the master PTY. Each call clones
    /// the underlying file descriptor. Holding multiple readers
    /// concurrently is undefined; the caller should keep one reader.
    pub fn reader(&self) -> Box<dyn Read + Send> {
        self.master.try_clone_reader().expect("clone reader")
    }

    /// Returns a fresh `Write` handle on the master PTY.
    pub fn writer(&self) -> Box<dyn Write + Send> {
        self.master.take_writer().expect("take writer")
    }

    pub fn kill(&mut self) -> AppResult<()> {
        self.child.kill().map_err(|e| AppError::CommandFailed {
            cmd: "pty.kill".into(),
            msg: e.to_string(),
        })?;
        Ok(())
    }

    pub fn try_wait(&mut self) -> AppResult<Option<portable_pty::ExitStatus>> {
        self.child.try_wait().map_err(|e| AppError::CommandFailed {
            cmd: "pty.try_wait".into(),
            msg: e.to_string(),
        })
    }

    pub fn resize(&self, rows: u16, cols: u16) -> AppResult<()> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| AppError::CommandFailed {
                cmd: "pty.resize".into(),
                msg: e.to_string(),
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // (tests from Step 7.1 — keep unchanged)
}
```

In `src-tauri/src/platform/mod.rs` add:

```rust
pub mod pty;
```

(after the existing `pub mod paths;`).

- [ ] **Step 7.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib platform::pty 2>&1 | tail -10
```

Expected: `test result: ok. 6 passed`.

> If `pty_session_kill_terminates_child` flakes on Windows because `ping`
> ignores the kill, swap the Windows command to
> `["/C", "timeout /T 60 /NOBREAK"]` and re-run.

- [ ] **Step 7.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/platform/pty.rs src-tauri/src/platform/mod.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add platform/pty.rs cross-platform PTY wrapper

PtySession owns the master pty + child handle. Slave is closed in parent
right after spawn so child receives EOF when stdin writer is dropped.
Supports reader/writer/kill/try_wait/resize. Backed by portable-pty's
native_pty_system (ConPTY on Windows, openpty on Unix).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: `commands/agent_stream.rs` — NDJSON stream-json parser

**Files:**

- Create: `src-tauri/src/commands/agent_stream.rs`
- Modify: `src-tauri/src/commands/mod.rs`

Claude CLI's `--output-format stream-json` emits one JSON object per line. We
parse the subset Phase 1c needs:

| Type                                   | What we emit                                          |
| -------------------------------------- | ----------------------------------------------------- |
| `{"type":"system","subtype":"init",…}` | `AgentEvent::Init { session_id, model }`              |
| `{"type":"assistant","message":{…}}`   | `AgentEvent::Message { … }` (final, not partial)      |
| `{"type":"user","message":{…}}`        | `AgentEvent::Message { role: User, … }`               |
| `{"type":"result","subtype":"…"}`      | `AgentEvent::Status { status: AgentStatus::Stopped }` |

Tool use is parsed inline from `message.content[].type == "tool_use"`.

- [ ] **Step 8.1: Write failing tests**

```rust
// New file src-tauri/src/commands/agent_stream.rs:

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AgentEvent, AgentStatus, MessageRole};

    #[test]
    fn parses_system_init_into_agent_init() {
        let line = r#"{"type":"system","subtype":"init","session_id":"ses_abc","model":"claude-sonnet-4-6","tools":[],"cwd":"/tmp"}"#;
        let evs = parse_line(line).unwrap();
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::Init { session_id, model } => {
                assert_eq!(session_id, "ses_abc");
                assert_eq!(model, "claude-sonnet-4-6");
            }
            other => panic!("expected Init, got {other:?}"),
        }
    }

    #[test]
    fn parses_assistant_text_message() {
        let line = r#"{"type":"assistant","message":{"id":"msg_01","role":"assistant","content":[{"type":"text","text":"Hello!"}]}}"#;
        let evs = parse_line(line).unwrap();
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::Message { id, role, text, is_partial } => {
                assert_eq!(id, "msg_01");
                assert_eq!(role, &MessageRole::Assistant);
                assert_eq!(text, "Hello!");
                assert!(!is_partial);
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn parses_user_text_message() {
        let line = r#"{"type":"user","message":{"id":"msg_02","role":"user","content":[{"type":"text","text":"hi"}]}}"#;
        let evs = parse_line(line).unwrap();
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::Message { role, text, .. } => {
                assert_eq!(role, &MessageRole::User);
                assert_eq!(text, "hi");
            }
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn parses_assistant_with_multiple_text_blocks() {
        let line = r#"{"type":"assistant","message":{"id":"msg_03","role":"assistant","content":[{"type":"text","text":"part1 "},{"type":"text","text":"part2"}]}}"#;
        let evs = parse_line(line).unwrap();
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::Message { text, .. } => assert_eq!(text, "part1 part2"),
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn parses_tool_use_alongside_text() {
        let line = r#"{"type":"assistant","message":{"id":"msg_04","role":"assistant","content":[{"type":"text","text":"reading..."},{"type":"tool_use","id":"toolu_a","name":"Read","input":{"path":"/etc/hosts"}}]}}"#;
        let evs = parse_line(line).unwrap();
        assert_eq!(evs.len(), 2);
        assert!(matches!(evs[0], AgentEvent::Message { .. }));
        match &evs[1] {
            AgentEvent::ToolUse { message_id, tool_use } => {
                assert_eq!(message_id, "msg_04");
                assert_eq!(tool_use.name, "Read");
                assert_eq!(tool_use.id, "toolu_a");
            }
            _ => panic!("expected ToolUse"),
        }
    }

    #[test]
    fn parses_user_with_tool_result_block() {
        let line = r#"{"type":"user","message":{"id":"msg_05","role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_a","content":"127.0.0.1 localhost","is_error":false}]}}"#;
        let evs = parse_line(line).unwrap();
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::ToolResult { message_id, tool_result } => {
                assert_eq!(message_id, "msg_05");
                assert_eq!(tool_result.tool_use_id, "toolu_a");
                assert_eq!(tool_result.content, "127.0.0.1 localhost");
                assert!(!tool_result.is_error);
            }
            _ => panic!("expected ToolResult"),
        }
    }

    #[test]
    fn parses_result_subtype_into_status_stopped() {
        let line = r#"{"type":"result","subtype":"success","total_cost_usd":0.001,"is_error":false}"#;
        let evs = parse_line(line).unwrap();
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::Status { status } => assert_eq!(status, &AgentStatus::Stopped),
            _ => panic!("expected Status"),
        }
    }

    #[test]
    fn parse_line_returns_empty_for_empty_string() {
        let evs = parse_line("").unwrap();
        assert!(evs.is_empty());
    }

    #[test]
    fn parse_line_returns_empty_for_whitespace() {
        let evs = parse_line("   \n").unwrap();
        assert!(evs.is_empty());
    }

    #[test]
    fn parse_line_returns_err_on_invalid_json() {
        let result = parse_line("not json {{{");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 8.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::agent_stream 2>&1 | tail -15
```

Expected: compile error — `parse_line` not defined.

- [ ] **Step 8.3: Implement**

Replace `src-tauri/src/commands/agent_stream.rs` body with:

```rust
//! NDJSON stream-json parser for Claude Code CLI output.
//!
//! Each call accepts one whole line; the caller is responsible for line
//! framing (trim trailing `\n`). Returns 0..N AgentEvents — a single line
//! may produce multiple events when an assistant message contains both
//! text and tool_use blocks.

use crate::error::{AppError, AppResult};
use crate::state::{AgentEvent, AgentStatus, MessageRole, ToolResult, ToolUse};
use serde_json::Value;

pub fn parse_line(line: &str) -> AppResult<Vec<AgentEvent>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let v: Value = serde_json::from_str(trimmed).map_err(|e| AppError::ParseFailed {
        what: "stream-json line".into(),
        msg: e.to_string(),
    })?;

    let kind = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match kind {
        "system" if v.get("subtype").and_then(|s| s.as_str()) == Some("init") => {
            let session_id = v
                .get("session_id")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string();
            let model = v
                .get("model")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string();
            Ok(vec![AgentEvent::Init { session_id, model }])
        }
        "assistant" | "user" => parse_message(&v, kind),
        "result" => Ok(vec![AgentEvent::Status {
            status: AgentStatus::Stopped,
        }]),
        _ => Ok(Vec::new()),
    }
}

fn parse_message(v: &Value, kind: &str) -> AppResult<Vec<AgentEvent>> {
    let msg = v.get("message").ok_or_else(|| AppError::ParseFailed {
        what: "stream-json message".into(),
        msg: "missing 'message' field".into(),
    })?;
    let id = msg
        .get("id")
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();
    let role = match kind {
        "assistant" => MessageRole::Assistant,
        "user" => MessageRole::User,
        _ => MessageRole::User,
    };
    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_uses: Vec<ToolUse> = Vec::new();
    let mut tool_results: Vec<ToolResult> = Vec::new();

    if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
        for block in content {
            let btype = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match btype {
                "text" => {
                    if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                        text_parts.push(t.to_string());
                    }
                }
                "tool_use" => {
                    let tu = ToolUse {
                        id: block
                            .get("id")
                            .and_then(|s| s.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        name: block
                            .get("name")
                            .and_then(|s| s.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        input: block.get("input").cloned().unwrap_or(Value::Null),
                    };
                    tool_uses.push(tu);
                }
                "tool_result" => {
                    let tr = ToolResult {
                        tool_use_id: block
                            .get("tool_use_id")
                            .and_then(|s| s.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        content: block
                            .get("content")
                            .and_then(|s| s.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        is_error: block
                            .get("is_error")
                            .and_then(|b| b.as_bool())
                            .unwrap_or(false),
                    };
                    tool_results.push(tr);
                }
                _ => {} // ignore other block kinds (image, thinking, etc.)
            }
        }
    }

    let mut events: Vec<AgentEvent> = Vec::new();
    let combined_text: String = text_parts.join("");
    let has_text = !combined_text.is_empty();
    if has_text || (tool_uses.is_empty() && tool_results.is_empty()) {
        events.push(AgentEvent::Message {
            id: id.clone(),
            role,
            text: combined_text,
            is_partial: false,
        });
    }
    for tu in tool_uses {
        events.push(AgentEvent::ToolUse {
            message_id: id.clone(),
            tool_use: tu,
        });
    }
    for tr in tool_results {
        events.push(AgentEvent::ToolResult {
            message_id: id.clone(),
            tool_result: tr,
        });
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    // (tests from Step 8.1 — keep unchanged)
}
```

In `src-tauri/src/commands/mod.rs` add:

```rust
pub mod agent_stream;
```

(alphabetical, near the top).

> If `AppError::ParseFailed` doesn't exist yet, add it to
> `src-tauri/src/error.rs`:
>
> ```rust
> #[error("parse {what} failed: {msg}")]
> ParseFailed { what: String, msg: String },
> ```
>
> Run `cargo test --lib error::tests` to ensure existing tests pass.

- [ ] **Step 8.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::agent_stream 2>&1 | tail -10
```

Expected: `test result: ok. 10 passed`.

- [ ] **Step 8.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/agent_stream.rs src-tauri/src/commands/mod.rs src-tauri/src/error.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add commands/agent_stream.rs NDJSON parser

Parses system init, assistant/user messages, tool_use, tool_result, and
result subtype into typed AgentEvents. Handles single-line → multi-event
expansion (assistant text + tool_use in one line yields two events).
Tolerates empty/whitespace lines; returns ParseFailed AppError on invalid
JSON.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: `commands/agent.rs::spawn_agent` + context injection

**Files:**

- Create: `src-tauri/src/commands/agent.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 9.1: Write failing tests**

```rust
// New file src-tauri/src/commands/agent.rs (tests at the bottom):

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    fn make_state() -> Arc<Mutex<AppState>> {
        Arc::new(Mutex::new(AppState::default()))
    }

    fn make_data_dir() -> TempDir {
        let tmp = TempDir::new().unwrap();
        crate::platform::paths::ensure_data_dirs(tmp.path()).unwrap();
        tmp
    }

    fn write_workspace(state: &Arc<Mutex<AppState>>, ws_id: &str, repo_id: &str, worktree: PathBuf) {
        use crate::state::{KanbanColumn, WorkspaceInfo, WorkspaceStatus};
        let mut s = state.lock().unwrap();
        s.workspaces.insert(
            ws_id.into(),
            WorkspaceInfo {
                id: ws_id.into(),
                repo_id: repo_id.into(),
                title: "T".into(),
                description: String::new(),
                branch: "feat/x".into(),
                base_branch: "main".into(),
                custom_branch: false,
                status: WorkspaceStatus::NotStarted,
                column: KanbanColumn::InProgress,
                created_at: 0,
                updated_at: 0,
                worktree_dir: worktree,
            },
        );
    }

    #[test]
    fn spawn_agent_unknown_workspace_returns_err() {
        let state = make_state();
        let tmp = make_data_dir();
        let result = spawn_agent_inner(state.clone(), tmp.path(), "ws_missing", None);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("not found") || msg.contains("missing"), "got: {msg}");
    }

    #[test]
    fn spawn_agent_already_running_returns_err() {
        use tokio::sync::mpsc;
        let state = make_state();
        let tmp = make_data_dir();
        let worktree = tmp.path().join("workspaces/ws_a");
        std::fs::create_dir_all(&worktree).unwrap();
        write_workspace(&state, "ws_a", "repo_a", worktree);
        // Pre-insert an AgentHandle to simulate running agent.
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        state.lock().unwrap().agents.insert(
            "ws_a".into(),
            crate::state::AgentHandle {
                workspace_id: "ws_a".into(),
                stdin_tx: tx,
                session_id: None,
            },
        );
        let result = spawn_agent_inner(state, tmp.path(), "ws_a", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already running"));
    }

    #[test]
    fn spawn_agent_uses_explicit_claude_path_when_provided() {
        let state = make_state();
        let tmp = make_data_dir();
        let worktree = tmp.path().join("workspaces/ws_b");
        std::fs::create_dir_all(&worktree).unwrap();
        write_workspace(&state, "ws_b", "repo_b", worktree);
        // Use /bin/echo as the "claude binary" — spawn succeeds, immediately exits.
        let echo_path = if cfg!(windows) {
            std::path::PathBuf::from("C:\\Windows\\System32\\where.exe")
        } else {
            std::path::PathBuf::from("/bin/echo")
        };
        let result = spawn_agent_inner(state, tmp.path(), "ws_b", Some(echo_path));
        assert!(result.is_ok(), "got err: {:?}", result.err());
    }

    #[test]
    fn spawn_agent_marks_workspace_running_in_state() {
        use crate::state::WorkspaceStatus;
        let state = make_state();
        let tmp = make_data_dir();
        let worktree = tmp.path().join("workspaces/ws_c");
        std::fs::create_dir_all(&worktree).unwrap();
        write_workspace(&state, "ws_c", "repo_c", worktree);
        let echo_path = if cfg!(windows) {
            std::path::PathBuf::from("C:\\Windows\\System32\\where.exe")
        } else {
            std::path::PathBuf::from("/bin/echo")
        };
        spawn_agent_inner(state.clone(), tmp.path(), "ws_c", Some(echo_path)).unwrap();
        let s = state.lock().unwrap();
        let ws = s.workspaces.get("ws_c").unwrap();
        assert_eq!(ws.status, WorkspaceStatus::Running);
    }

    #[test]
    fn spawn_agent_inserts_handle_into_agents_map() {
        let state = make_state();
        let tmp = make_data_dir();
        let worktree = tmp.path().join("workspaces/ws_d");
        std::fs::create_dir_all(&worktree).unwrap();
        write_workspace(&state, "ws_d", "repo_d", worktree);
        let echo_path = if cfg!(windows) {
            std::path::PathBuf::from("C:\\Windows\\System32\\where.exe")
        } else {
            std::path::PathBuf::from("/bin/echo")
        };
        spawn_agent_inner(state.clone(), tmp.path(), "ws_d", Some(echo_path)).unwrap();
        let s = state.lock().unwrap();
        assert!(s.agents.contains_key("ws_d"));
    }

    #[test]
    fn build_system_prompt_prefix_includes_context_when_present() {
        let tmp = make_data_dir();
        let ctx_dir = tmp.path().join("contexts/repo_x");
        std::fs::create_dir_all(&ctx_dir).unwrap();
        std::fs::write(ctx_dir.join("context.md"), "## Repo conventions\nUse Rust.").unwrap();
        std::fs::write(ctx_dir.join("hot.md"), "Recent: bug in login.").unwrap();
        let prefix = build_system_prompt_prefix(tmp.path(), "repo_x");
        assert!(prefix.contains("Repo conventions"));
        assert!(prefix.contains("Use Rust."));
        assert!(prefix.contains("Recent: bug in login."));
    }

    #[test]
    fn build_system_prompt_prefix_returns_empty_when_files_missing() {
        let tmp = make_data_dir();
        let prefix = build_system_prompt_prefix(tmp.path(), "repo_y");
        assert!(prefix.is_empty());
    }
}
```

- [ ] **Step 9.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::agent::tests 2>&1 | tail -15
```

Expected: compile errors — `spawn_agent_inner`, `build_system_prompt_prefix` not
defined.

- [ ] **Step 9.3: Implement**

Replace `src-tauri/src/commands/agent.rs` body with:

```rust
//! Agent lifecycle: spawn / send / stop a Claude CLI process inside a
//! workspace worktree, stream events to the frontend over Tauri Channel.

use crate::commands::agent_stream::parse_line;
use crate::error::AppResult;
use crate::platform::pty::{spawn as pty_spawn, PtySession};
use crate::state::{AgentEvent, AgentHandle, AgentStatus, AppState, WorkspaceStatus};
use portable_pty::CommandBuilder;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tokio::sync::mpsc;

#[tauri::command]
pub async fn spawn_agent(
    workspace_id: String,
    on_event: Channel<AgentEvent>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    let claude_path = state
        .lock()
        .unwrap()
        .settings
        .claude_binary_override
        .clone();
    let session = spawn_agent_inner(state.inner().clone(), &data_dir, &workspace_id, claude_path)
        .map_err(|e| e.to_string())?;
    spawn_reader_thread(session, on_event, state.inner().clone(), workspace_id);
    Ok(())
}

/// The pure (testable) part of spawn_agent. Takes the data_dir and an
/// explicit claude_path so tests can substitute a real binary.
pub fn spawn_agent_inner(
    state: Arc<Mutex<AppState>>,
    data_dir: &Path,
    workspace_id: &str,
    claude_path: Option<PathBuf>,
) -> AppResult<PtySession> {
    use crate::error::AppError;

    let (worktree_dir, repo_id) = {
        let s = state.lock().unwrap();
        let ws = s
            .workspaces
            .get(workspace_id)
            .ok_or_else(|| AppError::NotFound(format!("workspace {workspace_id} not found")))?;
        (ws.worktree_dir.clone(), ws.repo_id.clone())
    };

    {
        let s = state.lock().unwrap();
        if s.agents.contains_key(workspace_id) {
            return Err(AppError::CommandFailed {
                cmd: "spawn_agent".into(),
                msg: format!("agent already running for {workspace_id}"),
            });
        }
    }

    let claude = claude_path
        .or_else(|| crate::platform::binary::claude_binary(None))
        .ok_or_else(|| AppError::CommandFailed {
            cmd: "spawn_agent".into(),
            msg: "claude binary not found".into(),
        })?;

    let mut cmd = CommandBuilder::new(&claude);
    cmd.args([
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-mode",
        "bypassPermissions",
        "--disallowedTools",
        "EnterWorktree,ExitWorktree",
    ]);
    cmd.cwd(&worktree_dir);

    // Inject context.md / hot.md as system prompt prefix.
    let prefix = build_system_prompt_prefix(data_dir, &repo_id);
    if !prefix.is_empty() {
        cmd.args(["--append-system-prompt", &prefix]);
    }

    let session = pty_spawn(cmd)?;

    // Wire stdin mpsc → PTY writer thread.
    let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<String>();
    let mut writer = session.writer();
    std::thread::spawn(move || {
        while let Some(line) = stdin_rx.blocking_recv() {
            if writeln!(writer, "{line}").is_err() {
                break;
            }
        }
    });

    {
        let mut s = state.lock().unwrap();
        if let Some(ws) = s.workspaces.get_mut(workspace_id) {
            ws.status = WorkspaceStatus::Running;
        }
        s.agents.insert(
            workspace_id.into(),
            AgentHandle {
                workspace_id: workspace_id.into(),
                stdin_tx,
                session_id: None,
            },
        );
    }

    Ok(session)
}

pub fn build_system_prompt_prefix(data_dir: &Path, repo_id: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let ctx_dir = data_dir.join("contexts").join(repo_id);
    for fname in ["context.md", "hot.md"] {
        let p = ctx_dir.join(fname);
        if let Ok(content) = std::fs::read_to_string(&p) {
            parts.push(content);
        }
    }
    parts.join("\n\n---\n\n")
}

fn spawn_reader_thread(
    mut session: PtySession,
    on_event: Channel<AgentEvent>,
    state: Arc<Mutex<AppState>>,
    workspace_id: String,
) {
    let _ = on_event.send(AgentEvent::Status {
        status: AgentStatus::Running,
    });
    std::thread::spawn(move || {
        let reader = session.reader();
        let mut br = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            match br.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    match parse_line(&line) {
                        Ok(events) => {
                            for ev in events {
                                if let AgentEvent::Init { session_id, .. } = &ev {
                                    if let Some(handle) =
                                        state.lock().unwrap().agents.get_mut(&workspace_id)
                                    {
                                        handle.session_id = Some(session_id.clone());
                                    }
                                }
                                let _ = on_event.send(ev);
                            }
                        }
                        Err(e) => {
                            let _ = on_event.send(AgentEvent::Error {
                                message: format!("parse: {e}"),
                            });
                        }
                    }
                }
                Err(e) => {
                    let _ = on_event.send(AgentEvent::Error {
                        message: format!("read: {e}"),
                    });
                    break;
                }
            }
        }
        // Cleanup on EOF: mark workspace waiting, drop handle.
        let _ = session.try_wait();
        let mut s = state.lock().unwrap();
        if let Some(ws) = s.workspaces.get_mut(&workspace_id) {
            ws.status = WorkspaceStatus::Waiting;
        }
        s.agents.remove(&workspace_id);
        let _ = on_event.send(AgentEvent::Status {
            status: AgentStatus::Stopped,
        });
    });
}

#[cfg(test)]
mod tests {
    // (tests from Step 9.1 — keep unchanged)
}
```

> Add to `AppSettings` in `state.rs` (if not already present):
>
> ```rust
> pub claude_binary_override: Option<PathBuf>,
> ```
>
> Update tests in `state.rs` accordingly.

> If `AppError::NotFound` doesn't exist, add to `error.rs`:
>
> ```rust
> #[error("not found: {0}")]
> NotFound(String),
> ```

In `src-tauri/src/commands/mod.rs` add:

```rust
pub mod agent;
```

(alphabetical).

- [ ] **Step 9.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::agent 2>&1 | tail -10
```

Expected: `test result: ok. 7 passed`.

- [ ] **Step 9.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/agent.rs src-tauri/src/commands/mod.rs src-tauri/src/state.rs src-tauri/src/error.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add commands/agent.rs::spawn_agent with context injection

Spawn `claude -p --output-format stream-json --verbose` inside the
workspace worktree via PTY. Reader thread parses NDJSON and forwards
typed AgentEvents over the Tauri Channel. Stdin writes go through an
mpsc channel to a dedicated writer thread, decoupling from blocking
PTY writes. context.md + hot.md are prepended as --append-system-prompt
when present.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: `commands/agent.rs::send_message`

**Files:**

- Modify: `src-tauri/src/commands/agent.rs`

- [ ] **Step 10.1: Write failing tests**

```rust
// Append to src-tauri/src/commands/agent.rs  #[cfg(test)] mod tests:

#[test]
fn send_message_inner_sends_to_stdin_channel() {
    use tokio::sync::mpsc;
    let state = make_state();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    state.lock().unwrap().agents.insert(
        "ws_send_a".into(),
        crate::state::AgentHandle {
            workspace_id: "ws_send_a".into(),
            stdin_tx: tx,
            session_id: None,
        },
    );
    send_message_inner(state, "ws_send_a", "Hello!").unwrap();
    let received = rx.try_recv().unwrap();
    assert_eq!(received, "Hello!");
}

#[test]
fn send_message_inner_no_agent_returns_err() {
    let state = make_state();
    let result = send_message_inner(state, "ws_none", "hi");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("no agent"));
}

#[test]
fn send_message_inner_appends_message_to_disk() {
    use crate::persistence::messages::load_messages;
    let tmp = make_data_dir();
    let state = make_state();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    state.lock().unwrap().agents.insert(
        "ws_send_b".into(),
        crate::state::AgentHandle {
            workspace_id: "ws_send_b".into(),
            stdin_tx: tx,
            session_id: None,
        },
    );
    send_message_inner_with_persist(state, tmp.path(), "ws_send_b", "Persist me").unwrap();
    let on_disk = load_messages(tmp.path(), "ws_send_b").unwrap();
    assert_eq!(on_disk.len(), 1);
    assert_eq!(on_disk[0].text, "Persist me");
    assert_eq!(on_disk[0].role, crate::state::MessageRole::User);
}

#[test]
fn send_message_inner_persist_handles_existing_messages() {
    use crate::persistence::messages::{load_messages, save_messages};
    use crate::state::{Message, MessageRole};
    let tmp = make_data_dir();
    let state = make_state();
    save_messages(
        tmp.path(),
        "ws_send_c",
        &vec![Message {
            id: "msg_old".into(),
            workspace_id: "ws_send_c".into(),
            role: MessageRole::Assistant,
            text: "previous".into(),
            is_partial: false,
            tool_use: None,
            tool_result: None,
            created_at: 0,
        }],
    )
    .unwrap();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    state.lock().unwrap().agents.insert(
        "ws_send_c".into(),
        crate::state::AgentHandle {
            workspace_id: "ws_send_c".into(),
            stdin_tx: tx,
            session_id: None,
        },
    );
    send_message_inner_with_persist(state, tmp.path(), "ws_send_c", "next").unwrap();
    let on_disk = load_messages(tmp.path(), "ws_send_c").unwrap();
    assert_eq!(on_disk.len(), 2);
    assert_eq!(on_disk[0].text, "previous");
    assert_eq!(on_disk[1].text, "next");
}
```

- [ ] **Step 10.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::agent::tests::send 2>&1 | tail -15
```

Expected: errors — `send_message_inner`, `send_message_inner_with_persist` not
defined.

- [ ] **Step 10.3: Implement**

Add to `src-tauri/src/commands/agent.rs` (after `spawn_reader_thread`):

```rust
#[tauri::command]
pub async fn send_message(
    workspace_id: String,
    text: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    send_message_inner_with_persist(state.inner().clone(), &data_dir, &workspace_id, &text)
        .map_err(|e| e.to_string())
}

pub fn send_message_inner(
    state: Arc<Mutex<AppState>>,
    workspace_id: &str,
    text: &str,
) -> AppResult<()> {
    use crate::error::AppError;
    let s = state.lock().unwrap();
    let handle = s.agents.get(workspace_id).ok_or_else(|| AppError::CommandFailed {
        cmd: "send_message".into(),
        msg: format!("no agent for workspace {workspace_id}"),
    })?;
    handle
        .stdin_tx
        .send(text.to_string())
        .map_err(|e| AppError::CommandFailed {
            cmd: "send_message".into(),
            msg: format!("stdin closed: {e}"),
        })?;
    Ok(())
}

pub fn send_message_inner_with_persist(
    state: Arc<Mutex<AppState>>,
    data_dir: &Path,
    workspace_id: &str,
    text: &str,
) -> AppResult<()> {
    use crate::ids::message_id;
    use crate::persistence::messages::{load_messages, save_messages};
    use crate::state::{Message, MessageRole};

    send_message_inner(state, workspace_id, text)?;

    let mut current = load_messages(data_dir, workspace_id).unwrap_or_default();
    current.push(Message {
        id: message_id(),
        workspace_id: workspace_id.into(),
        role: MessageRole::User,
        text: text.into(),
        is_partial: false,
        tool_use: None,
        tool_result: None,
        created_at: crate::commands::helpers::now_unix(),
    });
    save_messages(data_dir, workspace_id, &current)?;
    Ok(())
}
```

- [ ] **Step 10.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::agent 2>&1 | tail -10
```

Expected: `test result: ok. 11 passed` (7 from Task 9 + 4 new).

- [ ] **Step 10.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/agent.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add send_message command with disk persistence

User text goes through the AgentHandle's stdin mpsc channel to the PTY
writer thread, then a Message is appended to messages/<ws-id>.json so
the chat survives app restart. send_message_inner is the pure variant
without persistence (used by future tools).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: `commands/agent.rs::stop_agent`

**Files:**

- Modify: `src-tauri/src/commands/agent.rs`

- [ ] **Step 11.1: Write failing tests**

```rust
// Append to src-tauri/src/commands/agent.rs  #[cfg(test)] mod tests:

#[test]
fn stop_agent_inner_no_handle_returns_ok_silently() {
    let state = make_state();
    // Calling stop on a workspace with no agent is a no-op (idempotent).
    let result = stop_agent_inner(state, "ws_no_agent");
    assert!(result.is_ok());
}

#[test]
fn stop_agent_inner_drops_stdin_tx() {
    use tokio::sync::mpsc;
    let state = make_state();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    state.lock().unwrap().agents.insert(
        "ws_stop_a".into(),
        crate::state::AgentHandle {
            workspace_id: "ws_stop_a".into(),
            stdin_tx: tx,
            session_id: None,
        },
    );
    stop_agent_inner(state.clone(), "ws_stop_a").unwrap();
    // After stop, the sender side is dropped, so try_recv returns Disconnected.
    assert!(matches!(
        rx.try_recv(),
        Err(mpsc::error::TryRecvError::Disconnected)
    ));
}

#[test]
fn stop_agent_inner_removes_handle_from_map() {
    use tokio::sync::mpsc;
    let state = make_state();
    let (tx, _rx) = mpsc::unbounded_channel::<String>();
    state.lock().unwrap().agents.insert(
        "ws_stop_b".into(),
        crate::state::AgentHandle {
            workspace_id: "ws_stop_b".into(),
            stdin_tx: tx,
            session_id: None,
        },
    );
    stop_agent_inner(state.clone(), "ws_stop_b").unwrap();
    assert!(!state.lock().unwrap().agents.contains_key("ws_stop_b"));
}

#[test]
fn stop_agent_inner_marks_workspace_waiting() {
    use crate::state::WorkspaceStatus;
    use tokio::sync::mpsc;
    let state = make_state();
    let tmp = make_data_dir();
    let worktree = tmp.path().join("workspaces/ws_stop_c");
    std::fs::create_dir_all(&worktree).unwrap();
    write_workspace(&state, "ws_stop_c", "repo_c", worktree);
    state.lock().unwrap().workspaces.get_mut("ws_stop_c").unwrap().status =
        WorkspaceStatus::Running;
    let (tx, _rx) = mpsc::unbounded_channel::<String>();
    state.lock().unwrap().agents.insert(
        "ws_stop_c".into(),
        crate::state::AgentHandle {
            workspace_id: "ws_stop_c".into(),
            stdin_tx: tx,
            session_id: None,
        },
    );
    stop_agent_inner(state.clone(), "ws_stop_c").unwrap();
    let s = state.lock().unwrap();
    assert_eq!(
        s.workspaces.get("ws_stop_c").unwrap().status,
        WorkspaceStatus::Waiting
    );
}
```

- [ ] **Step 11.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::agent::tests::stop 2>&1 | tail -15
```

Expected: `stop_agent_inner` not found.

- [ ] **Step 11.3: Implement**

Append to `src-tauri/src/commands/agent.rs`:

```rust
#[tauri::command]
pub async fn stop_agent(
    workspace_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    stop_agent_inner(state.inner().clone(), &workspace_id).map_err(|e| e.to_string())
}

pub fn stop_agent_inner(state: Arc<Mutex<AppState>>, workspace_id: &str) -> AppResult<()> {
    let mut s = state.lock().unwrap();
    // Remove handle (drops the stdin_tx sender, which closes the PTY writer thread).
    s.agents.remove(workspace_id);
    if let Some(ws) = s.workspaces.get_mut(workspace_id) {
        ws.status = WorkspaceStatus::Waiting;
    }
    Ok(())
}
```

> Note: stop_agent is idempotent — removing a non-existent handle is a no-op.
> Killing the underlying PTY child is the reader thread's job: when stdin
> closes, claude exits, the reader hits EOF, and cleanup runs. If you need
> synchronous kill, extend `AgentHandle` to hold a
> `kill_tx: oneshot::Sender<()>` paired with a kill thread.

- [ ] **Step 11.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::agent 2>&1 | tail -10
```

Expected: `test result: ok. 15 passed` (11 + 4 new).

- [ ] **Step 11.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/agent.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add stop_agent command (idempotent handle removal)

Drops the AgentHandle from the agents map, which drops the stdin_tx
mpsc sender. The PTY writer thread sees the channel close and exits;
claude sees EOF on stdin and shuts down; the reader thread hits EOF
and runs final cleanup. Workspace status flips to Waiting.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: Wire commands into `lib.rs` + capabilities

**Files:**

- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/capabilities/default.json`

- [ ] **Step 12.1: Write failing tests**

```rust
// Append to src-tauri/src/lib.rs  #[cfg(test)] mod tests:

#[test]
fn all_agent_commands_exist_as_public_fns() {
    let _ = crate::commands::agent::spawn_agent as *const ();
    let _ = crate::commands::agent::send_message as *const ();
    let _ = crate::commands::agent::stop_agent as *const ();
}

#[test]
fn app_state_has_agents_field_at_startup() {
    use crate::state::{AppSettings, AppState};
    use std::collections::HashMap;
    let state = AppState {
        repos: HashMap::new(),
        workspaces: HashMap::new(),
        tasks: HashMap::new(),
        agents: HashMap::new(),
        settings: AppSettings::default(),
    };
    assert!(state.agents.is_empty());
}
```

- [ ] **Step 12.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib tests::all_agent 2>&1 | tail -15
```

Expected: errors — agent commands not yet registered or unresolved.

- [ ] **Step 12.3: Implement**

In `src-tauri/src/lib.rs`, extend the `invoke_handler` registration:

```rust
.invoke_handler(tauri::generate_handler![
    crate::commands::system::get_app_version,
    crate::commands::repo::add_repo,
    crate::commands::repo::list_repos,
    crate::commands::repo::remove_repo,
    crate::commands::repo::update_gh_profile,
    crate::commands::workspace::create_workspace,
    crate::commands::workspace::list_workspaces,
    crate::commands::workspace::remove_workspace,
    crate::commands::task::add_task,
    crate::commands::task::list_tasks,
    crate::commands::task::update_task,
    crate::commands::task::move_task,
    crate::commands::task::remove_task,
    crate::commands::agent::spawn_agent,        // NEW
    crate::commands::agent::send_message,       // NEW
    crate::commands::agent::stop_agent,         // NEW
])
```

In `src-tauri/capabilities/default.json` — no new permissions needed (commands
are exposed by default in the default capability when registered via
`generate_handler!`). If `default.json` uses an explicit allowlist, add:

```json
{
  "permissions": ["core:default", "dialog:default", "dialog:allow-open"]
}
```

(Keep as-is if it uses `core:default` which permits all registered commands.)

- [ ] **Step 12.4: Run tests + smoke check — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib 2>&1 | tail -15
cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -20
```

Expected: all tests pass, clippy clean.

```bash
cd /home/handokobeni/Work/ai-editor
bun run check 2>&1 | tail -5
```

Expected: 0 errors / 0 warnings — frontend types still compile (will need the
IPC wrappers in Phase 1c-frontend).

- [ ] **Step 12.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/lib.rs src-tauri/capabilities/default.json
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): wire spawn_agent / send_message / stop_agent into Tauri handler

All three commands registered in generate_handler!; default.json
capability already covers them via core:default. Confirms backend
agent layer is fully invokable from the frontend.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Final verification

```bash
cd /home/handokobeni/Work/ai-editor
bun run lint 2>&1 | tail -5
cd src-tauri && cargo fmt --all -- --check && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -10
cd .. && bun run check
cd src-tauri && cargo test --lib 2>&1 | tail -5
```

Expected outcome of all four checks: **green**.

Coverage targets (verify with `cargo tarpaulin` or your existing coverage
script): ≥95% line + branch on changed Rust files. The `agent_stream.rs` parser,
`messages.rs` persistence, and `pty.rs` wrapper are pure / easily testable;
`agent.rs::spawn_agent` covers the mutation paths via `spawn_agent_inner` so the
thin Tauri command wrapper has minimal untested code.

---

## Out of scope (defer to Phase 1c-frontend or later)

- xterm.js terminal panel (Phase 2)
- Diff viewer (Phase 2)
- @-file mentions in chat input (Phase 2)
- Per-message tool-call expansion UI (Phase 4)
- Auto-capture skill on PR merge (Phase 5 — Hermes-inspired)
- Streaming partial assistant chunks (the parser sets `is_partial: false` for
  now; partial-text deltas would need to track in-progress messages by id and
  emit `is_partial: true` until the message turn ends).
- LSP integration (Phase 6)

---

## Done criteria

- All 12 tasks complete; all tests green.
- `cargo test --lib` shows ≥40 new tests (3 + 8 + 3 + 6 + 5 + 3 + 6 + 10 + 7 +
  4 + 4 + 2).
- `bun run check` and `cargo clippy` clean.
- The Tauri app builds (`cargo check`) and the three new commands appear in the
  registered handler list.
