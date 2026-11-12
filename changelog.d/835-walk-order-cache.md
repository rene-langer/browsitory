### Faster history paging

- Changed: the commit graph's later pages, "Go to" and further search pages no longer sort the
  whole history again: the worker keeps the walk order and the branch and tag badges until a
  ref or the selection changes, so scrolling through a large repository costs milliseconds a
  page instead of a full walk each (#835).
