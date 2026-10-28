import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { FileSide, RepoClient } from "../ipc/RepoClient";
import { ImageDiff } from "./ImageDiff";

function sides(old: FileSide, next: FileSide) {
  const getFileSides = vi.fn(async () => ({ old, new: next }));
  return { getFileSides } as unknown as RepoClient & { getFileSides: typeof getFileSides };
}

const png = (base64: string, size = 3): FileSide => ({ kind: "Loaded", size, base64, mime: "image/png" });

describe("ImageDiff", () => {
  it("shows before and after images side by side with size and loaded dimensions", async () => {
    const client = sides(png("Zm9v"), png("YmFy", 2048));
    render(<ImageDiff client={client} repoPath="/r" source={{ kind: "Unstaged" }} path="logo.png" />);
    const before = await screen.findByRole("img", { name: "Before: logo.png" });
    expect(before).toHaveAttribute("src", "data:image/png;base64,Zm9v");
    expect(screen.getByText("Before")).toBeInTheDocument();
    expect(screen.getByText("After")).toBeInTheDocument();
    const after = screen.getByRole("img", { name: "After: logo.png" });
    Object.defineProperty(after, "naturalWidth", { value: 64 });
    Object.defineProperty(after, "naturalHeight", { value: 32 });
    fireEvent.load(after);
    expect(await screen.findByText("64 × 32 px · 2.0 KB")).toBeInTheDocument();
    expect(screen.getByText("3 B")).toBeInTheDocument(); // before not loaded yet: size only
    expect(client.getFileSides).toHaveBeenCalledWith("/r", "logo.png", { kind: "Unstaged" });
  });

  it("renders the sniffed type, not the one the extension suggests", async () => {
    const jpeg: FileSide = { kind: "Loaded", size: 3, base64: "Zm9v", mime: "image/jpeg" };
    render(<ImageDiff client={sides({ kind: "Missing" }, jpeg)} repoPath="/r" source={{ kind: "Staged", oldPath: null }} path="photo.png" />);
    expect(await screen.findByRole("img", { name: "After: photo.png" })).toHaveAttribute(
      "src",
      "data:image/jpeg;base64,Zm9v",
    );
  });

  it("labels added, deleted and too-large sides, and decode failures", async () => {
    const { unmount } = render(
      <ImageDiff
        client={sides({ kind: "Missing" }, { kind: "TooLarge", size: 25_270_000 })}
        repoPath="/r"
        source={{ kind: "Commit", commitId: "c", oldPath: null }}
        path="a.png"
      />,
    );
    expect(await screen.findByText("Added")).toBeInTheDocument();
    expect(screen.getByText("Too large to preview (24.1 MB)")).toBeInTheDocument();
    unmount();

    render(
      <ImageDiff
        client={sides(png("AA==", 1), { kind: "Missing" })}
        repoPath="/r"
        source={{ kind: "Commit", commitId: "c", oldPath: null }}
        path="b.png"
      />,
    );
    expect(await screen.findByText("Deleted")).toBeInTheDocument();
    fireEvent.error(screen.getByRole("img", { name: "Before: b.png" }));
    expect(await screen.findByText("Can't display this image")).toBeInTheDocument();
    expect(screen.queryByRole("img")).toBeNull();
  });

  it("does not render bytes that are not a previewable image", async () => {
    const html: FileSide = { kind: "Loaded", size: 3, base64: "Zm9v", mime: null };
    render(<ImageDiff client={sides({ kind: "Missing" }, html)} repoPath="/r" source={{ kind: "Unstaged" }} path="x.png" />);
    expect(await screen.findByText("Can't display this image")).toBeInTheDocument();
    expect(screen.queryByRole("img")).toBeNull();
  });

  it("renders SVG only through an img element", async () => {
    const svg: FileSide = { kind: "Loaded", size: 10, base64: "PHN2Zy8+", mime: "image/svg+xml" };
    const { container } = render(
      <ImageDiff client={sides({ kind: "Missing" }, svg)} repoPath="/r" source={{ kind: "Staged", oldPath: null }} path="i.svg" />,
    );
    const img = await screen.findByRole("img", { name: "After: i.svg" });
    expect(img.tagName).toBe("IMG");
    expect(img).toHaveAttribute("src", "data:image/svg+xml;base64,PHN2Zy8+");
    expect(container.querySelector("svg, object, iframe, embed")).toBeNull();
    expect(container.innerHTML).not.toContain("<svg");
  });

  it("shows a load failure with Retry, and refetches on a new refresh key", async () => {
    const getFileSides = vi
      .fn()
      .mockRejectedValueOnce(new Error("boom"))
      .mockResolvedValue({ old: png("Zm9v"), new: png("YmFy") });
    const client = { getFileSides } as unknown as RepoClient;
    const { rerender } = render(
      <ImageDiff client={client} repoPath="/r" source={{ kind: "Unstaged" }} path="a.png" refreshKey={0} />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("Loading preview…");
    expect(await screen.findByRole("alert")).toHaveTextContent("boom");
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("img", { name: "After: a.png" })).toBeInTheDocument();
    expect(getFileSides).toHaveBeenCalledTimes(2);
    rerender(<ImageDiff client={client} repoPath="/r" source={{ kind: "Unstaged" }} path="a.png" refreshKey={1} />);
    await waitFor(() => expect(getFileSides).toHaveBeenCalledTimes(3));
  });
});
