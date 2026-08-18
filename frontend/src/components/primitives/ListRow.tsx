import type { MouseEvent, ReactNode } from "react";
import styles from "./ListRow.module.css";

export function ListRow({
  selected,
  onClick,
  onContextMenu,
  className,
  children,
}: {
  selected?: boolean;
  onClick: () => void;
  onContextMenu?: (event: MouseEvent) => void;
  className?: string;
  children: ReactNode;
}) {
  return (
    <li
      className={`${styles.row} ${className ?? ""}`.trim()}
      aria-selected={selected}
      onClick={onClick}
      onContextMenu={onContextMenu}
    >
      {children}
    </li>
  );
}
