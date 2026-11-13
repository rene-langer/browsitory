import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { gitIn } from "../support/git";

// User-defined commands (#893–#896): a command defined in the app's settings, set for the commit
// menu, runs as a program with `${commit}` as one argument and writes it to a file. The command
// is a shell script run without a shell in between (`sh -c '…' sh ${commit} <file>`), so it is
// skipped on Windows.
const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const CONFIG_FILE = path.join(os.tmpdir(), "browsitory-e2e-config", "config.toml");
const OUTPUT = path.join(os.tmpdir(), "browsitory-e2e-user-command.txt");
const NAME = "E2E write commit";
const git = gitIn(E2E_REPO_PATH);

const COMMAND_TABLE = `
[[user_commands]]
id = "e2e-write-commit"
name = "${NAME}"
command_line = "sh -c 'printf %s \\"$1\\" > \\"$2\\"' sh \${commit} ${OUTPUT}"
targets = ["commit"]
show_output = false
refresh = false
`;

/** Opens the commit's menu (a synthetic `contextmenu`, as history-rewrite.spec.ts does) until `label` can be picked. */
async function chooseFromCommitMenu(commitId: string, label: string) {
  await browser.waitUntil(
    () =>
      browser.execute(
        (id: string, wanted: string) => {
          const item = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="menu"] button')).find(
            (button) => !button.disabled && (button.textContent ?? "").trim() === wanted,
          );
          if (item !== undefined) {
            item.click();
            return true;
          }
          document
            .querySelector(`li[data-commit-id="${id}"]`)
            ?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
          return false;
        },
        commitId,
        label,
      ),
    { timeoutMsg: `could not pick "${label}" from the menu of commit ${commitId}` },
  );
}

describe("Browsitory user commands", () => {
  let original = "";

  before(function () {
    if (process.platform === "win32") this.skip();
    original = fs.readFileSync(CONFIG_FILE, "utf8");
    // Arrays of tables go last in the file, as the app writes them.
    fs.writeFileSync(CONFIG_FILE, `${original.trimEnd()}\n${COMMAND_TABLE}`);
    fs.rmSync(OUTPUT, { force: true });
  });

  after(() => {
    if (process.platform === "win32") return;
    fs.writeFileSync(CONFIG_FILE, original);
    fs.rmSync(OUTPUT, { force: true });
  });

  it("runs a command from the commit menu with the commit as its argument", async () => {
    const head = git("rev-parse", "HEAD").trim();
    // The workspace loads the commands when it mounts: reload so it reads the new table.
    await browser.refresh();
    await chooseFromCommitMenu(head, NAME);
    await browser.waitUntil(() => fs.existsSync(OUTPUT) && fs.readFileSync(OUTPUT, "utf8") === head, {
      timeoutMsg: `the command did not write ${head} to ${OUTPUT}`,
    });
    expect(fs.readFileSync(OUTPUT, "utf8")).toBe(head);
  });
});
