# Slash Command Autocomplete — Design

**Date:** 2026-05-28 **Status:** Approved (brainstorming) **Author:** Handoko
Beni (with Claude)

## Goal

Show an autocomplete picker in Ansambel's workspace **chat input** when the user
types `/` at the start of a line, listing available slash commands (built-in,
user-defined, and plugin/skill) with name + description. Selecting an entry
inserts `/full-name ` into the textarea; the user submits with Enter and the
existing chat→agent IPC carries the command through.

## Background — current state

- Chat input is a plain Svelte `<textarea>` that sends user messages through
  `send_message` IPC to the workspace's `claude` CLI process (run in stream-JSON
  / non-interactive mode).
- Claude CLI's slash-command dropdown is a TUI-level affordance that only
  renders when the CLI is interactive. With Ansambel's stream-JSON pipe, the
  dropdown never appears.
- Slash commands themselves still work — when a user message starts with `/`,
  the CLI parses it as a slash command regardless of mode. The gap is purely
  **discoverability** in the UI.

A precondition task (Task 1 of the plan) verifies the assumption that typing
`/help` (or similar) manually into Ansambel's chat input today actually invokes
the slash command via the existing IPC. If it does, this spec ships as pure
UI/discovery work. If it does not, scope expands to a special submission path —
flagged here so the plan can branch.

## Data model

```rust
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct SlashCommand {
    /// The command identifier WITHOUT the leading slash. E.g. "writing-plans".
    pub name: String,
    /// Short description shown next to the name in the picker. Empty when no
    /// frontmatter description was found and no fallback first-line is usable.
    pub description: String,
    /// Where the command was discovered.
    pub source: SlashCommandSource,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SlashCommandSource {
    Builtin,
    User,
    Plugin { plugin: String },
}
```

TypeScript mirror in `src/lib/types.ts`:

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

## Discovery (backend)

New Tauri command `list_slash_commands() -> Result<Vec<SlashCommand>, String>`.
Returns a deduped, sorted vector.

### Sources

1. **Builtin** — hardcoded curated list. The set below was chosen from
   commonly-used claude CLI commands; expanding it is an opt-in code change.

   ```
   help, clear, agents, compact, config, context, copy, diff, doctor, effort,
   fast, init, loop, model, release-notes, resume, review, run, schedule,
   verify
   ```

   Each carries a one-line description summarising the command's purpose.

2. **User** — scan `~/.claude/commands/*.md`. For each file:
   - `name` = filename without `.md` extension.
   - `description` = YAML frontmatter `description:` field if present, else the
     first non-blank non-frontmatter line of the body (trimmed, truncated to
     ~120 chars), else empty.
   - `source` = `User`.

3. **Plugin** — for each entry `<plugin>` in `~/.claude/plugins/`:
   - Scan `<plugin>/commands/*.md` — same field rules as User, but
     `source = Plugin { plugin: <plugin> }`.
   - Scan `<plugin>/skills/*/SKILL.md` (and
     `<plugin>/<version>/skills/*/SKILL.md` to match the on-disk layout actually
     used by the plugin cache). For each:
     - `name` = the YAML frontmatter `name:` field if present, else the parent
       directory name.
     - `description` = YAML frontmatter `description:` field; first non-blank
       line of body as fallback.

The discovery code MUST be fail-soft: a missing `~/.claude` directory, an
unreadable plugin subdirectory, or an unparseable file logs at `tracing::warn!`
and continues. Returning a partial list is better than returning an error.

### Dedupe + sort

After collecting candidates:

1. **Dedupe by name**, priority `User > Plugin > Builtin`. A user-defined
   `writing-plans` shadows a plugin-provided `writing-plans` shadows a built-in
   of the same name. Plugin-vs-plugin collisions keep the alphabetically-first
   plugin's entry; remaining duplicates are dropped.
2. **Sort**: source bucket order `Builtin → User → Plugin`; within each bucket,
   alphabetical by name (case-insensitive). Plugin entries are alphabetical by
   `(plugin_name, command_name)`.

### Caching

The frontend caches the result for the session. A separate
`refresh_slash_commands()` command re-runs discovery on demand (exposed as
`api.slashCommands.refresh()` for a future settings button — not surfaced in
this UI yet).

## UX

### Trigger

The picker opens when **the current line** in the chat textarea matches the
regex `^/([\w-]*)$` and the cursor is at the end of that token (no characters
after it on the line). "Current line" = the substring from the previous newline
to the cursor.

- Typing `/` at the start of a line → picker opens with full list, filter empty.
- Typing more letters after `/` → filter applies (prefix match against `name`,
  case-insensitive).
- Typing a space → picker closes (the user has started arguments).
- Moving the cursor away from the token → picker closes.
- Esc → picker closes; textarea content unchanged.

The picker does NOT open mid-word, after `/foo bar /baz` (only at line start),
or in selection-mode.

### Render per item

```
/writing-plans     [superpowers]  Use when you have a spec or requirements…
^ name (mono)      ^ source badge ^ description (muted, truncated)
```

- Built-in source: badge `built-in` (or no badge — least visual noise; final
  call left to the implementer's eye, but the picker tests pin the data-testid,
  not the badge text).
- User source: badge `user`.
- Plugin source: badge `<plugin-name>` (the plugin id).

The picker uses a fixed max-height with scroll. Highlighted item has the
standard `--bg-hover` treatment.

### Keyboard

- `ArrowUp` / `ArrowDown`: move highlight (wraps at top/bottom).
- `Enter` or `Tab`: select the highlighted item:
  1. Replace the `/partial` token in the textarea with `/full-name ` (with a
     single trailing space).
  2. Move the cursor to immediately after the inserted space.
  3. Close the picker.
- `Esc`: close the picker; textarea content unchanged.
- Clicking an item: same effect as Enter on that item.

