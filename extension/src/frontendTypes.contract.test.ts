import * as fs from "node:fs";
import * as path from "node:path";
import ts from "typescript";
import { describe, expect, it } from "vitest";

// The extension never imports from `frontend/`, so it keeps its own copies of the message types
// it exchanges with the webview (#1059). This test reads both declarations and fails when they
// drift: a renamed field would compile on both sides and only fail at run time.
const REPO_CLIENT = path.resolve(__dirname, "../../frontend/src/ipc/RepoClient.ts");

/** `name`'s type alias in `file`, printed without comments, one space between tokens. */
function typeAliasText(file: string, name: string): string {
  const source = ts.createSourceFile(file, fs.readFileSync(file, "utf8"), ts.ScriptTarget.Latest, true);
  const alias = source.statements.find(
    (statement): statement is ts.TypeAliasDeclaration => ts.isTypeAliasDeclaration(statement) && statement.name.text === name,
  );
  if (alias === undefined) throw new Error(`${path.basename(file)} declares no type alias ${name}`);
  const printer = ts.createPrinter({ removeComments: true, newLine: ts.NewLineKind.LineFeed });
  return printer.printNode(ts.EmitHint.Unspecified, alias.type, source).replace(/\s+/g, " ").trim();
}

describe("message types the extension copies from the frontend (#1059)", () => {
  it.each([
    ["HostRequest", "hostRequests.ts"],
    ["DiffSource", "toolUris.ts"],
  ])("%s in %s matches RepoClient.ts", (name, file) => {
    const copy = typeAliasText(path.join(__dirname, file), name);
    const original = typeAliasText(REPO_CLIENT, name);
    expect(copy, `extension/src/${file}'s ${name} differs from frontend/src/ipc/RepoClient.ts's`).toBe(original);
  });
});
