### Collapse merges in the commit graph

- Added: **Collapse merges** (a toggle in the search bar above the graph and a palette command,
  saved per repository) makes the graph follow only first parents from every branch it shows.
  Each merge row gets a **+N** control with the number of commits it brought in; it expands them
  right below the merge, with their lanes, and **−N** hides them again (→ and ← from the
  keyboard, **More** for a long side branch). Off by default (#617).
- Added: Go to commit and the parent key expand the collapsed merge that holds a commit instead
  of saying the graph does not show it; commit search still finds those commits. With a single
  isolated branch, merge rows expand the same way (#617).
