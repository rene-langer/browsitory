### Signing programs and credentials

- Security: selecting a signed commit no longer runs a program that the repository's own
  `.git/config` names. `gpg.program`, `gpg.openpgp.program`, `gpg.ssh.program` and
  `gpg.ssh.allowedSignersFile` now count only from your own git config (global, XDG or system),
  for checking signatures and for signing. When only the repository sets a program, the
  signature shows "Signature could not be checked" and signing fails, both saying which setting
  to move to your global config; an allowed signers file set only there is ignored (#1121).
- Fixed: the operation log, the log file, VS Code's output channel and "Copy details" kept a URL's
  password when it held `'`, `(`, `)`, `,`, `[`, `]` or a raw `/` (#1125).
- Fixed: the token typed in the clone dialog was sent to another host when the server redirected
  the clone there. It now goes only to the host and port of the clone URL (#1126).
