# Platform Quirks

Cross-platform gotchas hit while building Ansambel. Each entry: symptom, root
cause, fix pattern, references. Add to this file whenever you fix an OS-specific
bug — the next person (or the next session) should not have to rediscover the
same thing.

Tested on: Ubuntu 22.04 (Linux), Windows Server 2022, macOS 14 (Sonoma — Apple
Silicon).

---

## Windows ConPTY does not EOF on clean child exit

**Symptom.** A reader thread reading from a PTY master blocks indefinitely after
the child process exits cleanly (e.g. `exit\r`, process returns 0, `Ctrl+D`
analogue). On Linux the kernel propagates EOF to the master when the slave
closes; on Windows ConPTY this does not happen reliably.

**Root cause.** ConPTY is a separate `conhost.exe`-hosted pseudo-console — not a
kernel construct. When the child exits, the pipe end held by conhost can stay
open. `child.kill()` on an already dead child is a no-op and does NOT close the
master.

**Fix pattern.** Drop the master itself. That triggers `ClosePseudoConsole(hPC)`
and propagates EOF to the reader. Implemented via
`PtySession.master: Option<Box<dyn MasterPty>>` + `close_master()` that does
`self.master.take()`. The terminal/scripts exit watchdogs call `close_master()`
after `kill()`.

**See.** `src-tauri/src/platform/pty.rs` (close_master),
`src-tauri/src/commands/terminal.rs::spawn_exit_watchdog`.

---

## Windows cmd.exe needs `\r`, not `\n`, as line terminator

**Symptom.** Sending `b"exit\n"` over the PTY shows `exit` echoed on screen but
the shell never executes it.

**Root cause.** ConPTY translates terminal input as if from a real console. Real
Enter on Windows = `\r` (CR), not `\n` (LF). cmd.exe ignores bare `\n` as a
command terminator. xterm.js already sends `\r` for the Enter key — production
users never hit this, but unit tests that hardcode `\n` will hang on Windows.

**Fix pattern.** Use `\r\n` for line terminators in tests that send shell
commands. Works on Unix shells (bash/zsh/sh accept either) and cmd.exe (sees the
`\r`, processes the line, ignores the trailing `\n`).

**See.** `src-tauri/src/commands/terminal.rs::tests`.

---

## portable-pty CommandBuilder starts with an empty environment

**Symptom.** Spawned shell prints no prompt; `bash` exits immediately; binaries
fail to resolve.

**Root cause.** `portable_pty::CommandBuilder::new(...)` does NOT inherit the
parent process's environment. PATH, HOME, USER, TERM are all unset by default.

**Fix pattern.** Explicitly inherit the env, then set `TERM`:

```rust
for (k, v) in std::env::vars() {
    cmd.env(k, v);
}
cmd.env("TERM", "xterm-256color");
```

Without `TERM`, xterm.js can't negotiate ANSI; without `HOME`/`USER`, bash
refuses to print PS1; without `PATH`, the shell can't find any binary the user
types.

**See.** `src-tauri/src/commands/terminal.rs::build_shell_command`,
`src-tauri/src/commands/scripts.rs::script_run_inner`.

---

## Interactive Unix shells need `-i` to print a prompt

**Symptom.** Shell is alive (writes pass through) but no PS1 prompt appears in
the terminal.

**Root cause.** Bash/zsh/sh check whether stdin is a TTY AND whether the shell
was invoked with `-i` (interactive). Just attaching a PTY isn't always enough —
depending on how the parent set up file descriptors, the shell can decide it's
non-interactive and skip sourcing dotfiles / printing the prompt.

**Fix pattern.** Pass `-i` when the binary basename ends with `bash`, `zsh`, or
`sh`:

```rust
if shell.ends_with("bash") || shell.ends_with("zsh") || shell.ends_with("sh") {
    c.arg("-i");
}
```

Skip on Windows — `cmd.exe` doesn't accept `-i`.

**See.** `src-tauri/src/commands/terminal.rs::build_shell_command`.

---

## Tauri Channel<T> is single-use across invokes

**Symptom.** After a `reattach` IPC call, a second `invoke()` (e.g.
`terminal_spawn` after `terminal_reattach` failed) produces a console warning:
`[TAURI] Couldn't find callback id NNNNN`, and bytes never arrive on the
frontend even though the backend is sending them.

