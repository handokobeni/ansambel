# Phase 2c — @-file mentions in chat input

**Branch:** `feat/phase-2c-mentions` (target: `main`) **Effort:** ~0.5 wk (3–4
days) **Ship gate:** v0.2.0 (final sub-phase of Phase 2)

## Goal

Saat user mengetik `@` di chat input, muncul autocomplete dropdown berisi file
dari worktree. User memilih (klik atau keyboard) → text `@<partial>` di textarea
di-replace jadi `@<full-path>`. Pattern familiar dari Claude Code CLI, Cursor,
dll.

E2E golden path (dari plan Phase 2 overview):

```
Type `@src/li` in chat input
  → autocomplete shows `src/lib/...` files
  → pick one
  → message body contains `@src/lib/whatever.ts`
```

## Scope

**In.**

- `@`-trigger detection di textarea chat input
- Fuzzy-ranked file list dari worktree saat ini
- Dropdown UI dengan keyboard nav (↑/↓/Enter/Esc/Tab)
- Replace `@<partial>` → `@<full-path>` saat user pilih
- Cache file list per-workspace (di frontend) supaya autocomplete responsive

**Out.**

- @-mention untuk symbol / function → Phase 6 (LSP)
- @-mention untuk past message → never (sengaja, scope-bloat)
- Drag-drop file ke chat → sudah ada (Phase 1)
- Inline file preview di hover → Phase 8 polish

## Sub-tasks

1. **Backend.** `workspace_files_recursive(workspace_id) → Vec<String>` —
   gitignore-aware, returns ALL paths in worktree (relative, `/`-normalized).
   Existing `workspace_files` cuma immediate children (max_depth=1) — gak cocok
   untuk mention search. Pakai `ignore::WalkBuilder` tanpa max_depth, same
   filter chain.
2. **Frontend logic (pure helpers).**
   - `parseMention(text, caretPos)` →
     `{ trigger: '@', query: 'src/li', start: N }` atau `null`. Pure function,
     unit-testable.
   - `rankFiles(files, query)` → top-20 sorted. Substring match dengan bonus
     untuk: match di basename (>>), match di segment start (>), consecutive char
     run (>). No external library — code-it-yourself, cheap enough.
3. **Component.** `MentionAutocomplete.svelte`:
   - Props: `files: string[]`, `query: string`, `onSelect(path: string)`,
     `onDismiss()`
   - Positioned ABOVE textarea (textarea bisa multi-line, dropdown jangan
     menutupi yang lagi diketik)
   - Keyboard: ↑/↓ navigate, Enter/Tab select highlighted, Esc dismiss
   - Click anywhere outside → dismiss
4. **Wire-in.** `MessageInput.svelte`:
   - Tambah `oninput` + `onselectionchange` handler → panggil `parseMention`
   - Kalau detect mention: fetch (atau pakai cache) file list, render
     `MentionAutocomplete`
   - On select: replace `@<partial>` di-position dengan `@<full-path> `
     (trailing space supaya user lanjut ngetik)
   - Tab key NEVER untuk indent ditextarea kalau autocomplete open
5. **Caching.**
   - File list cached per-workspace di module-level store (Svelte 5 rune)
   - Invalidate kalau workspace switch
   - Background refresh saat pertama buka workspace; cache
     stale-while-revalidate
6. **Tests.**
   - Unit: `parseMention`, `rankFiles` (pure functions, banyak edge cases)
   - Component: `MentionAutocomplete` render + kbd + select
   - Component: `MessageInput` integration — type → see autocomplete → select →
     text replaced
   - Backend: `workspace_files_recursive_inner` happy path + gitignore + missing
     worktree
   - E2E: golden path

## Architecture decisions (binding)

- **Single backend command, not per-keystroke search.** Mention search happens
  client-side after the file list is fetched. Network/IPC cost paid once per
  workspace; subsequent keystrokes are pure-frontend.
- **No fuzzy-match library.** `package.json` would gain `fuse.js` or similar
  (~10KB gz). Phase 2c is small enough to hand-roll a ranking function (~40
  LOC). Fewer deps, less version churn.
- **Cache strategy = "per-workspace, eager on focus".** When the user focuses
  the chat input, kick off `workspace_files_recursive` if not cached. By the
  time they type `@`, the list is ready. If they type `@` before fetch
  completes, show "loading…" then re-render.
- **Position dropdown ABOVE textarea.** Textarea auto-grows up to 12 lines;
  dropdown below would push send button off-screen. Above is also where most
  editors (VS Code IntelliSense, Slack /-commands) put it.
