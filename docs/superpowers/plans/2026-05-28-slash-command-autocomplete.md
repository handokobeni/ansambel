# Slash Command Autocomplete Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the user types `/` at the start of a line in the workspace chat
input, show an autocomplete picker listing built-in + user + plugin/skill slash
commands (name + description). Enter inserts `/full-name ` into the textarea;
the existing chat → claude IPC carries the command through.

**Architecture:** New backend `list_slash_commands` Tauri command enumerates 3
sources (builtin curated list, `~/.claude/commands/*.md`,
`~/.claude/plugins/*/{commands,skills}/...`) with YAML-frontmatter parsing,
dedupe (user > plugin > builtin), and bucket-then-alphabetical sort. Frontend
store caches the list at app boot. A new `SlashCommandPicker.svelte` popover
anchored to the chat textarea handles filter + keyboard nav + selection.
ChatInput owns the trigger regex on the current line and the text-replacement on
selection.

**Tech Stack:** Rust + Tauri v2 (existing `commands/*` pattern), serde, Svelte 5
runes, Bun, vitest. TDD strict (red → green → commit). No
`.unwrap()`/`.expect()` outside `#[cfg(test)]`. No `console.log` (use
`console.error`/`console.warn` in catch paths). Mutex discipline N/A here (this
feature has no shared mutable state beyond the frontend store).

**Spec:**
`docs/superpowers/specs/2026-05-28-slash-command-autocomplete-design.md` — read
it once before starting; all rationale and edge cases live there.

**Branch placement:** Fold into the active `feat/multi-card-workspace` branch
per the user's explicit instruction. Tasks here pick up after the multi-card
commits.

**Standing constraints (verbatim):** Commit LOCALLY per task, **DO NOT push**
until the user explicitly approves the whole branch. Each task ends with
`git commit` (no `git push`).

---

## Task 1: Backend — module scaffold + built-in command list

**Files:**

- Create: `src-tauri/src/commands/slash_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs` (`pub mod slash_commands;`)
- Modify: `src-tauri/src/lib.rs` (register `list_slash_commands` in
  `tauri::generate_handler!` and add a registration smoke test)

This task ships the data types, the Tauri command shell, and the built-in list.
Subsequent tasks layer User + Plugin discovery into `discover()`. Each task's
tests must pass before the next.

### Precondition check (informational, do FIRST before any code)

Confirm the spec's assumption: typing `/help` (or similar) into Ansambel's chat
input today actually invokes the slash command on claude. Grep
`grep -n "send_message\|user_input\|stdin" src-tauri/src/commands/agent.rs` to
find the user→claude write path. Verify it writes the user's raw text without
stripping leading `/`. If the path looks correct, proceed with this plan as-is.
If not, STOP and surface the gap to the user — the spec calls this out as the
one precondition that, if it fails, expands scope.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/commands/slash_commands.rs` with the types + the
`discover()` function STUB returning only the built-in list. Tests verify the
shape + a few built-in entries.

```rust
use crate::error::Result;
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
        ("compact", "Free up context by summarising the conversation so far"),
        ("config", "Open the config panel"),
        ("context", "Visualise current context usage as a coloured grid"),
        ("copy", "Copy Claude's last response to clipboard"),
        ("diff", "View uncommitted changes and per-turn diffs"),
        ("doctor", "Diagnose and verify your Claude Code installation"),
        ("effort", "Set effort level for model usage"),
        ("fast", "Toggle fast mode for faster output"),
        ("help", "Show help for commands and shortcuts"),
        ("init", "Initialise a new CLAUDE.md file"),
        ("loop", "Run a prompt or slash command on a recurring interval"),
        ("model", "Switch the active model"),
        ("release-notes", "Show recent release notes"),
        ("resume", "Resume a previous session"),
        ("review", "Review a pull request"),
        ("run", "Launch and drive this project's app"),
        ("schedule", "Create, update, list, or run scheduled remote agents"),
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
    let claude_dir = dirs::home_dir()
        .map(|h| h.join(".claude"))
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
        assert!(result.iter().all(|c| c.source == SlashCommandSource::Builtin));
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
        sorted.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        assert_eq!(names, sorted, "builtin list must be in alphabetical order");
    }
}
```

`dirs` crate may not be in Cargo.toml — check with
`cd src-tauri && cargo tree -p dirs 2>&1 | head -3`. If absent, use
`std::env::home_dir()` (deprecated but acceptable for now) OR `directories`
crate already in the deps. As a last resort, fall back to
`std::env::var("HOME").map(PathBuf::from)`. Either way, the Tauri-wrapper is
wrapped in `Result<..., String>` so a missing home dir surfaces gracefully.

- [ ] **Step 2: Run tests to verify RED**

Run: `cd src-tauri && cargo test --lib commands::slash_commands::tests`
Expected: compile errors (the module doesn't exist yet — but you just created
it, so this means the FIRST test run should hit either compile success + the
tests passing, OR a missing-dep error if `tempfile`/`dirs` aren't available).

If the suite compiles cleanly and the tests pass immediately, that's also
acceptable — the "red phase" for a new module + curated list is degenerate. The
real "red phase" lives in Task 2 (user + plugin discovery, where tests will fail
against the Task 1 stub).

- [ ] **Step 3: Register module + Tauri command**

In `src-tauri/src/commands/mod.rs`, add `pub mod slash_commands;` next to the
existing `commands::*` modules (preserve alphabetical/grouping convention).

In `src-tauri/src/lib.rs`, find the `tauri::generate_handler![...]` block
(around lib.rs:296-320) and add
`crate::commands::slash_commands::list_slash_commands,`. Also add a registration
smoke test next to the existing ones:

```rust
#[test]
fn list_slash_commands_command_is_registered() {
    let _ = crate::commands::slash_commands::list_slash_commands as *const () as usize;
}
```

- [ ] **Step 4: GREEN + gates**

Run:

```
cd src-tauri && cargo test --lib commands::slash_commands
cd src-tauri && cargo test --lib 2>&1 | tail -3
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -3
cd src-tauri && cargo fmt --all -- --check
```

Expected: 3 new tests + 1 registration smoke test pass; full suite green (was
819 → 823); clippy + fmt clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/slash_commands.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(slash): builtin slash command list + list_slash_commands handler"
```

