### Internal: collapse merges tests

- Internal: tests for the "Collapse merges" code the mutation job found untested in #716: the
  commit time order of a merge's merged commits, an octopus merge holding a hidden commit, a
  commit that is already a row of the collapsed graph, and the binary search over a first-parent
  line at every position, guarded so a search that never ends fails instead of hanging (#617).
