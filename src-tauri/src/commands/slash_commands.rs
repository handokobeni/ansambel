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
pub fn discover(claude_dir: &Path) -> Vec<SlashCommand> {
    let mut all: Vec<SlashCommand> = Vec::new();
    all.extend(builtin_commands());
    all.extend(scan_user_commands(&claude_dir.join("commands")));
    all.extend(scan_plugins(&claude_dir.join("plugins")));
    dedupe_and_sort(all)
}

fn scan_user_commands(dir: &Path) -> Vec<SlashCommand> {
    scan_markdown_dir(dir, SlashCommandSource::User)
}

fn scan_plugins(plugins_dir: &Path) -> Vec<SlashCommand> {
    let entries = match std::fs::read_dir(plugins_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<SlashCommand> = Vec::new();
    for plugin_entry in entries.flatten() {
        let plugin_path = plugin_entry.path();
        if !plugin_path.is_dir() {
            continue;
        }
        let plugin_name = match plugin_path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let source = SlashCommandSource::Plugin {
            plugin: plugin_name.clone(),
        };
        // Commands directory.
        out.extend(scan_markdown_dir(
            &plugin_path.join("commands"),
            source.clone(),
        ));
        // Skills (one level deeper: <plugin>/skills/<skill>/SKILL.md).
        let skills_root = plugin_path.join("skills");
        if let Ok(skill_entries) = std::fs::read_dir(&skills_root) {
            for skill_entry in skill_entries.flatten() {
                let skill_dir = skill_entry.path();
                if !skill_dir.is_dir() {
                    continue;
                }
                let skill_md = skill_dir.join("SKILL.md");
                if let Some(cmd) = parse_md_command(&skill_md, source.clone()) {
                    out.push(cmd);
                }
            }
        }
        // Plugin layouts also sometimes nest <plugin>/<version>/skills/... — be
        // tolerant: scan one level of intermediate dirs that aren't `commands`
        // or `skills` themselves.
        if let Ok(plugin_inner) = std::fs::read_dir(&plugin_path) {
            for inner in plugin_inner.flatten() {
                let p = inner.path();
                let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !p.is_dir() || fname == "commands" || fname == "skills" {
                    continue;
                }
                // Treat `<plugin>/<inner>/skills/*` and `<plugin>/<inner>/commands/*`
                // the same way as the top-level forms.
                out.extend(scan_markdown_dir(&p.join("commands"), source.clone()));
                if let Ok(skill_entries) = std::fs::read_dir(p.join("skills")) {
                    for skill_entry in skill_entries.flatten() {
                        let skill_dir = skill_entry.path();
                        if !skill_dir.is_dir() {
                            continue;
                        }
                        let skill_md = skill_dir.join("SKILL.md");
                        if let Some(cmd) = parse_md_command(&skill_md, source.clone()) {
                            out.push(cmd);
                        }
                    }
                }
            }
        }
    }
    out
}

fn scan_markdown_dir(dir: &Path, source: SlashCommandSource) -> Vec<SlashCommand> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<SlashCommand> = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Some(cmd) = parse_md_command(&p, source.clone()) {
            out.push(cmd);
        }
    }
    out
}

fn parse_md_command(path: &Path, source: SlashCommandSource) -> Option<SlashCommand> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %path.display(),
                "slash_commands: skip unreadable file"
            );
            return None;
        }
    };
    let (frontmatter_name, frontmatter_desc, body) = parse_frontmatter(&content);
    // Name: prefer frontmatter `name:` if present, else file/dir basename.
    let basename = if path.file_name().and_then(|n| n.to_str()) == Some("SKILL.md") {
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    } else {
        path.file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    };
    let name = frontmatter_name.unwrap_or(basename);
    if name.is_empty() {
        return None;
    }
    let description = frontmatter_desc
        .or_else(|| first_non_blank_body_line(body))
        .unwrap_or_default();
    Some(SlashCommand {
        name,
        description,
        source,
    })
}

