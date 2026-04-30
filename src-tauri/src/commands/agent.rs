// Thin Tauri command wrappers — excluded from unit-test coverage because they
// require a live Tauri AppHandle / Channel, which cannot be constructed in unit
// tests.  All business logic lives in `agent_core.rs` (fully covered).
pub use crate::commands::agent_core::{
    build_system_prompt_prefix, event_to_persisted_message, process_reader_events,
    process_reader_events_with_cancel, reattach_agent_inner, send_message_inner,
    send_message_inner_with_persist, spawn_agent_inner, stderr_line_to_event, stop_agent_inner,
    AgentProcess,
};

use crate::persistence::message_writer::MessageWriter;
use crate::persistence::messages::list_messages_paginated;
use crate::state::{AgentEvent, AgentStatus, AppState, Message};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::Manager;

#[tauri::command]
pub async fn spawn_agent(
    workspace_id: String,
    on_event: Channel<AgentEvent>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    writer: tauri::State<'_, MessageWriter>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    let claude_path = state
        .lock()
        .map_err(|e| format!("state lock poisoned: {e}"))?
        .settings
        .claude_binary_override
        .clone();
    let session = spawn_agent_inner(state.inner().clone(), &data_dir, &workspace_id, claude_path)
        .map_err(|e| e.to_string())?;
    spawn_reader_thread(
        session,
        on_event,
        state.inner().clone(),
        writer.inner().clone(),
        workspace_id,
        data_dir,
    );
    Ok(())
}

/// What the frontend sends for each attached file. The backend copies the
/// file into the app data dir and constructs an `Attachment` record from
/// the resulting canonical path.
#[derive(serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInput {
    /// Absolute path the user picked via the file dialog.
    pub source_path: String,
    /// MIME type — must start with `image/`.
    pub media_type: String,
    /// Original basename, optional. Falls back to source_path's basename.
    pub filename: Option<String>,
}

#[tauri::command]
pub async fn send_message(
    workspace_id: String,
    text: String,
    attachments: Option<Vec<AttachmentInput>>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    writer: tauri::State<'_, MessageWriter>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    let attachments = attachments.unwrap_or_default();
    crate::commands::agent_core::send_message_inner_with_persist_and_attachments(
        state.inner().clone(),
        writer.inner(),
        &data_dir,
        &workspace_id,
        &text,
        &attachments,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_agent(
    workspace_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    stop_agent_inner(state.inner().clone(), &workspace_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_messages(
    workspace_id: String,
    limit: Option<usize>,
    before_id: Option<String>,
    app: tauri::AppHandle,
) -> Result<Vec<Message>, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    list_messages_paginated(&data_dir, &workspace_id, limit, before_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reattach_agent(
    workspace_id: String,
    on_event: Channel<AgentEvent>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let rx =
        reattach_agent_inner(state.inner().clone(), &workspace_id).map_err(|e| e.to_string())?;
    forward_subscriber(rx, on_event);
    Ok(())
}

/// Bridges a tokio broadcast Receiver to a Tauri Channel by spawning a
/// dedicated thread that pumps events one-by-one. Returns when the
/// broadcaster closes or the Channel handler is dropped.
fn forward_subscriber(
    mut rx: tokio::sync::broadcast::Receiver<AgentEvent>,
    channel: Channel<AgentEvent>,
) {
    std::thread::spawn(move || loop {
        match rx.blocking_recv() {
            Ok(ev) => {
                if channel.send(ev).is_err() {
                    return; // frontend dropped its handler
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
        }
    });
}

fn spawn_reader_thread(
    mut process: AgentProcess,
    initial_subscriber: Channel<AgentEvent>,
    state: Arc<Mutex<AppState>>,
    message_writer: MessageWriter,
    workspace_id: String,
    data_dir: PathBuf,
) {
    let (event_tx, cancel) = match state.lock() {
        Ok(s) => match s.agents.get(&workspace_id) {
            Some(h) => (h.event_tx.clone(), h.cancel.clone()),
            None => {
                let _ = initial_subscriber.send(AgentEvent::Error {
                    message: "agent handle missing immediately after spawn".into(),
                });
                return;
            }
        },
        Err(e) => {
            let _ = initial_subscriber.send(AgentEvent::Error {
                message: format!("state lock: {e}"),
            });
            return;
        }
    };
    forward_subscriber(event_tx.subscribe(), initial_subscriber);
    // The agent process is alive but no user prompt has landed yet — emit
    // Waiting (idle, ready) rather than Running so the live turn indicator
    // stays hidden until an actual turn starts. send_message bumps the
    // status to Running when the user fires off a prompt.
    let _ = event_tx.send(AgentEvent::Status {
        status: AgentStatus::Waiting,
    });
    let event_tx_reader = event_tx.clone();
    std::thread::spawn(move || {
        let reader = match process.reader() {
            Ok(r) => r,
            Err(e) => {
                let _ = event_tx_reader.send(AgentEvent::Error {
                    message: format!("reader: {e}"),
                });
                return;
            }
        };
        process_reader_events_with_cancel(
            reader,
            state,
            &workspace_id,
            cancel,
            &|ev: AgentEvent| {
                // Persist assistant + tool events through the debounced writer
                // so a tool-heavy turn (5+ tool_use + tool_result + assistant
                // text) collapses into a single disk write per ~500 ms window.
                // User messages take the same path via send_message_inner.
                if let Some(msg) = event_to_persisted_message(&ev, &workspace_id) {
                    if let Err(e) = message_writer.queue(&data_dir, &workspace_id, msg) {
                        tracing::warn!(
                            workspace_id = %workspace_id,
                            error = %e,
                            "agent reader: queue failed"
                        );
                    }
                }
                let _ = event_tx_reader.send(ev);
            },
        );
        let _ = process.try_wait();
        let _ = event_tx_reader.send(AgentEvent::Status {
            status: AgentStatus::Stopped,
        });
    });
}
