# Releasing

How Browsitory versions and publishes releases. Moved out of `AGENTS.md` so the agent
instructions stay short; `AGENTS.md` keeps the rules an agent must not break.

## Versioning & releases

Semantic versioning. Pushing a `release/MAJOR.MINOR.PATCH` tag (no `v` prefix) triggers
`.github/workflows/release.yml`: builds standalone distributables for Linux/macOS/Windows
(`tauri-apps/tauri-action`, same build as `scripts/build-dist.*`) and publishes a draft GitHub
Release. The workflow stamps the version into `crates/tauri-app/tauri.conf.json` at build time.
Every installer carries `THIRD_PARTY_NOTICES.md` (a Tauri resource), and
`publish-vsix-release` also attaches it to the draft release. It is generated, so a release
needs no extra step for it (see `docs/LICENSE_COMPLIANCE.md`). The `verify` job runs first on
the tagged commit: `cargo test --workspace`, the frontend test suite,
`scripts/generate-third-party-notices.py --check`, so a stale notices file stops the release,
and `scripts/fold-changelog.py --check`, so unfolded changelog fragments stop it too.

### Preparing the changelog

Pull requests don't edit `CHANGELOG.md`. Each adds a fragment, `changelog.d/<issue>-<slug>.md`
(format: `changelog.d/README.md`). Before tagging, open a release-prep pull request that:

1. runs `python3 scripts/fold-changelog.py` in a full (not shallow) clone. It moves every
   fragment to the top of `## [Unreleased]` and deletes the fragment files. Uncommitted
   fragments go first, by file name. Committed ones follow, newest first by the date of the
   commit that added each; ties go by file name. It changes nothing if `## [Unreleased]` is
   missing, a fragment is malformed, `changelog.d/` holds a file that isn't a `*.md` fragment
   or `README.md`, or the clone is shallow. With no fragments left it does nothing.
2. renames `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`, adds a new empty `## [Unreleased]`
   above it, and edits the text where needed.

Merge it, then tag that commit. The `verify` job fails while any fragment is left
(`scripts/fold-changelog.py --check`), because every VSIX ships `CHANGELOG.md` and the app's
release notes are built from it.

### Before a release: docs and screenshots

In the same release-prep pull request (or one before it):

1. Re-run the screenshots: `e2e/screenshots/capture.sh` on Linux (see
   `e2e/screenshots/README.md`). Look at every changed image in `docs/guide/assets/` and commit
   the ones that show a real UI change.
2. Read the pages in `docs/guide/` that cover what changed since the last release, and fix
   anything the changelog makes out of date. Check `docs/guide/install.md` against how this
   release ships (file names, pre-release or not, see `VSIX_PRE_RELEASE` below).
3. CI's `Docs` workflow (`.github/workflows/docs.yml`) builds the site with
   `mkdocs build --strict` on the pull request and publishes it to
   <https://rene-langer.github.io/browsitory/> once it is on `main`.

The Marketplace and Open VSX pages show `extension/README.md`, with images loaded from `main`
(`raw.githubusercontent.com/.../main/docs/guide/assets/`), so merge the new screenshots before
the release is published.

### Publishing the extension

The same tag builds one `.vsix` per platform (`build-vsix-release`, version stamped from the
tag) and attaches them to the draft release (`publish-vsix-release`). Only after that, and after
the desktop build, two jobs publish the same files:

- **Visual Studio Marketplace** (`publish-vsix-marketplace`): GitHub OIDC into Microsoft Entra
  ID, no stored token. `azure/login`, then `vsce publish --azure-credential`.
- **Open VSX** (`publish-vsix-open-vsx`): `OVSX_PAT`, a token for the Open VSX namespace
  `browsitory`. `ovsx` runs via `npx` only (EPL-2.0, build-only tool; see
  `docs/LICENSE_COMPLIANCE.md`).

Both jobs run in the GitHub environment `marketplace` (deployment tags restricted to
`release/*`), and all three secrets are **environment** secrets there: `AZURE_CLIENT_ID`,
`AZURE_TENANT_ID`, `OVSX_PAT`. Each job skips with a notice while its secret is empty.
The owner is a required reviewer on `marketplace`, so both publish jobs wait for the owner's
approval before they start: open the release run's page in Actions and use **Review
deployments**, or find the run with `gh run view` and approve it there.

**Pre-release switch:** `VSIX_PRE_RELEASE` at the top of `release.yml` (`"true"` now). Set it to
`"false"` to package and publish stable releases. Packaging reads it too, because `vsce publish
--pre-release` refuses a package that wasn't packaged as a pre-release. A stable version must be
higher than every pre-release version already published.

**Never re-tag a published version at a different commit.** Both registries refuse to replace a
published version. `--skip-duplicate` only makes re-running the *same* commit's release safe; a
moved tag would leave the registries on the old build. Bump the version instead.

Owner setup (once):

