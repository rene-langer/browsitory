### Command palette: commits beyond the loaded history

- Fixed: a commit selected in the search results or a file history that the graph hasn't loaded
  yet got none of the commit commands in the palette. Cherry-pick, Revert, Reset, Checkout and
  Create tag are now offered for it, with the same availability as the commit menu (#515).
