use crate::error::{AppError, Result};
use crate::state::{AgentEvent, AgentStatus, MessageRole, ToolResult, ToolUse};
use serde_json::Value;

pub fn parse_line(line: &str) -> Result<Vec<AgentEvent>> {
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
        "system" if v.get("subtype").and_then(|s| s.as_str()) == Some("compact_boundary") => {
            // Claude's auto-compaction marker. The compact_metadata payload
            // carries the trigger (auto/manual) and a pre-compaction token
            // count. Both are optional from our perspective — fall back to
            // sensible defaults rather than failing the whole stream.
            let meta = v.get("compact_metadata");
            let trigger = meta
                .and_then(|m| m.get("trigger"))
                .and_then(|s| s.as_str())
                .unwrap_or("auto")
                .to_string();
            let pre_tokens = meta
                .and_then(|m| m.get("pre_tokens"))
                .and_then(|n| n.as_u64());
            Ok(vec![AgentEvent::Compact {
                trigger,
                pre_tokens,
            }])
        }
        "assistant" | "user" => parse_message(&v, kind),
        // "result" marks the end of a single turn in stream-json mode.
        // The agent process stays alive for the next user message, so we
        // don't kill it — but we DO transition the user-facing status from
        // "running" (currently processing a turn) to "waiting" (idle, ready
        // for next prompt). The TurnStatusBar above the input gates on
        // running, so without this signal the indicator would otherwise
        // hang forever after a turn completes.
        "result" => Ok(vec![AgentEvent::Status {
            status: AgentStatus::Waiting,
        }]),
        _ => Ok(Vec::new()),
    }
}

/// Stateful parser for Claude's `--include-partial-messages` stream. The
/// CLI emits Anthropic-API-style `stream_event` lines that need cross-line
/// state to accumulate text deltas into partial Message events.
///
/// Non-stream_event lines (system init, assistant, result) are forwarded
/// to `parse_line` unchanged so this is a strict superset of the pure
/// parser. The frontend store de-dupes by message id, so the final
/// non-partial assistant line cleanly overwrites the last partial.
pub struct StreamParser {
    /// Id of the currently-streaming message (set on `message_start`,
    /// cleared on `message_stop`). Delta events are no-ops when this is
    /// `None`, which keeps the parser robust against truncated streams.
    current_message_id: Option<String>,
    /// Accumulated text for the current message id.
    accumulated: String,
    /// Accumulated thinking content for the current message id. Tracked
    /// separately from `accumulated` because thinking blocks emit a
    /// dedicated AgentEvent variant the UI styles distinctly.
    thinking: String,
}

impl Default for StreamParser {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamParser {
    pub fn new() -> Self {
        Self {
            current_message_id: None,
            accumulated: String::new(),
            thinking: String::new(),
        }
    }

    pub fn parse_line(&mut self, line: &str) -> Result<Vec<AgentEvent>> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let v: Value = serde_json::from_str(trimmed).map_err(|e| AppError::ParseFailed {
            what: "stream-json line".into(),
            msg: e.to_string(),
        })?;
        if v.get("type").and_then(|t| t.as_str()) == Some("stream_event") {
            return self.parse_stream_event(&v);
        }
        // Fall back to the pure parser for non-streaming wire shapes.
        parse_line(trimmed)
    }

