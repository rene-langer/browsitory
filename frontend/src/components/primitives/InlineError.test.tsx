import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { InlineError } from "./InlineError";
import styles from "./InlineError.module.css";

describe("InlineError", () => {
  it("renders the message as an alert", () => {
    render(<InlineError message="Something failed" onDismiss={() => {}} />);
    expect(screen.getByRole("alert")).toHaveTextContent("Something failed");
  });

  it("calls onDismiss when the dismiss button is clicked", () => {
    const onDismiss = vi.fn();
    render(<InlineError message="Something failed" onDismiss={onDismiss} />);
    fireEvent.click(screen.getByRole("button", { name: "Dismiss error" }));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("offers Retry and a next-step hint when provided", () => {
    const onRetry = vi.fn();
    render(<InlineError message="Boom" hint="Check your connection." onRetry={onRetry} onDismiss={() => {}} />);
    expect(screen.getByText("Check your connection.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("omits Retry by default", () => {
    render(<InlineError message="Boom" onDismiss={() => {}} />);
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
  });

  it("copies the message, hint and earlier errors with Copy details (#663)", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    try {
      render(<InlineError message="Push failed" hint="Fetch first." copyDetails earlier={["Fetch failed"]} onDismiss={() => {}} />);
      fireEvent.click(screen.getByRole("button", { name: "Copy details" }));
      await waitFor(() => expect(screen.getByRole("button", { name: "Copied" })).toBeInTheDocument());
      expect(writeText).toHaveBeenCalledWith("Push failed\nFetch first.\n\nEarlier errors:\n- Fetch failed");
    } finally {
      Reflect.deleteProperty(navigator, "clipboard");
    }
  });

  it("copies details without the credentials a URL or header carries (#1056)", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    try {
      render(
        <InlineError
          message="Fetch failed: https://ghp_secret@example.test/o/r.git"
          hint="Check https://alice:pw@example.test/"
          details="Authorization: Bearer tok123"
          copyDetails
          earlier={["Push to http://bob@example.test/r failed"]}
          onDismiss={() => {}}
        />,
      );
      fireEvent.click(screen.getByRole("button", { name: "Copy details" }));
      await waitFor(() => expect(writeText).toHaveBeenCalled());
      expect(writeText).toHaveBeenCalledWith(
        [
          "Fetch failed: https://***@example.test/o/r.git",
          "Check https://***@example.test/",
          "Authorization: ***",
          "",
          "Earlier errors:",
          "- Push to http://***@example.test/r failed",
        ].join("\n"),
      );
    } finally {
      Reflect.deleteProperty(navigator, "clipboard");
    }
  });

  it("says when the clipboard refused the copy", async () => {
    render(<InlineError message="Boom" copyDetails onDismiss={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Copy details" }));
    expect(await screen.findByRole("button", { name: "Couldn't copy" })).toBeInTheDocument();
  });

  it("offers no Copy details by default", () => {
    render(<InlineError message="Boom" onDismiss={() => {}} />);
    expect(screen.queryByRole("button", { name: "Copy details" })).toBeNull();
  });

  it("hides libgit2's class and code from the message, but copies them (#997)", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const raw = "failed to open repository: reference 'refs/heads/main' not found; class=Reference (4); code=UnbornBranch (-9)";
    try {
      render(<InlineError message={raw} copyDetails onDismiss={() => {}} />);
      const alert = screen.getByRole("alert");
      expect(alert).toHaveTextContent("failed to open repository: reference 'refs/heads/main' not found");
      expect(alert).not.toHaveTextContent("class=");
      expect(alert).not.toHaveTextContent("code=");
      fireEvent.click(screen.getByRole("button", { name: "Copy details" }));
      await waitFor(() => expect(writeText).toHaveBeenCalledWith(raw));
    } finally {
      Reflect.deleteProperty(navigator, "clipboard");
    }
  });

  it("puts its actions, then Details, then dismiss in one row; the first can be primary (#984)", () => {
    const pull = vi.fn();
    const forcePush = vi.fn();
    const onDetails = vi.fn();
    render(
      <InlineError
        message="Push was rejected."
        actions={[
          { label: "Pull", primary: true, onClick: pull },
          { label: "Force push…", onClick: forcePush },
        ]}
        onDetails={onDetails}
        onDismiss={() => {}}
      />,
    );
    const alert = screen.getByRole("alert");
    const row = alert.firstElementChild as HTMLElement;
    expect([...row.querySelectorAll("button")].map((button) => button.getAttribute("aria-label") ?? button.textContent)).toEqual([
      "Pull",
      "Force push…",
      "Details",
      "Dismiss error",
    ]);
    expect(screen.getByRole("button", { name: "Pull" })).toHaveClass(styles.primary!);
    expect(screen.getByRole("button", { name: "Force push…" })).not.toHaveClass(styles.primary!);
    fireEvent.click(screen.getByRole("button", { name: "Pull" }));
    fireEvent.click(screen.getByRole("button", { name: "Force push…" }));
    fireEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(pull).toHaveBeenCalledOnce();
    expect(forcePush).toHaveBeenCalledOnce();
    expect(onDetails).toHaveBeenCalledOnce();
  });

  it("offers no Details by default", () => {
    render(<InlineError message="Boom" onDismiss={() => {}} />);
    expect(screen.queryByRole("button", { name: "Details" })).toBeNull();
  });

  it("lists the earlier errors it replaced, collapsed (#663)", () => {
    render(<InlineError message="Third" earlier={["Second", "First"]} onDismiss={() => {}} />);
    expect(screen.getByText("2 earlier errors")).toBeInTheDocument();
    expect(screen.getByText("Second")).toBeInTheDocument();
    expect(screen.getByText("First")).toBeInTheDocument();
  });
});
