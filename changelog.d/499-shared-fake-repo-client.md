### Internal: one shared fake RepoClient for frontend tests

- Changed: frontend tests build their `RepoClient` fakes with one `Proxy`-based helper,
  `frontend/src/test/fakeRepoClient.ts`, instead of five complete hand-written copies, 21
  inline ones in the `useAppState` tests and about 20 partial casts. A method a test doesn't
  override fails with `not used in this test: <method>`, or answers an empty value in `"empty"`
  mode, so a new `RepoClient` method needs no edit in the tests (#499).