    fn parse_stream_event(&mut self, v: &Value) -> Result<Vec<AgentEvent>> {
        let event = match v.get("event") {
            Some(e) => e,
            None => return Ok(Vec::new()),
        };
        let etype = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match etype {
            "message_start" => {
                if let Some(id) = event
                    .get("message")
                    .and_then(|m| m.get("id"))
                    .and_then(|s| s.as_str())
                {
                    self.current_message_id = Some(id.to_string());
                    self.accumulated.clear();
                    self.thinking.clear();
                }
                Ok(Vec::new())
            }
            "content_block_delta" => {
                let dtype = event
                    .get("delta")
                    .and_then(|d| d.get("type"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                let id = match self.current_message_id.as_ref() {
                    Some(id) => id.clone(),
                    None => return Ok(Vec::new()),
                };
                match dtype {
                    "text_delta" => {
                        let chunk = event
                            .get("delta")
                            .and_then(|d| d.get("text"))
                            .and_then(|s| s.as_str())
                            .unwrap_or("");
                        self.accumulated.push_str(chunk);
                        Ok(vec![AgentEvent::Message {
                            id,
                            role: MessageRole::Assistant,
                            text: self.accumulated.clone(),
                            is_partial: true,
                        }])
                    }
                    "thinking_delta" => {
                        let chunk = event
                            .get("delta")
                            .and_then(|d| d.get("thinking"))
                            .and_then(|s| s.as_str())
                            .unwrap_or("");
                        self.thinking.push_str(chunk);
                        Ok(vec![AgentEvent::Thinking {
                            message_id: id,
                            text: self.thinking.clone(),
                            is_partial: true,
                        }])
                    }
                    // input_json_delta and other deltas have no partial UI
                    // representation; the final assistant line will carry
                    // the completed tool_use input.
                    _ => Ok(Vec::new()),
                }
            }
            "message_stop" => {
                self.current_message_id = None;
                self.accumulated.clear();
                self.thinking.clear();
                Ok(Vec::new())
            }
            _ => Ok(Vec::new()),
        }
    }
}

fn parse_message(v: &Value, kind: &str) -> Result<Vec<AgentEvent>> {
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
        _ => MessageRole::User,
    };
    let mut text_parts: Vec<String> = Vec::new();
    let mut thinking_parts: Vec<String> = Vec::new();
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
                "thinking" => {
                    if let Some(t) = block.get("thinking").and_then(|t| t.as_str()) {
                        thinking_parts.push(t.to_string());
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
                _ => {}
            }
        }
    }

