### Documentation and user guide

- Added: a user guide website, <https://rene-langer.github.io/browsitory/>, with one page per
  topic, navigation and search. **Open user guide** in the command palette and **User guide**
  in the release notes open it in your browser, in the desktop app and in VSCode (#320).
- Added: the guide now covers the theme switch, the release notes, the update banner,
  **Expand all sections** and **Collapse all sections**, choosing which branches the commit
  graph shows, **Split view**, and the palette's commands and keyboard shortcuts (#320).
- Fixed: the guide described some controls wrongly, for example the upstream buttons at the
  bottom of the Branches section, a "Conflicted" group that doesn't exist, and the names of the
  squash, fixup and drop buttons (#320).
- Changed: "Pull Requests", "Uncommitted Changes", "Release Notes", "New Workspace",
  "Edit Workspace" and "Choose Root Folder" are now written in sentence case, like the rest of
  the app. Recent commands in the palette keep working (#320).
- Fixed: with no GitHub or Bitbucket remote, the Pull requests section pointed to a Remotes
  section that no longer exists. It now points to **Add remote** in the Branches section (#320).
- Changed: the README, the VSCode Marketplace page and the screenshots were rewritten for 0.3.0
  (#320).
- Internal: the screenshots come from `e2e/screenshots/capture.sh`, which builds a fixed demo
  repository and drives the app through the E2E harness. The site is built with MkDocs
  (`mkdocs.yml`, `.github/workflows/docs.yml`); build and contributor notes moved to
  `CONTRIBUTING.md` (#320).
