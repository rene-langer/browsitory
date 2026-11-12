import fs from "node:fs";
import os from "node:os";
import path from "node:path";

// A fake external tool for `external-tools.spec.ts` (#128): one script that stands in for the
// editor and the git diff tool and records each launch to a log, so the spec can check what the
// app ran without any real tool.
//
// The diff tool comes from git's global config, which the app's libgit2 reads from $HOME. So the
// session running that spec gets its own HOME (`wdio.conf.ts`'s `beforeSession`), holding a
// `.gitconfig` that names this tool: the developer's own git config is never read, and no
// real diff tool can open.

export const FAKE_TOOL_HOME = path.join(os.tmpdir(), "browsitory-e2e-tools-home");
export const FAKE_TOOL = path.join(FAKE_TOOL_HOME, "fake-tool.sh");
export const FAKE_TOOL_LOG = path.join(FAKE_TOOL_HOME, "launches.log");
export const FAKE_DIFF_TOOL_NAME = "e2efake";

const LAUNCH_MARKER = "--- launch";
const END_MARKER = "--- end";

/** Whether a session runs the spec that needs the fake tool's HOME. */
export function needsFakeToolHome(specs: string[]): boolean {
  return specs.some((spec) => spec.endsWith("external-tools.spec.ts"));
}

/**
 * A fresh HOME with the fake tool and a git config whose `diff.tool` runs it. `cmd` runs
 * through `sh -c` with LOCAL/REMOTE in the environment, as git difftool runs it.
 */
export function prepareFakeToolHome(): void {
  fs.rmSync(FAKE_TOOL_HOME, { recursive: true, force: true });
  fs.mkdirSync(path.join(FAKE_TOOL_HOME, ".config"), { recursive: true });
  // Arguments one per line; for a diff, the LOCAL side's contents too, since the side files are
  // removed once the tool exits. Each launch is built in its own file and appended to the log
  // whole, ending in END_MARKER, so the spec never reads a launch the tool is still writing
  // (#916).
  fs.writeFileSync(
    FAKE_TOOL,
    [
      "#!/bin/sh",
      `log='${FAKE_TOOL_LOG}'`,
      'launch="$log.$$"',
      "{",
      `  echo "${LAUNCH_MARKER}"`,
      '  for arg in "$@"; do echo "arg: $arg"; done',
      '  if [ -n "$LOCAL" ]; then echo "LOCAL contents:"; cat "$LOCAL"; echo "REMOTE: $REMOTE"; fi',
      `  echo "${END_MARKER}"`,
      '} > "$launch"',
      'cat "$launch" >> "$log"',
      'rm -f "$launch"',
      "",
    ].join("\n"),
    { mode: 0o755 },
  );
  fs.writeFileSync(
    path.join(FAKE_TOOL_HOME, ".gitconfig"),
    `[diff]\n\ttool = ${FAKE_DIFF_TOOL_NAME}\n` +
      `[difftool "${FAKE_DIFF_TOOL_NAME}"]\n\tcmd = '${FAKE_TOOL}' diff \\"$LOCAL\\" \\"$REMOTE\\"\n`,
  );
}

/**
 * The launches recorded so far, each as its log lines without the markers. A launch without its
 * end marker is still being appended and is left out.
 */
export function fakeToolLaunches(): string[][] {
  if (!fs.existsSync(FAKE_TOOL_LOG)) return [];
  return fs
    .readFileSync(FAKE_TOOL_LOG, "utf8")
    .split(`${LAUNCH_MARKER}\n`)
    .map((launch) => launch.split("\n").filter((line) => line !== ""))
    .filter((lines) => lines.at(-1) === END_MARKER)
    .map((lines) => lines.slice(0, -1));
}