/// Returns (name?, description?, body-without-frontmatter).
fn parse_frontmatter(content: &str) -> (Option<String>, Option<String>, &str) {
    // Minimal hand-rolled YAML frontmatter: between two `---` lines at the
    // very start of the file. We only need `name:` and `description:`.
    let rest = match content.strip_prefix("---\n") {
        Some(r) => r,
        None => return (None, None, content),
    };
    let end = match rest.find("\n---") {
        Some(e) => e,
        None => return (None, None, content),
    };
    let frontmatter = &rest[..end];
    let body_start = end + "\n---".len();
    // Skip the trailing newline after `---`.
    let body = rest[body_start..].trim_start_matches('\n');
    let mut name: Option<String> = None;
    let mut desc: Option<String> = None;
    for line in frontmatter.lines() {
        if let Some(v) = line.strip_prefix("name:") {
            name = Some(v.trim().trim_matches('"').to_string()).filter(|s| !s.is_empty());
        } else if let Some(v) = line.strip_prefix("description:") {
            desc = Some(v.trim().trim_matches('"').to_string()).filter(|s| !s.is_empty());
        }
    }
    (name, desc, body)
}

fn first_non_blank_body_line(body: &str) -> Option<String> {
    body.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.chars().take(120).collect())
}

fn dedupe_and_sort(mut all: Vec<SlashCommand>) -> Vec<SlashCommand> {
    // Priority by source for dedupe: User > Plugin > Builtin.
    let source_priority = |s: &SlashCommandSource| -> u8 {
        match s {
            SlashCommandSource::User => 0,
            SlashCommandSource::Plugin { .. } => 1,
            SlashCommandSource::Builtin => 2,
        }
    };
    // For deterministic plugin-vs-plugin tie-breaking when two plugins
    // define the same name, prefer the alphabetically-first plugin.
    let plugin_key = |s: &SlashCommandSource| -> String {
        if let SlashCommandSource::Plugin { plugin } = s {
            plugin.to_lowercase()
        } else {
            String::new()
        }
    };
    // Sort by (name, source_priority, plugin_key) so the .dedup_by below
    // keeps the highest-priority entry for each name.
    all.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| source_priority(&a.source).cmp(&source_priority(&b.source)))
            .then_with(|| plugin_key(&a.source).cmp(&plugin_key(&b.source)))
    });
    all.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    // Final display order: bucket Builtin → User → Plugin, then alphabetical
    // (case-insensitive) within each bucket.
    let bucket = |s: &SlashCommandSource| -> u8 {
        match s {
            SlashCommandSource::Builtin => 0,
            SlashCommandSource::User => 1,
            SlashCommandSource::Plugin { .. } => 2,
        }
    };
    all.sort_by(|a, b| {
        bucket(&a.source)
            .cmp(&bucket(&b.source))
            .then_with(|| plugin_key(&a.source).cmp(&plugin_key(&b.source)))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    all
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

    fn write_md_with_frontmatter(path: &Path, description: &str, body: &str) {
        let content = format!("---\ndescription: {description}\n---\n\n{body}\n");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn write_plain_md(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn discover_includes_user_commands_from_commands_dir() {
        let tmp = tempfile::tempdir().unwrap();
        write_md_with_frontmatter(
            &tmp.path().join("commands/deploy.md"),
            "Deploy the current branch to staging",
            "step-by-step body",
        );
        let result = discover(tmp.path());
        let deploy = result
            .iter()
            .find(|c| c.name == "deploy")
            .expect("deploy must be discovered");
        assert_eq!(deploy.description, "Deploy the current branch to staging");
        assert_eq!(deploy.source, SlashCommandSource::User);
    }

    #[test]
    fn discover_falls_back_to_first_body_line_when_frontmatter_absent() {
        let tmp = tempfile::tempdir().unwrap();
        write_plain_md(
            &tmp.path().join("commands/plain.md"),
            "First line is the description\n\nFurther body content.",
        );
        let result = discover(tmp.path());
        let plain = result.iter().find(|c| c.name == "plain").unwrap();
        assert_eq!(plain.description, "First line is the description");
        assert_eq!(plain.source, SlashCommandSource::User);
    }

    #[test]
    fn discover_includes_plugin_commands_and_skills() {
        let tmp = tempfile::tempdir().unwrap();
        write_md_with_frontmatter(
            &tmp.path()
                .join("plugins/superpowers/commands/writing-plans.md"),
            "Use when you have a spec for a multi-step task",
            "body",
        );
        write_md_with_frontmatter(
            &tmp.path()
                .join("plugins/superpowers/skills/brainstorming/SKILL.md"),
            "Turn ideas into designs",
            "body",
        );
        let result = discover(tmp.path());
        let plans = result.iter().find(|c| c.name == "writing-plans").unwrap();
        assert_eq!(
            plans.source,
            SlashCommandSource::Plugin {
                plugin: "superpowers".into()
            }
        );
        assert!(plans.description.contains("multi-step task"));
        let brain = result.iter().find(|c| c.name == "brainstorming").unwrap();
        assert_eq!(
            brain.source,
            SlashCommandSource::Plugin {
                plugin: "superpowers".into()
            }
        );
    }

    #[test]
    fn discover_dedupes_user_over_plugin_over_builtin() {
        let tmp = tempfile::tempdir().unwrap();
        // `help` is a builtin. Add a plugin `help` and a user `help` — only the
        // user one should survive.
        write_md_with_frontmatter(
            &tmp.path().join("commands/help.md"),
            "User override of help",
            "body",
        );
        write_md_with_frontmatter(
            &tmp.path().join("plugins/foo/commands/help.md"),
            "Plugin help (shadowed)",
            "body",
        );
        let result = discover(tmp.path());
        let helps: Vec<_> = result.iter().filter(|c| c.name == "help").collect();
        assert_eq!(helps.len(), 1, "dedupe must collapse to a single 'help'");
        assert_eq!(helps[0].source, SlashCommandSource::User);
        assert_eq!(helps[0].description, "User override of help");
    }

    #[test]
    fn discover_sort_is_bucket_then_alphabetical() {
        let tmp = tempfile::tempdir().unwrap();
        write_md_with_frontmatter(&tmp.path().join("commands/zeta-user.md"), "z", "");
        write_md_with_frontmatter(
            &tmp.path().join("plugins/aaa/commands/alpha-plugin.md"),
            "a",
            "",
        );
        let result = discover(tmp.path());
        // The first entry must be a Builtin; the last must be a Plugin.
        assert_eq!(result.first().unwrap().source, SlashCommandSource::Builtin);
        assert!(matches!(
            result.last().unwrap().source,
            SlashCommandSource::Plugin { .. }
        ));
        // Within the User bucket, only 'zeta-user' exists; spot-check it is
        // positioned after every Builtin and before every Plugin entry.
        let user_pos = result.iter().position(|c| c.name == "zeta-user").unwrap();
        let plugin_pos = result
            .iter()
            .position(|c| c.name == "alpha-plugin")
            .unwrap();
        assert!(user_pos < plugin_pos);
        let last_builtin_pos = result
            .iter()
            .rposition(|c| c.source == SlashCommandSource::Builtin)
            .unwrap();
        assert!(last_builtin_pos < user_pos);
    }

    #[test]
    fn discover_is_fail_soft_for_unreadable_files() {
        // A malformed frontmatter file MUST NOT crash discovery.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("commands/broken.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "---\nthis is not valid yaml: [unterminated\n").unwrap();
        // discover() must still return the built-in list — the broken file may
        // or may not appear, but the call MUST succeed.
        let result = discover(tmp.path());
        assert!(result
            .iter()
            .any(|c| c.source == SlashCommandSource::Builtin));
    }
}
