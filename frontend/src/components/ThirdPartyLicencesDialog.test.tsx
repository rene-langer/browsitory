import { act, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ThirdPartyLicencesDialog } from "./ThirdPartyLicencesDialog";

describe("ThirdPartyLicencesDialog", () => {
  it("shows a loading state, then the notices text", async () => {
    let resolve: (text: string) => void = () => {};
    const load = vi.fn(() => new Promise<string>((r) => (resolve = r)));
    render(<ThirdPartyLicencesDialog load={load} onClose={vi.fn()} />);

    expect(screen.getByRole("heading", { name: "Third-party licences" })).toBeInTheDocument();
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
});
