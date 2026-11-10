### Privacy notice

- Added: a privacy notice in the user guide (`docs/guide/privacy.md`, linked from `PRIVACY.md`
  and both READMEs). Browsitory collects no data; the notice lists every connection the app
  makes, the data it keeps on your computer and how to delete it (#627).
- Changed: the user guide site no longer loads its fonts from Google Fonts; it uses your
  system's fonts, so visiting it no longer contacts Google (#627).
- Internal: CI's `audit` job fails when the code names a host the privacy notice doesn't list
  (`scripts/check-privacy-hosts.py`, #627).
