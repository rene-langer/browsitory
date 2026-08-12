import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { RepoClient, StatusEntry } from "../ipc/RepoClient";
import { StatusView } from "./StatusView";

function fakeClient(entries: StatusEntry[]): RepoClient {
  return {
    openRepo: async () => {},
    getStatus: async () => entries,
  };
}

describe("StatusView", () => {
  it("renders each status entry's path", async () => {
    const client = fakeClient([
      { path: "src/main.rs", staged: false, kind: "Modified" },
      { path: "README.md", staged: true, kind: "New" },
    ]);

    render(<StatusView client={client} />);

    expect(await screen.findByText("src/main.rs")).toBeInTheDocument();
    expect(await screen.findByText("README.md")).toBeInTheDocument();
  });

  it("renders nothing extra when there are no changes", async () => {
    const client = fakeClient([]);

    render(<StatusView client={client} />);

    expect(await screen.findByText("No changes")).toBeInTheDocument();
  });

  it("renders the error instead of the empty state when getStatus rejects", async () => {
    const client: RepoClient = {
      openRepo: async () => {},
      getStatus: async () => {
        throw new Error("no repo open");
      },
    };

    render(<StatusView client={client} />);

    expect(await screen.findByRole("alert")).toHaveTextContent("no repo open");
    expect(screen.queryByText("No changes")).not.toBeInTheDocument();
  });
});
