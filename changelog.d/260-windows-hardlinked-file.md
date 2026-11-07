### Hard-linked files on Windows

- Fixed: on Windows, Save resolution, Add to .gitignore and Undo/Redo failed with "a folder of the
  working tree changed while it was being written" for a tracked file that is a hard link to a
  file in another folder, and Mark resolved and the image preview couldn't read it. Windows may report
  the file's other name, so it looked moved. Such a file is now written like any other: it is
  replaced by a new file, as git's checkout does, and the other name keeps its old content. A
  folder replaced by a junction or symbolic link while it is written is still refused (#260).