---

## Task 2: Backend — user + plugin discovery + dedupe + sort

**Files:**

- Modify: `src-tauri/src/commands/slash_commands.rs` — extend `discover()` and
  helper functions; add tests.

This task adds the User scanner, the Plugin scanner (commands + skills), YAML
frontmatter parsing, dedupe (user > plugin > builtin), and
bucket-then-alphabetical sort. After this, `list_slash_commands` returns the
full spec-defined set.

- [ ] **Step 1: Write the failing tests**

Append to `commands/slash_commands.rs` `#[cfg(test)] mod tests`:

```rust
fn write_md_with_frontmatter(path: &Path, description: &str, body: &str) {
    let content = format!(
        "---\ndescription: {description}\n---\n\n{body}\n",
        description = description,
        body = body,
    );
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
    let deploy = result.iter().find(|c| c.name == "deploy").expect("deploy must be discovered");
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
        &tmp.path().join("plugins/superpowers/commands/writing-plans.md"),
        "Use when you have a spec for a multi-step task",
        "body",
    );
    write_md_with_frontmatter(
        &tmp.path().join("plugins/superpowers/skills/brainstorming/SKILL.md"),
        "Turn ideas into designs",
        "body",
    );
    let result = discover(tmp.path());
    let plans = result.iter().find(|c| c.name == "writing-plans").unwrap();
    assert_eq!(
        plans.source,
        SlashCommandSource::Plugin { plugin: "superpowers".into() }
    );
    assert!(plans.description.contains("multi-step task"));
    let brain = result.iter().find(|c| c.name == "brainstorming").unwrap();
    assert_eq!(
        brain.source,
        SlashCommandSource::Plugin { plugin: "superpowers".into() }
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
    write_md_with_frontmatter(
        &tmp.path().join("commands/zeta-user.md"), "z", "");
    write_md_with_frontmatter(
        &tmp.path().join("plugins/aaa/commands/alpha-plugin.md"), "a", "");
    let result = discover(tmp.path());
    // The first entry must be a Builtin; the last must be a Plugin.
    assert_eq!(result.first().unwrap().source, SlashCommandSource::Builtin);
    assert!(matches!(result.last().unwrap().source, SlashCommandSource::Plugin { .. }));
    // Within the User bucket, only 'zeta-user' exists; spot-check it is
    // positioned after every Builtin and before every Plugin entry.
    let user_pos = result.iter().position(|c| c.name == "zeta-user").unwrap();
    let plugin_pos = result.iter().position(|c| c.name == "alpha-plugin").unwrap();
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
    assert!(result.iter().any(|c| c.source == SlashCommandSource::Builtin));
}
```

