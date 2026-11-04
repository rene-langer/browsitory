# Releasing

How Browsitory versions and publishes releases. Moved out of `AGENTS.md` so the agent
instructions stay short; `AGENTS.md` keeps the rules an agent must not break.

## Versioning & releases

Semantic versioning. Pushing a `release/MAJOR.MINOR.PATCH` tag (no `v` prefix) triggers
`.github/workflows/release.yml`: builds standalone distributables for Linux/macOS/Windows
(`tauri-apps/tauri-action`, same build as `scripts/build-dist.*`) and publishes a draft GitHub
Release. The workflow stamps the version into `crates/tauri-app/tauri.conf.json` at build time.
`cargo test --workspace` and the frontend test suite must pass on the tagged commit first.

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

### Package checks

Every pull request runs `vsix-smoke` (`ci.yml`). It packages the linux-x64 `.vsix` the way the
release does and runs `extension/scripts/check-vsix.mjs` on it. The script reads the `.vsix`
itself and checks:

- identity: publisher `browsitory`, name `browsitory`, the expected version and target, and the
  pre-release flag;
- listing metadata: icon, repository link, licence;
- files: only the expected ones (no source maps, tests or sources), a non-empty
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

What the jobs mean:

- **dry-run-guard** fails if `dry_run` is not `true` or the ref is not a `release/` tag.
- **dry-run-marketplace** signs in to Entra ID through OIDC (`azure/login`) and prints a notice
  with the Entra app's **Marketplace member id**. Then `vsce verify-pat browsitory
  --azure-credential` checks that the app has a role on publisher `browsitory`. If it fails with
  a 403 (or "verification has failed"), add the printed id to the publisher as
  **Contributor** at <https://marketplace.visualstudio.com/manage/publishers/browsitory>, then
  re-run the job. A failure in `azure/login` means the federated credential or the
  `AZURE_CLIENT_ID` / `AZURE_TENANT_ID` secrets are wrong.
- **dry-run-open-vsx** runs `ovsx verify-pat browsitory` with `OVSX_PAT`. It fails if the token
  is invalid or cannot publish to the namespace.

All three green means the first real release can publish to both registries. Delete the tag
afterwards if you like; it is not needed for releases.
