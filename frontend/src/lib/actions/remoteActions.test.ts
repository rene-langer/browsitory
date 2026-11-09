import { describe, expect, it } from "vitest";
import { addRemoteAction, remoteActions } from "./remoteActions";

describe("remoteActions", () => {
  it("labels each action for the menu and, naming the remote, for the palette", () => {
    const actions = remoteActions({ blocked: false, reason: null }, "fork");
    expect(Object.values(actions).map((action) => [action.label, action.paletteLabel, action.enabled])).toEqual([
      ["Fetch", "Fetch fork", true],
      ["Push current branch here", "Push to fork", true],
      ["Edit remote", "Edit remote fork…", true],
      ["Manage credentials", "Manage credentials for fork…", true],
      ["Remove remote", "Remove remote fork…", true],
    ]);
  });

  it("disables every action with the busy reason", () => {
    for (const action of Object.values(remoteActions({ blocked: true, reason: "A transfer is in progress." }, "fork"))) {
      expect(action).toMatchObject({ enabled: false, reason: "A transfer is in progress." });
    }
  });

  it("always offers Add remote, whose form carries the busy rule", () => {
    expect(addRemoteAction()).toEqual({ label: "Add remote…", paletteLabel: "Add remote", enabled: true, reason: undefined });
  });
});
