# Contributing to Browsitory

Thank you for your interest in contributing to Browsitory! We welcome contributions of all
kinds, including bug reports, feature requests, documentation improvements, and code
contributions.

## Code of Conduct

Be respectful and constructive in all interactions with other contributors.

## Getting Started

1. **Fork the repository** on GitHub
2. **Clone your fork** locally
3. **Run `scripts/setup-dev.sh`** — installs Rust (via rustup) and the system packages
   `eframe`/`winit` and `git2`'s vendored libgit2 build need
4. **Create a feature branch** with a descriptive name
5. **Make your changes** and test them
6. **Commit your changes** with clear, descriptive messages
7. **Push to your fork** and **open a pull request**

## Development Setup

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for detailed setup instructions.

## Making Changes

### Before You Start
- Check if an issue exists for your change
- For large changes, open an issue first to discuss
- Follow the project's code style (`cargo fmt` + `cargo clippy`)

### While Developing
- Write clear, self-documenting code — comments only for non-obvious logic (a hidden
  constraint, a workaround, something that would surprise a reader)
- Keep functions small and focused
- New `git-core` operations: a plain function taking `&git2::Repository` explicitly (not a
  singleton), in its own module, re-exported from `lib.rs`, tested against a real temp-dir repo
  in `crates/git-core/tests/` — see [CLAUDE.md](CLAUDE.md) for the full rationale

### Testing Your Changes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

## Pull Request Process

1. **Update documentation** if your changes require it
2. **Add/update tests** for any `git-core`/`config` changes
3. **Keep commits clean** — use clear commit messages following Conventional Commits
4. **Reference related issues** in your PR description
5. **Be responsive** to feedback during review

### PR Title Format
Use Conventional Commits format:
- `feat: add feature description`
- `fix: fix issue description`
- `docs: update documentation`
- `refactor: refactor module name`

### PR Description Template
```markdown
## Description
Brief description of what this PR does

## Changes
- Change 1
- Change 2

## Related Issue
Fixes #123

## Testing
How to test these changes (`cargo test`, and manual verification steps for UI changes)

## Screenshots (if applicable)
Before/after screenshots for UI changes
```

## Feature Requests

When suggesting a new feature:
1. Use a clear, descriptive title
2. Provide a detailed description
3. Explain the use case and why it's needed

## Bug Reports

When reporting a bug:
1. Use a clear, descriptive title
2. Describe the exact steps to reproduce
3. Describe the observed vs. expected behavior
4. Include your environment (OS, `rustc --version`)

## License Compliance

- All contributions must be compatible with the MIT License, with the one documented exception
  already in place (`git2`/libgit2 — see [docs/LICENSE_COMPLIANCE.md](docs/LICENSE_COMPLIANCE.md))
- Verify a new dependency's license with `cargo info <crate-name>` before adding it
- Document any new dependency's license in `docs/LICENSE_COMPLIANCE.md`
- A new GPL/LGPL/AGPL dependency needs the same explicit rationale write-up as the `git2`
  exception, not a silent addition

## Architecture Guidelines

Before making architectural changes:
1. Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
2. Discuss major changes in an issue first
3. Update architecture docs if needed

## Code Style

### Rust
- Run `cargo fmt` before committing; CI enforces `cargo fmt --check`
- Run `cargo clippy --workspace --all-targets -- -D warnings` and fix all warnings
- Prefer explicit error types (`thiserror`) over `unwrap()`/`expect()` outside of tests
- Keep `git-core` functions free of any UI/eframe dependency

### Git
- Create a new branch for each feature/fix
- Keep branches up-to-date with main
- Rebase before opening a PR (no merge commits)

## Documentation

Please update relevant documentation for your changes:
- `README.md` for user-facing changes
- `docs/DEVELOPMENT.md` for development setup changes
- `docs/ARCHITECTURE.md` for architectural changes
- `CLAUDE.md` for anything a future contributor (human or AI) would need to know before
  touching a given area — version-specific API gotchas, non-obvious design decisions, etc.

## Questions?

- Check existing issues and discussions
- Read the documentation first
- Open a GitHub discussion if unclear

## Recognition

Contributors will be recognized in the GitHub contributors page and release notes.

Thank you for making Browsitory better!
