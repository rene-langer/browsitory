# Releasing

How Browsitory versions and publishes releases. Moved out of `AGENTS.md` so the agent
instructions stay short; `AGENTS.md` keeps the rules an agent must not break.

## Versioning & releases

Semantic versioning. Pushing a `release/MAJOR.MINOR.PATCH` tag (no `v` prefix) triggers
`.github/workflows/release.yml`: builds standalone distributables for Linux/macOS/Windows
(`tauri-apps/tauri-action`, same build as `scripts/build-dist.*`) and publishes a draft GitHub
Release. The workflow stamps the version into `crates/tauri-app/tauri.conf.json` at build time.
Every installer carries `THIRD_PARTY_NOTICES.md` (a Tauri resource), and
`publish-vsix-release` also attaches it to the draft release. It is generated and checked in
CI, so a release needs no extra step for it (see `docs/LICENSE_COMPLIANCE.md`).
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
4. First release: the Marketplace job prints a notice with the Entra app's Marketplace member
   id (`az rest -u https://app.vssps.visualstudio.com/_apis/profile/profiles/me --resource
   499b84ac-1321-427f-aa17-267ca6975798 --query id -o tsv`). Until that id is a member, the
   publish step fails. Add the id to publisher `browsitory` as **Contributor**, then re-run the
   failed job. Later releases need nothing.
