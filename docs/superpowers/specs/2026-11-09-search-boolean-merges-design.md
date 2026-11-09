# Search: boolean operators and merge filters (#450)

## Grammar

Parsing stays in `git-core::search::parse_query`; the backend is the only source of truth for
what a query means, and the frontend never parses one.

```
query := or
or    := and ("or" and)*
and   := unary (["and"] unary)*
unary := ("not" | "-") unary | "(" or ")" | term
term  := key ":" value | value
```

- `and`/`or`/`not` are operators only as whole, unquoted words (any case). `-` negates only at
  the start of a token. `(` starts a group only at the start of a token; inside a bare value a
  `(` keeps its matching `)`, so `fix(ui)` stays one word while `(a or b)` closes the group.
- The parser produces `SearchExpr` (`Term`, `Not`, `And`, `Or`), flattening nested `And`s and
  `Or`s, so a query without operators is the same `And` of terms as before.
- New key `merges:only|none` (`SearchTerm::Merges`): parent count > 1 or ≤ 1.

## Terms that are not per-commit tests

- `branch:` picks the walk's start points. It must be a top-level AND term (at most one);
  under `not` or `or` it is rejected.
- `line:` is a set computed once per page (line history of the range); as a set-membership test
  it is a per-commit predicate, so `not`/`or` apply to it like any other term.
- `contents:` keeps looking only at the files of the `path:` terms that are top-level AND terms,
  as before. A `path:` inside `or`/`not` is an independent test and scopes nothing.

## Evaluation

`prepare` turns the expression into a tree of prepared terms; every `And`/`Or` orders its parts
by the most expensive term inside (line set/SHA/merges, dates, signatures, message, path diff,
contents diff), and evaluation short-circuits. The first-parent diff shared by `path:` and
`contents:` is still built at most once per commit.

## Errors

`SearchError::InvalidQuery` now displays as `Invalid search: <message>`. `ErrorKind::of` maps
that to a new `ErrorKind::InvalidSearchQuery`, so the frontend can show it under the search
field (`aria-invalid`, `role="alert"`) instead of as a results error with Retry. The wire DTO
change is only the new enum variant; generated files are regenerated.

## Out of scope

A recent-search dropdown (follow-up issue).