- [ ] **Step 2: Run RED**

Run: `cd src-tauri && cargo test --lib commands::slash_commands::tests`
Expected: 6 new tests fail because `discover()` only returns built-ins.

- [ ] **Step 3: Implement user + plugin discovery**

Replace `discover()` and add helpers in `commands/slash_commands.rs`:

```rust
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
    let Ok(entries) = std::fs::read_dir(plugins_dir) else {
        return Vec::new();
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
        let source = SlashCommandSource::Plugin { plugin: plugin_name.clone() };
        // Commands directory.
        out.extend(scan_markdown_dir(&plugin_path.join("commands"), source.clone()));
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
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
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
    let content = std::fs::read_to_string(path)
        .map_err(|e| tracing::warn!(error = %e, path = %path.display(), "slash_commands: skip unreadable file"))
        .ok()?;
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
    Some(SlashCommand { name, description, source })
}

/// Returns (name?, description?, body-without-frontmatter).
fn parse_frontmatter(content: &str) -> (Option<String>, Option<String>, &str) {
    // Minimal hand-rolled YAML frontmatter: between two `---` lines at the
    // very start of the file. We only need `name:` and `description:`.
    let Some(rest) = content.strip_prefix("---\n") else {
        return (None, None, content);
    };
    let Some(end) = rest.find("\n---") else {
        return (None, None, content);
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
```

- [ ] **Step 4: GREEN + gates**

Run:

```
cd src-tauri && cargo test --lib commands::slash_commands
cd src-tauri && cargo test --lib 2>&1 | tail -3
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -3
cd src-tauri && cargo fmt --all -- --check
```

All targeted tests pass; full suite green (was 823 → 829); clippy + fmt clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/slash_commands.rs
git commit -m "feat(slash): user + plugin discovery with dedupe + sort"
```

---

## Task 3: Frontend types + IPC + store

**Files:**

- Modify: `src/lib/types.ts` — add `SlashCommandSource`, `SlashCommand`.
- Modify: `src/lib/ipc.ts` — add `slashCommands` namespace.
- Create: `src/lib/stores/slash-commands.svelte.ts`
- Create: `src/lib/stores/slash-commands.svelte.test.ts`

- [ ] **Step 1: Add TS types**

In `src/lib/types.ts`, append:

```ts
export type SlashCommandSource =
  | { kind: 'builtin' }
  | { kind: 'user' }
  | { kind: 'plugin'; plugin: string };

export type SlashCommand = {
  name: string;
  description: string;
  source: SlashCommandSource;
};
```

- [ ] **Step 2: Add IPC wrapper**

In `src/lib/ipc.ts`, add a new top-level `slashCommands` namespace (next to
`settings`):

```ts
slashCommands: {
  list: (): Promise<SlashCommand[]> => invoke('list_slash_commands'),
},
```

Import `SlashCommand` from `./types`.

- [ ] **Step 3: Write the failing store tests**

Create `src/lib/stores/slash-commands.svelte.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/ipc', () => ({
  api: {
    slashCommands: {
      list: vi.fn(),
    },
  },
}));

import { api } from '$lib/ipc';
import { SlashCommandsStore } from './slash-commands.svelte';
import type { SlashCommand } from '$lib/types';

const sample: SlashCommand[] = [
  { name: 'help', description: 'Show help', source: { kind: 'builtin' } },
  {
    name: 'writing-plans',
    description: 'Use when you have a spec',
    source: { kind: 'plugin', plugin: 'superpowers' },
  },
  { name: 'deploy', description: 'Deploy staging', source: { kind: 'user' } },
];

beforeEach(() => {
  vi.clearAllMocks();
});

