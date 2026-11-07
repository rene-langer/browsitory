### Stash all with a case-only rename

- Fixed: on macOS, Windows or with `core.ignorecase`, **Stash all** with a staged rename that
  changes only the case of a name (`Old` to `old`) wrote a stash that didn't undo the rename
  (Linux), or likely refused with a misleading message about an ignored file. It now refuses
  before writing anything, like **Stash file…**: "Can't stash 'old': it differs from 'Old' only
  in case. Commit the rename first, or undo it." (#370)
