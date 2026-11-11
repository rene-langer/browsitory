### Patches: preview, apply and stage, apply as commits

- Added: **Apply patch…** shows a preview first: each mail's subject, author and date, the files
  it changes with their lines added and removed, and why it doesn't apply when it doesn't. From
  there, **Apply**, **Apply and stage**, or **Apply as commits**, which commits each
  `git format-patch` mail with its own author, date and message (you are the committer), all or
  nothing and one Undo step (#658).
- Added: a commit's file menu has **Revert this file and stage** and **Apply this file and
  stage**; Shift+click on **Revert hunk** or **Apply hunk** stages the result too. A file with
  unstaged changes is named in the error instead of a bare "does not match index" (#536).
