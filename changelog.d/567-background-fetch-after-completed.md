### Fetch automatically

- Fixed: a background fetch that was due while another fetch, pull or push ran could be skipped
  until the next interval, when it started the moment that operation finished: the finished
  operation still counted as running for a moment after it reported its end. This also made a
  repo-service test fail now and then on Windows (#567).
