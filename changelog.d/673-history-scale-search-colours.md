### History that scales, search in the graph, matching branch colours

- Changed: the commit graph loads older history as you scroll towards the end instead of with
  **Load more**, reads only the next page (from where the last one ended) instead of reloading
  everything, and draws only the rows on screen, so long histories stay fast (#685).
- Changed: a commit search highlights its matches in the graph instead of replacing it: the
  other commits are dimmed, a count shows which match is selected ("3 of 42"), and Enter / F3
  and Shift+Enter / Shift+F3 (or the arrows next to the count) go to the next and previous
  match, loading history or expanding a collapsed merge as needed; **List** shows the matches
  alone as before (#687).
- Changed: a branch's dot in the sidebar and its badges in the graph now have the colour the
  graph draws that branch in; a branch whose latest commit isn't loaded has a grey dot, and
  only the checked-out branch's badge is filled (#673).
