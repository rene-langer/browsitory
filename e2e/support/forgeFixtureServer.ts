import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";

// A minimal loopback HTTP server standing in for GitHub's and Bitbucket's real pull-request
// APIs during `e2e/specs/pull-requests.spec.ts`. `crates/repo-service/src/pull_requests.rs`
// otherwise always builds requests against the hardcoded real hosts (see that module's doc
// comment: unit tests use an in-process `ForgeApi` fake instead, precisely so they never need a
// mock HTTP server) — a black-box WebDriver E2E test has no seam inside the running process to
// substitute a fake, so it needs the *destination* redirected instead. `wdio.conf.ts` points
// both `BROWSITORY_FORGE_GITHUB_API_BASE_URL` and `BROWSITORY_FORGE_BITBUCKET_API_BASE_URL` at
// this single server's `url` before the app process is spawned (env vars propagate at spawn
// time only, like `BROWSITORY_E2E_CREDENTIAL_KEY`/`_CERT` already do for the Git HTTPS
// credential E2E flow); the server tells GitHub- from Bitbucket-shaped requests apart purely by
// their path shape, matching `build_list_request`/`build_create_request`.
//
// `wdio run`'s local runner loads spec files in a separate worker *process* from the one that
// runs `wdio.conf.ts`'s `onPrepare` (confirmed empirically: a plain in-memory module singleton
// set in `onPrepare` reads back as unset from a spec file). So this server exposes its own
// control surface (`/__control/...`) over the same loopback port, and `ForgeFixtureClient` below
// — used from spec files — is a thin `fetch()` wrapper around it, rather than a shared object
// reference. The server itself is real, so this is not a workaround for correctness, only for
// how the two Node processes talk to it.

export interface CapturedForgeRequest {
  method: string;
  path: string;
  authorization: string | null;
  body: unknown;
}

export type RouteKey = "github-list" | "github-create" | "bitbucket-list" | "bitbucket-create";

interface RouteResponse {
  status: number;
  body: unknown;
}

const defaultResponses: Record<RouteKey, RouteResponse> = {
  "github-list": { status: 200, body: [] },
  "github-create": { status: 201, body: {} },
  "bitbucket-list": { status: 200, body: { values: [] } },
  "bitbucket-create": { status: 201, body: {} },
};

function classifyProviderRoute(method: string, pathname: string): RouteKey | null {
  const isPulls = pathname.includes("/pulls");
  const isPullRequests = pathname.includes("/pullrequests");
  if (method === "GET" && isPulls) return "github-list";
  if (method === "POST" && isPulls) return "github-create";
  if (method === "GET" && isPullRequests) return "bitbucket-list";
  if (method === "POST" && isPullRequests) return "bitbucket-create";
  return null;
}

async function readJsonBody(request: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  for await (const chunk of request) chunks.push(chunk as Buffer);
  const raw = Buffer.concat(chunks).toString("utf8");
  return raw === "" ? null : (JSON.parse(raw) as unknown);
}

/** Runs only in the `wdio.conf.ts` launcher process — started from `onPrepare`. */
export class ForgeFixtureServer {
  url = "";
  private readonly server: Server;
  private responses: Record<RouteKey, RouteResponse> = { ...defaultResponses };
  private requests: CapturedForgeRequest[] = [];

  private constructor(server: Server) {
    this.server = server;
  }

  static async start(): Promise<ForgeFixtureServer> {
    let instance!: ForgeFixtureServer;
    const server = createServer((request, response) => {
      instance.handle(request, response).catch((error: unknown) => {
        response.writeHead(500, { "Content-Type": "application/json" });
        response.end(JSON.stringify({ message: String(error) }));
      });
    });
    instance = new ForgeFixtureServer(server);
    await new Promise<void>((resolve, reject) => {
      server.once("error", reject);
      server.listen(0, "127.0.0.1", resolve);
    });
    const address = server.address();
    if (address === null || typeof address === "string") {
      throw new Error("forge fixture server has no TCP address");
    }
    instance.url = `http://127.0.0.1:${address.port}`;
    return instance;
  }

