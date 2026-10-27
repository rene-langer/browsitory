import { ChevronDown, ChevronRight } from "lucide-react";
import styles from "./DiffPane.module.css";

export function CollapseToggle({ collapsed, path, onToggle }: { collapsed: boolean; path: string; onToggle: () => void }) {
  return (
    <button
      type="button"
      className={styles.collapseToggle}
      aria-label={collapsed ? `Expand ${path}` : `Collapse ${path}`}
      onClick={(event) => {
        event.stopPropagation();
        onToggle();
      }}
    >
      {collapsed ? (
        <ChevronRight size={14} aria-hidden="true" />
      ) : (
        <ChevronDown size={14} aria-hidden="true" />
      )}
    </button>
  );
}

export function CollapseAllToggle({ allCollapsed, onToggle }: { allCollapsed: boolean; onToggle: () => void }) {
  return (
    <button type="button" onClick={onToggle}>
      {allCollapsed ? "Expand all" : "Collapse all"}
    </button>
  );
}
