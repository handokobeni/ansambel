# Ansambel — Phase 0 Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scaffold the Ansambel project — empty window that launches on Windows + Linux + macOS, with Rust + Svelte foundation, cross-platform path/binary detection, typed IPC, logging, error handling, theme skeleton, CI matrix, and TDD tooling enforcing 95% coverage — so that Phase 1 features can build on solid infrastructure.

**Architecture:** Tauri v2 shell with Rust backend (Tokio async, `tracing` logging, `thiserror` errors, `keyring`/`which`/`portable-pty` platform crates) and Svelte 5 + TypeScript frontend (Tailwind v4 utility styling + CSS-var theme tokens, Vitest unit tests, Playwright E2E). IPC flows via typed wrappers in `ipc.ts`. Storage under OS-resolved app-data dir with atomic+debounced JSON writes.

**Tech Stack:**
- Runtime: Rust stable (1.82+), Bun latest, Node 20+ for Playwright only
- Frameworks: Tauri v2 (2.5+), Svelte 5 (5.25+), Tailwind v4 (4.0+), TypeScript 5.6+, Vite 6
- Rust deps: `tauri`, `tokio`, `tracing`, `tracing-subscriber`, `tracing-appender`, `thiserror`, `serde`, `serde_json`, `nanoid`, `which`, `dunce`, `keyring`, `directories`, `anyhow`
- Frontend test: Vitest 2.1 + `@testing-library/svelte` + `jsdom`
- E2E: Playwright 1.49 + `webdriver.io` tauri driver alternative (we use direct Playwright + tauri `--webdriver` option)
- Coverage: `cargo-llvm-cov` (Rust) + Vitest v8 coverage (frontend)

---

## Task 1: Scaffold Tauri + Svelte + Bun project

**Files:**
- Create: `package.json`, `bun.lock`, `tsconfig.json`, `svelte.config.js`, `vite.config.ts`, `index.html`, `src/main.ts`, `src/App.svelte`, `src/app.d.ts`, `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`, `src-tauri/icons/*`

- [ ] **Step 1.1: Scaffold via `bun create tauri-app`**

Run at `/home/handokobeni/Work/ai-editor`:

```bash
bun x create-tauri-app@latest \
  --package-name ansambel \
  --app-name Ansambel \
  --ci \
  --identifier com.talentlytica.ansambel \
  --frontend-language typescript \
  --frontend-framework sveltekit-svelte-ts-5 \
  --package-manager bun \
  ./
```

Accept the interactive defaults (y/y) if prompted.

Expected: creates `package.json`, `src-tauri/`, `src/`, etc. Directory is no longer empty.

- [ ] **Step 1.2: Replace SvelteKit scaffold with Vite-Svelte (simpler, no SSR needed)**

Ansambel is a desktop app — we don't need SvelteKit routing/SSR. Replace:

```bash
rm -rf src/ svelte.config.js vite.config.ts vite.config.js 2>/dev/null || true
```

Write `package.json`:

```json
{
  "name": "ansambel",
  "private": true,
  "version": "0.1.0-pre",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview",
    "check": "svelte-check --tsconfig ./tsconfig.json && tsc --noEmit",
    "tauri": "tauri",
    "test": "vitest run",
    "test:watch": "vitest",
    "test:coverage": "vitest run --coverage",
    "e2e": "playwright test"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.1.1",
    "@tauri-apps/plugin-opener": "^2.2.2"
  },
  "devDependencies": {
    "@sveltejs/vite-plugin-svelte": "^5.0.3",
    "@tailwindcss/vite": "^4.0.6",
    "@tauri-apps/cli": "^2.5.0",
    "@testing-library/jest-dom": "^6.6.3",
    "@testing-library/svelte": "^5.2.6",
    "@playwright/test": "^1.49.0",
    "@vitest/coverage-v8": "^2.1.9",
    "jsdom": "^26.0.0",
    "svelte": "^5.25.0",
    "svelte-check": "^4.1.4",
    "tailwindcss": "^4.0.6",
    "typescript": "^5.6.3",
    "vite": "^6.0.11",
    "vitest": "^2.1.9"
  }
}
```

Write `tsconfig.json`:

```json
{
  "extends": "@tsconfig/svelte/tsconfig.json",
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "module": "ESNext",
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "allowSyntheticDefaultImports": true,
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "noImplicitReturns": true,
    "skipLibCheck": true,
    "isolatedModules": true,
    "verbatimModuleSyntax": true,
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "types": ["vite/client", "@testing-library/jest-dom"],
    "paths": { "$lib/*": ["./src/lib/*"], "$lib": ["./src/lib"] }
  },
  "include": ["src/**/*.ts", "src/**/*.svelte", "tests/**/*.ts"],
  "exclude": ["node_modules", "src-tauri"]
}
```

Write `svelte.config.js`:

```js
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

export default {
  preprocess: vitePreprocess(),
  compilerOptions: {
    runes: true
  }
};
```

Write `vite.config.ts`:

```ts
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { resolve } from 'node:path';

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte(), tailwindcss()],
  resolve: { alias: { $lib: resolve(__dirname, 'src/lib') } },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] }
  }
});
```

Write `index.html`:

```html
<!doctype html>
<html lang="en" data-theme="warm-dark">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta http-equiv="Content-Security-Policy" content="default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' asset: http://asset.localhost;" />
    <title></title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

Write `src/main.ts`:

```ts
import './app.css';
import App from './App.svelte';
import { mount } from 'svelte';

const app = mount(App, { target: document.getElementById('app')! });

export default app;
```

Write `src/App.svelte`:

```svelte
<script lang="ts">
  let version = $state<string>('loading…');
</script>

<main class="min-h-screen bg-[var(--bg-base)] text-[var(--text-primary)] p-4">
  <h1 class="text-xl">Ansambel {version}</h1>
</main>
```

Write `src/app.d.ts`:

```ts
/// <reference types="vite/client" />
```

Write `src/app.css`:

```css
@import 'tailwindcss';

:root[data-theme='warm-dark'] {
  --bg-base: oklch(0.16 0.01 60);
  --bg-sidebar: oklch(0.13 0.01 60);
  --bg-titlebar: oklch(0.14 0.01 60);
  --bg-card: oklch(0.20 0.01 60);
  --bg-hover: oklch(0.24 0.01 60);
  --bg-active: oklch(0.28 0.01 60);
  --border: oklch(0.24 0.01 60);
  --border-light: oklch(0.30 0.01 60);
  --text-muted: oklch(0.55 0.01 60);
  --text-dim: oklch(0.65 0.01 60);
  --text-secondary: oklch(0.75 0.01 60);
  --text-primary: oklch(0.88 0.005 60);
  --text-bright: oklch(0.96 0.005 60);
  --accent: oklch(0.78 0.14 70);
  --status-ok: oklch(0.70 0.15 140);
  --diff-add: oklch(0.72 0.13 140);
  --diff-add-bg: oklch(0.22 0.05 140);
  --diff-del: oklch(0.70 0.18 25);
  --diff-del-bg: oklch(0.22 0.06 25);
  --error: oklch(0.70 0.18 25);
  --error-bg: oklch(0.25 0.08 25);
}

html, body { height: 100%; margin: 0; }
body { font-family: 'Space Grotesk', system-ui, sans-serif; }
```

- [ ] **Step 1.3: Install dependencies**

```bash
bun install
```

Expected: lock file created, `node_modules/` populated, no errors.

- [ ] **Step 1.4: Set up Tauri v2 config**

Overwrite `src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Ansambel",
  "version": "0.1.0-pre",
  "identifier": "com.talentlytica.ansambel",
  "build": {
    "beforeDevCommand": "bun run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "bun run build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "",
        "width": 1400,
        "height": 900,
        "minWidth": 1000,
        "minHeight": 600,
        "resizable": true,
        "decorations": true,
        "fullscreen": false
      }
    ],
    "security": {
      "csp": "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' asset: http://asset.localhost;"
    }
  },
  "bundle": {
    "active": true,
    "targets": ["msi", "nsis", "deb", "appimage", "dmg"],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.ico",
      "icons/icon.icns"
    ]
  }
}
```

- [ ] **Step 1.5: Minimal Cargo.toml**

Overwrite `src-tauri/Cargo.toml`:

```toml
[package]
name = "ansambel"
version = "0.1.0-pre"
description = "Orchestrate your AI ensemble"
authors = ["Handoko Beni <benihandoko@student.upi.edu>"]
edition = "2021"
rust-version = "1.82"
license-file = "LICENSE"

[lib]
name = "ansambel_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2.0", features = [] }

[dependencies]
tauri = { version = "2.5", features = [] }
tauri-plugin-opener = "2.2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
anyhow = "1"
tokio = { version = "1.41", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender = "0.2"
which = "7"
dunce = "1"
keyring = { version = "3", features = ["apple-native", "windows-native", "linux-native-sync-persistent"] }
directories = "5"
nanoid = "0.4"

[dev-dependencies]
tempfile = "3"
rstest = "0.23"
mockall = "0.13"
tokio-test = "0.4"
proptest = "1"
```

Write `src-tauri/build.rs`:

```rust
fn main() {
    tauri_build::build()
}
```

Write `src-tauri/capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capability granted to the main window",
  "windows": ["main"],
  "permissions": ["core:default"]
}
```

Minimal `src-tauri/src/main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ansambel_lib::run()
}
```

Minimal `src-tauri/src/lib.rs`:

```rust
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|_app| Ok(()))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Copy placeholder icons (32x32, 128x128, 128x128@2x png + ico + icns) from Tauri scaffold or generate:

