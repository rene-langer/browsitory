### Internal

- Changed: the changelog check (pre-push hook and CI) requires an entry when a `package.json`
  under `extension/` or `e2e/` changes outside its dependency fields, such as the extension's
  `contributes` block; Renovate's dependency-only changes stay exempt (#1094).
- Changed: Renovate proposes tauri-driver updates in their own pull request instead of the Tauri
  group (#1095).
