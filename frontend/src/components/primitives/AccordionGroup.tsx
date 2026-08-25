import {
  createContext,
  useCallback,
  useContext,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MutableRefObject,
  type ReactNode,
  type RefObject,
} from "react";

// `RefObject<HTMLButtonElement | null>`, not `RefObject<HTMLButtonElement>`: this is what
// `useRef<HTMLButtonElement>(null)` actually returns (a ref to a DOM node is null before mount
// and after unmount), and every caller (`AccordionSection`, `AccordionGroup.test.tsx`) creates
// its ref that way.
interface RegisteredHeader {
  ref: RefObject<HTMLButtonElement | null>;
  setOpen: (open: boolean) => void;
}

export interface AccordionGroupContextValue {
  isActive: (ref: RefObject<HTMLButtonElement | null>) => boolean;
  register: (header: RegisteredHeader) => () => void;
  onHeaderFocus: (ref: RefObject<HTMLButtonElement | null>) => void;
  onHeaderKeyDown: (event: KeyboardEvent<HTMLButtonElement>, ref: RefObject<HTMLButtonElement | null>) => void;
}

const AccordionGroupContext = createContext<AccordionGroupContextValue | null>(null);

export interface AccordionGroupHandle {
  expandAll: () => void;
  collapseAll: () => void;
}

/**
 * Groups sibling `AccordionSection` headers so they share one WAI-ARIA APG roving-tabindex
 * keyboard region (Up/Down/Home/End between headers) and one `expandAll`/`collapseAll` handle.
 * `Sidebar` wraps its 7 top-level sections in one `AccordionGroup`; `PullRequestPanel` wraps its
 * per-forge-repository cards in a second, independently scoped one nested inside the first —
 * arrow-key nav in one group never crosses into the other.
 */
export function AccordionGroup({
  children,
  groupRef,
}: {
  children: ReactNode;
  groupRef?: MutableRefObject<AccordionGroupHandle | null>;
}) {
  const headersRef = useRef<RegisteredHeader[]>([]);
  const [activeRef, setActiveRefState] = useState<RefObject<HTMLButtonElement | null> | null>(null);

  const register = useCallback((header: RegisteredHeader) => {
    headersRef.current = [...headersRef.current, header];
    setActiveRefState((current) => current ?? header.ref);
    return () => {
      headersRef.current = headersRef.current.filter((entry) => entry !== header);
      setActiveRefState((current) => (current === header.ref ? (headersRef.current[0]?.ref ?? null) : current));
    };
  }, []);

  const isActive = useCallback((ref: RefObject<HTMLButtonElement | null>) => activeRef === ref, [activeRef]);

  const onHeaderFocus = useCallback((ref: RefObject<HTMLButtonElement | null>) => {
    setActiveRefState(ref);
  }, []);

  const focusIndex = useCallback((index: number) => {
    const headers = headersRef.current;
    if (headers.length === 0) return;
    const wrapped = (index + headers.length) % headers.length;
    const target = headers[wrapped];
    setActiveRefState(target.ref);
    target.ref.current?.focus();
  }, []);

  const onHeaderKeyDown = useCallback(
    (event: KeyboardEvent<HTMLButtonElement>, ref: RefObject<HTMLButtonElement | null>) => {
      const headers = headersRef.current;
      const index = headers.findIndex((entry) => entry.ref === ref);
      if (index === -1) return;
      if (event.key === "ArrowDown") {
        event.preventDefault();
        focusIndex(index + 1);
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        focusIndex(index - 1);
      } else if (event.key === "Home") {
        event.preventDefault();
        focusIndex(0);
      } else if (event.key === "End") {
        event.preventDefault();
        focusIndex(headers.length - 1);
      }
    },
    [focusIndex],
  );

  useImperativeHandle(
    groupRef,
    () => ({
      expandAll: () => headersRef.current.forEach((header) => header.setOpen(true)),
      collapseAll: () => headersRef.current.forEach((header) => header.setOpen(false)),
    }),
    [],
  );

  const value = useMemo<AccordionGroupContextValue>(
    () => ({ isActive, register, onHeaderFocus, onHeaderKeyDown }),
    [isActive, register, onHeaderFocus, onHeaderKeyDown],
  );

  return <AccordionGroupContext.Provider value={value}>{children}</AccordionGroupContext.Provider>;
}

// Small utility hook colocated with its provider component; splitting it into a separate file
// isn't warranted for one hook.
// eslint-disable-next-line react-refresh/only-export-components
export function useAccordionGroup(): AccordionGroupContextValue | null {
  return useContext(AccordionGroupContext);
}
