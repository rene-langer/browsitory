/** Stub so the tests run red against an assertion; the strip comes with #985's implementation. */
export function GraphFilterStrip(props: {
  selection: { local: string[] | null; remote: string[] | null };
  onShowAll: () => void;
}) {
  void props;
  return null;
}
