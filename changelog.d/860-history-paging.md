### File and line history on large repositories

- Changed: file and line history keep their place between loads. "Load more" goes on from
  where the last load stopped instead of searching from the newest commit again, and a refresh
  while HEAD hasn't moved (after a fetch, say) answers at once instead of searching again
  (#860).
- Changed: a long search no longer holds up other actions. It runs a quarter of a second at a
  time, the changes found so far show at once, and the list's footer reads "Searching older
  commits… (N changes so far)" until a page of 300 is found or the history ends. Closing the
  history stops the search (#860).
- Fixed: "Load more" no longer appears under a history of exactly 300 changes (#860).
