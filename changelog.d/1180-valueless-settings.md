### Settings written without a value

- Fixed: Delete branch and Rename branch crashed the app's backend (or the VS Code extension's
  helper) when the branch had a setting written without a value (a line such as `flag` alone in
  `.git/config`), and so did Rename remote and Remove remote for such a setting of the remote, and
  a failed worktree add removing its new branch. Delete and Remove now remove the setting as git
  does; Rename carries it over as `true`, which is what git reads it as (#1180).
- Fixed: Rename remote, Remove remote and Add remote crashed on a `branch.<name>.remote` or a
  left-over `remote.<name>.fetch` written without a value; they now refuse and name the setting,
  as git does. A setting with several values, one of them without a value, is refused the same
  way by the branch and remote commands (#1180).
