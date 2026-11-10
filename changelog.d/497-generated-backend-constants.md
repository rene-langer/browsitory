### Internal: backend limits and defaults generated for the frontend

- Changed: the frontend's diff context limits (`MAX_CONTEXT_LINES`, `WHOLE_FILE_CONTEXT_LINES`),
  the commit message column range, the default diff, commit message and fetch settings, and the
  commit search keys now come from `frontend/src/ipc/generated/wire-constants.ts`, which
  `wire_bindings_are_current` writes from the Rust constants and `Default` impls and checks for
  freshness. The `config` crate uses `git-core`'s `MAX_CONTEXT_LINES` instead of its own copy,
  the Preferences column fields take their range from the generated constant, and a test checks
  the search help's keys against the parser's. Behaviour is unchanged (#497).
