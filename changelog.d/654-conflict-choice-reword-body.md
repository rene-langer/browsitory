### Conflict choices and the rebase planner

- Fixed: the Resolve conflict pane chose "ours" for every block on its own, so one click on
  **Save resolution** took your side everywhere. Blocks now start unresolved, the chosen button
  shows as pressed, a **Result** preview shows what the block becomes, a counter reads
  "1 of 3 resolved", and **Save resolution** is enabled only when every block has a choice (#654).
- Changed: the conflict sides are labelled with what they are ("Ours (main)", "Theirs
  (feature)"; during a rebase the base and the rebased branch), use neutral colours instead of
  the diff's addition and deletion colours, and **Previous conflict**/**Next conflict** move
  between blocks (#654).
- Fixed: rewording a commit in the rebase planner dropped its body, because the message field
  held only the summary. Reword now loads the whole message into a multi-line field (#655).
- Fixed: the rebase planner showed nothing while it listed the commits and nothing when that
  failed, with **Start rebase** enabled on an empty plan. It now shows loading, error (with
  **Retry**) and empty states, and Start waits for them (#655).
- Changed: the rebase planner says its rows are oldest first, names each row's controls after its
  commit for screen readers, and moves the row you are in with Alt+Up/Alt+Down (#655).
