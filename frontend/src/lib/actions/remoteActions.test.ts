import { describe, expect, it } from "vitest";
import { addRemoteAction, remoteActions } from "./remoteActions";

describe("remoteActions", () => {
  it("labels each action for the menu and, naming the remote, for the palette", () => {
    const actions = remoteActions({ blocked: false, reason: null }, "fork", "main");
    expect(Object.values(actions).map((action) => [action.label, action.paletteLabel, action.enabled])).toEqual([
      ["Fetch", "Fetch fork", true],
      ["Push current branch to fork", "Push current branch to fork", true],
      ["Edit remote…", "Edit remote fork…", true],
      ["Manage credentials…", "Manage credentials for fork…", true],
      ["Remove remote…", "Remove remote fork…", true],
    ]);
  });

  it("disables every action with the busy reason", () => {
    for (const action of Object.values(remoteActions({ blocked: true, reason: "A transfer is in progress." }, "fork", "main"))) {
      expect(action).toMatchObject({ enabled: false, reason: "A transfer is in progress." });
    }
  });

  // #1119: the backend pushes the checked-out branch, so a detached HEAD has nothing to push.
  it("refuses Push current branch on a detached HEAD, saying why", () => {
    const actions = remoteActions({ blocked: false, reason: null }, "fork", null);
    expect(actions.pushCurrentBranch).toMatchObject({ enabled: false, reason: "Check out a branch first." });
    for (const action of [actions.fetch, actions.edit, actions.credentials, actions.remove]) {
      expect(action.enabled).toBe(true);
    }
  });

  it("gives the busy reason first on a detached HEAD", () => {
    expect(remoteActions({ blocked: true, reason: "A transfer is in progress." }, "fork", null).pushCurrentBranch).toMatchObject({
      enabled: false,
      reason: "A transfer is in progress.",
    });
  });

  it("always offers Add remote, whose form carries the busy rule", () => {
    expect(addRemoteAction()).toEqual({ label: "Add remote…", paletteLabel: "Add remote…", enabled: true, reason: undefined });
  });
});
