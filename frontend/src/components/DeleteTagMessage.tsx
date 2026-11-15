/**
 * The local-tag delete confirmation's message (#989), from the tag panel and the graph alike: what
 * goes, that a remote's copy stays, and that Undo brings the tag back.
 */
export function DeleteTagMessage({ name }: { name: string }) {
  return (
    <>
      <p>Removes {name} from this repository. A copy of the tag on a remote stays.</p>
      <p>Undo brings it back.</p>
    </>
  );
}
