### Windows: FAT32, exFAT and ReFS drives

- Fixed: on a FAT32 or exFAT drive (a USB stick, an SD card), changing files in a working tree
  failed with "The parameter is incorrect." (#228).
- Fixed: on ReFS and Dev Drive, rewriting a file dropped its "no scrub data" attribute (#228).
- Internal: Windows CI tests on FAT32, exFAT and ReFS volumes, and tests that swap a link in or
  rename the file between classifying a leaf and reopening it (#228).
