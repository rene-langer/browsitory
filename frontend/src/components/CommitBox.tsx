import { useState, type KeyboardEvent } from "react";

export function CommitBox({
  onCommit,
  disabled,
}: {
  onCommit: (message: string) => void;
  disabled: boolean;
}) {
  const [message, setMessage] = useState("");

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
    </div>
  );
}
