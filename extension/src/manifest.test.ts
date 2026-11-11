import { readFileSync } from "node:fs";
import * as path from "node:path";
import { describe, expect, it } from "vitest";

interface Manifest {
  activationEvents: string[];
  contributes: {
    commands: Array<{ command: string; title: string }>;
    keybindings: Array<{ command: string; key: string; mac?: string; when?: string }>;
    menus: Record<string, Array<{ command: string; when?: string; group?: string }>>;
    configuration: { properties: Record<string, { type: string; default: unknown }> };
  };
}

const manifest = JSON.parse(readFileSync(path.join(__dirname, "..", "package.json"), "utf8")) as Manifest;

describe("extension/package.json's integration points (#759)", () => {
  it("binds Browsitory: Open to Ctrl+K Ctrl+G (Cmd+K Cmd+G on macOS)", () => {
    expect(manifest.contributes.keybindings).toContainEqual({
      command: "browsitory.open",
      key: "ctrl+k ctrl+g",
      mac: "cmd+k cmd+g",
      // Inside the panel, Ctrl/Cmd+K is the app's own command palette (#679).
      when: "!browsitory.webviewFocused",
    });
  });

  it("offers Show Commit in Browsitory on the Timeline's commits only", () => {
    expect(manifest.contributes.commands).toContainEqual({ command: "browsitory.showCommit", title: "Show Commit in Browsitory" });
    expect(manifest.contributes.menus["timeline/item/context"]).toEqual([
      {
        command: "browsitory.showCommit",
        when: "timelineItem =~ /git:(file|folder|workspace):commit\\b/ && !listMultiSelection",
        group: "browsitory@1",
      },
    ]);
    expect(manifest.contributes.menus["commandPalette"]).toContainEqual({ command: "browsitory.showCommit", when: "false" });
  });

  it("starts after VSCode does, for the status bar button, which a setting hides", () => {
    expect(manifest.activationEvents).toContain("onStartupFinished");
    expect(manifest.contributes.configuration.properties["browsitory.statusBar.enabled"]).toMatchObject({
      type: "boolean",
      default: true,
    });
  });
});