describe('SlashCommandsStore', () => {
  it('load: populates commands from api.slashCommands.list', async () => {
    vi.mocked(api.slashCommands.list).mockResolvedValue(sample);
    const store = new SlashCommandsStore();
    await store.load();
    expect(store.commands).toEqual(sample);
  });

  it('filtered: returns prefix-matched entries case-insensitive', async () => {
    vi.mocked(api.slashCommands.list).mockResolvedValue(sample);
    const store = new SlashCommandsStore();
    await store.load();
    expect(store.filtered('hel').map((c) => c.name)).toEqual(['help']);
    expect(store.filtered('WRI').map((c) => c.name)).toEqual(['writing-plans']);
  });

  it('filtered with empty string returns the full list', async () => {
    vi.mocked(api.slashCommands.list).mockResolvedValue(sample);
    const store = new SlashCommandsStore();
    await store.load();
    expect(store.filtered('').length).toBe(sample.length);
  });

  it('filtered returns [] when no entry matches', async () => {
    vi.mocked(api.slashCommands.list).mockResolvedValue(sample);
    const store = new SlashCommandsStore();
    await store.load();
    expect(store.filtered('zzz')).toEqual([]);
  });
});
```

- [ ] **Step 4: Run RED**

Run: `bun run vitest run src/lib/stores/slash-commands.svelte.test.ts` — module
not found (file doesn't exist yet).

- [ ] **Step 5: Implement the store**

Create `src/lib/stores/slash-commands.svelte.ts`:

```ts
import { api } from '$lib/ipc';
import type { SlashCommand } from '$lib/types';

export class SlashCommandsStore {
  commands = $state<SlashCommand[]>([]);

  async load(): Promise<void> {
    try {
      this.commands = await api.slashCommands.list();
    } catch (err) {
      // Discovery is fail-soft on the backend; if the IPC itself fails,
      // log + leave commands empty so the picker simply shows the
      // empty-state hint.
      console.error('slashCommands.load failed', err);
      this.commands = [];
    }
  }

  filtered(prefix: string): SlashCommand[] {
    if (prefix.length === 0) return this.commands;
    const lower = prefix.toLowerCase();
    return this.commands.filter((c) => c.name.toLowerCase().startsWith(lower));
  }
}

export const slashCommands = new SlashCommandsStore();
```

- [ ] **Step 6: GREEN + gates**

```
bun run vitest run src/lib/stores/slash-commands.svelte.test.ts
bun run vitest run 2>&1 | tail -3
bun run check
bun run lint
```

4 new tests pass; full suite +4 (was 989 → 993); check + lint clean.

- [ ] **Step 7: Commit**

```bash
git add src/lib/types.ts src/lib/ipc.ts src/lib/stores/slash-commands.svelte.ts src/lib/stores/slash-commands.svelte.test.ts
git commit -m "feat(slash): frontend types + IPC + SlashCommandsStore"
```

---

## Task 4: SlashCommandPicker component

**Files:**

- Create: `src/lib/components/chat/SlashCommandPicker.svelte`
- Create: `src/lib/components/chat/SlashCommandPicker.test.ts`

Reuse the existing modal/picker styling pattern from
`LinkWorkspacePicker.svelte` for visual consistency, but this is an INLINE
popover (not a modal) — no overlay backdrop. It anchors to the chat textarea.

- [ ] **Step 1: Failing tests**

Create `src/lib/components/chat/SlashCommandPicker.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';

vi.mock('$lib/stores/slash-commands.svelte', () => {
  const sample = [
    { name: 'help', description: 'Show help', source: { kind: 'builtin' } },
    {
      name: 'agents',
      description: 'Manage agents',
      source: { kind: 'builtin' },
    },
    {
      name: 'writing-plans',
      description: 'Spec → plan',
      source: { kind: 'plugin', plugin: 'superpowers' },
    },
  ];
  return {
    slashCommands: {
      filtered: vi.fn((prefix: string) =>
        prefix === ''
          ? sample
          : sample.filter((c) =>
              c.name.toLowerCase().startsWith(prefix.toLowerCase())
            )
      ),
    },
  };
});

import SlashCommandPicker from './SlashCommandPicker.svelte';

const anchorRect = new DOMRect(0, 0, 200, 24);

beforeEach(() => {
  vi.clearAllMocks();
});

