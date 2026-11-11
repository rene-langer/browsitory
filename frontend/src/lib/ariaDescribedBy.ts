/** `aria-describedby` for a button `BlockedReason` explains: its id while there is a reason (#748). */
export function describedBy(id: string, reason: string | null | undefined): string | undefined {
  return reason === null || reason === undefined ? undefined : id;
}
