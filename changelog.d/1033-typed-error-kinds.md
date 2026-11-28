### Precise next steps for a held lock and a diverged merge

- Changed: when another Git program holds a lock file (`index.lock`, a branch's or the config's),
  the error now says so and what to do: wait for it to finish, or delete the leftover lock file
  if none is running. This also applies to a fetch, pull or push stopped by a lock, and to the
  operation log (#1033).
- Changed: a **Fast-forward only** merge of a branch that has diverged now suggests merging
  without it or rebasing first; other failures of such a merge get the usual next step (#1033).
