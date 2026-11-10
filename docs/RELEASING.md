# Releasing

How Browsitory versions and publishes releases. Moved out of `AGENTS.md` so the agent
instructions stay short; `AGENTS.md` keeps the rules an agent must not break.

## Versioning & releases

Semantic versioning. Pushing a `release/MAJOR.MINOR.PATCH` tag (no `v` prefix) triggers
`.github/workflows/release.yml`: builds standalone distributables for Linux/macOS/Windows
(`tauri-apps/tauri-action`, same build as `scripts/build-dist.*`) and publishes a draft GitHub
Release. `tauri-action` runs the Tauri CLI that `frontend/pnpm-lock.yaml` locks
(`@tauri-apps/cli`, through its `tauriScript` input), so a re-run builds with the same CLI;
without `tauriScript` it would install the newest 2.x (#397). Bumping that lockfile entry
changes the CLI the next release builds with. The workflow stamps the version into `crates/tauri-app/tauri.conf.json` at build time.
Every installer carries `THIRD_PARTY_NOTICES.md` (a Tauri resource), and
`publish-vsix-release` also attaches it to the draft release. It is generated, so a release
needs no extra step for it (see `docs/LICENSE_COMPLIANCE.md`). The `verify` job runs first on
the tagged commit: `cargo test --workspace`, the frontend test suite,
`scripts/generate-third-party-notices.py --check`, so a stale notices file stops the release,
and `scripts/fold-changelog.py --check`, so unfolded changelog fragments stop it too.

### Linux build image (glibc floor)

The Linux bundles (AppImage, `.deb`, `.rpm`) and the linux-x64 VSIX's `vscode-sidecar` need at
least the glibc of the image they are built on. So `build-release` and `build-vsix-release` run
their Linux builds on a pinned `ubuntu-24.04`, not `ubuntu-latest`, which moves to Ubuntu 26.04
(glibc 2.43) from October 2026 (#386). Built on 24.04, both need glibc 2.39 (0.3.1, checked with
`objdump -T`): Ubuntu 24.04, Debian 13, Fedora 40 or newer, the floor `docs/guide/install.md`
states. CI's `build-dist` and `build-vsix` pin the same image, so main builds what a release
ships. The other Linux jobs stay on `ubuntu-latest`; CI ran them on `ubuntu-26.04` before the
switch (#386). To raise the floor, move all four pins together and update
`docs/guide/install.md`; do it before GitHub retires the `ubuntu-24.04` image.

### Bundler tools

While bundling, the Tauri CLI downloads tools from GitHub release downloads, each tried once:
WiX 3.14 and NSIS 3.13 on Windows (into `%LOCALAPPDATA%\tauri`), AppRun, linuxdeploy and its
AppImage plugin on Linux (into `~/.cache/tauri`); macOS downloads nothing. A GitHub 504 there
failed CI's `build-dist` on both platforms (#454). So `build-release` and `build-dist` cache that
folder with `actions/cache`, keyed on the OS and the `@tauri-apps/cli` version
`frontend/pnpm-lock.yaml` locks, and run `scripts/prefetch-bundler-tools.py` before bundling: it
downloads whatever is missing with retries and checks each file's hash, so the bundler finds
every tool in place. A tag's run can restore the caches main's `build-dist` saved, so a release
normally bundles with the tools main already used and downloads nothing.

The script's tool list (URLs, file names, hashes) is copied from the bundler of the CLI version
in its `TAURI_CLI_VERSION`. When the lockfile moves to another CLI version, the script only
prints a warning in those jobs and the bundler downloads its own tools again, without retries
(the cache still works). CI's `changelog` job runs `prefetch-bundler-tools.py --check-version`
on every pull request, so a CLI bump (usually Renovate's Tauri group PR) gets a warning
annotation and a line in the job summary; the check never fails (#458).

After a CLI bump, in that PR or right after it: compare
`crates/tauri-bundler/src/bundle/{windows/msi,windows/nsis,linux/appimage}` in the Tauri
repository at the new `tauri-cli-v<version>` tag with the list; update any changed URL, file
name or required file and its hash (sha256 of the file at the URL where the bundler pins none);
then set `TAURI_CLI_VERSION` to the new version. The warning goes away and the retries are
back. If a release's `build-release` still fails on a download, re-run that job:
the tag doesn't need to move.

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
   `zensical build --strict` (plus `scripts/check-docs-nav.py`) on the pull request and publishes it to
   <https://rene-langer.github.io/browsitory/> once it is on `main`.

The Marketplace and Open VSX pages show `extension/README.md`, with images loaded from `main`
(`raw.githubusercontent.com/.../main/docs/guide/assets/`), so merge the new screenshots before
the release is published.

### Updater manifest (`latest.json`)

Every installed desktop app checks
`https://github.com/rene-langer/browsitory/releases/latest/download/latest.json`
(`plugins.updater.endpoints` in `crates/tauri-app/tauri.conf.json`). From the release after
0.5.0 on (#477), it falls back to a copy on the user guide site,
`https://rene-langer.github.io/browsitory/latest.json`, when that URL fails (see
[the second endpoint](#second-endpoint-the-user-guide-site) below); older installs know only the
first. A bad manifest or bundle link makes updating fail for every installed copy until the next
release fixes it; a changed endpoint or `pubkey` strands them for good. The app checks at start
and every 6 hours; after a failed check or download it retries after 10 minutes, doubling up to
3 hours, until a check succeeds (`frontend/src/components/UpdateBanner.tsx`).

`build-release` writes the manifest with
`tauri-apps/tauri-action`: each of its three jobs uploads its bundles and `.sig` files to the
draft release and merges its platforms into the draft's `latest.json`. `release.yml` pins the
action by commit with a `# vX.Y.Z` comment (#399), not by the floating `v1` tag, so a new
release of it can't change the manifest unreviewed. Renovate proposes each update as its own PR,
outside the Tauri and GitHub Actions groups (`renovate.json`). CI doesn't run `tauri-action`, so a
green PR says nothing about it: read its release notes for changes to `latest.json`, the asset
names or the release handling below before merging.

What `tauri-action` v1 (since 0.4.0, #363) does differently from v0:

- **Download URLs.** Each `url` in `latest.json` is the bundle's API asset URL,
  `https://api.github.com/repos/rene-langer/browsitory/releases/assets/<id>`, not the browser
  URL `https://github.com/rene-langer/browsitory/releases/download/<tag>/<file>` of v0. v1 has no
  input to keep the old form: it builds the URL from `githubBaseUrl`, which also sets the API it
  uploads to. The updater (`tauri-plugin-updater` 2.14.1, also in 0.3.0 and 0.3.1) downloads with
  `Accept: application/octet-stream` and its own User-Agent; the API answers that request with a
  redirect to the file (checked against 0.3.1's assets: the same bytes as the browser URL).
  Without that header the API returns the asset's JSON description, which fails the signature
  check. The asset ids stay the same when the draft is published.
- **Rate limit.** These downloads are unauthenticated API requests: 60 per hour per IP address,
  shared with any other unauthenticated GitHub API use from that address. Only the download
  counts, one request per update; the update check reads `latest.json` from `github.com`, which
  is not limited this way. Many installs behind one address updating within the same hour can
  get a `403` ("Download request failed"); updating works again after the hour. A network that
  allows `github.com` but blocks `api.github.com` can't download updates any more.
- **macOS bundle name.** `Browsitory_<version>_universal.app.tar.gz` (v0:
  `Browsitory_universal.app.tar.gz`). Nothing depends on it; `latest.json` links to it.
- **Drafts only.** With `releaseDraft: true`, v1 only uses a draft release for the tag. After
  publishing, re-running a `build-release` job fails (or creates a second draft for the tag;
  delete that) and doesn't touch the published release. Don't re-run `build-release` after
  publishing. Re-running the publish jobs (#344) is unaffected: they don't run `tauri-action`.
- **Name and body.** v1 overwrites the name and body of an existing release only if it is
  published, never a draft, so re-running `build-release` before publishing keeps notes written
  into the draft. It does replace that platform's assets (new asset ids) and its entries in
  `latest.json`. The `notes` in `latest.json` are always `releaseBody` from `release.yml`, not
  the notes edited on the release page.

**Before publishing the draft**, the manifest is checked automatically (#398): a step of
`publish-vsix-release`, which runs once all three `build-release` jobs are done, runs
`scripts/check-updater-manifest.py` on the draft's `latest.json`, its `.sig` files and its asset
list, before that job uploads anything. It fails the run before the marketplace jobs start and
lists every problem it found. It checks:

- `version` is `X.Y.Z`;
- each of `linux-x86_64`, `linux-x86_64-appimage`, `linux-x86_64-deb`, `linux-x86_64-rpm`,
  `windows-x86_64`, `windows-x86_64-msi`, `windows-x86_64-nsis`, `darwin-aarch64`,
  `darwin-x86_64`, `darwin-aarch64-app` and `darwin-x86_64-app` is present (v1 also writes
  `darwin-universal` and `darwin-universal-app`; any extra platform is checked the same way);
- each `url` is the API URL of an uploaded asset of this release, of the platform's kind (for
  example `.msi` for `windows-x86_64-msi`, `.app.tar.gz` for the `darwin-*` keys);
- each `signature` is the content of that asset's `.sig`.

A missing platform means two jobs wrote `latest.json` at the same time: re-run that platform's
`build-release` job (the draft is still a draft). GitHub re-runs the jobs that depend on it too,
so `publish-vsix-release` checks the manifest again. To run the same check by hand:

```bash
tag=release/X.Y.Z
dir="$(mktemp -d)"
gh release download "$tag" -D "$dir" -p latest.json -p '*.sig'
gh release view "$tag" --json assets > "$dir/assets.json"
python3 scripts/check-updater-manifest.py --version "${tag#release/}" --assets "$dir/assets.json" "$dir"
```

**After publishing**, check what an installed app sees:

1. `curl -sL https://github.com/rene-langer/browsitory/releases/latest/download/latest.json`
   shows the new version.
2. Download one bundle the way the updater does; it prints `200` and the bundle's size:
   `curl -sSL -o /dev/null -w '%{http_code} %{size_download}\n' -A tauri-plugin-updater -H
   'Accept: application/octet-stream' '<a url from latest.json>'`.
3. Updater smoke test: install the previous release and update it through the app, on Linux,
   and on macOS and Windows where possible.
4. The second endpoint, once the `Docs` run that the release started has finished
   (`gh run list -w docs.yml -L 2`): `curl -sL https://rene-langer.github.io/browsitory/latest.json`
   shows the same manifest as step 1.

#### Second endpoint: the user guide site

The updater tries its endpoints in order and reads the next one when one fails (no answer, or
not a 2xx status). The second is `latest.json` on the user guide's Pages site, so an outage of
the release download host (on 2026-11-09 it answered `504` while the API asset URLs, which the
bundles are downloaded from, kept working) doesn't stop updates. `release.yml` doesn't write it:
the `Docs` workflow (`.github/workflows/docs.yml`) copies it from the published release each time
it builds the site.

- **Never ahead of the published release.** The build asks the API for `releases/latest`, the
  newest published release that is not a pre-release (drafts are never returned), the same
  release the first endpoint resolves to. It downloads that release's `latest.json`, `.sig`
  files and asset list and runs `scripts/check-updater-manifest.py` on them, as
  `publish-vsix-release` does on the draft. Only a manifest that passes goes into the site. A
  published release's files can't change (immutable releases), so the copy stays right.
- **Follows each release.** Publishing the draft fires `release: published`; the
  `redeploy-on-release` job of `docs.yml` then starts the workflow on `main`
  (`workflow_dispatch`), which rebuilds and publishes the site with the new `latest.json`. Every
  other publish of the site (a docs change on `main`) fetches it again too.
- **Bundle URLs on the copy (#519).** The copy is not the release's file byte for byte: the
  `--pages-copy` option of `check-updater-manifest.py` rewrites each `url` to
  `https://github.com/rene-langer/browsitory/releases/download/release%2FX.Y.Z/<file>` (the v0
  form). An app that reached the second endpoint because `api.github.com` failed can then still
  download the bundle. The signatures cover the bundle, so they stay valid; the release's own
  `latest.json` keeps the API URLs. (If `github.com` is the host that fails, both endpoints are
  down anyway.)
- **A failure doesn't break the docs.** If the release can't be read or the check fails, the
  build warns ("Updater manifest") and publishes the site without `latest.json`. The second
  endpoint then answers `404`; installed apps still use the first. The run also ends red
  (job `manifest-missing`; the site is still published), and the daily scheduled run of
  `docs.yml` on `main` tries again. Pull requests run the same step but publish nothing.
- **If it is missing or old** (the red run, the warning, or step 4 above shows an older
  version): fix the cause, then `gh workflow run docs.yml --ref main`, or re-run the latest
  `Docs` run on `main`.
- **Plugin limit.** The updater moves to the second endpoint only after a non-2xx status or a
  request error. A first endpoint that answers `200` with a body it can't parse or use is an
  error of the check and does not fall through (`tauri-plugin-updater` 2.14.1). Not fixed here;
  `check-updater-manifest.py` on the draft is what keeps such a body from being published.
- A pre-release, or a release published with `--latest=false`, doesn't change either endpoint.
- Installed apps keep this URL like the first one: moving the user guide site (another Pages
  address, a custom domain without a redirect) or dropping `latest.json` from it takes the
  fallback away from every install that has it.

**0.4.0 only (#363):** 0.4.0 is the first release built with v1. #363 stays open until an
installed 0.3.1 updates to 0.4.0 through the app on Linux, macOS and Windows, after the checks
above. If the download fails while `latest.json` is right otherwise, put back the v0 URLs by
hand: replace each `url` with the browser URL
`https://github.com/rene-langer/browsitory/releases/download/release%2FX.Y.Z/<file>`, keep the
signatures (they sign the bundle, not the URL), and upload the file with
`gh release upload release/X.Y.Z latest.json --clobber`. (The check above then reports those
URLs as not API asset URLs; that is expected.)

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

**Pre-release switch:** `VSIX_PRE_RELEASE` at the top of `release.yml`. It is `"false"` since
0.3.1: releases are packaged and published as stable (0.3.0 was a pre-release).
Set it to `"true"` to publish pre-releases again. Packaging reads it too, because `vsce publish
--pre-release` refuses a package that wasn't packaged as a pre-release. A stable version must be
higher than every pre-release version already published. `vsix-smoke` in `ci.yml` packages the
same way (stable now); change it with the switch.

**GitHub release:** publish the draft as a normal release, not a pre-release. The app's updater
reads `releases/latest/download/latest.json`, and GitHub's `latest` skips pre-releases. Check
the [updater manifest](#updater-manifest-latestjson) before and after publishing.

Publish **the draft** that `release.yml` created (Edit, then **Publish release**, or
`gh release edit release/X.Y.Z --draft=false --latest`). Never create a new release for the
tag on the releases page: the repository has **immutable releases** turned on, so the first
release published for a tag locks that tag for good. An empty release published by mistake
can be deleted, but the tag stays locked even with immutability turned off, and the draft with
the bundles can then never be published. The only way out is the next patch version from the
same commit (0.4.0 went out this way as 0.4.1). Keep immutability on: it stops published
release files from being replaced.

**Never re-tag a published version at a different commit.** Both registries refuse to replace a
published version. `--skip-duplicate` only makes re-running the *same* commit's release safe; a
moved tag would leave the registries on the old build. Bump the version instead.

**Missing platform packages (#344).** The Marketplace can accept a package and still drop it.
In 0.3.0, `vsce` printed "Published" for all four targets and the job passed, but only the
first target (darwin-arm64) appeared. The other three were uploaded seconds later, while the
Marketplace was still validating the first package of the new version (about 7 minutes). Their
download URLs answered 404, and VSCode on Linux called the extension unsupported on that
platform. So the jobs now:

- publish one target first, wait until the Marketplace lists it as validated, then publish the
  other three;
- after publishing, check that every target is listed and downloadable on the public gallery
  (`scripts/verify-marketplace-listing.py`, read-only, no token). The Open VSX job runs the same
  check against the Open VSX API. Each waits up to 30 minutes.

If a check fails, the error names the missing targets. **Re-run the failed job.**
`--skip-duplicate` skips the targets already published and uploads only the missing ones. To
check a version by hand: `python3 scripts/verify-marketplace-listing.py --version 0.3.0
darwin-arm64 darwin-x64 linux-x64 win32-x64` (add `--registry open-vsx` for Open VSX).

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
