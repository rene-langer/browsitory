### Faster commit search

- Changed: commit search checks each commit with less work: no id formatting or lowercased
  copies for ASCII text, and a `path:` term naming a file or folder compares tree entries
  instead of building a diff for every commit, so a page scans more history (#848).
