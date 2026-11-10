import { describe, expect, it } from "vitest";
import { fakeRepoClient } from "./fakeRepoClient";

describe("fakeRepoClient", () => {
  it("answers an overridden method with the override", async () => {
    const client = fakeRepoClient({ listBranches: async () => [{ name: "main", isCurrent: true }] });
    await expect(client.listBranches("/repo")).resolves.toEqual([{ name: "main", isCurrent: true }]);
  });

  it("rejects any other method, naming it", async () => {
    const client = fakeRepoClient();
    await expect(client.listTags("/repo")).rejects.toThrow("not used in this test: listTags");
  });

  it("throws synchronously from an unused subscription, which returns no promise", () => {
    expect(() => fakeRepoClient().subscribeHookProgress(() => {})).toThrow("not used in this test: subscribeHookProgress");
  });

  it("answers the empty value in empty mode, and still rejects a method that has none", async () => {
    const client = fakeRepoClient({}, "empty");
    await expect(client.listTags("/repo")).resolves.toEqual([]);
    const unsubscribe = client.subscribeHookProgress(() => {});
    expect(unsubscribe()).toBeUndefined();
    await expect(client.getStatus("/repo")).rejects.toThrow("not used in this test: getStatus");
  });

  it("keeps supportsCloneAndInit off unless overridden", () => {
    expect(fakeRepoClient().supportsCloneAndInit).toBe(false);
    expect(fakeRepoClient({ supportsCloneAndInit: true }).supportsCloneAndInit).toBe(true);
  });

  it("returns the same stub on every read, as a real client returns the same method", () => {
    const client = fakeRepoClient();
    expect(client.getStatus).toBe(client.getStatus);
  });

  it("is not a thenable, so awaiting or resolving it yields the client itself", async () => {
    const client = fakeRepoClient();
    expect(await Promise.resolve(client)).toBe(client);
  });
});
