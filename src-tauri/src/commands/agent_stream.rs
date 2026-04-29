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
        "assistant" | "user" => parse_message(&v, kind),
        "result" => Ok(vec![AgentEvent::Status {
            status: AgentStatus::Stopped,
        }]),
        _ => Ok(Vec::new()),
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
                _ => {}
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
    fn parses_result_subtype_into_status_stopped() {
        let line =
            r#"{"type":"result","subtype":"success","total_cost_usd":0.001,"is_error":false}"#;
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
