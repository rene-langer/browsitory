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
  // Tracks the last `initialMessage` value that was auto-seeded into `message`, so we can tell
  // "user hasn't touched the field since it was seeded" (message === lastSeeded) apart from "user
  // edited it" (message !== lastSeeded) — and only ever overwrite the former.
  const [lastSeeded, setLastSeeded] = useState("");

  // Seeds the field when a merge starts or when the pre-fill message changes to a different
  // merge's message, and clears it when a merge ends — but in every case only if the field still
  // holds exactly what was last auto-seeded, so a user's own edit is never clobbered.
  useEffect(() => {
    if (
      initialMessage !== undefined &&
      message === lastSeeded &&
      initialMessage !== lastSeeded
    ) {
      // Deliberate seed write, not a synchronization loop.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setMessage(initialMessage);
      setLastSeeded(initialMessage);
    } else if (initialMessage === undefined && message === lastSeeded && lastSeeded !== "") {
      // Merge ended (abort or otherwise) — clear the pre-fill, but only if the user hasn't typed
      // something different in the meantime.
      setMessage("");
      setLastSeeded("");
    }
  }, [initialMessage, message, lastSeeded]);

  const commitIfReady = () => {
    if (disabled || message.trim() === "") {
      return;
    }
    onCommit(message);
    setMessage("");
    setLastSeeded("");
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
