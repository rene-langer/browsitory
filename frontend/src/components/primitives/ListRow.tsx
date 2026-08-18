import type { MouseEvent, ReactNode } from "react";
import styles from "./ListRow.module.css";

export function ListRow({
  selected,
  onClick,
  onContextMenu,
  children,
}: {
  selected: boolean;
  onClick: () => void;
  onContextMenu?: (event: MouseEvent) => void;
  children: ReactNode;
}) {
  return (
    <li
      className={styles.row}
      aria-selected={selected}
      onClick={onClick}
      onContextMenu={onContextMenu}
    >
      {children}
    </li>
  );
}
