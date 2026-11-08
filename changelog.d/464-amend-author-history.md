### Amend author with file history open

- Fixed: with a file history or search results shown in the graph, amending left the **Author**
  field empty when HEAD wasn't among those results. It now takes HEAD's author from the graph's
  commits (#464).