    let mut events: Vec<AgentEvent> = Vec::new();
    let combined_text: String = text_parts.join("");
    let has_text = !combined_text.is_empty();
    if has_text {
        events.push(AgentEvent::Message {
            id: id.clone(),
            role,
            text: combined_text,
            is_partial: false,
        });
    }
    let combined_thinking: String = thinking_parts.join("");
    if !combined_thinking.is_empty() {
        events.push(AgentEvent::Thinking {
            message_id: id.clone(),
            text: combined_thinking,
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
    // Pull token usage off the assistant message when present. The CLI
    // emits this on the final (non-streaming) `assistant` line, so a turn
    // produces one Usage event per message — perfect for accumulating into
    // a per-turn total in the frontend.
    if kind == "assistant" {
        if let Some(usage) = msg.get("usage") {
            let input_tokens = usage
                .get("input_tokens")
                .and_then(|n| n.as_u64())
                .unwrap_or(0);
            let cache_creation_input_tokens = usage
                .get("cache_creation_input_tokens")
                .and_then(|n| n.as_u64())
                .unwrap_or(0);
            let cache_read_input_tokens = usage
                .get("cache_read_input_tokens")
                .and_then(|n| n.as_u64())
                .unwrap_or(0);
            let output_tokens = usage
                .get("output_tokens")
                .and_then(|n| n.as_u64())
                .unwrap_or(0);
            let total_input = input_tokens + cache_creation_input_tokens + cache_read_input_tokens;
            events.push(AgentEvent::Usage {
                message_id: id.clone(),
                input_tokens,
                cache_creation_input_tokens,
                cache_read_input_tokens,
                output_tokens,
                total_input,
            });
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AgentEvent, MessageRole};

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
            AgentEvent::Message {
                id,
                role,
                text,
                is_partial,
            } => {
                assert_eq!(id, "msg_01");
                assert_eq!(role, &MessageRole::Assistant);
                assert_eq!(text, "Hello!");
                assert!(!is_partial);
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn parses_assistant_usage_into_usage_event_with_summed_total_input() {
        // Real Claude CLI line includes message.usage. The frontend turn-
        // status indicator depends on this — without the Usage event the
        // "↓ Yk tokens" never updates.
        let line = r#"{"type":"assistant","message":{"id":"msg_u1","role":"assistant","content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":12,"cache_creation_input_tokens":50,"cache_read_input_tokens":4500,"output_tokens":230}}}"#;
        let evs = parse_line(line).unwrap();
        // First the Message, then the Usage.
        assert!(evs.iter().any(|e| matches!(e, AgentEvent::Message { .. })));
        let usage = evs
            .iter()
            .find(|e| matches!(e, AgentEvent::Usage { .. }))
            .expect("expected a Usage event after the Message");
        match usage {
            AgentEvent::Usage {
                message_id,
                input_tokens,
                cache_creation_input_tokens,
                cache_read_input_tokens,
                output_tokens,
                total_input,
            } => {
                assert_eq!(message_id, "msg_u1");
                assert_eq!(*input_tokens, 12);
                assert_eq!(*cache_creation_input_tokens, 50);
                assert_eq!(*cache_read_input_tokens, 4500);
                assert_eq!(*output_tokens, 230);
                // total_input = input + cache_creation + cache_read per the
                // project rule. Cache reads count against context just as
                // much as fresh input does.
                assert_eq!(*total_input, 12 + 50 + 4500);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn assistant_without_usage_emits_no_usage_event() {
        // Older or mock CLIs may omit the usage block entirely. The parser
        // must still succeed and just not emit Usage.
        let line = r#"{"type":"assistant","message":{"id":"msg_u2","role":"assistant","content":[{"type":"text","text":"hi"}]}}"#;
        let evs = parse_line(line).unwrap();
        assert!(!evs.iter().any(|e| matches!(e, AgentEvent::Usage { .. })));
    }

    #[test]
    fn assistant_usage_with_missing_fields_defaults_to_zero() {
        // Defensive — partial usage shapes shouldn't panic. Absent counts
        // are treated as zero so downstream sums stay safe.
        let line = r#"{"type":"assistant","message":{"id":"msg_u3","role":"assistant","content":[{"type":"text","text":"x"}],"usage":{"output_tokens":5}}}"#;
        let evs = parse_line(line).unwrap();
        let usage = evs
            .iter()
            .find_map(|e| match e {
                AgentEvent::Usage {
                    input_tokens,
                    cache_creation_input_tokens,
                    cache_read_input_tokens,
                    output_tokens,
                    total_input,
                    ..
                } => Some((
                    *input_tokens,
                    *cache_creation_input_tokens,
                    *cache_read_input_tokens,
                    *output_tokens,
                    *total_input,
                )),
                _ => None,
            })
            .expect("expected Usage even when fields are missing");
        assert_eq!(usage, (0, 0, 0, 5, 0));
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
            AgentEvent::ToolUse {
                message_id,
                tool_use,
            } => {
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
            AgentEvent::ToolResult {
                message_id,
                tool_result,
            } => {
                assert_eq!(message_id, "msg_05");
                assert_eq!(tool_result.tool_use_id, "toolu_a");
                assert_eq!(tool_result.content, "127.0.0.1 localhost");
                assert!(!tool_result.is_error);
            }
            _ => panic!("expected ToolResult"),
        }
    }

    #[test]
    fn parses_thinking_block_emits_thinking_event() {
        // Extended-thinking content lands in assistant.message.content as a
        // block of type "thinking". The parser must surface it as a
        // dedicated event so the UI can show "what Claude is doing".
        let line = r#"{"type":"assistant","message":{"id":"msg_th","role":"assistant","content":[{"type":"thinking","thinking":"Let me check the file structure first."}]}}"#;
        let evs = parse_line(line).unwrap();
        // No empty-text Message must be emitted alongside the thinking block —
        // the parser used to anchor a bubble with text="" for thinking-only
        // turns and the chat would render it as an empty rounded box.
        assert!(
            !evs.iter()
                .any(|e| matches!(e, AgentEvent::Message { text, .. } if text.is_empty())),
            "thinking-only turn must not emit an empty Message: {evs:?}"
        );
        let thinking = evs
            .iter()
            .find_map(|e| match e {
                AgentEvent::Thinking {
                    message_id,
                    text,
                    is_partial,
                } => Some((message_id, text, is_partial)),
                _ => None,
            })
            .expect("expected Thinking event");
        assert_eq!(thinking.0, "msg_th");
        assert_eq!(thinking.1, "Let me check the file structure first.");
        assert!(!thinking.2);
    }

    #[test]
    fn assistant_turn_with_text_and_thinking_emits_both() {
        let line = r#"{"type":"assistant","message":{"id":"msg_mix","role":"assistant","content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"hello"}]}}"#;
        let evs = parse_line(line).unwrap();
        let has_thinking = evs
            .iter()
            .any(|e| matches!(e, AgentEvent::Thinking { text, .. } if text == "hmm"));
        let has_text = evs
            .iter()
            .any(|e| matches!(e, AgentEvent::Message { text, .. } if text == "hello"));
        assert!(has_thinking, "expected a Thinking event in {evs:?}");
        assert!(has_text, "expected a text Message in {evs:?}");
    }

    #[test]
    fn assistant_turn_with_only_tool_use_does_not_emit_empty_message() {
        // The store creates a synthetic Message from the ToolUse event, so
        // the parser must NOT also emit a trailing empty-text Message —
        // that produced the visible "empty bubble" in production chats.
        let line = r#"{"type":"assistant","message":{"id":"msg_t","role":"assistant","content":[{"type":"tool_use","id":"toolu_z","name":"Read","input":{"file_path":"/x"}}]}}"#;
        let evs = parse_line(line).unwrap();
        assert!(
            !evs.iter()
                .any(|e| matches!(e, AgentEvent::Message { text, .. } if text.is_empty())),
            "tool-only turn must not emit an empty Message: {evs:?}"
        );
        // ToolUse must still be there.
        assert!(
            evs.iter().any(|e| matches!(e, AgentEvent::ToolUse { .. })),
            "expected ToolUse event in {evs:?}"
        );
    }

    #[test]
    fn assistant_turn_completely_empty_emits_nothing() {
        // Pathological "assistant message with no recognised content blocks"
        // (only an unknown content type, or genuinely empty content array) —
        // the parser must yield zero events rather than an empty bubble.
        let line =
            r#"{"type":"assistant","message":{"id":"msg_empty","role":"assistant","content":[]}}"#;
        let evs = parse_line(line).unwrap();
        assert!(
            evs.is_empty(),
            "empty content must yield zero events: {evs:?}"
        );

        let line2 = r#"{"type":"assistant","message":{"id":"msg_unknown","role":"assistant","content":[{"type":"weird_block","payload":"x"}]}}"#;
        let evs2 = parse_line(line2).unwrap();
        assert!(
            evs2.is_empty(),
            "assistant message with only unknown blocks must yield zero events: {evs2:?}"
        );
    }

    #[test]
    fn stream_parser_emits_partial_thinking_on_thinking_delta() {
        // Mirrors the text_delta partial flow but for thinking blocks so the
        // UI can stream "Claude is thinking…" content as it arrives.
        let mut p = StreamParser::new();
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_thx","role":"assistant","content":[]}}}"#,
        )
        .unwrap();
        let evs = p
            .parse_line(
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me"}}}"#,
            )
            .unwrap();
        match &evs[0] {
            AgentEvent::Thinking {
                message_id,
                text,
                is_partial,
            } => {
                assert_eq!(message_id, "msg_thx");
                assert_eq!(text, "Let me");
                assert!(*is_partial);
            }
            other => panic!("expected partial Thinking, got {other:?}"),
        }
        // Subsequent thinking deltas accumulate into the same id.
        let evs2 = p
            .parse_line(
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":" inspect"}}}"#,
            )
            .unwrap();
        match &evs2[0] {
            AgentEvent::Thinking { text, .. } => assert_eq!(text, "Let me inspect"),
            other => panic!("expected accumulated Thinking, got {other:?}"),
        }
    }

    #[test]
    fn parses_system_compact_boundary_with_full_metadata() {
        let line = r#"{"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"auto","pre_tokens":45000}}"#;
        let evs = parse_line(line).unwrap();
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::Compact {
                trigger,
                pre_tokens,
            } => {
                assert_eq!(trigger, "auto");
                assert_eq!(*pre_tokens, Some(45_000));
            }
            other => panic!("expected Compact, got {other:?}"),
        }
    }

    #[test]
    fn parses_system_compact_boundary_without_pre_tokens() {
        // Older CLI variants drop pre_tokens — the parser must still emit
        // a Compact event so the UI can flag the boundary.
        let line = r#"{"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"manual"}}"#;
        let evs = parse_line(line).unwrap();
        match &evs[0] {
            AgentEvent::Compact {
                trigger,
                pre_tokens,
            } => {
                assert_eq!(trigger, "manual");
                assert_eq!(*pre_tokens, None);
            }
            other => panic!("expected Compact, got {other:?}"),
        }
    }

    #[test]
    fn parses_system_compact_boundary_without_metadata_falls_back_to_auto() {
        // Defensive: if the CLI ever ships a bare boundary marker, we still
        // surface a useful event rather than dropping the line on the floor.
        let line = r#"{"type":"system","subtype":"compact_boundary"}"#;
        let evs = parse_line(line).unwrap();
        match &evs[0] {
            AgentEvent::Compact {
                trigger,
                pre_tokens,
            } => {
                assert_eq!(trigger, "auto");
                assert!(pre_tokens.is_none());
            }
            other => panic!("expected Compact, got {other:?}"),
        }
    }

    #[test]
    fn parses_result_into_status_waiting_in_stream_json() {
        // The "result" event marks end-of-turn. The agent process stays
        // alive (real shutdown is signalled by EOF on the reader), but the
        // user-facing status must drop back to Waiting so the live turn
        // indicator stops counting up.
        let line =
            r#"{"type":"result","subtype":"success","total_cost_usd":0.001,"is_error":false}"#;
        let evs = parse_line(line).unwrap();
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::Status { status } => assert_eq!(*status, AgentStatus::Waiting),
            other => panic!("expected Status::Waiting, got {other:?}"),
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

    // ── StreamParser tests (stateful, for --include-partial-messages) ─────────
    #[test]
    fn stream_parser_passes_through_non_stream_event_lines() {
        let mut p = StreamParser::new();
        let line = r#"{"type":"system","subtype":"init","session_id":"s","model":"m","tools":[],"cwd":"/"}"#;
        let evs = p.parse_line(line).unwrap();
        assert!(matches!(evs[0], AgentEvent::Init { .. }));
    }

    #[test]
    fn stream_parser_message_start_emits_no_event_but_tracks_id() {
        let mut p = StreamParser::new();
        let line = r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_99","role":"assistant","content":[]}},"session_id":"s","parent_tool_use_id":null}"#;
        let evs = p.parse_line(line).unwrap();
        assert!(evs.is_empty());
    }

    #[test]
    fn stream_parser_emits_partial_message_on_text_delta() {
        let mut p = StreamParser::new();
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","role":"assistant","content":[]}}}"#,
        )
        .unwrap();
        let evs = p
            .parse_line(
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}"#,
            )
            .unwrap();
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::Message {
                id,
                role,
                text,
                is_partial,
            } => {
                assert_eq!(id, "msg_1");
                assert_eq!(role, &MessageRole::Assistant);
                assert_eq!(text, "Hello");
                assert!(*is_partial);
            }
            other => panic!("expected partial Message, got {other:?}"),
        }
    }

    #[test]
    fn stream_parser_accumulates_multiple_text_deltas() {
        let mut p = StreamParser::new();
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_2","role":"assistant","content":[]}}}"#,
        )
        .unwrap();
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}"#,
        )
        .unwrap();
        let evs = p
            .parse_line(
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}}"#,
            )
            .unwrap();
        match &evs[0] {
            AgentEvent::Message { text, .. } => assert_eq!(text, "Hello world"),
            other => panic!("expected partial Message, got {other:?}"),
        }
    }

    #[test]
    fn stream_parser_message_stop_clears_state_and_emits_nothing() {
        let mut p = StreamParser::new();
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_3","role":"assistant","content":[]}}}"#,
        )
        .unwrap();
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}}"#,
        )
        .unwrap();
        let evs = p
            .parse_line(r#"{"type":"stream_event","event":{"type":"message_stop"}}"#)
            .unwrap();
        assert!(evs.is_empty());

        // After message_stop, a new message_start with a different id starts
        // fresh — no leakage from the previous message's accumulated text.
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_4","role":"assistant","content":[]}}}"#,
        )
        .unwrap();
        let evs = p
            .parse_line(
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"new"}}}"#,
            )
            .unwrap();
        match &evs[0] {
            AgentEvent::Message { text, .. } => assert_eq!(text, "new"),
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn stream_parser_ignores_delta_without_prior_message_start() {
        let mut p = StreamParser::new();
        // Spurious delta arrives without context; should be a no-op rather
        // than crashing or producing a Message with an empty id.
        let evs = p
            .parse_line(
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"orphan"}}}"#,
            )
            .unwrap();
        assert!(evs.is_empty());
    }

    #[test]
    fn stream_parser_ignores_non_text_delta_types() {
        // input_json_delta (for tool_use blocks) currently has no UI
        // representation; verify it doesn't get rendered as text.
        let mut p = StreamParser::new();
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_5","role":"assistant","content":[]}}}"#,
        )
        .unwrap();
        let evs = p
            .parse_line(
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}}"#,
            )
            .unwrap();
        assert!(evs.is_empty());
    }

    #[test]
    fn stream_parser_passes_through_final_assistant_message() {
        // After all the partial events, Claude still emits a regular
        // "assistant" line with the full content. That must still produce a
        // non-partial Message so the frontend can flip is_partial off.
        let mut p = StreamParser::new();
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_6","role":"assistant","content":[]}}}"#,
        )
        .unwrap();
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}}"#,
        )
        .unwrap();
        p.parse_line(r#"{"type":"stream_event","event":{"type":"message_stop"}}"#)
            .unwrap();
        let evs = p
            .parse_line(
                r#"{"type":"assistant","message":{"id":"msg_6","role":"assistant","content":[{"type":"text","text":"Hi"}]}}"#,
            )
            .unwrap();
        match &evs[0] {
            AgentEvent::Message {
                id,
                text,
                is_partial,
                ..
            } => {
                assert_eq!(id, "msg_6");
                assert_eq!(text, "Hi");
                assert!(!is_partial);
            }
            other => panic!("expected final Message, got {other:?}"),
        }
    }

    #[test]
    fn stream_parser_default_matches_new() {
        // The Default impl is what `derive`-style call sites (and library
        // ergonomics) rely on; without a covering test it stays a dead
        // fall-through that silently rots if `new()` ever gains required
        // setup the Default forgets.
        let p_default = StreamParser::default();
        let p_new = StreamParser::new();
        // Neither carries observable state on a fresh instance — both must
        // round-trip an init line identically.
        let mut a = p_default;
        let mut b = p_new;
        let line = r#"{"type":"system","subtype":"init","session_id":"s","model":"m","tools":[],"cwd":"/"}"#;
        assert_eq!(a.parse_line(line).unwrap(), b.parse_line(line).unwrap());
    }
}
