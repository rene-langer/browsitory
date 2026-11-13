### Internal: Windows walker coverage

- Added: Windows CI tests on FAT32, exFAT and ReFS volumes, and tests that swap a link in or
  rename the file between classifying a leaf and reopening it, through the real read and write
  paths (#228).
