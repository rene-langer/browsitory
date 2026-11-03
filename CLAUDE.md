@AGENTS.md

# Claude Code

`AGENTS.md` (imported above) is the shared guidance for every coding agent; put project rules
there, not here. This file holds only what is specific to Claude Code.

- Project skills live in `.claude/skills/` (`browsitory-conventions`, `graphify`).
- graphify is opt-in: run `/graphify` yourself when you want a graph. Its output
  (`graphify-out/`) is local and ignored by git, and no hook redirects tool calls to it. To wire
  graphify hooks into your own sessions, put them in `.claude/settings.local.json` (ignored).
