### Internal

- Fixed: the desktop E2E test that opens Edit remote from the command palette could press Enter
  before the app had listed the remote the test added, so nothing ran and the palette stayed open.
  It now waits until the palette highlights that command (#434).
