import { Redo2, Undo2 } from "lucide-react";
import { ReasonButton } from "./primitives/ReasonButton";
import { Toolbar } from "./primitives/Toolbar";

/**
 * Undo/redo the last recorded Git operation; tooltips name the step. Busy, each says why next to
 * itself on hover and focus (#664).
 */
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
  const busy = disabled ? (disabledReason ?? "Another operation is in progress.") : null;
  return (
    <Toolbar aria-label="Undo">
      <ReasonButton aria-label={undoName} title={undoName} reason={busy ?? (undoLabel === null ? "Nothing to undo." : null)} onClick={onUndo}>
        <Undo2 size={14} aria-hidden="true" />
      </ReasonButton>
      <ReasonButton aria-label={redoName} title={redoName} reason={busy ?? (redoLabel === null ? "Nothing to redo." : null)} onClick={onRedo}>
        <Redo2 size={14} aria-hidden="true" />
      </ReasonButton>
    </Toolbar>
  );
}
