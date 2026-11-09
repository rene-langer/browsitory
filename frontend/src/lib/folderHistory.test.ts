import { describe, expect, it } from "vitest";
import { folderOf, folderSearchQuery } from "./folderHistory";

describe("folderHistory (#447)", () => {
  it("finds a file's folder, or none at the root", () => {
    expect(folderOf("src/ui/a.ts")).toBe("src/ui");
    expect(folderOf("a.ts")).toBeNull();
  });

  it("searches everything under the folder, quoting when the parser needs it", () => {
    expect(folderSearchQuery("src/ui")).toBe("path:src/ui/");
    expect(folderSearchQuery("my docs")).toBe('path:"my docs/"');
    expect(folderSearchQuery('odd"name\\x')).toBe('path:"odd\\"name\\\\x/"');
  });
});