**Root cause.** A `Channel<T>` instance internally registers one callback id.
Once Rust drops the Channel (e.g. an `Err` return from the invoke handler), the
JS side de-registers the callback. Reusing the same Channel object in a
subsequent invoke means the backend sends to an id that no longer exists on the
JS side.

**Fix pattern.** Create a fresh `Channel<T>` for every invoke. Use a factory
function that re-binds the same handler:

```ts
const makeChannel = (): Channel<TerminalChunk> => {
  const ch = new Channel<TerminalChunk>();
  ch.onmessage = handle;
  return ch;
};

try {
  await api.terminal.reattach(workspaceId, makeChannel());
} catch {
  await api.terminal.spawn(workspaceId, makeChannel(), cols, rows);
}
```

**See.** `src/lib/components/workspace/Terminal.svelte`.

---

## xterm.js render is blank when mounted on a hidden container

**Symptom.** xterm canvas mounts, no errors, but the terminal panel is empty (or
rendered as a one-row strip). Bytes from the backend arrive but never appear.

**Root cause.** `term.open(container)` measures `container.offsetWidth` /
`offsetHeight` to size the renderer. If the container is in a `display: none`
ancestor, both are zero — xterm picks 0×0 dimensions and never recovers when the
container later becomes visible.

**Fix pattern.** Wait for the container to have layout before calling
`term.open()`. A ResizeObserver-driven helper:

```ts
async function waitForLayout(el: HTMLElement, timeoutMs = 500): Promise<void> {
  if (el.offsetWidth > 0 && el.offsetHeight > 0) return;
  return new Promise((resolve) => {
    const ro = new ResizeObserver(() => {
      if (el.offsetWidth > 0 && el.offsetHeight > 0) {
        ro.disconnect();
        resolve();
      }
    });
    ro.observe(el);
    setTimeout(() => {
      ro.disconnect();
      resolve();
    }, timeoutMs);
  });
}
```

Also import the xterm CSS once at the top of `main.ts`:

```ts
import '@xterm/xterm/css/xterm.css';
```

**See.** `src/lib/components/workspace/Terminal.svelte::waitForLayout`,
`src/main.ts`.

---

## Tauri CSP must allow `ipc:` and `http://ipc.localhost`

**Symptom.** Frontend can invoke commands once, but Channel messages never
deliver — silent fail or "Couldn't find callback id" in the console.

**Root cause.** Tauri v2's IPC uses the custom `ipc:` protocol (and
`http://ipc.localhost` on some backends). If `connect-src` in the window CSP
omits these, the browser blocks the IPC connection.

**Fix pattern.** Set:

```
connect-src 'self' ipc: http://ipc.localhost ws: wss:;
```

`ws:` / `wss:` is for any dev-server websocket (e.g. Vite HMR).

**See.** `src-tauri/tauri.conf.json`.

---

## Test inputs need cross-platform line terminators

**Rule of thumb.** Anything written to a PTY in a test must use `\r\n` or just
`\r`. Anything written to a file system path must use forward slashes for JSON /
portable-pty / cross-platform tools, native separator (`PathBuf::push`) for
actual filesystem operations.

**Why.** Bash/zsh/sh accept both `\r` and `\n`; cmd.exe only accepts `\r`. Tests
written on Unix with `\n` pass locally but hang on Windows. Tests written with
hardcoded `\\` paths fail on Unix.

---

## Per-OS dev surface (what we DON'T test in unit tests anymore)

Real PTY spawns are flaky in coverage-instrumented CI. We test the state-machine
logic (write forwarding, watchdog, reattach, kill idempotency) against `MockPty`
— pure in-memory, deterministic across all OS. The real shell only spawns in two
designated integration tests per file
(`spawn_terminal_inner_starts_shell_and_streams_output`,
`script_run_inner_resolves_workspace_and_script_then_spawns`) as smoke checks
that `build_shell_command` + `portable-pty` actually work on the host. E2E
(Playwright) handles user-flow validation.

**See.** `src-tauri/src/platform/pty.rs::MockPty`,
`src-tauri/src/commands/terminal.rs::tests::spawn_with_mock`.

---

## Adding to this file

Every time you fix a bug that was OS-specific (passes on one OS, fails on
another), add an entry here. Keep it short:

1. **Symptom** — what the user / tester sees
2. **Root cause** — why it happens, in one paragraph
3. **Fix pattern** — the canonical solution, with a code snippet if non-obvious
4. **See** — file paths to the implementation

Future-you will thank you.
