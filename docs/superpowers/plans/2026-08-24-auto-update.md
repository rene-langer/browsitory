# Auto-Update Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add VSCode-style auto-update to Browsitory: on startup and every 6 hours, check GitHub Releases for a newer signed build, auto-download it, and let the user restart to apply it.

**Architecture:** `tauri-plugin-updater` polls a `latest.json` manifest that `tauri-apps/tauri-action` (already used in `.github/workflows/release.yml`) generates and signs automatically once a minisign keypair and `plugins.updater` config exist. A new `frontend/src/ipc/updater.ts` wraps the plugin/process JS bindings (mirroring how `tauriRepoClient.ts` is the sole importer of `@tauri-apps/api`); a new `UpdateBanner` component uses that wrapper to check, auto-download, and offer restart.

**Tech Stack:** Rust: `tauri-plugin-updater`, `tauri-plugin-process` (Cargo, `crates/tauri-app`). Frontend: `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process` (pnpm, `frontend/`). CI: `tauri-apps/tauri-action` (existing).

**Spec:** `docs/superpowers/specs/2026-08-24-auto-update-design.md`

## Global Constraints

- Release workflow keeps `releaseDraft: true` — the updater only sees a release once a human manually publishes it on GitHub. Do not change this.
- Update check timing: on app startup, then every 6 hours while the app stays open.
- Download is automatic once an update is found (no "click to download" step) — only the restart is a user action.
- A failed check or download is swallowed silently (console log only) — never surfaced to the user as an error.
- `frontend/src/ipc/updater.ts` is the only frontend file that imports `@tauri-apps/plugin-updater` or `@tauri-apps/plugin-process` — `UpdateBanner.tsx` (under `src/components/`) may only import from `../ipc/updater`, per the existing `no-restricted-imports` ESLint rule barring `@tauri-apps/*` in `src/components/**`/`src/state/**`.
- This feature is deliberately **not** part of the `RepoClient` interface (see spec's "Why not through RepoClient").

---

### Task 1: Generate and register the updater signing key

**Files:**
- Modify: `crates/tauri-app/tauri.conf.json`
- Create (locally, not committed): minisign keypair via `cargo tauri signer generate`

**Interfaces:**
- Produces: a public key string embedded in `tauri.conf.json`'s `plugins.updater.pubkey`, and a private key + password that Task 5 wires into GitHub Actions secrets. No code depends on this task; it's a prerequisite artifact for Tasks 2 and 5.

- [ ] **Step 1: Install the Tauri CLI's signer subcommand if not already available**

Run: `cargo install tauri-cli --locked` (skip if `cargo tauri --version` already works — this repo's `cargo tauri dev`/`build` commands in `CLAUDE.md` imply it's already installed).

- [ ] **Step 2: Generate the keypair**

Run:
```bash
cargo tauri signer generate -w ~/.tauri/browsitory-updater.key
```

This prompts for a password (set one — required for `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` in Task 5) and prints the public key to stdout, e.g.:
```
Your keypair was generated successfully
Private: ~/.tauri/browsitory-updater.key (Keep it secret!)
Public: dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk...
```

Keep the terminal output — you need both the public key (this task) and the private key file contents + password (Task 5).

- [ ] **Step 3: Add the `plugins.updater` block to `tauri.conf.json`**

Edit `crates/tauri-app/tauri.conf.json`, adding a top-level `plugins` key (there isn't one yet — the file currently has `build`, `app`, `bundle` at the top level):

```json
{
  "productName": "Browsitory",
  "version": "0.1.0",
  "identifier": "com.browsitory.app",
  "build": {
    "beforeDevCommand": "pnpm --dir ../frontend dev",
    "beforeBuildCommand": "pnpm --dir ../frontend build",
    "devUrl": "http://localhost:5173",
    "frontendDist": "../../frontend/dist"
  },
  "app": {
    "enableGTKAppId": true,
    "windows": [
      {
        "title": "Browsitory",
        "width": 1200,
        "height": 800
      }
    ]
  },
  "bundle": {
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  },
  "plugins": {
    "updater": {
      "pubkey": "PASTE_THE_PUBLIC_KEY_FROM_STEP_2_HERE",
      "endpoints": [
        "https://github.com/OWNER/REPO/releases/latest/download/latest.json"
      ]
    }
  }
}
```

Replace `OWNER/REPO` with this repo's actual GitHub `owner/repo` (check with `git remote get-url origin`).

- [ ] **Step 4: Verify the JSON is valid**

Run: `node -e "JSON.parse(require('fs').readFileSync('crates/tauri-app/tauri.conf.json', 'utf8'))"`
Expected: no output (parses cleanly). If it throws, fix the JSON syntax.

- [ ] **Step 5: Commit**

```bash
git add crates/tauri-app/tauri.conf.json
git commit -m "feat(tauri-app): add updater plugin config with signing pubkey"
```

Do **not** commit the private key file or password anywhere in the repo.

---

### Task 2: Add updater/process plugins to the Rust app

**Files:**
- Modify: `crates/tauri-app/Cargo.toml`
- Modify: `crates/tauri-app/src/main.rs`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: the `tauri-plugin-updater` and `tauri-plugin-process` JS bindings become callable from the frontend (`@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`) once the app is registered — Task 4's `ipc/updater.ts` depends on this being done first, since without plugin registration those JS calls fail at runtime (not compile time, so this task's own test only proves the Rust side builds and starts).

- [ ] **Step 1: Add the two dependencies**

Edit `crates/tauri-app/Cargo.toml`, in the `[dependencies]` section, add after the existing `tauri-plugin-opener = "2"` line:

```toml
tauri-plugin-updater = "2"
tauri-plugin-process = "2"
```

- [ ] **Step 2: Register both plugins in `main.rs`**

Edit `crates/tauri-app/src/main.rs`. Change:

```rust
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
```

to:

```rust
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::default())
```

- [ ] **Step 3: Build to confirm it compiles**

Run: `cargo build -p tauri-app --features tauri-app/custom-protocol`
Expected: builds successfully (no test to write — this task only wires plugin registration; behavior is exercised from the frontend side in Task 4).

- [ ] **Step 4: Commit**

```bash
git add crates/tauri-app/Cargo.toml crates/tauri-app/Cargo.lock crates/tauri-app/src/main.rs
git commit -m "feat(tauri-app): register updater and process plugins"
```

(If `Cargo.lock` lives at the workspace root instead of per-crate, `git add Cargo.lock` from the repo root instead — check with `git status` before committing.)

---

### Task 3: Add frontend plugin dependencies

**Files:**
- Modify: `frontend/package.json`

**Interfaces:**
- Consumes: nothing.
- Produces: `@tauri-apps/plugin-updater` and `@tauri-apps/plugin-process` importable as npm packages — Task 4 imports both.

- [ ] **Step 1: Install the packages**

Run from `frontend/`:
```bash
pnpm add @tauri-apps/plugin-updater @tauri-apps/plugin-process
```

- [ ] **Step 2: Verify they're present**

Run: `pnpm list @tauri-apps/plugin-updater @tauri-apps/plugin-process`
Expected: both listed with resolved versions (matching the `^2.x` line already used for `@tauri-apps/api`).

- [ ] **Step 3: Commit**

```bash
git add frontend/package.json frontend/pnpm-lock.yaml
git commit -m "feat(frontend): add updater and process plugin dependencies"
```

---

### Task 4: `frontend/src/ipc/updater.ts` wrapper + tests

**Files:**
- Create: `frontend/src/ipc/updater.ts`
- Test: `frontend/src/ipc/updater.test.ts`

**Interfaces:**
- Consumes: `@tauri-apps/plugin-updater`'s `check()` (returns `Promise<Update | null>`, where `Update` has `.version: string`, `.body?: string`, `.downloadAndInstall(): Promise<void>`), and `@tauri-apps/plugin-process`'s `relaunch()` (returns `Promise<void>`).
- Produces (for Task 5): 
  - `checkForUpdate(): Promise<UpdateInfo | null>` where `UpdateInfo = { version: string; install: () => Promise<void> }`
  - `relaunchApp(): Promise<void>`

Wrapping the raw `Update` object behind `{ version, install }` (rather than re-exporting the plugin's `Update` type) keeps `UpdateBanner.tsx` decoupled from the plugin's exact shape — the component only ever needs a version string to display and a zero-arg function to call.

- [ ] **Step 1: Write the failing test**

Create `frontend/src/ipc/updater.test.ts`:

```typescript
import { beforeEach, describe, expect, it, vi } from "vitest";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { checkForUpdate, relaunchApp } from "./updater";

vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: vi.fn(),
}));

describe("checkForUpdate", () => {
  beforeEach(() => {
    vi.mocked(check).mockReset();
  });

  it("returns null when no update is available", async () => {
    vi.mocked(check).mockResolvedValue(null);

    const result = await checkForUpdate();

    expect(result).toBeNull();
  });

  it("returns version and an install function when an update is found", async () => {
    const downloadAndInstall = vi.fn().mockResolvedValue(undefined);
    vi.mocked(check).mockResolvedValue({
      version: "1.2.3",
      downloadAndInstall,
    } as never);

    const result = await checkForUpdate();

    expect(result?.version).toBe("1.2.3");
    await result?.install();
    expect(downloadAndInstall).toHaveBeenCalledTimes(1);
  });

  it("returns null when the check rejects", async () => {
    vi.mocked(check).mockRejectedValue(new Error("network error"));

    const result = await checkForUpdate();

    expect(result).toBeNull();
  });
});

describe("relaunchApp", () => {
  it("calls the plugin's relaunch", async () => {
    vi.mocked(relaunch).mockResolvedValue(undefined);

    await relaunchApp();

    expect(relaunch).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd frontend && pnpm test -- --run src/ipc/updater.test.ts`
Expected: FAIL — `Cannot find module './updater'` (file doesn't exist yet).

- [ ] **Step 3: Write the implementation**

Create `frontend/src/ipc/updater.ts`:

```typescript
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export interface UpdateInfo {
  version: string;
  install: () => Promise<void>;
}

export async function checkForUpdate(): Promise<UpdateInfo | null> {
  try {
    const update = await check();
    if (update === null) return null;
    return {
      version: update.version,
      install: () => update.downloadAndInstall(),
    };
  } catch (error) {
    console.error("Update check failed", error);
    return null;
  }
}

export async function relaunchApp(): Promise<void> {
  await relaunch();
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd frontend && pnpm test -- --run src/ipc/updater.test.ts`
Expected: PASS, 4 tests.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ipc/updater.ts frontend/src/ipc/updater.test.ts
git commit -m "feat(frontend): add ipc/updater wrapper around Tauri updater plugin"
```

---

### Task 5: `UpdateBanner` component + tests

**Files:**
- Create: `frontend/src/components/UpdateBanner.tsx`
- Create: `frontend/src/components/UpdateBanner.module.css`
- Test: `frontend/src/components/UpdateBanner.test.tsx`

**Interfaces:**
- Consumes: `checkForUpdate(): Promise<UpdateInfo | null>` and `relaunchApp(): Promise<void>` from `../ipc/updater` (Task 4). `UpdateInfo` is `{ version: string; install: () => Promise<void> }`.
- Produces: `UpdateBanner` — a zero-prop React component. Task 6 renders `<UpdateBanner />` in `App.tsx`.

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/components/UpdateBanner.test.tsx`:

```typescript
import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateBanner } from "./UpdateBanner";
import * as updater from "../ipc/updater";

vi.mock("../ipc/updater", () => ({
  checkForUpdate: vi.fn(),
  relaunchApp: vi.fn(),
}));

describe("UpdateBanner", () => {
  beforeEach(() => {
    vi.mocked(updater.checkForUpdate).mockReset();
    vi.mocked(updater.relaunchApp).mockReset();
  });

  it("renders nothing when no update is found", async () => {
    vi.mocked(updater.checkForUpdate).mockResolvedValue(null);

    render(<UpdateBanner />);

    await waitFor(() => expect(updater.checkForUpdate).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("auto-downloads and shows a restart banner when an update is found", async () => {
    const install = vi.fn().mockResolvedValue(undefined);
    vi.mocked(updater.checkForUpdate).mockResolvedValue({ version: "1.2.3", install });

    render(<UpdateBanner />);

    await waitFor(() => expect(install).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/1\.2\.3/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /restart/i })).toBeInTheDocument();
  });

  it("calls relaunchApp when the restart button is clicked", async () => {
    const install = vi.fn().mockResolvedValue(undefined);
    vi.mocked(updater.checkForUpdate).mockResolvedValue({ version: "1.2.3", install });
    vi.mocked(updater.relaunchApp).mockResolvedValue(undefined);

    render(<UpdateBanner />);

    const button = await screen.findByRole("button", { name: /restart/i });
    await act(async () => {
      button.click();
    });

    expect(updater.relaunchApp).toHaveBeenCalledTimes(1);
  });

  it("stays hidden when download fails", async () => {
    const install = vi.fn().mockRejectedValue(new Error("download failed"));
    vi.mocked(updater.checkForUpdate).mockResolvedValue({ version: "1.2.3", install });

    render(<UpdateBanner />);

    await waitFor(() => expect(install).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("button", { name: /restart/i })).toBeNull();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && pnpm test -- --run src/components/UpdateBanner.test.tsx`
Expected: FAIL — `Cannot find module './UpdateBanner'`.

- [ ] **Step 3: Write the CSS module**

Create `frontend/src/components/UpdateBanner.module.css`:

```css
.banner {
  position: fixed;
  bottom: var(--space-3);
  right: var(--space-3);
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-md);
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  box-shadow: var(--shadow-elevated);
  z-index: 100;
}

.restartButton {
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
  background: var(--color-accent);
  color: var(--color-accent-foreground);
  border: none;
  cursor: pointer;
}
```

(Token names copied from `TransferPanel.module.css`'s established `var(--color-*)`/`var(--space-*)` conventions; if any token above doesn't exist in `frontend/src/styles` when you write this, grep `frontend/src/styles/tokens.css` for the nearest equivalent and use that instead — do not invent new tokens.)

- [ ] **Step 4: Write the implementation**

Create `frontend/src/components/UpdateBanner.tsx`:

```typescript
import { useEffect, useRef, useState } from "react";
import { checkForUpdate, relaunchApp, type UpdateInfo } from "../ipc/updater";
import styles from "./UpdateBanner.module.css";

const RECHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

export function UpdateBanner() {
  const [readyUpdate, setReadyUpdate] = useState<UpdateInfo | null>(null);
  const checking = useRef(false);

  useEffect(() => {
    async function runCheck() {
      if (checking.current) return;
      checking.current = true;
      try {
        const update = await checkForUpdate();
        if (update === null) return;
        await update.install();
        setReadyUpdate(update);
      } catch (error) {
        console.error("Update download failed", error);
      } finally {
        checking.current = false;
      }
    }

    void runCheck();
    const timer = window.setInterval(() => void runCheck(), RECHECK_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, []);

  if (readyUpdate === null) return null;

  return (
    <div className={styles.banner} role="status">
      <span>Update v{readyUpdate.version} ready</span>
      <button className={styles.restartButton} onClick={() => void relaunchApp()}>
        Restart to update
      </button>
    </div>
  );
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd frontend && pnpm test -- --run src/components/UpdateBanner.test.tsx`
Expected: PASS, 4 tests.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/UpdateBanner.tsx frontend/src/components/UpdateBanner.module.css frontend/src/components/UpdateBanner.test.tsx
git commit -m "feat(frontend): add UpdateBanner component"
```

---

### Task 6: Mount `UpdateBanner` in the app

**Files:**
- Modify: `frontend/src/App.tsx`

**Interfaces:**
- Consumes: `UpdateBanner` (zero-prop component, Task 5).

- [ ] **Step 1: Import `UpdateBanner`**

In `frontend/src/App.tsx`, add to the imports (alphabetical among the other `./components/*` imports, near `TransferPanel`):

```typescript
import { UpdateBanner } from "./components/UpdateBanner";
```

- [ ] **Step 2: Render it once at the top level of `App`'s returned JSX**

Find the outer `<main ...>` element `App` returns (the one whose children start with `<header>...</header>` — same JSX block read in the "App.tsx bottom" excerpt during planning) and add `<UpdateBanner />` as its first child, before `<header>`:

```tsx
    <main ...>
      <UpdateBanner />
      <header>
```

(It renders `null` until an update is ready, and is `position: fixed` in its own CSS module, so placement among siblings doesn't affect layout — first child keeps it simple to find.)

- [ ] **Step 3: Confirm the frontend still builds and lints clean**

Run from `frontend/`:
```bash
pnpm build
pnpm lint
```
Expected: both succeed — `pnpm lint` in particular confirms `UpdateBanner.tsx`'s `../ipc/updater` import doesn't trip the `no-restricted-imports` rule (it would fail if `UpdateBanner.tsx` imported `@tauri-apps/*` directly instead).

- [ ] **Step 4: Run the full frontend test suite**

Run: `cd frontend && pnpm test -- --run`
Expected: all tests pass, including the new `updater.test.ts` and `UpdateBanner.test.tsx`.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/App.tsx
git commit -m "feat(frontend): mount UpdateBanner in the app shell"
```

---

### Task 7: Wire signing secrets into the release workflow

**Files:**
- Modify: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: the `TAURI_SIGNING_PKEY` / `TAURI_SIGNING_PKEY_PASSWORD` GitHub Actions secrets — already created by the user (repo Settings → Secrets), holding the private key file contents and password from Task 1's keygen. Note these secret *names* differ from the env var names `tauri-action` itself reads (`TAURI_SIGNING_PRIVATE_KEY`/`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`); the workflow step below maps one to the other via `env: TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PKEY }}`.

- [ ] **Step 1: Add the secrets to the `tauri-apps/tauri-action` step's `env`**

In `.github/workflows/release.yml`, find the `build-release` job's final step:

```yaml
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
```

Change to:

```yaml
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PKEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PKEY_PASSWORD }}
        with:
```

- [ ] **Step 2: Validate the workflow YAML**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))"` (or any available YAML validator — the repo doesn't pin one, so use whatever's on the machine; the goal is only to catch a syntax typo before pushing).
Expected: no error.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci(release): sign updater artifacts with tauri-action"
```

- [ ] **Step 4: Confirm the GitHub Actions secrets already exist**

The user already created `TAURI_SIGNING_PKEY` and `TAURI_SIGNING_PKEY_PASSWORD` in the repo's Settings → Secrets and variables → Actions, holding the private key file contents and password from Task 1's keygen. No action needed here beyond confirming Step 1's `env:` block references those exact secret names — if the secrets are ever renamed, this workflow step must be updated to match.

The next tag push to `release/*.*.*` will fail at the `tauri-action` step if these secrets are missing or misnamed (it errors when `plugins.updater` is configured but no signing key is available).

---

## Manual Verification (after all tasks land)

Not automatable in CI (see spec's "Testing" section — no E2E coverage for this feature). After the next real `release/x.y.z` tag:

1. Install the build from the *previous* release.
2. Publish the new draft release on GitHub (un-draft it).
3. Launch the old build; within a few seconds (startup check) the banner should appear reading "Update v{new version} ready — Restart to update".
4. Click "Restart to update" and confirm the app relaunches on the new version.
