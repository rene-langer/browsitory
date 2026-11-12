import { memo, type ComponentType, type ReactElement } from "react";
import { useStableValue } from "./useStableValue";

/**
 * `memo` for a view whose parent builds its props anew in every render (#844): inline arrow
 * functions and object literals. The props go through {@link useStableValue} first, so the view
 * renders again only when something it shows changed, while its callbacks still reach the
 * parent's latest closures. Only the props themselves are stabilized: an object prop (a client,
 * a list) must keep its identity at the source.
 */
export function stableMemo<P extends object>(Component: ComponentType<P>, name: string): (props: P) => ReactElement {
  const Memoized = memo(Component) as unknown as ComponentType<P>;
  function Stable(props: P): ReactElement {
    const stable = useStableValue(props, 1);
    return <Memoized {...stable} />;
  }
  Stable.displayName = name;
  Memoized.displayName = name;
  return Stable;
}
