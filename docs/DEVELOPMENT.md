# Development Guide

## Prerequisites

- Rust (stable, pinned in `rust-toolchain.toml`) — installed automatically by
  `scripts/setup-dev.sh` if missing
- A C toolchain + `cmake` (for `git2`'s vendored libgit2 build)
- Linux only: `libxkbcommon`, `libwayland`, `libx11`/`libxcb` dev headers (for `eframe`/`winit`)
- Linux only: `Xvfb` (`xvfb-run`) — lets GUI verification run under an isolated virtual
  display instead of the real desktop; GNOME Wayland's screenshot D-Bus API refuses
  unsandboxed callers, so this is the automatable path for screenshotting UI changes
- Git

## Setup

```bash
git clone <repository-url>
cd browsitory
./scripts/setup-dev.sh   # installs Rust + system deps, then builds the workspace
cargo run -p app
```

## Project Structure

```
browsitory/
├── crates/
│   ├── git-core/      # git2-based service layer — status, log, diff, stage, commit, ...
│   ├── config/        # repo registry + preferences (TOML)
│   └── app/           # egui/eframe desktop UI
│       └── src/
│           ├── worker.rs   # per-repo worker thread
│           ├── state.rs    # AppState, RepoSession
│           ├── ui/         # one module per panel
│           └── main.rs
├── docs/
└── Cargo.toml
```

## Development Workflow

### Code Style

```bash
cargo fmt --all -- --check                              # formatting
cargo clippy --workspace --all-targets -- -D warnings    # lints
cargo fmt --all                                          # auto-format
```

### Git Workflow

1. Create a feature branch: `git checkout -b feature/feature-name`
2. Make your changes
3. Run `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
4. Commit with a descriptive message following Conventional Commits (`feat:`, `fix:`, `docs:`,
   `refactor:`, `test:`, `chore:`)
5. Push and open a pull request

## Key Patterns

### Adding a git-core operation

Add a function to the relevant module (or a new module) in `crates/git-core/src/`, taking
`&git2::Repository` (or a path) as an explicit argument — never a singleton/global. Re-export
it from `lib.rs`. Add integration tests in `crates/git-core/tests/` against a real temp-dir
repo (see `tests/common/mod.rs`'s `init_repo()` helper), not a mocked `Repository`.

```rust
// crates/git-core/src/example.rs
use git2::Repository;
use crate::repo::Result;

pub fn example_operation(repo: &Repository, arg: &str) -> Result<()> {
    // ...
    Ok(())
}
```

### Wiring a git-core operation into the UI

1. Add a `Command`/`Event` variant pair in `crates/app/src/worker.rs`.
2. Handle the command in `worker::handle`, calling the `git-core` function.
3. Apply the resulting event to `RepoSession` state in `RepoSession::poll_events`
   (`crates/app/src/state.rs`).
4. Trigger it from a UI panel in `crates/app/src/ui/` via `RepoSession::send`/a dedicated method
   on `RepoSession` (see `stage`/`unstage`/`commit` for the pattern) — UI code never calls
   `git-core` directly.

### egui/eframe version notes

This project pins `eframe`/`egui` 0.35. The `App` trait's method is `fn ui(&mut self, ui: &mut
egui::Ui, frame: &mut eframe::Frame)`, and `SidePanel`/`TopBottomPanel` from older egui
tutorials don't exist — use `egui::Panel::left/right/top/bottom(id)` instead, and note that
every panel's `.show()` (including `CentralPanel`) takes `&mut Ui`, not `&Context`. See
`CLAUDE.md`'s "egui/eframe version-specific API notes" for more, and check the pinned version
in `crates/app/Cargo.toml` before trusting online example code.

## Testing

```bash
cargo test --workspace                       # all tests
cargo test -p git-core                       # one crate
cargo test -p git-core --test log            # one test file
cargo test -p git-core -- some_test_name      # one test by name
```

`git-core` and `config` tests use real temp directories (`tempfile`), never mocks of `git2` or
the filesystem.

## Building for Production

```bash
cargo build --workspace --release
```

The release binary is at `target/release/app` (or `app.exe` on Windows).

## Troubleshooting

### Build fails on a fresh Linux machine
Missing system packages for `eframe`/`winit` or `git2`'s vendored libgit2 build — run
`scripts/setup-dev.sh`, or see its contents for the exact `apt`/`dnf`/`pacman` package names.

### `cargo build` is very slow the first time
Expected — `git2` builds vendored libgit2 from source via `cmake`, and `eframe`'s dependency
tree (`wgpu`, `winit`, `egui`) is large. Subsequent builds are incremental and much faster.

### Git operations behave unexpectedly
Check that the target directory is actually a git repository (`git2::Repository::discover`
walks up from the given path looking for `.git`, same as the `git` CLI) and that file paths
passed to `git-core` functions are relative to the repo root, not absolute.