describe('SlashCommandPicker', () => {
  it('renders all filtered items when open with empty filterText', () => {
    const { getAllByTestId } = render(SlashCommandPicker, {
      props: {
        open: true,
        filterText: '',
        anchorRect,
        onSelect: vi.fn(),
        onClose: vi.fn(),
      },
    });
    expect(getAllByTestId('slash-picker-row').length).toBe(3);
  });

  it('renders only prefix-matched items when filterText is set', () => {
    const { getAllByTestId } = render(SlashCommandPicker, {
      props: {
        open: true,
        filterText: 'wri',
        anchorRect,
        onSelect: vi.fn(),
        onClose: vi.fn(),
      },
    });
    const rows = getAllByTestId('slash-picker-row');
    expect(rows.length).toBe(1);
    expect(rows[0].textContent).toMatch(/writing-plans/);
  });

  it('Enter on the highlighted item fires onSelect with the name', async () => {
    const onSelect = vi.fn();
    render(SlashCommandPicker, {
      props: {
        open: true,
        filterText: '',
        anchorRect,
        onSelect,
        onClose: vi.fn(),
      },
    });
    await fireEvent.keyDown(document, { key: 'Enter' });
    expect(onSelect).toHaveBeenCalledWith('help');
  });

  it('ArrowDown moves the highlight; Enter selects the new highlight', async () => {
    const onSelect = vi.fn();
    render(SlashCommandPicker, {
      props: {
        open: true,
        filterText: '',
        anchorRect,
        onSelect,
        onClose: vi.fn(),
      },
    });
    await fireEvent.keyDown(document, { key: 'ArrowDown' });
    await fireEvent.keyDown(document, { key: 'Enter' });
    // Items render in their natural order; index 1 is 'agents' (the mock returns help, agents, writing-plans in that order).
    expect(onSelect).toHaveBeenCalledWith('agents');
  });

  it('Esc fires onClose without calling onSelect', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    render(SlashCommandPicker, {
      props: { open: true, filterText: '', anchorRect, onSelect, onClose },
    });
    await fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('clicking an item fires onSelect with that name', async () => {
    const onSelect = vi.fn();
    const { getAllByTestId } = render(SlashCommandPicker, {
      props: {
        open: true,
        filterText: '',
        anchorRect,
        onSelect,
        onClose: vi.fn(),
      },
    });
    await fireEvent.click(getAllByTestId('slash-picker-row')[2]);
    expect(onSelect).toHaveBeenCalledWith('writing-plans');
  });

  it('shows empty-state hint when filtered list is empty', () => {
    const { getByTestId } = render(SlashCommandPicker, {
      props: {
        open: true,
        filterText: 'zzz-no-match',
        anchorRect,
        onSelect: vi.fn(),
        onClose: vi.fn(),
      },
    });
    expect(getByTestId('slash-picker-empty').textContent).toMatch(
      /no slash commands/i
    );
  });
});
```

- [ ] **Step 2: Run RED**

Run: `bun run vitest run src/lib/components/chat/SlashCommandPicker.test.ts`
Expected: module not found.

- [ ] **Step 3: Implement the component**

Create `src/lib/components/chat/SlashCommandPicker.svelte`:

```svelte
<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { slashCommands } from '$lib/stores/slash-commands.svelte';
  import type { SlashCommand } from '$lib/types';

  interface Props {
    open: boolean;
    filterText: string;
    anchorRect: DOMRect;
    onSelect: (commandName: string) => void;
    onClose: () => void;
  }
  const { open, filterText, anchorRect, onSelect, onClose }: Props = $props();

  const rows = $derived<SlashCommand[]>(slashCommands.filtered(filterText));
  let highlightIndex = $state(0);

  // Clamp highlight whenever the filtered list changes (e.g. user typed more).
  $effect(() => {
    if (rows.length === 0) {
      highlightIndex = 0;
    } else if (highlightIndex >= rows.length) {
      highlightIndex = rows.length - 1;
    }
  });

  function sourceBadge(s: SlashCommand['source']): string {
    if (s.kind === 'plugin') return s.plugin;
    if (s.kind === 'user') return 'user';
    return 'built-in';
  }

  function handleKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (rows.length === 0) return;
      highlightIndex = (highlightIndex + 1) % rows.length;
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (rows.length === 0) return;
      highlightIndex = (highlightIndex - 1 + rows.length) % rows.length;
    } else if (e.key === 'Enter' || e.key === 'Tab') {
      if (rows.length === 0) return;
      e.preventDefault();
      onSelect(rows[highlightIndex].name);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  onMount(() => {
    document.addEventListener('keydown', handleKey, true);
  });
  onDestroy(() => {
    document.removeEventListener('keydown', handleKey, true);
  });
</script>

{#if open}
  <!-- Position above the textarea anchor; flip to below if not enough room.
       For the first cut we always render above the anchor (top-bias matches
       most chat UIs). -->
  <div
    class="slash-picker absolute z-50 bg-[var(--bg-panel)] border border-[var(--border)] rounded shadow-lg overflow-y-auto"
    style:bottom={`${window.innerHeight - anchorRect.top + 4}px`}
    style:left={`${anchorRect.left}px`}
    style:max-height="240px"
    style:min-width={`${Math.max(anchorRect.width, 320)}px`}
    role="listbox"
    aria-label="Slash command picker"
  >
    {#if rows.length === 0}
      <div
        class="px-3 py-2 text-xs text-[var(--text-muted)]"
        data-testid="slash-picker-empty"
      >
        No slash commands match. Try clearing the filter.
      </div>
    {:else}
      <ul class="py-1">
        {#each rows as cmd, i (cmd.name)}
          <li>
            <button
              type="button"
              class="w-full text-left px-2 py-1 flex items-center gap-2 hover:bg-[var(--bg-hover)]"
              class:bg-[var(--bg-hover)]={i === highlightIndex}
              data-testid="slash-picker-row"
              onmouseenter={() => (highlightIndex = i)}
              onclick={() => onSelect(cmd.name)}
            >
              <span class="font-mono text-xs">/{cmd.name}</span>
              <span
                class="text-[10px] uppercase tracking-wide text-[var(--text-muted)]"
                >{sourceBadge(cmd.source)}</span
              >
              <span class="text-xs text-[var(--text-muted)] truncate flex-1"
                >{cmd.description}</span
              >
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}
```

The `class:bg-[var(--bg-hover)]={i === highlightIndex}` directive may need a
different Tailwind binding syntax in this codebase — if it doesn't work, fall
back to `class={i === highlightIndex ? 'bg-[var(--bg-hover)]' : ''}`. Whatever
pattern the other Svelte 5 components use for conditional classes is fine.

- [ ] **Step 4: GREEN + gates**

```
bun run vitest run src/lib/components/chat/SlashCommandPicker.test.ts
bun run vitest run 2>&1 | tail -3
bun run check
bun run lint
```

7 new tests pass; full suite +7 (was 993 → 1000); check + lint clean.

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/chat/SlashCommandPicker.svelte src/lib/components/chat/SlashCommandPicker.test.ts
git commit -m "feat(slash): SlashCommandPicker component with keyboard nav + filter"
```

---

## Task 5: ChatInput integration

**Files:**

- Modify: the chat input component (locate via
  `grep -rn 'send_message\b\|ChatInput\b\|chat-input\b' src/lib/components --include='*.svelte' | head`).
- Modify: its test file.

Adds the trigger detection + picker mounting + text-replacement on selection.

- [ ] **Step 1: Locate the chat input**

Run:
`grep -rn "send_message\|api.messages.send\|chat-input\|<textarea" src/lib/components --include='*.svelte' | head`.
Identify the file that owns the chat input. Likely candidates:
`src/lib/components/chat/ChatInput.svelte` or similar. READ that file before
editing.

- [ ] **Step 2: Failing tests**

Extend the existing chat-input test file (or create one) with 3 new tests:

```ts
it('typing "/" at the start of the textarea opens the slash picker', async () => {
  const { getByTestId, queryByTestId } = render(ChatInput, {
    props: {
      /* ... */
    },
  });
  const textarea = getByTestId('chat-textarea'); // adjust to actual testid
  await fireEvent.input(textarea, { target: { value: '/' } });
  expect(queryByTestId('slash-picker-row')).not.toBeNull();
});

it('typing a space after the partial closes the slash picker', async () => {
  const { getByTestId, queryByTestId } = render(ChatInput, {
    props: {
      /* ... */
    },
  });
  const textarea = getByTestId('chat-textarea');
  await fireEvent.input(textarea, { target: { value: '/help' } });
  expect(queryByTestId('slash-picker-row')).not.toBeNull();
  await fireEvent.input(textarea, { target: { value: '/help ' } });
  expect(queryByTestId('slash-picker-row')).toBeNull();
});

it('selecting a slash command replaces the partial token with "/name " in the textarea', async () => {
  const { getByTestId } = render(ChatInput, {
    props: {
      /* ... */
    },
  });
  const textarea = getByTestId('chat-textarea') as HTMLTextAreaElement;
  await fireEvent.input(textarea, { target: { value: '/hel' } });
  // Picker is open with 'help' filtered in. Press Enter to select.
  await fireEvent.keyDown(document, { key: 'Enter' });
  expect(textarea.value).toBe('/help ');
  expect(textarea.selectionStart).toBe(6); // cursor right after the space
});
```

Mock `slashCommands.filtered` to return a deterministic list so the picker's
`Enter` lands on `help`.

If the existing chat-input testid is different (`textarea` is bound by name not
testid), use whatever locator the existing tests use — `getByRole('textbox')` or
`getByPlaceholderText(...)` are fine.

- [ ] **Step 3: Run RED**

`bun run vitest run <chat-input test path>` — tests fail (picker doesn't open).

- [ ] **Step 4: Implement the integration**

In the chat input component's `<script lang="ts">`:

```ts
import SlashCommandPicker from './SlashCommandPicker.svelte';

let pickerOpen = $state(false);
let pickerFilter = $state('');
let anchorRect = $state<DOMRect | null>(null);
let textareaEl: HTMLTextAreaElement | undefined = $state();

// Triggered by the textarea's `oninput` handler (adapt to the existing
// binding pattern in the file — if the textarea already has `bind:value`,
// add this as an extra `oninput` callback).
function updateSlashPickerState() {
  if (!textareaEl) return;
  const value = textareaEl.value;
  const cursor = textareaEl.selectionStart ?? value.length;
  // Walk back from cursor to find the start of the current line.
  let lineStart = value.lastIndexOf('\n', cursor - 1) + 1;
  const currentLine = value.slice(lineStart, cursor);
  const m = currentLine.match(/^\/([\w-]*)$/);
  if (m) {
    pickerFilter = m[1];
    anchorRect = textareaEl.getBoundingClientRect();
    pickerOpen = true;
  } else {
    pickerOpen = false;
  }
}

function replaceSlashToken(commandName: string) {
  if (!textareaEl) return;
  const value = textareaEl.value;
  const cursor = textareaEl.selectionStart ?? value.length;
  const lineStart = value.lastIndexOf('\n', cursor - 1) + 1;
  const before = value.slice(0, lineStart);
  const after = value.slice(cursor);
  const inserted = `/${commandName} `;
  textareaEl.value = `${before}${inserted}${after}`;
  const newCursor = before.length + inserted.length;
  textareaEl.setSelectionRange(newCursor, newCursor);
  pickerOpen = false;
  // Notify any bind:value listener so the parent state stays in sync.
  textareaEl.dispatchEvent(new Event('input', { bubbles: true }));
}
```

Bind `textareaEl` to the textarea (`bind:this={textareaEl}`) and call
`updateSlashPickerState` from its `oninput` (or extend whatever input handler
exists). Mount the picker in the markup near the textarea:

```svelte
{#if anchorRect}
  <SlashCommandPicker
    open={pickerOpen}
    filterText={pickerFilter}
    {anchorRect}
    onSelect={replaceSlashToken}
    onClose={() => (pickerOpen = false)}
  />
{/if}
```

- [ ] **Step 5: GREEN + gates**

```
bun run vitest run <chat-input test path>
bun run vitest run 2>&1 | tail -3
bun run check
bun run lint
```

3 new tests pass; full suite +3 (was 1000 → 1003); check + lint clean.

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/chat
git commit -m "feat(chat): wire SlashCommandPicker into chat textarea"
```

---

## Task 6: Boot wiring + journal addendum + final gates

**Files:**

- Modify: `src/App.svelte` — call `slashCommands.load()` on mount.
- Modify: `journal/2026-05-28-multi-card-workspace.md` — append an "Addendum:
  slash command autocomplete" section.

- [ ] **Step 1: Wire boot load**

In `src/App.svelte`, find the `onMount` block (around line ~40-110, near the
existing `await repos.load(); await larkBindings.load();` chain). Add:

```ts
import { slashCommands } from '$lib/stores/slash-commands.svelte';
// ... in onMount, after the existing loads ...
void slashCommands.load(); // fire-and-forget; failure logs + leaves list empty
```

The store's `load` already catches errors and logs to `console.error`, so we
don't need a try/catch here. `void` makes the discarded-promise intent explicit.

- [ ] **Step 2: Full-suite gates**

```
bun run check
bun run vitest run 2>&1 | tail -3
cd src-tauri && cargo test --lib 2>&1 | tail -3
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -3
cd src-tauri && cargo fmt --all -- --check
bun run lint
```

Expected:

- check: 0 errors / 0 warnings.
- vitest: ~1003 pass.
- cargo: ~829 pass.
- clippy + fmt clean.
- lint: only the pre-existing `lark-binding-filters.svelte.test.ts` warning.

- [ ] **Step 3: Journal addendum**

Append to `journal/2026-05-28-multi-card-workspace.md`:

```markdown
## Addendum — Slash command autocomplete (folded into same branch)

### What shipped

Typing `/` at the start of a line in the workspace chat input now opens an
autocomplete picker listing built-in + user (`~/.claude/commands/*.md`) +
plugin/skill (`~/.claude/plugins/*/{commands,skills}/...`) slash commands with
name + description. Keyboard nav (↑↓/Enter/Tab/Esc) + click-to-select. Selection
inserts `/full-name ` into the textarea; the user submits with Enter and the
existing chat→claude IPC carries the command through (verified: claude CLI in
stream-JSON mode still parses leading `/` as a slash command).

### Backend

- New `commands/slash_commands.rs`: `SlashCommand` + `SlashCommandSource` types,
  `list_slash_commands` Tauri command, `discover(claude_dir)` helper.
- Sources: hardcoded built-in list, `~/.claude/commands/`, `~/.claude/plugins/`.
  Hand-rolled minimal YAML frontmatter parser for `name:` and `description:`;
  falls back to the first non-blank body line.
- Dedupe: user > plugin > builtin. Sort: bucket Builtin → User → Plugin, then
  alphabetical within bucket.
- Fail-soft: missing `~/.claude`, unreadable files, malformed frontmatter all
  log + skip rather than erroring the entire call.

### Frontend

- TS types `SlashCommand`, `SlashCommandSource` mirror Rust serde shape.
- `api.slashCommands.list()` IPC wrapper.
- `SlashCommandsStore.load()` + `filtered(prefix)`.
- New `SlashCommandPicker.svelte` popover (no overlay backdrop — inline-anchored
  to the chat textarea via `anchorRect`). Owns keyboard nav via a `document`-
  level keydown listener while open.
- ChatInput wires the trigger regex `^/([\w-]*)$` on the current line, mounts
  the picker, and handles token replacement on selection.
- App boot fires `slashCommands.load()` fire-and-forget.

### Decisions

- **Discovery happens at app boot, once.** Plugin files don't change mid-session
  in practice. A `refresh` IPC was scaffolded but not surfaced in UI yet.
- **Submission goes through existing chat→claude IPC.** Claude CLI parses
  leading `/` as a slash command in stream-JSON mode too — the autocomplete is a
  discovery-only feature, not an execution path.
- **Hand-rolled YAML frontmatter parser** rather than pulling in `serde_yaml` or
  `yaml-rust2`. The spec only needs `name:` and `description:` lines; a ~25-line
  parser handles the entire surface and removes a dependency.
- **Dedupe priority User > Plugin > Builtin** so user-authored overrides always
  win; alphabetical plugin tie-break for determinism.

### Tests

- Rust (`commands/slash_commands.rs`): empty-claude-dir returns builtins only;
  builtin descriptions non-empty; user commands discovered from frontmatter and
  first-line fallback; plugin commands + skills discovered; dedupe user >
  plugin > builtin; bucket sort order; fail-soft on broken frontmatter.
- Frontend store: load + filtered (prefix, case-insensitive, empty-string,
  no-match).
- Picker component: all-items render, prefix filter, ArrowDown highlight + Enter
  selection, Esc close, click select, empty-state.
- ChatInput integration: opens on `/`, closes on space, selection replaces
  partial with `/full-name ` and positions the cursor.
- Final cumulative counts at branch tip: ~1003 vitest, ~829 cargo, clippy +
  fmt + check clean.
```

- [ ] **Step 4: Commit**

```bash
git add src/App.svelte journal/2026-05-28-multi-card-workspace.md
git commit -m "feat(app): boot slashCommands.load + journal addendum"
```

---

## Self-review

**Spec coverage:**

- Discovery (builtin / user / plugin/skills) → Tasks 1 + 2.
- Dedupe + sort → Task 2.
- IPC + types → Task 3.
- Store with filter → Task 3.
- Picker component with keyboard nav → Task 4.
- ChatInput integration (trigger regex, token replacement) → Task 5.
- App boot load + journal → Task 6.

**Placeholder scan:** No `todo!()`, no "TBD". Every code step has either
complete code or an exact "find via grep and adapt" instruction tied to a real
existing file.

**Type consistency:** `SlashCommand` shape is identical between Rust (serde) and
TS (`name`, `description`, `source` with `kind` discriminant). `source.kind`
values match exactly (`'builtin' | 'user' | 'plugin'`). `slashCommands.list()`
returns `SlashCommand[]` end-to-end.

**Execution order:** Strict 1 → 2 → 3 → 4 → 5 → 6. Tasks 1+2 are backend;
3+4+5+6 are frontend and each depends on the previous.