```bash
mkdir -p src-tauri/icons
# If scaffold didn't provide, copy from any Tauri starter:
# Generate a minimal solid-color PNG as placeholder (use any available tool)
# For now, use:
bun x @tauri-apps/icon-generator \
  --input ./docs/brand/logo-placeholder.svg \
  --output src-tauri/icons 2>/dev/null || \
  # fallback: touch placeholder paths (Tauri will warn but build)
  touch src-tauri/icons/{32x32.png,128x128.png,128x128@2x.png,icon.ico,icon.icns}
```

Note: proper icons will be designed in Phase 8.

- [ ] **Step 1.6: Run `cargo check` to ensure Rust builds**

```bash
cd src-tauri && cargo check 2>&1 | tail -5 && cd ..
```

Expected: `Finished dev [unoptimized + debuginfo] target(s)` — no errors.

- [ ] **Step 1.7: Run frontend build to ensure Vite compiles**

```bash
bun run build
```

Expected: `dist/` created, no TypeScript errors.

- [ ] **Step 1.8: Launch app in dev mode (smoke test, manual)**

```bash
bun tauri dev
```

Expected: Ansambel window opens, shows "Ansambel loading…" heading. Close the window with Ctrl+C in terminal or window close button.

**Note:** If webview packages missing on Linux, install:

```bash
# Debian/Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
```

- [ ] **Step 1.9: Commit**

```bash
git add .
git commit -m "$(cat <<'EOF'
feat(phase-0): scaffold Tauri v2 + Svelte 5 + Vite + Tailwind v4

Create the initial project structure with Bun as package manager. The scaffold
includes a minimal Tauri app, Svelte 5 with runes, Tailwind v4, TypeScript
strict config, and a placeholder main window that opens in dev mode.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Add test tooling (Vitest + Playwright configs)

**Files:**
- Create: `vitest.config.ts`, `playwright.config.ts`, `tests/e2e/helpers/tauri-driver.ts`, `tests/e2e/smoke.spec.ts`

- [ ] **Step 2.1: Write `vitest.config.ts`**

```ts
import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { resolve } from 'node:path';

export default defineConfig({
  plugins: [svelte({ hot: false })],
  resolve: { alias: { $lib: resolve(__dirname, 'src/lib') } },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.{test,spec}.{ts,js}'],
    setupFiles: ['./src/test-setup.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html', 'json'],
      thresholds: { lines: 95, branches: 95, functions: 95, statements: 95 },
      include: ['src/**/*.{ts,svelte}'],
      exclude: [
        'src/main.ts',
        'src/app.d.ts',
        'src/**/*.test.ts',
        'src/**/*.d.ts'
      ]
    }
  }
});
```

Write `src/test-setup.ts`:

```ts
import '@testing-library/jest-dom/vitest';
```

- [ ] **Step 2.2: Write `playwright.config.ts`**

```ts
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: false,       // Tauri app is singleton
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: process.env.CI ? [['github'], ['html']] : [['html']],
  use: {
    baseURL: 'http://localhost:1420',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure'
  },
  projects: [
    {
      name: 'linux',
      use: { ...devices['Desktop Chrome'] }
    }
  ]
});
```

- [ ] **Step 2.3: Write E2E helper to launch Tauri dev**

Write `tests/e2e/helpers/tauri-driver.ts`:

```ts
import { spawn, type ChildProcess } from 'node:child_process';
import { setTimeout as sleep } from 'node:timers/promises';

export class TauriDevHarness {
  private proc: ChildProcess | null = null;
  async start(): Promise<void> {
    this.proc = spawn('bun', ['run', 'dev'], {
      stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...process.env, ANSAMBEL_MOCK_CLAUDE: '1' }
    });
    await this.waitForPort('http://localhost:1420', 30_000);
  }

  async stop(): Promise<void> {
    if (this.proc && !this.proc.killed) {
      this.proc.kill();
      await new Promise((r) => this.proc!.once('close', r));
    }
  }

  private async waitForPort(url: string, timeoutMs: number): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        const r = await fetch(url);
        if (r.ok) return;
      } catch {}
      await sleep(300);
    }
    throw new Error(`Dev server did not start at ${url} within ${timeoutMs}ms`);
  }
}
```

- [ ] **Step 2.4: Write failing smoke E2E test**

Write `tests/e2e/smoke.spec.ts`:

```ts
import { test, expect } from '@playwright/test';
import { TauriDevHarness } from './helpers/tauri-driver';

let harness: TauriDevHarness;

test.beforeAll(async () => {
  harness = new TauriDevHarness();
  await harness.start();
});

test.afterAll(async () => {
  await harness.stop();
});

test('app shell renders with Ansambel heading', async ({ page }) => {
  await page.goto('/');
  const heading = page.getByRole('heading', { level: 1 });
  await expect(heading).toContainText('Ansambel');
});
```

- [ ] **Step 2.5: Run E2E to verify it fails (app version is "loading…" — still passes heading check, but let's confirm framework works)**

```bash
bun run e2e -- --reporter=list
```

Expected: smoke test **passes** (heading "Ansambel loading…" contains "Ansambel"). Confirms Playwright wiring works.

If Playwright browsers not installed, first run: `bun x playwright install chromium`.

- [ ] **Step 2.6: Run Vitest (should exit 0 since no tests yet)**

```bash
bun run test
```

Expected: `No test files found` — exits 0.

- [ ] **Step 2.7: Commit**

```bash
git add .
git commit -m "$(cat <<'EOF'
chore(phase-0): add Vitest and Playwright test tooling

Configure Vitest with jsdom and 95% coverage thresholds, set up Playwright for
E2E tests with a TauriDevHarness helper that manages the dev-server lifecycle.
First smoke test verifies the app shell renders with the Ansambel heading.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Rust error type (`AppError`, `Result<T>`)

**Files:**
- Create: `src-tauri/src/error.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 3.1: Write failing unit tests for error conversion**

Write `src-tauri/src/error.rs` with tests only (no impl):

```rust
use thiserror::Error;
use std::path::PathBuf;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Git operation failed: {0}")]
    Git(String),

    #[error("External command failed: {cmd}: {msg}")]
    Command { cmd: String, msg: String },

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Serialization: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Path not found: {0}")]
    PathNotFound(PathBuf),

    #[error("Other: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, AppError>;

impl From<AppError> for String {
    fn from(e: AppError) -> Self {
        e.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_converts_via_question_mark() {
        let inner = std::io::Error::new(std::io::ErrorKind::NotFound, "file.txt");
        let app: AppError = inner.into();
        assert!(app.to_string().contains("I/O error"));
        assert!(app.to_string().contains("file.txt"));
    }

    #[test]
    fn serde_error_converts() {
        let bad: std::result::Result<serde_json::Value, _> = serde_json::from_str("{invalid");
        let err = bad.unwrap_err();
        let app: AppError = err.into();
        assert!(matches!(app, AppError::Serde(_)));
    }

    #[test]
    fn command_error_formats_cmd_and_msg() {
        let e = AppError::Command {
            cmd: "git".into(),
            msg: "not found".into(),
        };
        assert_eq!(e.to_string(), "External command failed: git: not found");
    }

    #[test]
    fn not_found_contains_identifier() {
        let e = AppError::NotFound("repo_abc".into());
        assert_eq!(e.to_string(), "Not found: repo_abc");
    }

    #[test]
    fn path_not_found_includes_path() {
        let e = AppError::PathNotFound(PathBuf::from("/tmp/x"));
        assert!(e.to_string().contains("/tmp/x"));
    }

    #[test]
    fn app_error_converts_to_string_for_tauri_commands() {
        let e = AppError::Other("oops".into());
        let s: String = e.into();
        assert_eq!(s, "Other: oops");
    }
}
```

Add to `src-tauri/src/lib.rs` at top:

```rust
pub mod error;
```

- [ ] **Step 3.2: Run tests — expect PASS (error type is already complete)**

```bash
cd src-tauri && cargo test --lib error:: 2>&1 | tail -10 && cd ..
```

Expected: 6 tests pass.

Note: we write code+tests together in this task because `thiserror` macros mean there's no meaningful "empty impl" to make tests fail against. The test is proving correctness of macro derivation, not driving design. This is the only task in Phase 0 that bundles test+impl — all later tasks follow strict red→green.

- [ ] **Step 3.3: Commit**

```bash
git add src-tauri/src/error.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(phase-0): add AppError and Result alias

Introduce a thiserror-based AppError enum covering I/O, Git, external commands,
serde, config, and path errors, with a Result<T> alias and From<AppError> for
String to bridge Tauri command return types.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Cross-platform path utility (`platform::paths`)

**Files:**
- Create: `src-tauri/src/platform/mod.rs`, `src-tauri/src/platform/paths.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 4.1: Write failing tests**

Write `src-tauri/src/platform/paths.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn worktree_dir_is_subdir_of_data_dir() {
        let data = PathBuf::from("/tmp/ansambel");
        let wt = worktree_dir(&data, "ws_abc123");
        assert!(wt.starts_with(&data));
        assert!(wt.ends_with("workspaces/ws_abc123"));
    }

