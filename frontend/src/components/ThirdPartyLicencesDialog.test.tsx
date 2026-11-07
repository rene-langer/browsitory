import { act, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ThirdPartyLicencesDialog } from "./ThirdPartyLicencesDialog";
import styles from "./ThirdPartyLicencesDialog.module.css";

describe("ThirdPartyLicencesDialog", () => {
  it("shows a loading state, then the notices text", async () => {
    let resolve: (text: string) => void = () => {};
    const load = vi.fn(() => new Promise<string>((r) => (resolve = r)));
    render(<ThirdPartyLicencesDialog load={load} onClose={vi.fn()} />);

    expect(screen.getByRole("heading", { name: "Third-party licences" })).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Third-party licences" })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Loading licences…");

    await act(async () => resolve("# Third-party notices\n\nlibgit2 licence text"));

    const text = screen.getByRole("region", { name: "Licence texts" });
    expect(text).toHaveTextContent("libgit2 licence text");
    // Keyboard users can scroll the long text.
    expect(text).toHaveAttribute("tabindex", "0");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(load).toHaveBeenCalledOnce();
  });

  it("shows the error with a hint and retries on request", async () => {
    const load = vi
      .fn<() => Promise<string>>()
      .mockRejectedValueOnce(new Error("HTTP 404"))
      .mockResolvedValueOnce("notices");
    render(<ThirdPartyLicencesDialog load={load} onClose={vi.fn()} />);

    expect(await screen.findByText("HTTP 404")).toBeInTheDocument();
    expect(screen.getByText(/THIRD_PARTY_NOTICES\.md/)).toBeInTheDocument();

    act(() => screen.getByRole("button", { name: "Retry" }).click());

    await waitFor(() => expect(screen.getByRole("region", { name: "Licence texts" })).toHaveTextContent("notices"));
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("calls onClose from the close button", () => {
    const onClose = vi.fn();
    render(<ThirdPartyLicencesDialog load={() => new Promise(() => {})} onClose={onClose} />);

    screen.getByRole("button", { name: "Close" }).click();

    expect(onClose).toHaveBeenCalledOnce();
  });

  it("keeps Close outside the scrolling content, so it stays in view", async () => {
    render(<ThirdPartyLicencesDialog load={() => Promise.resolve("notices")} onClose={vi.fn()} />);

    const text = await screen.findByRole("region", { name: "Licence texts" });
    const close = screen.getByRole("button", { name: "Close" });
    expect(close.closest("footer")).not.toBeNull();
    expect(text.parentElement).not.toContainElement(close);
  });

  it("lets the licence text fill the body as its only scroll area (#383)", async () => {
    render(<ThirdPartyLicencesDialog load={() => Promise.resolve("notices")} onClose={vi.fn()} />);

    // jsdom doesn't lay out; check the structure the CSS relies on. The text sits directly in
    // the panel's body, which is a flex column (styles.body) the text shrinks into, so the body
    // never scrolls around the text's own scroll area.
    const text = await screen.findByRole("region", { name: "Licence texts" });
    expect(text).toHaveClass(styles.text);
    expect(text.parentElement).toHaveClass(styles.body);
    expect(text.parentElement).toContainElement(screen.getByText(/includes open source software/));
  });
});
