use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SlashCommandSource {
    Builtin,
    User,
    Plugin { plugin: String },
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    pub source: SlashCommandSource,
}

/// Curated list of well-known claude CLI built-in commands. Extending the
/// set is an opt-in code change — the picker only shows what's listed here.
fn builtin_commands() -> Vec<SlashCommand> {
    let entries: &[(&str, &str)] = &[
        ("agents", "Manage agent configurations"),
        ("clear", "Start a new session with empty context"),
        (
            "compact",
            "Free up context by summarising the conversation so far",
        ),
        ("config", "Open the config panel"),
        (
            "context",
            "Visualise current context usage as a coloured grid",
        ),
        ("copy", "Copy Claude's last response to clipboard"),
        ("diff", "View uncommitted changes and per-turn diffs"),
        (
            "doctor",
            "Diagnose and verify your Claude Code installation",
        ),
        ("effort", "Set effort level for model usage"),
        ("fast", "Toggle fast mode for faster output"),
        ("help", "Show help for commands and shortcuts"),
        ("init", "Initialise a new CLAUDE.md file"),
        (
            "loop",
            "Run a prompt or slash command on a recurring interval",
        ),
        ("model", "Switch the active model"),
        ("release-notes", "Show recent release notes"),
        ("resume", "Resume a previous session"),
        ("review", "Review a pull request"),
        ("run", "Launch and drive this project's app"),
        (
            "schedule",
            "Create, update, list, or run scheduled remote agents",
        ),
        ("verify", "Verify that a code change actually works"),
    ];
    entries
        .iter()
        .map(|(name, desc)| SlashCommand {
            name: (*name).to_string(),
            description: (*desc).to_string(),
            source: SlashCommandSource::Builtin,
        })
        .collect()
}

/// Enumerate slash commands from all sources (builtin + user + plugin),
/// deduped and sorted per spec §Discovery.
///
/// Path arg makes the function unit-testable with a tempdir. The Tauri
/// wrapper resolves `dirs::home_dir().map(|h| h.join(".claude"))` and
/// passes it in.
pub fn discover(_claude_dir: &Path) -> Vec<SlashCommand> {
    // Task 1: builtin only. Task 2 adds user + plugin discovery + dedupe.
    builtin_commands()
}

#[tauri::command]
pub async fn list_slash_commands() -> std::result::Result<Vec<SlashCommand>, String> {
    let claude_dir = directories::UserDirs::new()
        .map(|d| d.home_dir().join(".claude"))
        .unwrap_or_else(|| PathBuf::from(".claude"));
    Ok(discover(&claude_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_with_empty_claude_dir_returns_only_builtins() {
        let tmp = tempfile::tempdir().unwrap();
        let result = discover(tmp.path());
        // Built-in set is non-empty and contains canonical entries.
        assert!(!result.is_empty());
        let names: Vec<&str> = result.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"help"));
        assert!(names.contains(&"clear"));
        assert!(names.contains(&"agents"));
        // Every entry from the empty-dir path is Builtin.
        assert!(result
            .iter()
            .all(|c| c.source == SlashCommandSource::Builtin));
    }

    #[test]
    fn builtin_commands_carry_non_empty_descriptions() {
        for cmd in builtin_commands() {
            assert!(
                !cmd.description.is_empty(),
                "builtin '{}' is missing description",
                cmd.name
            );
        }
    }

    #[test]
    fn builtin_commands_are_sorted_alphabetically() {
        let names: Vec<String> = builtin_commands().into_iter().map(|c| c.name).collect();
        let mut sorted = names.clone();
        sorted.sort_by_key(|a| a.to_lowercase());
        assert_eq!(names, sorted, "builtin list must be in alphabetical order");
    }
}
