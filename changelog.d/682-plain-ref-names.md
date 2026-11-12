### Plain names instead of Git syntax

- Changed: a diff's hunk headers read "Lines 12–19" (or "Removed lines 4–6") instead of git's
  "@@ -10,6 +12,8 @@"; the tooltip names the lines before and after the change (#682).
- Changed: the Reflog panel names branches as the branch list does, "main" rather than
  "refs/heads/main", in its picker, Restore buttons and confirmation (#682).
- Changed: removing a remote that a branch still tracks names those branches in plain words
  instead of a bracketed list (#682).
