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
    all.extend(scan_plugins_via_manifest(claude_dir));
    dedupe_and_sort(all)
}

fn scan_user_commands(dir: &Path) -> Vec<SlashCommand> {
    scan_markdown_dir(dir, SlashCommandSource::User)
}

/// Enumerate plugin slash commands by reading the manifest Claude Code
/// maintains at `<claude_dir>/plugins/installed_plugins.json`. Each entry
/// resolves to one absolute `installPath`; we then scan
/// `<installPath>/commands/*.md` and `<installPath>/skills/<skill>/SKILL.md`.
///
/// Fail-soft on every error path: missing manifest, malformed JSON, missing
/// `installPath`, and stale paths all log a warning and yield an empty
/// contribution rather than aborting discovery.
fn scan_plugins_via_manifest(claude_dir: &Path) -> Vec<SlashCommand> {
    let manifest_path = claude_dir.join("plugins").join("installed_plugins.json");
    let raw = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %manifest_path.display(),
                "slash_commands: installed_plugins.json missing or unreadable; \
                 skipping plugin discovery"
            );
            return Vec::new();
        }
    };
    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %manifest_path.display(),
                "slash_commands: installed_plugins.json is not valid JSON; \
                 skipping plugin discovery"
            );
            return Vec::new();
        }
    };
    let plugins = match parsed.get("plugins").and_then(|p| p.as_object()) {
        Some(obj) => obj,
        None => {
            tracing::warn!(
                path = %manifest_path.display(),
                "slash_commands: installed_plugins.json has no `plugins` object; \
                 skipping plugin discovery"
            );
            return Vec::new();
        }
    };

    let mut out: Vec<SlashCommand> = Vec::new();
    for (qualified_name, entries_value) in plugins {
        // Display name = part before `@` in `name@marketplace`.
        let display_name = qualified_name
            .split_once('@')
            .map(|(name, _)| name)
            .unwrap_or(qualified_name.as_str())
            .to_string();
        let entries = match entries_value.as_array() {
            Some(arr) => arr,
            None => {
                tracing::warn!(
                    plugin = %qualified_name,
                    "slash_commands: plugin entries are not an array; skipping"
                );
                continue;
            }
        };
        if entries.is_empty() {
            continue;
        }
        // Pick the first `scope == "user"` entry; otherwise fall back to the
        // first entry in the array. Project-scoped installs are out of scope
        // for this discovery pass.
        let chosen = entries
            .iter()
            .find(|e| e.get("scope").and_then(|s| s.as_str()) == Some("user"))
            .unwrap_or(&entries[0]);
        let install_path_str = match chosen.get("installPath").and_then(|p| p.as_str()) {
            Some(s) if !s.is_empty() => s,
            _ => {
                tracing::warn!(
                    plugin = %qualified_name,
                    "slash_commands: plugin entry missing `installPath`; skipping"
                );
                continue;
            }
        };
        let install_path = Path::new(install_path_str);
        if !install_path.exists() {
            tracing::warn!(
                plugin = %qualified_name,
                path = %install_path.display(),
                "slash_commands: plugin installPath does not exist on disk; \
                 skipping (stale manifest entry?)"
            );
            continue;
        }

        let source = SlashCommandSource::Plugin {
            plugin: display_name,
        };
        // <installPath>/commands/*.md
        out.extend(scan_markdown_dir(
            &install_path.join("commands"),
            source.clone(),
        ));
        // <installPath>/skills/<skill>/SKILL.md
        let skills_root = install_path.join("skills");
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

    /// Writes a fake `<claude_dir>/plugins/installed_plugins.json` with the
    /// given (qualified_name, installPath) pairs. Mirrors the layout that
    /// Claude Code maintains on real systems so the discovery path is
    /// exercised end-to-end in unit tests.
    fn write_installed_plugins_manifest(claude_dir: &Path, entries: &[(&str, &str)]) {
        use std::fmt::Write as _;
        let mut json = String::from("{\"version\":2,\"plugins\":{");
        for (i, (qualified, install_path)) in entries.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            let _ = write!(
                &mut json,
                "\"{qualified}\":[{{\"scope\":\"user\",\"installPath\":\"{install_path}\",\"version\":\"1.0.0\"}}]"
            );
        }
        json.push_str("}}");
        let plugins_dir = claude_dir.join("plugins");
        std::fs::create_dir_all(&plugins_dir).unwrap();
        std::fs::write(plugins_dir.join("installed_plugins.json"), json).unwrap();
    }

    #[test]
    fn discover_includes_plugin_commands_and_skills() {
        let tmp = tempfile::tempdir().unwrap();
        // installPath lives outside <claude_dir>/plugins to mirror the real
        // cache layout (`~/.claude/plugins/cache/<marketplace>/<plugin>/<ver>`).
        let install_path = tmp.path().join("cache/official/superpowers/1.0.0");
        write_md_with_frontmatter(
            &install_path.join("commands/writing-plans.md"),
            "Use when you have a spec for a multi-step task",
            "body",
        );
        write_md_with_frontmatter(
            &install_path.join("skills/brainstorming/SKILL.md"),
            "Turn ideas into designs",
            "body",
        );
        write_installed_plugins_manifest(
            tmp.path(),
            &[(
                "superpowers@claude-plugins-official",
                install_path.to_str().unwrap(),
            )],
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
        let install_path = tmp.path().join("cache/official/foo/1.0.0");
        write_md_with_frontmatter(
            &install_path.join("commands/help.md"),
            "Plugin help (shadowed)",
            "body",
        );
        write_installed_plugins_manifest(
            tmp.path(),
            &[("foo@official", install_path.to_str().unwrap())],
        );
        let result = discover(tmp.path());
        let helps: Vec<_> = result.iter().filter(|c| c.name == "help").collect();
        assert_eq!(helps.len(), 1, "dedupe must collapse to a single 'help'");
        assert_eq!(helps[0].source, SlashCommandSource::User);
        assert_eq!(helps[0].description, "User override of help");
    }

    #[test]
    fn discover_skips_plugins_with_missing_install_path() {
        // Manifest references a path that doesn't exist on disk (e.g. the
        // plugin was uninstalled but the manifest entry wasn't pruned).
        // Discovery must fail-soft and still return at least the builtins.
        let tmp = tempfile::tempdir().unwrap();
        let bogus = tmp.path().join("does/not/exist");
        write_installed_plugins_manifest(
            tmp.path(),
            &[("ghost@official", bogus.to_str().unwrap())],
        );
        let result = discover(tmp.path());
        assert!(
            !result.is_empty(),
            "discover() must not crash on missing installPath"
        );
        assert!(
            result
                .iter()
                .all(|c| !matches!(c.source, SlashCommandSource::Plugin { .. })),
            "no plugin entries should be emitted when installPath is missing"
        );
        // Sanity: builtins are still there.
        assert!(result.iter().any(|c| c.name == "help"));
    }

    #[test]
    fn discover_strips_marketplace_suffix_from_plugin_name() {
        // The display name is the part before `@` in the qualified name.
        // `foo@official` → `Plugin { plugin: "foo" }`.
        let tmp = tempfile::tempdir().unwrap();
        let install_path = tmp.path().join("cache/official/foo/1.0.0");
        write_md_with_frontmatter(
            &install_path.join("commands/widget.md"),
            "a widget command",
            "body",
        );
        write_installed_plugins_manifest(
            tmp.path(),
            &[("foo@official", install_path.to_str().unwrap())],
        );
        let result = discover(tmp.path());
        let widget = result
            .iter()
            .find(|c| c.name == "widget")
            .expect("widget must be discovered");
        assert_eq!(
            widget.source,
            SlashCommandSource::Plugin {
                plugin: "foo".into()
            }
        );
    }

    #[test]
    fn discover_sort_is_bucket_then_alphabetical() {
        let tmp = tempfile::tempdir().unwrap();
        write_md_with_frontmatter(&tmp.path().join("commands/zeta-user.md"), "z", "");
        let install_path = tmp.path().join("cache/official/aaa/1.0.0");
        write_md_with_frontmatter(&install_path.join("commands/alpha-plugin.md"), "a", "");
        write_installed_plugins_manifest(
            tmp.path(),
            &[("aaa@official", install_path.to_str().unwrap())],
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
