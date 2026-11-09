import { folderOf } from "../lib/folderHistory";

/** "Folder history" for the folder a file is in; nothing for a file at the repository root. */
export function FolderHistoryButton({
  path,
  onOpen,
  stopPropagation = false,
}: {
  path: string;
  onOpen: (folder: string) => void;
  /** For a button inside a clickable row. */
  stopPropagation?: boolean;
}) {
  const folder = folderOf(path);
  if (folder === null) return null;
  return (
    <button
      type="button"
      aria-label={`History of folder ${folder}`}
      title={`Commits that changed anything in ${folder}/`}
      onClick={(event) => {
        if (stopPropagation) event.stopPropagation();
        onOpen(folder);
      }}
    >
      Folder history
    </button>
  );
}
