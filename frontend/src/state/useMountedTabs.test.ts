import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useMountedTabs } from "./useMountedTabs";

describe("useMountedTabs (#642)", () => {
  it("mounts the shown tab and keeps the ones shown before it, newest first, up to the limit", () => {
    const open = ["/a", "/b", "/c", "/d"];
    const { result, rerender } = renderHook(({ active }) => useMountedTabs(open, active, 3), {
      initialProps: { active: "/a" as string | null },
    });
    expect(result.current).toEqual(["/a"]);
    rerender({ active: "/b" });
    expect(result.current).toEqual(["/b", "/a"]);
    rerender({ active: "/c" });
    expect(result.current).toEqual(["/c", "/b", "/a"]);
    rerender({ active: "/a" });
    expect(result.current).toEqual(["/a", "/c", "/b"]);
    rerender({ active: "/d" });
    expect(result.current).toEqual(["/d", "/a", "/c"]);
  });

  it("drops a closed tab and mounts nothing without an active tab", () => {
    const { result, rerender } = renderHook(({ open, active }) => useMountedTabs(open, active), {
      initialProps: { open: ["/a", "/b"], active: "/a" as string | null },
    });
    rerender({ open: ["/a", "/b"], active: "/b" });
    expect(result.current).toEqual(["/b", "/a"]);
    rerender({ open: ["/b"], active: "/b" });
    expect(result.current).toEqual(["/b"]);
    rerender({ open: [], active: null });
    expect(result.current).toEqual([]);
  });
});
