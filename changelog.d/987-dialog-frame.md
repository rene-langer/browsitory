### Dialogs

Every dialog now has the same frame: a bold title, its buttons at the right of one row, and the
button that does the work highlighted.

- Changed: every dialog shows its title, and the dialog is named by it; Edit remote, Push, Set
  upstream, Credentials, Stash, Identity and every confirmation had none (#987).
- Changed: buttons sit at the right of one row in every dialog, the main one first and
  highlighted like the Commit button, Cancel last (#987).
- Changed: red marks only actions that lose data, in menus, panes and dialogs alike, with an
  outline and bold text, not colour alone. Mark resolved, Run, Open, Restore and Commit without
  hooks are no longer red; Reset to here, Discard all…, Delete untracked…, a stash's Drop… and
  the Abort buttons now are. Reset's button names the mode ("Reset hard") (#988).
- Changed: confirmations say what is lost and whether Undo brings it back. Force push says which
  commits the remote branch loses, that the lease refuses it if the branch moved, and that Undo
  can't take back a push; Remove remote says its remote-tracking branches go and Undo can't
  bring it back (its button is "Remove remote"); Delete tag says a copy on a remote stays and
  Undo brings the tag back (#989).
- Changed: Set upstream opens with the only remote (or origin) and the branch's own name filled
  in. Identity, Create tag, Delete remote branch and Set upstream say under the form why their
  button is off. Edit remote's Push URL says "Same as fetch URL" when empty and its copy buttons
  read "Copy". Credentials offers Forget only for a saved credential, and Cancel is plain
  "Cancel" (#1014).
- Changed: the keyboard shortcut sheet has padding, key chips, a key column as wide as its
  longest keys, and keeps Change shortcuts… and Close in view while the list scrolls (#1016).
- Changed: Merge's options are under "How to merge", and the Squash hint sits under its checkbox
  and says the choices above don't apply. Reset asks "What happens to your changes", with each
  explanation on its own line (#1018).
- Changed: a diverged pull's dialog is titled "main and origin/main have diverged" with the
  commit counts, its buttons are "Pull with merge" and "Pull with rebase" as in the Pull menu,
  and its checkbox reads "Always do this for ⟨repository⟩ (change it in the Pull menu)" (#1019).
- Changed: the rebase planner's title names the branch and the target ("Rebase main onto
  4c5beaf"), a line says how many commits it rewrites and that Undo restores the branch, the
  rows line up under one "Action" header, and the move buttons are arrows (#1015).
- Fixed: the Clone dialog's fields and Cancel button ran past its right edge (#974).
