import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useStableValue } from "./useStableValue";

describe("useStableValue (#844)", () => {
  it("keeps a function's identity across renders and calls the latest one", () => {
    const first = vi.fn(() => "first");
    const second = vi.fn(() => "second");
    const { result, rerender } = renderHook(({ value }) => useStableValue(value), {
      initialProps: { value: { onRun: first } },
    });
    const before = result.current;
    rerender({ value: { onRun: second } });
    expect(result.current).toBe(before);
    expect(result.current.onRun()).toBe("second");
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledOnce();
  });

  it("passes the arguments and `this` (the latest object holding it) through", () => {
    const holder = {
      prefix: "x",
      join(this: { prefix: string }, a: string, b: string) {
        return `${this.prefix}${a}${b}`;
      },
    };
    const { result } = renderHook(() => useStableValue({ tools: holder }));
    expect(result.current.tools.join("a", "b")).toBe("xab");
  });

  it("keeps a nested plain object built anew in each render while its leaves stay the same", () => {
    const list = ["a"];
    const { result, rerender } = renderHook(({ n }) => useStableValue({ nested: { list, count: 1, onGo: () => n } }), {
      initialProps: { n: 1 },
    });
    // A nested object is passed through as is until it changes identity once (see the next
    // test); from then on its stable copy is kept.
    rerender({ n: 2 });
    const before = result.current;
    rerender({ n: 3 });
    expect(result.current).toBe(before);
    expect(result.current.nested.onGo()).toBe(3);
  });

  it("passes a nested object that keeps its identity through as is, without copying it", () => {
    // A client behind a Proxy (as the tests' fake is) has methods that aren't own keys: a copy
    // would lose them.
    const client = new Proxy({} as { read: () => string }, { get: (_target, key) => (key === "read" ? () => "read" : undefined) });
    const { result, rerender } = renderHook(({ n }) => useStableValue({ client, n }), { initialProps: { n: 1 } });
    expect(result.current.client).toBe(client);
    rerender({ n: 2 });
    expect(result.current.client).toBe(client);
    expect(result.current.client.read()).toBe("read");
  });

  it("gives a new object, up to the root, when a data leaf changes; untouched siblings stay", () => {
    const { result, rerender } = renderHook(({ count }) => useStableValue({ a: { count }, b: { label: "same" } }), {
      initialProps: { count: 1 },
    });
    rerender({ count: 1 });
    const before = result.current;
    rerender({ count: 2 });
    expect(result.current).not.toBe(before);
    expect(result.current.a).not.toBe(before.a);
    expect(result.current.a.count).toBe(2);
    expect(result.current.b).toBe(before.b);
  });

  it("compares arrays and class instances by identity, without looking inside", () => {
    const { result, rerender } = renderHook(({ list }) => useStableValue({ list }), {
      initialProps: { list: [1, 2] },
    });
    const before = result.current;
    rerender({ list: [1, 2] });
    expect(result.current).not.toBe(before);
    const map = new Map([["k", 1]]);
    const { result: withMap, rerender: rerenderMap } = renderHook(({ value }) => useStableValue({ value }), {
      initialProps: { value: map },
    });
    const mapBefore = withMap.current;
    rerenderMap({ value: map });
    expect(withMap.current).toBe(mapBefore);
  });

  it("gives a new object when a function comes or goes, so an optional action shows or hides", () => {
    const { result, rerender } = renderHook(
      ({ on }) => useStableValue<{ onBack?: () => void }>(on ? { onBack: () => {} } : {}),
      { initialProps: { on: false } },
    );
    expect(result.current.onBack).toBeUndefined();
    rerender({ on: true });
    expect(typeof result.current.onBack).toBe("function");
    const withBack = result.current;
    rerender({ on: false });
    expect(result.current).not.toBe(withBack);
    expect(result.current.onBack).toBeUndefined();
    expect("onBack" in result.current).toBe(false);
  });

  it("keeps an undefined value distinct from a missing key", () => {
    const { result, rerender } = renderHook(
      ({ present }) => useStableValue<{ value?: number }>(present ? { value: undefined } : {}),
      { initialProps: { present: true } },
    );
    const before = result.current;
    rerender({ present: false });
    expect(result.current).not.toBe(before);
  });

  it("with a depth of 1, wraps the top-level functions and compares nested objects by identity", () => {
    // `stableMemo`'s props: a nested object is never copied, even when it changes identity in
    // every render (a test's fake client built per render is a Proxy whose methods a copy would lose).
    const makeClient = () =>
      new Proxy({} as { read: () => string }, { get: (_target, key) => (key === "read" ? () => "read" : undefined) });
    const { result, rerender } = renderHook(({ client }) => useStableValue({ client, onGo: () => 1 }, 1), {
      initialProps: { client: makeClient() },
    });
    const onGo = result.current.onGo;
    const next = makeClient();
    rerender({ client: next });
    expect(result.current.client).toBe(next);
    expect(result.current.client.read()).toBe("read");
    expect(result.current.onGo).toBe(onGo);
    const before = result.current;
    rerender({ client: next });
    expect(result.current).toBe(before);
  });
});