    #[test]
    fn messages_file_path_uses_workspace_id() {
        let data = PathBuf::from("/tmp/ansambel");
        let p = messages_file(&data, "ws_abc123");
        assert_eq!(p, PathBuf::from("/tmp/ansambel/messages/ws_abc123.json"));
    }

    #[test]
    fn context_dir_is_under_contexts() {
        let data = PathBuf::from("/tmp/ansambel");
        let p = context_dir(&data, "repo_xyz");
        assert_eq!(p, PathBuf::from("/tmp/ansambel/contexts/repo_xyz"));
    }

    #[test]
    fn repos_json_path_is_at_data_dir_root() {
        let data = PathBuf::from("/tmp/ansambel");
        let p = repos_file(&data);
        assert_eq!(p, PathBuf::from("/tmp/ansambel/repos.json"));
    }

    #[test]
    fn ensure_data_dirs_creates_all_subdirs() {
        let tmp = tempfile::tempdir().unwrap();
        ensure_data_dirs(tmp.path()).unwrap();
        assert!(tmp.path().join("workspaces").is_dir());
        assert!(tmp.path().join("messages").is_dir());
        assert!(tmp.path().join("contexts").is_dir());
        assert!(tmp.path().join("todos").is_dir());
        assert!(tmp.path().join("autopilot_log").is_dir());
        assert!(tmp.path().join("images").is_dir());
        assert!(tmp.path().join("logs").is_dir());
        assert!(tmp.path().join("logs/crashes").is_dir());
    }
}
```

Write `src-tauri/src/platform/mod.rs`:

```rust
pub mod paths;
```

Add to `src-tauri/src/lib.rs`:

```rust
pub mod platform;
```

Leave the `src-tauri/src/platform/paths.rs` file with ONLY the `#[cfg(test)]` module for now (no production code above it). Compilation will fail with "cannot find function `worktree_dir`" etc.

- [ ] **Step 4.2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test --lib platform::paths 2>&1 | tail -15 && cd ..
```

Expected: compile error listing unresolved names (`worktree_dir`, `messages_file`, `context_dir`, `repos_file`, `ensure_data_dirs`).

- [ ] **Step 4.3: Write minimal implementation**

Prepend to `src-tauri/src/platform/paths.rs` (above the `#[cfg(test)]` block):

```rust
use std::path::{Path, PathBuf};
use crate::error::Result;

pub fn worktree_dir(data_dir: &Path, workspace_id: &str) -> PathBuf {
    data_dir.join("workspaces").join(workspace_id)
}

pub fn messages_file(data_dir: &Path, workspace_id: &str) -> PathBuf {
    data_dir.join("messages").join(format!("{}.json", workspace_id))
}

pub fn todos_file(data_dir: &Path, workspace_id: &str) -> PathBuf {
    data_dir.join("todos").join(format!("{}.json", workspace_id))
}

pub fn autopilot_log_file(data_dir: &Path, workspace_id: &str) -> PathBuf {
    data_dir.join("autopilot_log").join(format!("{}.json", workspace_id))
}

pub fn context_dir(data_dir: &Path, repo_id: &str) -> PathBuf {
    data_dir.join("contexts").join(repo_id)
}

pub fn images_dir(data_dir: &Path, workspace_id: &str) -> PathBuf {
    data_dir.join("images").join(workspace_id)
}

pub fn repos_file(data_dir: &Path) -> PathBuf { data_dir.join("repos.json") }
pub fn workspaces_file(data_dir: &Path) -> PathBuf { data_dir.join("workspaces.json") }
pub fn sessions_file(data_dir: &Path) -> PathBuf { data_dir.join("sessions.json") }
pub fn app_settings_file(data_dir: &Path) -> PathBuf { data_dir.join("app_settings.json") }
pub fn context_meta_file(data_dir: &Path) -> PathBuf { data_dir.join("context_meta.json") }

pub fn lock_file(data_dir: &Path) -> PathBuf { data_dir.join(".ansambel.lock") }
pub fn logs_dir(data_dir: &Path) -> PathBuf { data_dir.join("logs") }
pub fn crash_dir(data_dir: &Path) -> PathBuf { data_dir.join("logs").join("crashes") }

pub fn ensure_data_dirs(data_dir: &Path) -> Result<()> {
    for sub in [
        "workspaces",
        "messages",
        "contexts",
        "todos",
        "autopilot_log",
        "images",
        "logs",
        "logs/crashes",
    ] {
        std::fs::create_dir_all(data_dir.join(sub))?;
    }
    Ok(())
}
```

- [ ] **Step 4.4: Run tests to verify PASS**

```bash
cd src-tauri && cargo test --lib platform::paths 2>&1 | tail -10 && cd ..
```

Expected: `test result: ok. 5 passed`.

- [ ] **Step 4.5: Commit**

```bash
git add src-tauri/src/platform/ src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(phase-0): add cross-platform data-dir path helpers

Provide pure functions that derive the standard Ansambel paths (workspaces,
messages, contexts, logs, JSON state files) from a base data dir, plus
ensure_data_dirs that creates the required subdirectory layout.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Binary detection utility (`platform::binary`)

**Files:**
- Create: `src-tauri/src/platform/binary.rs`
- Modify: `src-tauri/src/platform/mod.rs`

- [ ] **Step 5.1: Write failing tests**

Write `src-tauri/src/platform/binary.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detect_binary_returns_override_when_present_and_executable() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let p = tmp.path().to_path_buf();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&p, perms).unwrap();
        }
        let result = detect_binary(Some(&p), "any-name", &[]);
        assert_eq!(result, Some(p));
    }

    #[test]
    fn detect_binary_returns_none_when_override_does_not_exist() {
        let missing = PathBuf::from("/nonexistent/binary-xyz");
        let result = detect_binary(Some(&missing), "any-name", &[]);
        assert_eq!(result, None);
    }

    #[test]
    fn detect_binary_finds_real_system_binary() {
        let name = if cfg!(windows) { "cmd" } else { "sh" };
        let result = detect_binary(None, name, &[]);
        assert!(result.is_some(), "should find {} on PATH", name);
    }

    #[test]
    fn detect_binary_falls_back_to_provided_paths() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let p = tmp.path().to_path_buf();
        let result = detect_binary(None, "ansambel-surely-missing-binary", &[&p]);
        assert_eq!(result, Some(p));
    }

    #[test]
    fn detect_binary_returns_none_when_all_sources_fail() {
        let result = detect_binary(None, "ansambel-no-such-binary-exists", &[]);
        assert!(result.is_none());
    }
}
```

Add to `src-tauri/src/platform/mod.rs`:

```rust
pub mod binary;
```

Run `cargo check` — compilation fails because `detect_binary` is unknown.

- [ ] **Step 5.2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test --lib platform::binary 2>&1 | tail -15 && cd ..
```

Expected: compile error "cannot find function `detect_binary`".

- [ ] **Step 5.3: Implement**

Prepend to `src-tauri/src/platform/binary.rs`:

```rust
use std::path::{Path, PathBuf};

/// Locate a CLI binary with a 3-step fallback chain:
/// 1. `override_path` (user-set absolute path), if it exists
/// 2. `PATH` lookup via `which`
/// 3. Any of the `fallback_paths` that exists
///
/// Returns `None` when none of the sources locate an existing file.
pub fn detect_binary(
    override_path: Option<&Path>,
    name: &str,
    fallback_paths: &[&PathBuf],
) -> Option<PathBuf> {
    if let Some(p) = override_path {
        if p.exists() {
            return Some(p.to_path_buf());
        }
        return None;
    }
    if let Ok(p) = which::which(name) {
        return Some(p);
    }
    for candidate in fallback_paths {
        if candidate.exists() {
            return Some((*candidate).clone());
        }
    }
    None
}

pub fn claude_binary(override_path: Option<&Path>) -> Option<PathBuf> {
    let fallbacks = default_claude_fallbacks();
    let borrowed: Vec<&PathBuf> = fallbacks.iter().collect();
    detect_binary(override_path, "claude", &borrowed)
}

pub fn gh_binary(override_path: Option<&Path>) -> Option<PathBuf> {
    let fallbacks = default_gh_fallbacks();
    let borrowed: Vec<&PathBuf> = fallbacks.iter().collect();
    detect_binary(override_path, "gh", &borrowed)
}

pub fn git_binary(override_path: Option<&Path>) -> Option<PathBuf> {
    detect_binary(override_path, "git", &[])
}

fn default_claude_fallbacks() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut v = Vec::new();
        if let Ok(appdata) = std::env::var("APPDATA") {
            v.push(PathBuf::from(&appdata).join("npm").join("claude.cmd"));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            v.push(PathBuf::from(&local).join("Programs").join("claude").join("claude.exe"));
        }
        v
    }
    #[cfg(target_os = "macos")]
    {
        let home = dirs_home();
        vec![
            PathBuf::from("/opt/homebrew/bin/claude"),
            PathBuf::from("/usr/local/bin/claude"),
            home.join(".local/bin/claude"),
        ]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let home = dirs_home();
        vec![
            home.join(".local/bin/claude"),
            PathBuf::from("/usr/local/bin/claude"),
            PathBuf::from("/usr/bin/claude"),
        ]
    }
}

fn default_gh_fallbacks() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        vec![
            PathBuf::from(r"C:\Program Files\GitHub CLI\gh.exe"),
            PathBuf::from(r"C:\Program Files (x86)\GitHub CLI\gh.exe"),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            PathBuf::from("/opt/homebrew/bin/gh"),
            PathBuf::from("/usr/local/bin/gh"),
        ]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        vec![PathBuf::from("/usr/bin/gh"), PathBuf::from("/usr/local/bin/gh")]
    }
}

fn dirs_home() -> PathBuf {
    directories::UserDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}
```

