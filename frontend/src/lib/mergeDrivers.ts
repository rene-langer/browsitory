// Custom merge drivers Browsitory doesn't run (`merge.<name>.driver`, #1165): a file under one
// that both sides changed is left conflicted, and the Changes pane says why.

/** Why a conflicted file under `driver` is conflicted, and what to do. */
export function mergeDriverReason(driver: string): string {
  return (
    `Browsitory doesn't run the '${driver}' merge driver, so this file was left conflicted: ` +
    "keep one side or edit it as the driver would have, then mark it resolved. " +
    "Or merge on the command line, where git runs the driver."
  );
}
