### User-defined commands

- Added: in VS Code, your commands run only in a trusted workspace (#896). In Restricted Mode
  they are left out of the menus and the palette lists them as unavailable, saying why; the
  extension never passes a run to the backend there. A new guide page, "Your own commands",
  explains the variables with examples.
