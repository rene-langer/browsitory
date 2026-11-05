# Signing commits

Browsitory signs commits the way `git commit` does, from your git config. It has no setting of
its own:

- `commit.gpgSign = true` turns signing on.
- `gpg.format` picks `openpgp` (default, uses `gpg`) or `ssh` (uses `ssh-keygen`). `x509` is not
  supported.
- `user.signingKey` picks the key. For gpg it defaults to your committer identity. For SSH it is
  required: either a key file path or a literal public key (`key::ssh-ed25519 …`) whose private
  key is in your ssh-agent.
- Program overrides: for OpenPGP, `gpg.openpgp.program` wins over `gpg.program`; `gpg.program`
  applies to OpenPGP only. For SSH use `gpg.ssh.program`.

If you see "Signing program 'gpg' was not found", Browsitory may not have inherited your shell's
`PATH` (for example when launched from the macOS Dock, or with Git for Windows' gpg). Set
`gpg.program` to an absolute path.

Passphrases come from gpg-agent's pinentry or from ssh-agent. Browsitory cannot prompt in a
terminal. If signing fails, no commit is created and the signer's error is shown. While the
signing program runs, the commit box says "Waiting for the signing program…" with **Cancel**;
after 2 minutes Browsitory stops it and says so (unlock your key and commit again). Checking a
signature in the history gives up after 30 seconds and shows "Signature could not be checked".

Selecting a signed commit shows a **Signature** row in the commit details:

- **Good signature**: valid, from a trusted key (gpg) or a signer listed in
  `gpg.ssh.allowedSignersFile` (SSH).
- **Valid signature, signer not trusted**: cryptographically valid, but the key is not trusted
  or not listed.
- **Signed with an unknown key**: the public key is not in your keyring.
- **Signature expired** / **Signed with an expired key** / **Signed with a revoked key**.
- **Bad signature**: the commit does not match its signature.
- **Signature could not be checked**: for example gpg is not installed.
