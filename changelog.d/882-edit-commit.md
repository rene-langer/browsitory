### Edit an older commit

- Added: **Edit commit…** in the commit menu (and the palette) stops at the commit with its
  changes staged on its parent. The status strip says "Editing 1a2b3c4: change the files, then
  Continue", and the commit area offers the commit's message, editable. Continue commits the
  staged result with that message and the commit's author, then replays the commits after it;
  Abort puts the branch back. A paused edit survives a restart, and the whole edit is one Undo
  step ("Edit 1a2b3c4") (#882).