1. Marketplace: create publisher `browsitory` (<https://marketplace.visualstudio.com/manage>).
2. Entra ID: an app registration with a federated credential for GitHub Actions, repository
   `rene-langer/browsitory`, entity **Environment** `marketplace`
   (`repo:rene-langer/browsitory:environment:marketplace`). Store its client and tenant ids as
   `AZURE_CLIENT_ID` / `AZURE_TENANT_ID` in the `marketplace` environment.
3. Open VSX: create an account, sign the Eclipse publisher agreement, create the namespace
   (`npx ovsx@1.3.0 create-namespace browsitory -p <token>`), and store the token as `OVSX_PAT`
   in the `marketplace` environment.
4. Add the Entra app to the publisher. The Marketplace jobs print a notice with the Entra app's
   Marketplace member id (`az rest -u https://app.vssps.visualstudio.com/_apis/profile/profiles/me
   --resource 499b84ac-1321-427f-aa17-267ca6975798 --query id -o tsv`). Run the
   [dry run](#dry-run) to get it before the first release, and add the id to publisher
   `browsitory` as **Contributor**. Until then, the publish step fails; if a release gets there
   first, add the id and re-run the failed job. Later releases need nothing.
5. Add yourself as a **required reviewer** on the `marketplace` environment (Settings →
   Environments → `marketplace` → Deployment protection rules). Every job in that environment
   (both publish jobs and both credential jobs of the dry run) then waits for your approval.

### Package checks

Every pull request runs `vsix-smoke` (`ci.yml`). It packages the linux-x64 `.vsix` the way the
release does and runs `extension/scripts/check-vsix.mjs` on it. The script reads the `.vsix`
itself and checks:

- identity: publisher `browsitory`, name `browsitory`, the expected version and target, and the
  pre-release flag;
- listing metadata: icon, repository link, licence;
- files: only the expected ones (no source maps, tests or sources; the package does not ship
  the compiled code's `.js.map` files), a non-empty
  `THIRD_PARTY_NOTICES.md` (any content), and exactly one sidecar, an executable binary for the
  package's target.

The job then installs that `.vsix` into a downloaded VSCode with `code --install-extension`
(throwaway extensions and user data dirs) and opens a temp repository. It passes when the
installed copy activates, its bundled sidecar runs, and the repository's commit shows up.
Run it locally from `extension/e2e`: `pnpm --ignore-workspace smoke:vsix <file.vsix>` (needs a
display; use `xvfb-run` on a headless machine).

`build-vsix` (main) and `build-vsix-release` run the same check for all four targets;
`build-vsix-release` checks the tag's version and the `VSIX_PRE_RELEASE` flag before anything is
published.

### Dry run

Before the first release, check the publishing credentials without publishing anything. A
manual run of `release.yml` is always a dry run: it builds nothing, creates no GitHub release
and publishes nothing. Every build and publish job runs only for a pushed `release/X.Y.Z` tag.

The `marketplace` environment allows only `release/*` refs, so run it on the tag
`release/dry-run`. That tag matches the environment policy but not the `release/*.*.*` push
trigger, so pushing it starts nothing by itself. Point it at a `main` commit that has this dry
run in `release.yml`:

```bash
git fetch origin
git tag release/dry-run origin/main
git push origin release/dry-run
gh workflow run release.yml --ref release/dry-run -f dry_run=true
gh run watch "$(gh run list --workflow release.yml --event workflow_dispatch --limit 1 --json databaseId -q '.[0].databaseId')"
```

To run it again on a newer `main`, delete the tag first (it is not a version, so moving it is
safe): `git push origin :refs/tags/release/dry-run && git tag -d release/dry-run`, then repeat.

`dry-run-marketplace` and `dry-run-open-vsx` run in the `marketplace` environment, so they
wait for the owner's approval before they start. Approve them on the run's page in Actions
(**Review deployments**); `gh run view` shows the run and its link.

What the jobs mean:

- **dry-run-guard** fails if `dry_run` is not `true` or the ref is not a `release/` tag.
- **dry-run-marketplace** signs in to Entra ID through OIDC (`azure/login`) and prints a notice
  with the Entra app's **Marketplace member id**. Then `vsce verify-pat browsitory
  --azure-credential` checks that the sign-in works and that the app has some role on publisher
  `browsitory`. Any role passes, even Reader, so also check the publisher's **Members** page:
  publishing needs **Contributor** (or higher). If it fails with
  a 403 (or "verification has failed"), add the printed id to the publisher as
  **Contributor** at <https://marketplace.visualstudio.com/manage/publishers/browsitory>, then
  re-run the job. A failure in `azure/login` means the federated credential or the
  `AZURE_CLIENT_ID` / `AZURE_TENANT_ID` secrets are wrong.
- **dry-run-open-vsx** runs `ovsx verify-pat browsitory` with `OVSX_PAT`. It fails if the token
  is invalid or cannot publish to the namespace.

All three green means both sign-ins work, the Entra app holds some role on the Marketplace
publisher, and the Open VSX token can publish to its namespace. It does not prove the
Marketplace role is Contributor; check that on the publisher's Members page. Delete the tag
afterwards if you like; it is not needed for releases.
