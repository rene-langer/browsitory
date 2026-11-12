### Branch list refresh

- Changed: the branch list and the upstream counts keep each branch's ahead/behind count until
  the branch or its upstream moves, and the upstream reads of a refresh reuse the branch list's
  counts instead of counting every pair again. A refresh with many diverged tracking branches no
  longer holds up the repository's other reads for seconds (#112).
- Fixed: undoing a branch delete puts the branch back into the graph's saved branch selection
  (isolating it again if it was the isolated branch); redoing the delete takes it out again
  (#112).