  private async handle(request: IncomingMessage, response: ServerResponse) {
    const url = new URL(request.url ?? "/", "http://127.0.0.1");
    const method = request.method ?? "GET";

    if (url.pathname === "/__control/reset" && method === "POST") {
      this.requests = [];
      this.responses = { ...defaultResponses };
      response.writeHead(204);
      response.end();
      return;
    }
    if (url.pathname === "/__control/responses" && method === "POST") {
      const payload = (await readJsonBody(request)) as { key: RouteKey; status: number; body: unknown };
      this.responses[payload.key] = { status: payload.status, body: payload.body };
      response.writeHead(204);
      response.end();
      return;
    }
    if (url.pathname === "/__control/requests" && method === "GET") {
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(JSON.stringify(this.requests));
      return;
    }

    const body = await readJsonBody(request);
    this.requests.push({
      method,
      path: url.pathname + url.search,
      authorization: request.headers.authorization ?? null,
      body,
    });
    const key = classifyProviderRoute(method, url.pathname);
    if (key === null) {
      response.writeHead(404, { "Content-Type": "application/json" });
      response.end(JSON.stringify({ message: "no fixture route matched" }));
      return;
    }
    const route = this.responses[key];
    response.writeHead(route.status, { "Content-Type": "application/json" });
    response.end(JSON.stringify(route.body));
  }

  close(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.server.close((error) => (error === undefined ? resolve() : reject(error)));
    });
  }
}

let shared: ForgeFixtureServer | null = null;

export async function startSharedForgeFixtureServer(): Promise<ForgeFixtureServer> {
  shared = await ForgeFixtureServer.start();
  return shared;
}

export async function closeSharedForgeFixtureServer(): Promise<void> {
  if (shared === null) return;
  await shared.close();
  shared = null;
}

/**
 * Spec-file-side handle: a thin `fetch()` wrapper around the fixture server's `/__control/...`
 * routes, addressed via the same base URL the app itself was pointed at (see this module's
 * header comment for why a spec file can't just reach for a shared in-memory object).
 */
export class ForgeFixtureClient {
  constructor(private readonly baseUrl: string) {}

  static fromEnv(): ForgeFixtureClient {
    const baseUrl = process.env.BROWSITORY_FORGE_GITHUB_API_BASE_URL;
    if (baseUrl === undefined || baseUrl === "") {
      throw new Error(
        "BROWSITORY_FORGE_GITHUB_API_BASE_URL was not set before the Tauri session (see wdio.conf.ts's onPrepare)",
      );
    }
    return new ForgeFixtureClient(baseUrl);
  }

  async reset(): Promise<void> {
    await fetch(`${this.baseUrl}/__control/reset`, { method: "POST" });
  }

  async setResponse(key: RouteKey, status: number, body: unknown): Promise<void> {
    await fetch(`${this.baseUrl}/__control/responses`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ key, status, body }),
    });
  }

  private async requests(): Promise<CapturedForgeRequest[]> {
    const response = await fetch(`${this.baseUrl}/__control/requests`);
    return (await response.json()) as CapturedForgeRequest[];
  }

  async requestCount(): Promise<number> {
    return (await this.requests()).length;
  }

  async lastRequestFor(key: RouteKey): Promise<CapturedForgeRequest | undefined> {
    const wantsGet = key === "github-list" || key === "bitbucket-list";
    const wantsGithub = key.startsWith("github");
    const all = await this.requests();
    return [...all].reverse().find((entry) => {
      const matchesMethod = wantsGet ? entry.method === "GET" : entry.method === "POST";
      const matchesProvider = wantsGithub
        ? entry.path.includes("/pulls")
        : entry.path.includes("/pullrequests");
      return matchesMethod && matchesProvider;
    });
  }
}