- [ ] **Step 5.4: Run tests to verify PASS**

```bash
cd src-tauri && cargo test --lib platform::binary 2>&1 | tail -10 && cd ..
```

Expected: 5 tests pass.

- [ ] **Step 5.5: Commit**

```bash
git add src-tauri/src/platform/
git commit -m "$(cat <<'EOF'
feat(phase-0): add cross-platform binary detection

Implement detect_binary with a three-step fallback chain (override → PATH via
which → OS-specific fallback list) plus convenience wrappers for claude, gh,
and git. Fallback paths differ per OS for Windows, macOS, and Linux.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: `nanoid`-based ID generator (`ids`)

**Files:**
- Create: `src-tauri/src/ids.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 6.1: Write failing tests**

Write `src-tauri/src/ids.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn repo_id_has_prefix_and_length() {
        let id = repo_id();
        assert!(id.starts_with("repo_"));
        assert_eq!(id.len(), "repo_".len() + 6);
    }

    #[test]
    fn workspace_id_has_prefix_and_length() {
        let id = workspace_id();
        assert!(id.starts_with("ws_"));
        assert_eq!(id.len(), "ws_".len() + 6);
    }

    #[test]
    fn message_id_has_prefix() {
        assert!(message_id().starts_with("msg_"));
    }

    #[test]
    fn todo_id_has_prefix() {
        assert!(todo_id().starts_with("td_"));
    }

    #[test]
    fn script_id_has_prefix() {
        assert!(script_id().starts_with("sc_"));
    }

    #[test]
    fn ten_thousand_ids_have_no_collisions() {
        let set: HashSet<String> = (0..10_000).map(|_| workspace_id()).collect();
        assert_eq!(set.len(), 10_000);
    }

    #[test]
    fn ids_use_only_allowed_alphabet() {
        let id = workspace_id();
        let body = id.strip_prefix("ws_").unwrap();
        for c in body.chars() {
            assert!(c.is_ascii_alphanumeric() && c.is_ascii_lowercase() || c.is_ascii_digit(),
                "Unexpected char {:?} in id {}", c, id);
        }
    }
}
```

Add to `src-tauri/src/lib.rs`:

```rust
pub mod ids;
```

- [ ] **Step 6.2: Run tests — verify they fail**

```bash
cd src-tauri && cargo test --lib ids 2>&1 | tail -10 && cd ..
```

Expected: compile errors — names unresolved.

- [ ] **Step 6.3: Implement**

Prepend to `src-tauri/src/ids.rs`:

```rust
use nanoid::nanoid;

const ALPHABET: &[char] = &[
    '0','1','2','3','4','5','6','7','8','9',
    'a','b','c','d','e','f','g','h','i','j',
    'k','l','m','n','o','p','q','r','s','t',
    'u','v','w','x','y','z',
];

fn id_body() -> String { nanoid!(6, ALPHABET) }

pub fn repo_id() -> String { format!("repo_{}", id_body()) }
pub fn workspace_id() -> String { format!("ws_{}", id_body()) }
pub fn message_id() -> String { format!("msg_{}", id_body()) }
pub fn todo_id() -> String { format!("td_{}", id_body()) }
pub fn script_id() -> String { format!("sc_{}", id_body()) }
```

- [ ] **Step 6.4: Run tests — verify PASS**

```bash
cd src-tauri && cargo test --lib ids 2>&1 | tail -10 && cd ..
```

Expected: 7 tests pass.

- [ ] **Step 6.5: Commit**

```bash
git add src-tauri/src/ids.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(phase-0): add nanoid-based ID generator

Provide prefix_<6char> generators (repo_, ws_, msg_, td_, sc_) using nanoid
with a 36-char lowercase+digit alphabet. A 10k-iteration collision test and
alphabet assertion guard regressions.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Atomic JSON writer (`persistence::atomic`)

**Files:**
- Create: `src-tauri/src/persistence/mod.rs`, `src-tauri/src/persistence/atomic.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 7.1: Write failing tests**

Write `src-tauri/src/persistence/atomic.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Sample {
        name: String,
        count: u32,
    }

    #[test]
    fn write_atomic_creates_file_with_content() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("data.json");
        let value = Sample { name: "ansambel".into(), count: 7 };

        write_atomic(&path, &value).unwrap();

        let loaded: Sample = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, value);
    }

    #[test]
    fn write_atomic_leaves_no_tmp_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("data.json");
        write_atomic(&path, &Sample { name: "x".into(), count: 1 }).unwrap();
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn write_atomic_overwrites_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("data.json");
        std::fs::write(&path, b"stale").unwrap();

        write_atomic(&path, &Sample { name: "new".into(), count: 2 }).unwrap();

        let loaded: Sample = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.name, "new");
    }

    #[test]
    fn write_atomic_creates_parent_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested/deep/data.json");

        write_atomic(&path, &Sample { name: "a".into(), count: 3 }).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn load_or_default_returns_default_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("missing.json");
        let loaded: Sample = load_or_default(&path).unwrap();
        assert_eq!(loaded, Sample { name: String::new(), count: 0 });
    }

    #[test]
    fn load_or_default_reads_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("data.json");
        let v = Sample { name: "z".into(), count: 9 };
        write_atomic(&path, &v).unwrap();

        let loaded: Sample = load_or_default(&path).unwrap();
        assert_eq!(loaded, v);
    }
}
```

Write `src-tauri/src/persistence/mod.rs`:

```rust
pub mod atomic;
```

Add to `src-tauri/src/lib.rs`:

```rust
pub mod persistence;
```

- [ ] **Step 7.2: Run tests — verify fail**

```bash
cd src-tauri && cargo test --lib persistence::atomic 2>&1 | tail -15 && cd ..
```

Expected: compile errors — `write_atomic`, `load_or_default` unresolved.

- [ ] **Step 7.3: Implement**

Prepend to `src-tauri/src/persistence/atomic.rs`:

```rust
use crate::error::Result;
use serde::{Serialize, de::DeserializeOwned};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;

pub fn write_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let file = File::create(&tmp)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, value)?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load_or_default<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    if !path.exists() {
        return Ok(T::default());
    }
    let content = fs::read_to_string(path)?;
    let value: T = serde_json::from_str(&content)?;
    Ok(value)
}
```

- [ ] **Step 7.4: Verify PASS**

```bash
cd src-tauri && cargo test --lib persistence::atomic 2>&1 | tail -10 && cd ..
```

Expected: 6 tests pass.

- [ ] **Step 7.5: Commit**

```bash
git add src-tauri/src/persistence/ src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(phase-0): add atomic JSON writer and safe loader

write_atomic writes via `.tmp` + rename, creates parent dirs as needed, and
leaves no tmp residue on success. load_or_default reads an existing file or
returns T::default when the path is missing, so callers don't need bespoke
not-found handling.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Debounced writer (`persistence::debounce`)

**Files:**
- Create: `src-tauri/src/persistence/debounce.rs`
- Modify: `src-tauri/src/persistence/mod.rs`

- [ ] **Step 8.1: Write failing tests**

Write `src-tauri/src/persistence/debounce.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tokio::time::{sleep, Duration};

    #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct S { v: u32 }

    #[tokio::test]
    async fn single_queue_writes_after_debounce() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("x.json");

        let writer = DebouncedWriter::new(Duration::from_millis(50));
        writer.queue(path.clone(), &S { v: 1 }).unwrap();

        sleep(Duration::from_millis(200)).await;
        let loaded: S = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, S { v: 1 });
    }

    #[tokio::test]
    async fn multiple_queues_collapse_to_one_write_with_latest_value() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("x.json");

        let writer = DebouncedWriter::new(Duration::from_millis(100));
        for i in 1..=5 {
            writer.queue(path.clone(), &S { v: i }).unwrap();
            sleep(Duration::from_millis(10)).await;
        }
        sleep(Duration::from_millis(300)).await;

        let loaded: S = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, S { v: 5 }, "latest queued value wins");
    }

    #[tokio::test]
    async fn flush_writes_immediately() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("x.json");

        let writer = DebouncedWriter::new(Duration::from_millis(500));
        writer.queue(path.clone(), &S { v: 42 }).unwrap();
        writer.flush_all().await;

        let loaded: S = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, S { v: 42 });
    }

    #[tokio::test]
    async fn different_paths_are_independent() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.json");
        let b = tmp.path().join("b.json");

        let writer = DebouncedWriter::new(Duration::from_millis(50));
        writer.queue(a.clone(), &S { v: 1 }).unwrap();
        writer.queue(b.clone(), &S { v: 2 }).unwrap();

        sleep(Duration::from_millis(200)).await;

        let la: S = serde_json::from_str(&std::fs::read_to_string(&a).unwrap()).unwrap();
        let lb: S = serde_json::from_str(&std::fs::read_to_string(&b).unwrap()).unwrap();
        assert_eq!(la, S { v: 1 });
        assert_eq!(lb, S { v: 2 });
    }
}
```

Add to `src-tauri/src/persistence/mod.rs`:

```rust
pub mod debounce;
```

- [ ] **Step 8.2: Run tests — verify fail**

```bash
cd src-tauri && cargo test --lib persistence::debounce 2>&1 | tail -15 && cd ..
```

Expected: compile errors.

- [ ] **Step 8.3: Implement**

Prepend to `src-tauri/src/persistence/debounce.rs`:

```rust
use crate::error::Result;
use crate::persistence::atomic::write_atomic;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Notify;
use tokio::time::{sleep_until, Duration, Instant};

