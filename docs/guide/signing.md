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
  applies to OpenPGP only. For SSH use `gpg.ssh.program`. Set them in your global git config
  (`git config --global …`): Browsitory ignores a signing program set in a repository's own
  config, see below.

If you see "Signing program 'gpg' was not found", Browsitory may not have inherited your shell's
`PATH` (for example when launched from the macOS Dock, or with Git for Windows' gpg). Set
`gpg.program` to an absolute path in your global git config.

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
- **Signature could not be checked**: for example gpg is not installed. Hover over it for the
  reason.

??? note "Details: settings a repository can't choose"
    Browsitory checks a signature as soon as you select the commit, by running gpg or
    ssh-keygen. A repository's own config (`.git/config`) comes with the folder when you get a
    repository as an archive or on a shared drive, so whoever made it could otherwise make
    Browsitory run a program of their choice just by your looking at a commit. So these
    settings count only from your own git config (global, XDG or system), never from the
    repository's:

    - `gpg.program`, `gpg.openpgp.program` and `gpg.ssh.program`. When only the repository sets
      one, Browsitory runs nothing: the signature shows "Signature could not be checked" and
      committing with signing on fails, both saying which setting to move to your global config.
      When your own config sets one too, yours is used.
    - `gpg.ssh.allowedSignersFile`. When only the repository sets it, an SSH signature that
      checks out shows "Valid signature, signer not trusted", so a repository can't vouch for
      its own signers.

    The other signing settings (`commit.gpgSign`, `gpg.format`, `user.signingKey`) still count
    from the repository's config: they only choose between gpg and ssh-keygen and which of your
    keys signs. A repository you cloned has no settings of its author: `git clone` doesn't copy
    the config.
