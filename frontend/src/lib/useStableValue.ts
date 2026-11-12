import { useRef } from "react";

/**
 * Hand-written memoization for values built anew in every render (#844): props objects and
 * context values full of inline arrow functions. Returns a value equal to `value` that keeps
 * its identity, and the identity of every nested plain object, for as long as nothing in it
 * changed, so a `memo` view or a context consumer skips updates that don't concern it.
 *
 * - A function becomes a wrapper whose identity never changes and that calls the latest
 *   function at the same place (with the latest object holding it as `this`). Calling it during
 *   render, in an effect or in an event handler always reaches the current closure.
 * - A plain object (`{}` literal) built anew in each render is compared key by key,
 *   recursively; its stable copy keeps its identity while its keys and every stabilized value
 *   stay the same. A nested object that keeps its own identity is passed through as is.
 * - Anything else (arrays, `Map`s, class instances, React elements, primitives) is compared by
 *   identity, without looking inside: keep those stable at the source (state, `useMemo`).
 *
 * `depth` limits how many levels of objects are looked into: 1 wraps the root's functions and
 * compares nested objects by identity, never copying them (`stableMemo`'s props).
 *
 * A function that comes or goes (an optional action) changes the object holding it, so a view
 * that shows a control only when the action is there still updates.
 */
export function useStableValue<T>(value: T, depth = Number.POSITIVE_INFINITY): T {
  const latest = useRef(value);
  // Assigned during render, not in an effect: a child's effect or render may call a wrapper
  // before this component's effects run, and must reach this render's functions.
  // eslint-disable-next-line react-hooks/refs
  latest.current = value;
  const previous = useRef<StableNode | undefined>(undefined);
  // eslint-disable-next-line react-hooks/refs
  const node = stabilize(value, previous.current, [], latest, depth);
  // eslint-disable-next-line react-hooks/refs
  previous.current = node;
  return node.value as T;
}

type StableNode =
  | { kind: "function"; value: (...args: unknown[]) => unknown }
  | {
      kind: "object";
      /** The object last seen at this place. */
      raw: Record<string, unknown>;
      value: Record<string, unknown>;
      /** The stabilized entries of `value`; absent while `value` is `raw` itself. */
      children: Map<string, StableNode> | undefined;
    }
  | { kind: "leaf"; value: unknown };

function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null) return false;
  const prototype: unknown = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function stabilize(
  value: unknown,
  previous: StableNode | undefined,
  path: readonly string[],
  latest: { current: unknown },
  depth: number,
): StableNode {
  if (typeof value === "function") {
    if (previous?.kind === "function") return previous;
    return { kind: "function", value: latestFunction(path, latest) };
  }
  if (isPlainObject(value) && path.length < depth) {
    if (previous?.kind === "object" && previous.raw === value) return previous;
    // A nested object seen for the first time is passed through as is: one that keeps its
    // identity (a client, a memoized hook result, state) is never copied, which would lose
    // methods that aren't own keys (a Proxy). The root (the props or context value, built in
    // every render) is always copied.
    if (path.length > 0 && previous?.kind !== "object") return { kind: "object", raw: value, value, children: undefined };
    const keys = Object.keys(value);
    const previousChildren = previous?.kind === "object" ? previous.children : undefined;
    const children = new Map<string, StableNode>();
    let same = previousChildren !== undefined && previousChildren.size === keys.length;
    for (const key of keys) {
      const before = previousChildren?.get(key);
      const child = stabilize(value[key], before, [...path, key], latest, depth);
      children.set(key, child);
      if (before === undefined || before.value !== child.value) same = false;
    }
    if (same && previous?.kind === "object") return { ...previous, raw: value };
    const stable: Record<string, unknown> = {};
    for (const [key, child] of children) stable[key] = child.value;
    return { kind: "object", raw: value, value: stable, children };
  }
  if (previous?.kind === "leaf" && Object.is(previous.value, value)) return previous;
  return { kind: "leaf", value };
}

/** A function that calls whatever function sits at `path` in the latest value. */
function latestFunction(path: readonly string[], latest: { current: unknown }): (...args: unknown[]) => unknown {
  return function (this: unknown, ...args: unknown[]) {
    if (path.length === 0) return (latest.current as (...a: unknown[]) => unknown).apply(this, args);
    let holder = latest.current as Record<string, unknown>;
    for (const key of path.slice(0, -1)) holder = holder[key] as Record<string, unknown>;
    return (holder[path[path.length - 1]] as (...a: unknown[]) => unknown).apply(holder, args);
  };
}
