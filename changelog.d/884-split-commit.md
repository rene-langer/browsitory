### Split a commit

- Added: **Split commit…** in the commit menu (and the palette) stops at the commit with its
  changes staged, like Edit commit…. Unstage what belongs in a later commit, then **Commit
  part** commits what is staged with the message in the commit area (the original one is
  offered first); repeat for each part. Continue commits what is still staged as the last part
  and replays the commits after it. Every part keeps the commit's author; the status strip says
  "Splitting 1a2b3c4", and the whole split is one Undo step ("Split 1a2b3c4"). Edit commit… has
  Commit part too (#884).
- Changed: Continue at an edit stop no longer carries changes that aren't staged into the
  replay: it asks whether to commit them with the commit, discard them, or cancel (#884).
