/**
 * The drop-stash confirmation's message (#989), from the Stashes panel, the stash pane and the
 * palette alike: what goes, and that Undo can't bring it back. The title already names the stash.
 */
export function DropStashMessage() {
  return (
    <>
      <p>Deletes the stash and the changes it holds.</p>
      <p>Undo can&apos;t bring it back.</p>
    </>
  );
}