- **Trigger = `@` at start-of-text OR after whitespace.** `email@host.com` must
  NOT trigger autocomplete. Regex: `(?:^|\s)@([^\s]*)$` to detect in the
  substring up to caret.
- **Select inserts the path + trailing space.** User probably wants to keep
  typing. The path itself is plain text (no special markup) — reads cleanly in
  the message body and Claude CLI sees a normal string.
- **Backend command stays generic.** Don't bake "mention" into the Rust API.
  `workspace_files_recursive` is useful for other features later (Phase 6 LSP,
  Phase 8 command palette).

## Risks

1. **Worktree with 10k+ files.** Recursive list might be slow on first call.
   Mitigation: cache aggressively, run on focus (not on keystroke), show loading
   indicator. Spec says <500ms feels instant. If repos exceed that, add a
   path-prefix filter param to backend (defer to 2c+ if encountered).
2. **Caret position tracking in auto-resize textarea.** `selectionStart` should
   be reliable; native textarea behavior. Test with both keyboard typing and
   paste.
3. **Tab key conflict.** Tab in textarea is "insert tab character" by default.
   When autocomplete open, Tab should select; otherwise default behavior. Need
   careful `preventDefault` gate.
4. **IME composition.** User typing in CJK might have IME composition active
   when `@` triggers. Listen to `compositionend` not just `input`. (Likely Phase
   2c can ship without — Indonesian QWERTY users non-blocking, but document the
   limitation.)

## Testing strategy

Per hard rule: TDD, ≥95% line + branch coverage, ≥1 unit + ≥1 integration per
Tauri command, ≥1 test per Svelte component.

**Unit tests (pure functions) — write first.**

- `parseMention`:
  - Empty text + caret 0 → null
  - `"hello"` + caret 5 → null
  - `"@"` + caret 1 → `{ query: "", start: 0 }`
  - `"@src"` + caret 4 → `{ query: "src", start: 0 }`
  - `"hi @sr"` + caret 6 → `{ query: "sr", start: 3 }`
  - `"email@host"` + caret 10 → null (no whitespace before `@`)
  - `"@one @two"` + caret 9 → `{ query: "two", start: 5 }`
  - `"@src\n"` + caret 5 → null (after whitespace ends the mention)
- `rankFiles`:
  - Empty query → all files in alpha order, capped at 20
  - Exact basename match → top result
  - Substring at segment-start outranks mid-segment match
  - Case-insensitive
  - Stable ordering for ties

**Component tests.**

- `MentionAutocomplete`: render N items, highlight first by default, ↓ moves
  highlight, Enter calls onSelect, Esc calls onDismiss, click outside dismisses
- `MessageInput`: type `@src` → autocomplete renders, select first → textarea
  value contains the selected path, autocomplete closes

**Backend tests.**

- `workspace_files_recursive_inner`:
  - Returns nested paths
  - Respects `.gitignore` (drop `target/`, `node_modules/`)
  - Returns forward-slash paths even on Windows
  - Errors on invalid workspace_id, missing worktree

**E2E (Playwright + ANSAMBEL_MOCK_CLAUDE=1).**

- `tests/e2e/phase-2c/mentions.spec.ts`:
  - Open a workspace with known fixture files
  - Focus chat input
  - Type `@src/li`
  - Assert dropdown visible with at least one entry containing `lib`
  - Press Enter
  - Assert textarea value matches `/^@src\/lib\/[a-z.]+ $/`

## Acceptance criteria

- [ ] E2E golden path passes on Linux + Windows + macOS
- [ ] ≥95% line + branch coverage on new code (per repo hard rule)
- [ ] Backend command + IPC typed wrapper + ≥1 unit + ≥1 integration test
- [ ] All Svelte components have ≥1 happy-path + ≥1 edge-case test
- [ ] No regression in existing chat send flow (Ctrl+Enter, attachments)
- [ ] CI 9/9 hijau (rust × 3 OS, e2e × 3 OS, lint, frontend, commitlint)
- [ ] Plan doc + journal entry for the PR

## Implementation cadence

1. Day 1: Plan doc ✓, backend command (TDD), IPC wrapper, ranking helpers
2. Day 2: `MentionAutocomplete` component + tests, wire into `MessageInput`
3. Day 3: E2E test, polish (loading state, IME note in code comments), CI green
4. Day 4: Buffer for review feedback / unexpected issues

## Out of scope (explicit defer)

- @-mention untuk symbol → **Phase 6** (LSP integration)
- @-mention untuk past message → never (scope drift)
- File preview on hover → **Phase 8** (polish)
- Search-style modal trigger (Cmd+K) → **Phase 8** (command palette)
- IME (CJK composition) full support → document limitation in code, fix if user
  reports
