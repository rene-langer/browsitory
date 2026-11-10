### Theme and contrast

- Fixed: the light theme failed the WCAG AA contrast levels in many places: the accent as text,
  pressed toggles, branch badges, the Commit button, muted text, comments in code, placeholders
  and input borders. Both themes now meet WCAG 2.2 AA (4.5:1 for text, 3:1 for controls and the
  focus ring), and a selected row carries a marker besides its tint (#666).
- Added: forced-colours (high contrast) support: the system's colours, with the focus ring,
  selection and pressed toggles kept visible (#671).
- Changed: in VSCode the panel follows the editor's theme (light, dark and high contrast) and its
  fonts; the theme button is gone there (#676).
- Added: Preferences > Appearance > Theme: System, Light or Dark. System is the default and
  follows the system's setting while Browsitory runs (#691).
- Internal: every colour is a token in `styles/tokens.css`, and a test checks the contrast of the
  token pairs and that no stylesheet or script writes a colour literal (#666).