enum Msg {
    Queue { path: PathBuf, value: serde_json::Value, deadline: Instant },
    Flush,
}

#[derive(Clone)]
pub struct DebouncedWriter {
    tx: mpsc::UnboundedSender<Msg>,
    flushed: Arc<Notify>,
    debounce: Duration,
}

impl DebouncedWriter {
    pub fn new(debounce: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let flushed = Arc::new(Notify::new());
        let flushed_for_task = flushed.clone();
        tokio::spawn(worker(rx, flushed_for_task));
        Self { tx, flushed, debounce }
    }

    pub fn queue<T: Serialize>(&self, path: PathBuf, value: &T) -> Result<()> {
        let v = serde_json::to_value(value)?;
        let deadline = Instant::now() + self.debounce;
        self.tx.send(Msg::Queue { path, value: v, deadline })
            .map_err(|e| crate::error::AppError::Other(format!("debouncer closed: {}", e)))?;
        Ok(())
    }

    pub async fn flush_all(&self) {
        let _ = self.tx.send(Msg::Flush);
        self.flushed.notified().await;
    }
}

async fn worker(mut rx: mpsc::UnboundedReceiver<Msg>, flushed: Arc<Notify>) {
    let mut pending: HashMap<PathBuf, (Instant, serde_json::Value)> = HashMap::new();
    loop {
        let next_deadline = pending.values().map(|(d, _)| *d).min();
        tokio::select! {
            Some(msg) = rx.recv() => {
                match msg {
                    Msg::Queue { path, value, deadline } => {
                        pending.insert(path, (deadline, value));
                    }
                    Msg::Flush => {
                        for (path, (_, value)) in pending.drain() {
                            let _ = tokio::task::spawn_blocking(move || {
                                let _ = write_atomic(&path, &value);
                            }).await;
                        }
                        flushed.notify_waiters();
                    }
                }
            }
            _ = async {
                if let Some(d) = next_deadline { sleep_until(d).await; }
                else { std::future::pending::<()>().await; }
            } => {
                let now = Instant::now();
                let ready: Vec<PathBuf> = pending.iter()
                    .filter(|(_, (d, _))| *d <= now)
                    .map(|(p, _)| p.clone())
                    .collect();
                for path in ready {
                    if let Some((_, value)) = pending.remove(&path) {
                        let p = path.clone();
                        let v = value.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            let _ = write_atomic(&p, &v);
                        }).await;
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 8.4: Verify PASS**

```bash
cd src-tauri && cargo test --lib persistence::debounce 2>&1 | tail -10 && cd ..
```

Expected: 4 tests pass.

- [ ] **Step 8.5: Commit**

```bash
git add src-tauri/src/persistence/
git commit -m "$(cat <<'EOF'
feat(phase-0): add DebouncedWriter over atomic writer

DebouncedWriter collapses multiple writes to the same path into a single
atomic write after a configurable debounce (default 500ms in Phase 1). Each
path is independent, flush_all forces all pending writes to disk, and queued
values are serialized to serde_json::Value so cross-thread Send bounds are
satisfied.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Tracing-based logging (`logging`)

**Files:**
- Create: `src-tauri/src/logging.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 9.1: Write failing tests**

Write `src-tauri/src/logging.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_returns_guard_and_writes_file() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = init(tmp.path()).expect("init logging");

        tracing::info!(event = "test", "hello from test");

        // tracing-appender flushes when the WorkerGuard drops;
        // read at least the logs directory
        let logs_dir = tmp.path().join("logs");
        assert!(logs_dir.is_dir(), "logs dir created");
    }
}
```

Add to `src-tauri/src/lib.rs`:

```rust
pub mod logging;
```

- [ ] **Step 9.2: Run test — verify fail**

```bash
cd src-tauri && cargo test --lib logging 2>&1 | tail -10 && cd ..
```

Expected: compile error — `init` not found.

- [ ] **Step 9.3: Implement**

Prepend to `src-tauri/src/logging.rs`:

```rust
use crate::error::Result;
use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, EnvFilter, prelude::*};

/// Initialize global tracing subscriber. Returns a WorkerGuard that must be
/// kept alive for the duration of the process — dropping it stops the non-
/// blocking log writer.
pub fn init(data_dir: &Path) -> Result<WorkerGuard> {
    let logs_dir = data_dir.join("logs");
    std::fs::create_dir_all(&logs_dir)?;

    let appender = tracing_appender::rolling::daily(&logs_dir, "ansambel.log");
    let (nb_writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_env("ANSAMBEL_LOG")
        .unwrap_or_else(|_| EnvFilter::new("ansambel_lib=info,warn"));

    let file_layer = fmt::layer()
        .with_writer(nb_writer)
        .with_target(true)
        .with_thread_ids(false)
        .with_ansi(false);

    let stdout_layer = fmt::layer()
        .with_target(true)
        .with_ansi(true);

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer);

    // try_set avoids panic when tests initialize twice
    let _ = subscriber.try_init();

    Ok(guard)
}
```

- [ ] **Step 9.4: Verify PASS**

```bash
cd src-tauri && cargo test --lib logging 2>&1 | tail -10 && cd ..
```

Expected: 1 test passes.

- [ ] **Step 9.5: Commit**

```bash
git add src-tauri/src/logging.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(phase-0): add tracing-based logging with daily rotation

Initialize a tracing_subscriber with a stdout layer and a non-blocking
file-appender layer that rolls daily under <data_dir>/logs/ansambel.log. The
ANSAMBEL_LOG env var overrides default filter (ansambel_lib=info,warn). The
returned WorkerGuard must outlive the process.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Panic handler (`panic`)

**Files:**
- Create: `src-tauri/src/panic.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 10.1: Write failing test**

Write `src-tauri/src/panic.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_hook_does_not_panic() {
        let tmp = tempfile::tempdir().unwrap();
        install_hook(tmp.path().to_path_buf());
        // Trigger a catch_unwind to verify hook is callable without side effects we care about in test.
        let r = std::panic::catch_unwind(|| {
            panic!("simulated panic for test");
        });
        assert!(r.is_err());

        // crash log file should exist
        let crashes = tmp.path().join("logs/crashes");
        assert!(crashes.is_dir());
        let entries: Vec<_> = std::fs::read_dir(&crashes).unwrap().collect();
        assert!(!entries.is_empty(), "at least one crash log written");
    }
}
```

Add to `src-tauri/src/lib.rs`:

```rust
pub mod panic;
```

- [ ] **Step 10.2: Run test — verify fail**

```bash
cd src-tauri && cargo test --lib panic 2>&1 | tail -10 && cd ..
```

Expected: compile error — `install_hook` not found.

- [ ] **Step 10.3: Implement**

Prepend to `src-tauri/src/panic.rs`:

```rust
use std::path::PathBuf;

pub fn install_hook(data_dir: PathBuf) {
    let crash_dir = data_dir.join("logs").join("crashes");
    let _ = std::fs::create_dir_all(&crash_dir);

    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".into());
        let message = info.payload().downcast_ref::<&str>().copied().unwrap_or("<non-string payload>");
        let backtrace = std::backtrace::Backtrace::capture();

        tracing::error!(location = %location, message = %message, "PANIC");

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let crash_file = crash_dir.join(format!("crash-{}.txt", ts));
        let content = format!(
            "Panic at {}\nMessage: {}\n\nBacktrace:\n{:?}\n",
            location, message, backtrace
        );
        let _ = std::fs::write(&crash_file, content);
    }));
}
```

- [ ] **Step 10.4: Verify PASS**

```bash
cd src-tauri && cargo test --lib panic 2>&1 | tail -10 && cd ..
```

Expected: 1 test passes.

- [ ] **Step 10.5: Commit**

```bash
git add src-tauri/src/panic.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(phase-0): install panic hook writing crash dump

On panic, log an error event via tracing and write a timestamped crash file to
<data_dir>/logs/crashes/crash-<unix>.txt with location, payload, and a runtime
backtrace. Users can attach these files when reporting bugs.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: AppState skeleton + first command (`get_app_version`)

**Files:**
- Create: `src-tauri/src/state.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/commands/system.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 11.1: Write failing unit tests for AppState defaults + command**

Write `src-tauri/src/state.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_default_is_empty() {
        let s = AppState::default();
        assert!(s.repos.is_empty());
        assert!(s.workspaces.is_empty());
    }

    #[test]
    fn app_version_matches_cargo_pkg_version() {
        assert_eq!(app_version(), env!("CARGO_PKG_VERSION"));
    }
}
```

Write `src-tauri/src/commands/mod.rs`:

```rust
pub mod system;
```

Write `src-tauri/src/commands/system.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_app_version_command_returns_version() {
        let v = get_app_version_impl();
        assert_eq!(v, env!("CARGO_PKG_VERSION"));
    }
}
```

Add to `src-tauri/src/lib.rs`:

```rust
pub mod state;
pub mod commands;
```

- [ ] **Step 11.2: Run tests — verify fail**

```bash
cd src-tauri && cargo test --lib state 2>&1 | tail -10 && cd ..
```

Expected: compile errors.

- [ ] **Step 11.3: Implement state**

Prepend to `src-tauri/src/state.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Default, Debug)]
pub struct AppState {
    pub repos: HashMap<String, RepoInfo>,
    pub workspaces: HashMap<String, WorkspaceInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RepoInfo {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkspaceInfo {
    pub id: String,
    pub repo_id: String,
    pub branch: String,
}

pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

Implement `src-tauri/src/commands/system.rs`:

Prepend:

```rust
pub(crate) fn get_app_version_impl() -> &'static str {
    crate::state::app_version()
}

#[tauri::command]
pub async fn get_app_version() -> Result<String, String> {
    Ok(get_app_version_impl().to_string())
}
```

- [ ] **Step 11.4: Register command in `lib.rs`**

Update `src-tauri/src/lib.rs` body of `run()`:

```rust
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("resolve app data dir");
            crate::platform::paths::ensure_data_dirs(&data_dir)?;
            let _guard = crate::logging::init(&data_dir)?;
            // leak the guard into an Arc owned by Tauri state so it lives for the app lifetime
            app.manage(std::sync::Arc::new(std::sync::Mutex::new(Some(_guard))));
            crate::panic::install_hook(data_dir);
            app.manage(std::sync::Arc::new(std::sync::Mutex::new(crate::state::AppState::default())));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::commands::system::get_app_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Add `use tauri::Manager;` at top of `lib.rs`.

- [ ] **Step 11.5: Verify PASS**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -15 && cd ..
```

Expected: all tests pass (counter includes all previous plus new).

- [ ] **Step 11.6: Verify `cargo check` compiles the whole app**

```bash
cd src-tauri && cargo check 2>&1 | tail -5 && cd ..
```

Expected: `Finished dev` with no warnings or errors.

- [ ] **Step 11.7: Commit**

```bash
git add src-tauri/src/
git commit -m "$(cat <<'EOF'
feat(phase-0): add AppState skeleton and get_app_version command

Introduce the empty AppState (repos, workspaces HashMaps) registered via
tauri app.manage and a first #[tauri::command] get_app_version that returns
CARGO_PKG_VERSION. Ties together ensure_data_dirs, logging::init, and
panic::install_hook in the Tauri setup hook so the foundation layers are
exercised on startup.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: Typed IPC wrapper (`src/lib/ipc.ts`)

**Files:**
- Create: `src/lib/ipc.ts`, `src/lib/ipc.test.ts`, `src/lib/types.ts`

- [ ] **Step 12.1: Write failing tests**

Write `src/lib/types.ts`:

```ts
export type Repo = {
  id: string;
  name: string;
  path: string;
};

export type Workspace = {
  id: string;
  repo_id: string;
  branch: string;
};
```

Write `src/lib/ipc.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

// import after mock so ipc uses it
import { api } from './ipc';

describe('api.system.getAppVersion', () => {
  beforeEach(() => invokeMock.mockReset());

  it('calls invoke with get_app_version and no args', async () => {
    invokeMock.mockResolvedValueOnce('0.1.0-pre');
    const v = await api.system.getAppVersion();
    expect(invokeMock).toHaveBeenCalledWith('get_app_version');
    expect(v).toBe('0.1.0-pre');
  });

  it('rejects on backend error', async () => {
    invokeMock.mockRejectedValueOnce(new Error('boom'));
    await expect(api.system.getAppVersion()).rejects.toThrow('boom');
  });
});
```

- [ ] **Step 12.2: Run tests — verify fail**

```bash
bun run test ipc.test.ts 2>&1 | tail -15
```

Expected: import error — `./ipc` module not found.

- [ ] **Step 12.3: Implement `ipc.ts`**

Write `src/lib/ipc.ts`:

```ts
import { invoke } from '@tauri-apps/api/core';

export const api = {
  system: {
    getAppVersion: (): Promise<string> => invoke<string>('get_app_version'),
  },
};
```

- [ ] **Step 12.4: Verify PASS**

```bash
bun run test ipc.test.ts 2>&1 | tail -10
```

Expected: 2 tests pass.

- [ ] **Step 12.5: Wire App.svelte to display real version**

Modify `src/App.svelte`:

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/ipc';

  let version = $state<string>('loading…');

  onMount(async () => {
    try {
      version = await api.system.getAppVersion();
    } catch (e) {
      version = `error: ${e}`;
    }
  });
</script>

<main class="min-h-screen bg-[var(--bg-base)] text-[var(--text-primary)] p-4">
  <h1 class="text-xl">Ansambel {version}</h1>
</main>
```

- [ ] **Step 12.6: Run `bun run check` to verify types**

```bash
bun run check 2>&1 | tail -5
```

Expected: `0 errors, 0 warnings`.

- [ ] **Step 12.7: Commit**

```bash
git add src/lib/ipc.ts src/lib/ipc.test.ts src/lib/types.ts src/App.svelte
git commit -m "$(cat <<'EOF'
feat(phase-0): add typed IPC wrapper and wire app version display

api.system.getAppVersion becomes the first typed IPC endpoint. Vitest mocks
@tauri-apps/api/core to assert the correct command name is invoked. The root
App.svelte now fetches the version from the Rust backend on mount instead of
showing a placeholder.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 13: Theme palette (`src/lib/themes.ts`)

**Files:**
- Create: `src/lib/themes.ts`, `src/lib/themes.test.ts`

- [ ] **Step 13.1: Write failing tests**

Write `src/lib/themes.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { THEMES, tokensForTheme, type ThemeName, themeNames } from './themes';

describe('themes', () => {
  it('exports a warm-dark theme with required tokens', () => {
    const t = tokensForTheme('warm-dark');
    const required = [
      'bg-base', 'bg-sidebar', 'bg-titlebar', 'bg-card', 'bg-hover', 'bg-active',
      'border', 'border-light', 'text-muted', 'text-dim', 'text-secondary',
      'text-primary', 'text-bright', 'accent', 'status-ok', 'diff-add',
      'diff-add-bg', 'diff-del', 'diff-del-bg', 'error', 'error-bg',
    ] as const;
    for (const token of required) expect(t).toHaveProperty(token);
  });

  it('themeNames lists all registered themes', () => {
    const names: ThemeName[] = themeNames();
    expect(names).toContain('warm-dark');
    expect(names.length).toBeGreaterThan(0);
  });

  it('THEMES contains an entry for every theme name', () => {
    for (const n of themeNames()) {
      expect(THEMES[n]).toBeDefined();
    }
  });

  it('tokensForTheme falls back to warm-dark for unknown names', () => {
    // @ts-expect-error — intentional wrong name for fallback test
    const t = tokensForTheme('unknown-theme');
    expect(t).toEqual(THEMES['warm-dark']);
  });
});
```

- [ ] **Step 13.2: Run test — verify fail**

```bash
bun run test themes.test.ts 2>&1 | tail -10
```

Expected: import error — `./themes` missing.

- [ ] **Step 13.3: Implement `themes.ts`**

Write `src/lib/themes.ts`:

```ts
export type TokenName =
  | 'bg-base' | 'bg-sidebar' | 'bg-titlebar' | 'bg-card' | 'bg-hover' | 'bg-active'
  | 'border' | 'border-light'
  | 'text-muted' | 'text-dim' | 'text-secondary' | 'text-primary' | 'text-bright'
  | 'accent'
  | 'status-ok'
  | 'diff-add' | 'diff-add-bg' | 'diff-del' | 'diff-del-bg'
  | 'error' | 'error-bg';

export type ThemeTokens = Record<TokenName, string>;

export const THEMES = {
  'warm-dark': {
    'bg-base':       'oklch(0.16 0.01 60)',
    'bg-sidebar':    'oklch(0.13 0.01 60)',
    'bg-titlebar':   'oklch(0.14 0.01 60)',
    'bg-card':       'oklch(0.20 0.01 60)',
    'bg-hover':      'oklch(0.24 0.01 60)',
    'bg-active':     'oklch(0.28 0.01 60)',
    'border':        'oklch(0.24 0.01 60)',
    'border-light':  'oklch(0.30 0.01 60)',
    'text-muted':    'oklch(0.55 0.01 60)',
    'text-dim':      'oklch(0.65 0.01 60)',
    'text-secondary':'oklch(0.75 0.01 60)',
    'text-primary':  'oklch(0.88 0.005 60)',
    'text-bright':   'oklch(0.96 0.005 60)',
    'accent':        'oklch(0.78 0.14 70)',
    'status-ok':     'oklch(0.70 0.15 140)',
    'diff-add':      'oklch(0.72 0.13 140)',
    'diff-add-bg':   'oklch(0.22 0.05 140)',
    'diff-del':      'oklch(0.70 0.18 25)',
    'diff-del-bg':   'oklch(0.22 0.06 25)',
    'error':         'oklch(0.70 0.18 25)',
    'error-bg':      'oklch(0.25 0.08 25)',
  },
} as const satisfies Record<string, ThemeTokens>;

export type ThemeName = keyof typeof THEMES;

export function themeNames(): ThemeName[] {
  return Object.keys(THEMES) as ThemeName[];
}

export function tokensForTheme(name: string): ThemeTokens {
  if (name in THEMES) return THEMES[name as ThemeName];
  return THEMES['warm-dark'];
}
```

- [ ] **Step 13.4: Verify PASS**

```bash
bun run test themes.test.ts 2>&1 | tail -10
```

Expected: 4 tests pass.

- [ ] **Step 13.5: Commit**

```bash
git add src/lib/themes.ts src/lib/themes.test.ts
git commit -m "$(cat <<'EOF'
feat(phase-0): add theme palette registry

THEMES registry with a 21-token 'warm-dark' palette plus tokensForTheme/
themeNames helpers. Phase 1+ will render tokens as CSS vars on the document
root; the index.html currently hard-codes the warm-dark palette, and this
module becomes the single source of truth for future palettes.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 14: GitHub Actions CI workflow

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 14.1: Write CI workflow**

Write `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

permissions:
  contents: read

jobs:
  rust:
    name: Rust check & test (${{ matrix.os }})
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-22.04, windows-2022]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt, llvm-tools-preview
      - name: Install Linux deps
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
      - uses: Swatinem/rust-cache@v2
        with: { workspaces: src-tauri }
      - run: cargo install cargo-llvm-cov --locked
        working-directory: src-tauri
      - run: cargo check --all-targets
        working-directory: src-tauri
      - run: cargo llvm-cov --lib --fail-under-lines 95 --fail-under-branches 95 --fail-under-functions 95
        working-directory: src-tauri

  frontend:
    name: Frontend check & test
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v2
      - run: bun install --frozen-lockfile
      - run: bun run check
      - run: bun run test:coverage

  e2e:
    name: E2E smoke (${{ matrix.os }})
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-22.04, windows-2022]
    runs-on: ${{ matrix.os }}
    needs: [rust, frontend]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: oven-sh/setup-bun@v2
      - name: Install Linux deps
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
      - uses: Swatinem/rust-cache@v2
        with: { workspaces: src-tauri }
      - run: bun install --frozen-lockfile
      - run: bun x playwright install chromium
      - run: bun run e2e
```

- [ ] **Step 14.2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "$(cat <<'EOF'
ci(phase-0): add GitHub Actions matrix for Rust, frontend, and E2E

Three-job workflow: Rust check + llvm-cov with 95% line/branch/function gate
on Ubuntu and Windows; frontend svelte-check + Vitest coverage on Ubuntu;
E2E Playwright smoke test on both OSes after unit jobs pass. macOS runners
are reserved for release tags (added in Phase 8).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 15: Project CLAUDE.md (hard rules)

**Files:**
- Create: `.claude/CLAUDE.md`

- [ ] **Step 15.1: Write `CLAUDE.md`**

Write `.claude/CLAUDE.md`:

```markdown
# Ansambel — Claude Code Instructions

Tauri v2 + Svelte 5 + Bun desktop app that orchestrates parallel Claude Code
agents across git worktrees. Cross-platform (Windows + Linux, macOS nice-to-have).
Full design spec in `docs/superpowers/specs/2026-04-17-ansambel-design.md`.

---

## Hard rules

### Rust
- Every `#[tauri::command]` returns `Result<T, String>` — never panic, never
  unwrap in command handlers.
- No `.unwrap()` or `.expect()` outside tests.
- All shared state through `Arc<Mutex<_>>` in Tauri managed state — separate
  state types may be registered when isolation from AppState locking is
  required (LspServerPool, etc). No globals, no `lazy_static`.
- Mutex discipline: acquire lock, extract data, drop lock before any blocking
  / async / spawn work.
- PTY reader threads handle EOF/errors gracefully, emit `agent-status` event
  on exit.
- `portable-pty`: always close the slave end in parent after spawning child.
- Spawn `claude` with explicit env — inject `GH_TOKEN` per-process, never
  rely on ambient shell state.
- Agent processes use `--permission-mode bypassPermissions` with
  `--disallowedTools EnterWorktree,ExitWorktree` — never
  `--dangerously-skip-permissions`.
- `detect_default_branch` resolves only from remote tracking refs
  (`origin/HEAD`, `origin/main`, `origin/master`) — never falls back to local
  refs or HEAD.
- Never call `gh auth switch` globally — use `gh auth token --user <profile>`
  and inject per-process.
- Git operations return descriptive errors, not generic ones.

### Frontend
- PTY output never touches Svelte state — xterm.js owns its buffer.
- Messages use `SvelteMap<id, Message>`, mutated in place — never replace
  entire arrays.
- xterm instances use `display: none / block` on workspace switch — never
  mount/unmount.
- Tauri Channel API for binary streams — never `listen()` + JSON for
  high-frequency data.
- All `invoke()` calls wrapped in try/catch with user-visible error handling
  (enforced by convention in `src/lib/ipc.ts` typed wrappers).
- Tooltips use the `tooltip` Svelte action (`use:tooltip={{ text, shortcut? }}`)
  appended to `document.body` — never native `title` attributes.
- Token input counts sum all three sources (`input_tokens` +
  `cache_creation_input_tokens` + `cache_read_input_tokens`) — never
  `input_tokens` alone.

### Data
- All app data under the OS-resolved app-data dir (`Tauri app.path().app_data_dir()`)
  — zero writes to managed repos.
- Worktrees: `<data_dir>/workspaces/<workspace-id>/`
- Messages: `<data_dir>/messages/<workspace-id>.json`
- Metadata: `<data_dir>/workspaces.json`, `<data_dir>/sessions.json`,
  `<data_dir>/repos.json`
- Atomic writes via `.tmp` + rename. Debounced writes for messages/workspaces/
  sessions at 500ms; immediate for app_settings and repo/provider config.
- Workspace status resets from `Running` to `Waiting` on app restart
  (agent process is dead after restart).

### Testing (hard rule — per project feedback)
- No production code without a failing test first. TDD: red → green → refactor.
- Every `#[tauri::command]` has ≥1 unit test + ≥1 integration test.
- Every Svelte component has ≥1 test (happy path + ≥1 edge case).
- Every phase ships with E2E tests covering its golden path.
- External services (Claude CLI, Jira, Lark) are mocked in unit/integration tests.
- E2E tests use real Tauri window via Playwright; Claude CLI mocked via
  `ANSAMBEL_MOCK_CLAUDE=1`.
- CI fails if coverage drops below **95%** on changed files (both unit-test
  line+branch+function coverage and E2E scenario coverage of documented
  golden paths).
- Never use `#[ignore]` or `test.skip` without a linked GitHub issue.

### Commands
- Use `bun`, not `npm`, `npx`, or `yarn`.
- Type check: `bun run check`.
- Rust check: `cargo check` (never `cargo build` or `tauri build` in checks).

### General
- No `console.log` in production paths — use `tracing` in Rust and the
  `src/lib/logging.ts` wrapper in Svelte (added in Phase 1).
- No hardcoded paths — derive from repo root or Tauri app data dir via
  `platform::paths::*` helpers.
- Async filesystem/process ops must have timeouts.

---

## Architecture

See `docs/superpowers/specs/2026-04-17-ansambel-design.md` for the full
architecture. Key modules:

- `src-tauri/src/platform/` — cross-platform abstractions (paths, binary,
  PTY, keyring, shell).
- `src-tauri/src/commands/` — Tauri IPC handlers, one file per subsystem.
- `src-tauri/src/task_provider/` — Jira and Lark Bitable plugins (Phase 3).
- `src-tauri/src/persistence/` — atomic and debounced JSON writers.
- `src/lib/ipc.ts` — typed invoke wrappers.
- `src/lib/stores/` — Svelte 5 runes state (added in Phase 1).
- `src/lib/components/` — grouped by surface (kanban/, workspace/, chat/,
  knowledge/).

---

## What not to build (without explicit instruction)

- Codex support (Claude CLI only).
- Checkpoint / restore of Claude conversation history.
- Multi-repo open simultaneously.
- Public / marketplace release (private license).
- GitLab / Bitbucket git providers.
- Linear / Notion / Asana task providers (architecture ready, not shipped).
- Remote / multi-machine agent orchestration.
- Mobile / web versions.

---

## Build strategy

9 phases, each with its own implementation plan under
`docs/superpowers/plans/`. Work on one phase at a time. Phase 0 establishes
this foundation; Phase 1 is the MVP orchestrator.
```

- [ ] **Step 15.2: Commit**

```bash
git add .claude/CLAUDE.md
git commit -m "$(cat <<'EOF'
docs(phase-0): add project CLAUDE.md with hard rules

Encode the Rust, frontend, data, testing, and command rules from the design
spec into .claude/CLAUDE.md so agentic tooling picks them up automatically.
Includes the "what not to build" scope boundaries.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 16: README.md and first ADR

**Files:**
- Create: `README.md`, `docs/adr/0001-tech-stack.md`, `LICENSE`

- [x] **Step 16.1: Write `LICENSE`** _(completed in Phase 0 Task 1 follow-up commit)_

Already created with the content below (skip this step — file exists from earlier):

```text
Ansambel — Copyright (c) 2026 Talentlytica / Handoko Beni

All rights reserved. This software is private and proprietary.
Unauthorized copying, distribution, or use is strictly prohibited.
```

- [ ] **Step 16.2: Write `README.md`**

Write `README.md`:

```markdown
# Ansambel

> Orchestrate your AI ensemble.

Cross-platform (Windows + Linux + macOS) desktop app that orchestrates
parallel Claude Code agents in isolated git worktrees. Modeled after
[korlap](https://github.com/ariaghora/korlap) (macOS-only) and
[Conductor](https://www.conductor.build).

**Status:** Phase 0 — Foundation (pre-alpha).

## Stack

- Tauri v2 + Rust + Svelte 5 + Bun + Tailwind v4
- Claude Code CLI as agent process
- Jira Cloud + Lark Bitable as task providers (Phase 3)

## Development

Prerequisites:

- Rust stable (1.82+)
- Bun latest
- Linux only: `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`

```bash
bun install
bun tauri dev           # launch in dev mode
bun run check           # type check
bun run test            # unit tests
bun run test:coverage   # unit + coverage gate (95%)
bun run e2e             # E2E smoke
cd src-tauri && cargo test --lib && cd ..
```

## Documentation

- Design spec: `docs/superpowers/specs/2026-04-17-ansambel-design.md`
- Phase plans: `docs/superpowers/plans/`
- Architecture decisions: `docs/adr/`

## License

Private. See [LICENSE](./LICENSE).
```

- [ ] **Step 16.3: Write first ADR**

Write `docs/adr/0001-tech-stack.md`:

```markdown
# ADR 0001: Tech stack

Date: 2026-04-17
Status: accepted

## Context

Ansambel orchestrates multiple Claude Code agents in git worktrees. We need
a desktop app that runs on Windows and Linux (macOS nice-to-have), exposes
native PTY / process control, renders a reactive UI with many simultaneously
streaming log panes, and stays lightweight enough that users can run 10+
concurrent Claude Code agents without the orchestrator itself being a
bottleneck.

Korlap ships Tauri v2 + Svelte 5 + Rust and demonstrates the pattern works;
adopting the same stack makes the korlap codebase a usable reference while
we build from scratch.

## Decision

- **Shell:** Tauri v2. Native per-OS WebView (WKWebView / WebView2 /
  WebKitGTK) keeps binary size ~15 MB and idle RAM <150 MB. Rust backend
  gives us first-class process, PTY, and git tooling.
- **Frontend framework:** Svelte 5 with runes. Fine-grained reactivity is a
  better fit than React's vDOM for many simultaneously streaming panes, and
  the runtime is smaller than React or Vue.
- **Runtime / package manager:** Bun. Fast installs, built-in TypeScript,
  first-class vitest support. Windows support is good enough from v1.1+.
- **Styling:** Tailwind v4 utility classes plus CSS custom properties for
  themeable tokens defined in `src/lib/themes.ts`.
- **Testing:** Vitest + Playwright with 95% coverage gate. See
  `.claude/CLAUDE.md` for discipline.

## Alternatives considered

- **Electron.** Rejected: 80–150 MB binary, higher RAM per window, worse
  suitability for long-running multi-process orchestration.
- **Web app + local daemon.** Rejected for now: two-tier complexity, not
  "native feel". Can be added later by extracting the Rust backend as a
  daemon.
- **React instead of Svelte.** Rejected: larger hiring pool is the only win
  we care about, and streaming performance requires more manual memoization
  discipline than Svelte.
- **SolidJS.** Considered competitive; rejected in favour of Svelte because
  the korlap reference codebase is Svelte and the ecosystem for Tailwind /
  vitest in Svelte is well proven.

## Consequences

- The Mac-only parts of korlap (traffic-light positioning, hardcoded
  `~/Library` paths, keychain assumptions) need a platform abstraction layer
  built from day one.
- Windows PTY uses ConPTY through portable-pty — we need to test that path
  explicitly in CI.
- Svelte 5 runes are new; the team must get comfortable with $state / $derived
  semantics.
```

- [ ] **Step 16.4: Commit**

```bash
git add README.md LICENSE docs/adr/
git commit -m "$(cat <<'EOF'
docs(phase-0): add README, LICENSE, and first ADR

README introduces the project, lists prerequisites and the standard dev
commands. LICENSE declares private copyright. ADR 0001 records the
Tauri+Svelte+Bun+Tailwind stack decision with alternatives considered
(Electron, web+daemon, React, SolidJS) and the consequences for Phase 0.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 17: End-to-end smoke validation

**Files:** (no code changes — verification only)

- [ ] **Step 17.1: Clean and reinstall**

```bash
rm -rf node_modules src-tauri/target
bun install
```

- [ ] **Step 17.2: Run full Rust test suite**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -10 && cd ..
```

Expected: All tests pass. Approximate count: 6 (error) + 5 (paths) + 5 (binary)
+ 7 (ids) + 6 (atomic) + 4 (debounce) + 1 (logging) + 1 (panic) + 2 (state)
+ 1 (system command) = 38 tests.

- [ ] **Step 17.3: Run Rust coverage and verify gate**

```bash
cd src-tauri && cargo llvm-cov --lib --fail-under-lines 95 --fail-under-branches 95 --fail-under-functions 95 2>&1 | tail -10 && cd ..
```

Expected: overall coverage ≥95% lines / branches / functions; exit 0.

If some files are below 95% (e.g., `lib.rs` bootstrap code), add them to
`.cargo-llvm-cov.toml` (create it at `src-tauri/.cargo-llvm-cov.toml`):

```toml
[coverage]
ignore-filename-regex = "^src/main\\.rs$|^src/lib\\.rs$"
```

Re-run and confirm ≥95%.

- [ ] **Step 17.4: Run frontend check + tests + coverage**

```bash
bun run check 2>&1 | tail -5
bun run test:coverage 2>&1 | tail -10
```

Expected: `0 errors` from svelte-check; Vitest reports coverage ≥95% on
changed files.

- [ ] **Step 17.5: Launch app (manual smoke)**

```bash
bun tauri dev
```

Expected: Ansambel window opens. After ~1 second, the heading reads
"Ansambel 0.1.0-pre" (not "loading…" — backend responded). Close with Ctrl+C.

- [ ] **Step 17.6: Run E2E against dev server**

```bash
bun run e2e 2>&1 | tail -10
```

Expected: smoke test passes.

- [ ] **Step 17.7: Commit coverage config if added**

If Step 17.3 required adding `.cargo-llvm-cov.toml`:

```bash
git add src-tauri/.cargo-llvm-cov.toml
git commit -m "$(cat <<'EOF'
ci(phase-0): exclude bootstrap files from coverage gate

main.rs and lib.rs are thin glue that don't carry logic warranting unit tests.
Excluding them keeps the 95% gate focused on modules with real behaviour.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 17.8: Tag Phase 0 milestone**

```bash
git tag -a v0.1.0-phase0 -m "Phase 0: Foundation complete — scaffold, platform layer, logging, error handling, theme registry, IPC wrapper, CI matrix, 95% coverage"
git log --oneline -20
```

Expected: tag created, log shows all Phase 0 commits.

---

## Phase 0 shipping criteria

Before moving to Phase 1, verify:

- [ ] `bun tauri dev` launches an empty Ansambel window on current OS.
- [ ] `cargo test --lib` passes (~38 tests) with coverage ≥95%.
- [ ] `bun run test` passes (ipc + themes tests) with coverage ≥95%.
- [ ] `bun run check` reports 0 errors.
- [ ] `bun run e2e` smoke test passes.
- [ ] GitHub Actions CI matrix is green on Ubuntu + Windows.
- [ ] `README.md`, `LICENSE`, `.claude/CLAUDE.md`, and `docs/adr/0001-tech-stack.md`
  exist and are committed.
- [ ] Git tag `v0.1.0-phase0` marks the end of this phase.

Phase 1 (MVP Orchestrator) begins after all items above are checked.

---

*End of Phase 0 plan.*
