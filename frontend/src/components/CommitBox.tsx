import { useEffect, useState, type KeyboardEvent } from "react";

export function CommitBox({
  onCommit,
  disabled,
  onAbortMerge,
  initialMessage,
}: {
  onCommit: (message: string) => void;
  disabled: boolean;
  onAbortMerge: () => void;
  initialMessage?: string;
}) {
  const [message, setMessage] = useState("");

  // Seeds the field once when a merge starts (`initialMessage` goes from unset to set) —
  // deliberately not a dependency-driven re-seed on every render, or it would keep clobbering
  // whatever the user has typed since.
  useEffect(() => {
    if (initialMessage !== undefined) {
      // Deliberate seed-once write, not a synchronization loop.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setMessage((prev) => (prev === "" ? initialMessage : prev));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialMessage !== undefined]);

  const commitIfReady = () => {
    if (disabled || message.trim() === "") {
      return;
    }
    onCommit(message);
    setMessage("");
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      commitIfReady();
    }
  };

  return (
    <div>
      <textarea
        value={message}
        onChange={(event) => setMessage(event.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Commit message"
      />
      <button onClick={commitIfReady} disabled={disabled || message.trim() === ""}>
        Commit
      </button>
      {initialMessage !== undefined && <button onClick={onAbortMerge}>Abort merge</button>}
    </div>
  );
}
