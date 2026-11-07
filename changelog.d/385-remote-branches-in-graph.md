### Remote branches in the graph

- Fixed: turning on a remote branch's dot in the Branches section (**Show ⟨remote⟩/⟨branch⟩ in
  graph**) only toggled its badge; it now adds the branch's commits to the graph and to commit
  search, also those no local branch has. Until you pick remote branches the graph is unchanged
  (#385).
- Added: **Isolate branch** on a remote branch. A single branch in the graph, local or remote,
  shows its first-parent line (#385).
