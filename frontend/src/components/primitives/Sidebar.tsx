import { useRef, useState, type ReactNode } from "react";
import { ChevronsDownUp, ChevronsUpDown, Settings } from "lucide-react";
import { AccordionGroup, type AccordionGroupHandle } from "./AccordionGroup";
import styles from "./Sidebar.module.css";

export interface SidebarPanelToggle {
  id: string;
  label: string;
  visible: boolean;
  onToggle: (visible: boolean) => void;
}

export function Sidebar({ children, panelToggles }: { children: ReactNode; panelToggles?: SidebarPanelToggle[] }) {
  const groupRef = useRef<AccordionGroupHandle | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);

  return (
    <aside className={styles.sidebar} aria-label="Repository sections">
      <div className={styles.toolbar}>
        <button
          type="button"
          className={styles.toolbarButton}
          aria-label="Expand all sections"
          onClick={() => groupRef.current?.expandAll()}
        >
          <ChevronsUpDown size={14} aria-hidden="true" />
        </button>
        <button
          type="button"
          className={styles.toolbarButton}
          aria-label="Collapse all sections"
          onClick={() => groupRef.current?.collapseAll()}
        >
          <ChevronsDownUp size={14} aria-hidden="true" />
        </button>
        {panelToggles !== undefined && (
          <div className={styles.settingsWrapper}>
            <button
              type="button"
              className={styles.toolbarButton}
              aria-label="Sidebar section settings"
              aria-expanded={settingsOpen}
              onClick={() => setSettingsOpen((open) => !open)}
            >
              <Settings size={14} aria-hidden="true" />
            </button>
            {settingsOpen && (
              <div className={styles.settingsPopover} role="menu" aria-label="Toggle sidebar sections">
                {panelToggles.map((toggle) => (
                  <label key={toggle.id} className={styles.settingsRow}>
                    <input
                      type="checkbox"
                      checked={toggle.visible}
                      onChange={(event) => toggle.onToggle(event.target.checked)}
                    />
                    {toggle.label}
                  </label>
                ))}
              </div>
            )}
          </div>
        )}
      </div>
      <AccordionGroup groupRef={groupRef}>
        <div className={styles.sections}>{children}</div>
      </AccordionGroup>
    </aside>
  );
}
