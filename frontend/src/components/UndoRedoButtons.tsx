import { Toolbar } from "./primitives/Toolbar";

/** Undo/redo the last recorded Git operation; tooltips name the step (Sublime Merge-style). */
export function UndoRedoButtons({
  undoLabel,
  redoLabel,
  disabled,
  disabledReason,
  onUndo,
  onRedo,
}: {
  undoLabel: string | null;
  redoLabel: string | null;
  disabled: boolean;
  disabledReason: string | null;
  onUndo: () => void;
  onRedo: () => void;
}) {
  const undoName = undoLabel === null ? "Nothing to undo" : `Undo ${undoLabel}`;
  const redoName = redoLabel === null ? "Nothing to redo" : `Redo ${redoLabel}`;
  const busyTitle = disabled ? (disabledReason ?? undefined) : undefined;
  return (
    <Toolbar aria-label="Undo">
      <button type="button" aria-label={undoName} title={busyTitle ?? undoName} disabled={disabled || undoLabel === null} onClick={onUndo}>
        ↶
      </button>
      <button type="button" aria-label={redoName} title={busyTitle ?? redoName} disabled={disabled || redoLabel === null} onClick={onRedo}>
        ↷
      </button>
    </Toolbar>
  );
}
