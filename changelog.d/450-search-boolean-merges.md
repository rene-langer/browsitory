### Search: or, not, parentheses and merge filters

- Added: commit search understands `or`, `not` (or a leading `-`) and parentheses between terms;
  terms side by side still all have to match (#450).
- Added: `merges:only` and `merges:none` list only merge commits or leave them out (#450).
- Changed: a search that can't be read shows why under the search field, which is marked
  invalid, instead of in place of the results (#450).
- Internal: a new `InvalidSearchQuery` error kind tells an invalid query apart from other
  search failures (#450).
