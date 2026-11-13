### User-defined commands

- Added: your commands appear in the commit, branch, tag, file and repository tab menus they are
  set for, last in a group of their own, and in the command palette as "Run: <name>" (#895). A
  command whose variables can't be filled where it is offered (`${commit}` with no commit
  selected, say) is greyed out with the reason. It asks first when it is set to, shows its output
  in a window with Stop while it runs (or a toast with Show output when it fails), and refreshes
  the repository afterwards when set to.
