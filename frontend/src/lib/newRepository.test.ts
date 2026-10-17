import { describe, expect, it, vi } from "vitest";
import { CloneError, type RepoClient } from "../ipc/RepoClient";
import { cloneToastMessage, createNewRepository } from "./newRepository";

function client(picked: string | null, init: () => Promise<void>) {
  return { pickRepoFolder: vi.fn(async () => picked), initRepo: vi.fn(init) } as unknown as RepoClient;
}

describe("createNewRepository", () => {
  it("is cancelled when no folder is picked", async () => {
    const c = client(null, async () => {});
    expect(await createNewRepository(c)).toEqual({ kind: "cancelled" });
    expect(c.initRepo).not.toHaveBeenCalled();
  });

  it("inits the picked folder", async () => {
    expect(await createNewRepository(client("/src/new", async () => {}))).toEqual({ kind: "created", path: "/src/new" });
  });

  it("reports an existing repository instead of failing", async () => {
    const c = client("/src/old", async () => {
      throw new CloneError("AlreadyRepository", "already");
    });
    expect(await createNewRepository(c)).toEqual({ kind: "exists", path: "/src/old" });
  });

  it("rethrows other failures", async () => {
    const c = client("/src/x", async () => {
      throw new CloneError("InitFailed", "disk full");
    });
    await expect(createNewRepository(c)).rejects.toThrow("disk full");
  });
});

describe("cloneToastMessage", () => {
  const ok = { path: "/p", submoduleFailures: [], credentialSaveFailed: false };
  it("names the repository", () => expect(cloneToastMessage(ok, "repo")).toBe("Cloned repo"));
  it("counts failed submodules", () =>
    expect(cloneToastMessage({ ...ok, submoduleFailures: ["a", "b"] }, "repo")).toBe("Cloned repo — 2 submodules failed to update"));
  it("mentions an unsaved token", () =>
    expect(cloneToastMessage({ ...ok, credentialSaveFailed: true }, "repo")).toBe("Cloned repo, but the token couldn't be saved."));
});
