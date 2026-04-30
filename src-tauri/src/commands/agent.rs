// Thin Tauri command wrappers — excluded from unit-test coverage because they
// require a live Tauri AppHandle / Channel, which cannot be constructed in unit
// tests.  All business logic lives in `agent_core.rs` (fully covered).
pub use crate::commands::agent_core::{
    build_system_prompt_prefix, event_to_persisted_message, process_reader_events,
    send_message_inner, send_message_inner_with_persist, spawn_agent_inner, stop_agent_inner,
    AgentProcess,
};

use crate::persistence::messages::{append_message, list_messages_paginated};
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
        workspace_id,
        data_dir,
    );
    Ok(())
}

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

fn spawn_reader_thread(
    mut process: AgentProcess,
    on_event: Channel<AgentEvent>,
    state: Arc<Mutex<AppState>>,
    workspace_id: String,
    data_dir: PathBuf,
) {
    let _ = on_event.send(AgentEvent::Status {
        status: AgentStatus::Running,
    });
    std::thread::spawn(move || {
        let reader = match process.reader() {
            Ok(r) => r,
            Err(e) => {
                let _ = on_event.send(AgentEvent::Error {
                    message: format!("reader: {e}"),
                });
                return;
            }
        };
        process_reader_events(reader, state, &workspace_id, &|ev: AgentEvent| {
            // Persist assistant + tool events to disk so reopening the
            // workspace later rehydrates the history. User messages are
            // already saved by send_message_inner_with_persist on the
            // inbound path.
            if let Some(msg) = event_to_persisted_message(&ev, &workspace_id) {
                if let Err(e) = append_message(&data_dir, &workspace_id, &msg) {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        error = %e,
                        "agent reader: persist failed"
                    );
                }
            }
            let _ = on_event.send(ev);
        });
        let _ = process.try_wait();
        let _ = on_event.send(AgentEvent::Status {
            status: AgentStatus::Stopped,
        });
    });
}