### Submit

Pressing Enter while the picker is **closed** submits the textarea contents as a
user message via the existing chat IPC. The agent (claude CLI) parses the
leading `/` and dispatches to the named slash command — no new submission code
path required.

### Edge cases

- Empty list (no commands discovered): picker opens with a single muted "No
  slash commands found. Try `/help`." line; keyboard navigation no-ops.
- Filter that matches nothing: picker stays open with the same "no matches"
  hint; submitting still sends the typed text as-is.
- The user types `/` then arrow-down without picking: navigation works
  immediately; selection still inserts.

## Frontend architecture

### `src/lib/stores/slash-commands.svelte.ts`

```ts
class SlashCommandsStore {
  commands = $state<SlashCommand[]>([]);
  async load(): Promise<void> {
    /* one-shot IPC call, populates commands */
  }
  filtered(prefix: string): SlashCommand[] {
    /* prefix match, case-insensitive */
  }
}
export const slashCommands = new SlashCommandsStore();
```

Load once when the app boots (alongside the existing repo/workspace loads in
`App.svelte:onMount`).

### `src/lib/components/chat/SlashCommandPicker.svelte`

Popover anchored to the chat textarea. Props:

```ts
interface Props {
  /** Whether the picker should render. */
  open: boolean;
  /** Token after the leading `/`, used to filter. */
  filterText: string;
  /** Bounding rect of the textarea (or the slash position), so the picker can
   *  anchor above/below. The component computes its own position from this. */
  anchorRect: DOMRect;
  /** Called when the user selects an item (Enter / Tab / click). Receives the
   *  full command name (without leading slash). The caller is responsible for
   *  the textarea text replacement. */
  onSelect: (commandName: string) => void;
  /** Called when the picker should close without selection (Esc / click outside). */
  onClose: () => void;
}
```

The picker subscribes to `slashCommands.filtered(filterText)` reactively. It
owns keyboard navigation (ArrowUp/Down/Enter/Tab/Esc) via a keydown listener
attached to `document` while `open` is true; non-navigation keys propagate back
to the textarea normally (i.e. the picker doesn't swallow regular typing).

### `ChatInput.svelte` (existing component — search via `grep`)

Adds:

- `let pickerOpen = $state(false);`
- `let pickerFilter = $state('');`
- `let anchorRect = $state<DOMRect | null>(null);`
- A `oninput` handler that reads the current line, runs the trigger regex, and
  sets the three state vars.
- A `replaceToken(fullName)` function for the picker's `onSelect`: locate the
  `/partial` in the textarea value, replace it with `/fullName `, set the
  cursor, and dispatch an `input` event so any binding stays in sync.

## Tests

- **Backend (`commands/slash_commands.rs`):**
  - Empty `~/.claude` returns just the built-in list (length = the hardcoded
    set's size).
  - User commands in `~/.claude/commands/<file>.md` are discovered; description
    is taken from frontmatter when present, else first non-blank body line.
  - Plugin commands + skills under
    `~/.claude/plugins/<plugin>/{commands,skills}/` are discovered with
    `source = Plugin { plugin }`.
  - Dedupe: a user `foo` shadows a plugin `foo` shadows a built-in `foo` — only
    one entry, with `source = User`.
  - Sort: result is `Builtin` first (alphabetical), then `User`, then `Plugin`
    (alphabetical by `(plugin, name)`).
  - Malformed frontmatter or unreadable files do not crash the call — they
    surface as `tracing::warn!` and are skipped.

- **Frontend store (`stores/slash-commands.svelte.test.ts`):**
  - `load` calls the IPC and populates `commands`.
  - `filtered('write')` returns prefix-matched entries, case-insensitive.
  - `filtered('')` returns the full list.
  - Reload is a no-op for stale subscribers (state replacement is in place).

- **Picker component (`SlashCommandPicker.test.ts`):**
  - Renders all filtered items when open.
  - ArrowDown/Up moves highlight (and wraps).
  - Enter on highlighted item fires `onSelect` with that name and the picker
    visually closes (parent unmounts on `open=false`).
  - Esc fires `onClose` without calling `onSelect`.
  - Click on an item fires `onSelect`.
  - Empty-state hint when `filtered` returns 0 items.

- **ChatInput integration (extend the existing test file):**
  - Typing `/` at the start of the textarea opens the picker.
  - Typing space closes the picker.
  - Selecting an item replaces the `/partial` with `/full-name ` and leaves the
    cursor right after the space.

- **E2E (optional follow-up, not in this spec's golden path):** a single
  Playwright scenario typing `/help` into the chat, pressing Enter, and
  asserting the agent receives the `/help` message — kept as low priority
  because the backend command list lookups don't fit cleanly into the existing
  E2E shim. The spec ships green on the unit/component layer.

## Out of scope (YAGNI)

- Argument autocomplete (e.g. `/code-review <level>` doesn't tab-complete the
  level argument).
- Auto-execute on selection (selection always inserts text; the user submits
  with Enter).
- Filesystem watcher to refresh commands when plugins are added/removed mid-
  session (manual refresh via `api.slashCommands.refresh()` is good enough).
- Sticky / recently-used commands.
- Per-workspace command visibility (every workspace sees the same global list).
- Help-text expansion on hover (the description in the row is enough).

## Open precondition (verified in Task 1 of plan)

Before any UI work, confirm that submitting a message starting with `/` from
Ansambel's chat input today actually invokes the slash command on the
workspace's claude CLI. The simplest test is a temporary backend-side log of
incoming stdin payloads + a manual smoke test. If it works, this spec ships
unchanged. If it doesn't, add a backend translation step (interpret a user
message that begins with `/` and dispatch via a different IPC) — that work is
not enumerated here and would be planned separately.
