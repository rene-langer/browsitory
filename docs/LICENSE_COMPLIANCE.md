# License Compliance Report

Browsitory itself is licensed under the MIT License. Dependencies are expected to be MIT or a
compatible permissive license (Apache-2.0, ISC, BSD, MIT-0), with **one deliberate, documented
exception**: `git2`.

## The libgit2 exception

`git2` (the Rust binding crate) is itself dual-licensed MIT/Apache-2.0, but it links against
**libgit2**, which is licensed **GPL-2.0-with-linking-exception**. The linking exception
explicitly permits linking libgit2 from software under a different license (including
proprietary or MIT-licensed code) without that code inheriting the GPL — this is the same
linking-exception model used by, e.g., the GCC runtime libraries, and is why `git2` is safely
used by MIT-licensed projects like `cargo` itself.

This was a conscious choice, not an oversight: the pure-Rust alternative (`gitoxide`/`gix`, MIT/
Apache-2.0, no C dependency) would have been a closer license fit, but at the time this project
picked `git2`, `gitoxide`'s write-side operations (merge, rebase) were judged less mature than
`git2`'s native support for the same. See `docs/PROJECT_SETUP.md`'s "Key Decisions & Rationale"
for the full tradeoff discussion.

Practical effect: `git2` is configured with `default-features = false, features =
["vendored-libgit2", "vendored-openssl", "https", "ssh"]` (see `crates/git-core/Cargo.toml`),
so libgit2 (and OpenSSL, for HTTPS transport) are built from vendored source via `cmake` rather
than requiring system `libgit2-dev`/`libssl-dev` — this is a build-mechanism choice, not a
license-avoidance one; the GPL-2.0-with-linking-exception status is the same either way.

## Dependencies by crate

### `crates/git-core`
- **git2** — MIT/Apache-2.0 (binding); links libgit2, GPL-2.0-with-linking-exception (see
  above)
- **similar** — MIT/Apache-2.0 (line + word-level diffing, `unicode` feature enabled)
- **thiserror** — MIT/Apache-2.0 (typed error enum)
- **tempfile** (dev-dependency only, not shipped) — MIT/Apache-2.0

### `crates/config`
- **serde** (+ `derive`) — MIT/Apache-2.0
- **toml** — MIT/Apache-2.0
- **directories** — MIT/Apache-2.0 (resolves the OS config directory)
- **thiserror** — MIT/Apache-2.0
- **tempfile** (dev-dependency only) — MIT/Apache-2.0

### `crates/app`
- **eframe**, **egui** — MIT/Apache-2.0 (desktop UI; pulls in `winit`, `wgpu`, and platform
  windowing crates, all permissively licensed)
- **rfd** — MIT/Apache-2.0 (native folder-picker dialog; on Linux this uses the XDG desktop
  portal by default, not GTK, so no GTK dependency is pulled in)
- **similar** — MIT/Apache-2.0
- **git-core**, **config** — internal workspace crates (MIT, this project)

## Compliance Verification Process

```bash
# View a specific crate's license
cargo info <crate-name>

# List the full resolved dependency tree with licenses (requires cargo-license, not bundled)
cargo install cargo-license
cargo license
```

When adding a new dependency:
1. Check its license with `cargo info <crate-name>`.
2. Confirm it's MIT, Apache-2.0, ISC, BSD, or MIT-0 — or, if it's a genuine exception like
   `git2`, document the exception here with the same rationale depth as above (why it was
   chosen despite the license, and what the practical linking implications are).
3. Update this document.

## Excluded Licenses

Do not add dependencies with these licenses (the `git2`/libgit2 case above is the one
pre-approved, documented exception — new exceptions need the same explicit write-up, not a
silent addition):
- GPL (any version, without a linking exception)
- AGPL
- SSPL
- LGPL (already ruled out ZenFS in the old browser-era codebase on these grounds)
- Commercial/Proprietary
- Elastic License

---

Last updated: 2026 (Rust rewrite, branch `feat/rust_from_scratch`).
