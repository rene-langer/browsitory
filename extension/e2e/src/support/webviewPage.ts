// Drives the VSCode extension's webview panel over a raw Chrome DevTools Protocol connection,
// bypassing Playwright entirely.
//
// Why raw CDP: `playwright-core`'s own frame discovery (`page.frames()` after
// `chromium.connectOverCDP(...)`) does not reliably surface the webview's real content frame in
// this harness's environment. Diagnosed via manual CDP inspection (see the task report this
// module's history is attached to): VSCode renders each webview panel as a CDP target of type
// `"iframe"` (never a top-level `"page"` target — `context.pages()` never sees it), and *within*
// that target's own frame tree there are two nested frames sharing the `vscode-webview://`
// origin: an outer sandbox "presenter" frame (this target's root frame — never holds any real
// DOM) wrapping a second, inner frame that's the extension's actual rendered content. Even after
// correctly attaching to the outer target, Playwright's tracking of that inner frame's URL and
// execution context was observed to succeed quickly in some runs and never resolve at all within
// a 20+ second window in others — a reliability gap in Playwright's OOPIF/nested-frame handling
// for an externally-`connectOverCDP`'d Electron instance, not something fixable from this
// package's own code. Talking to the target's CDP session directly sidesteps that gap: this
// module attaches to the webview target with `Target.attachToTarget({ flatten: true })`, walks
// `Page.getFrameTree` itself to find the nested content frame, waits for that frame's own
// `Runtime.executionContextCreated` event, and evaluates DOM operations directly in that context.
//
// Everything here is polled against a deadline, one budget per stage. The inner content frame is
// created at runtime by VSCode's own webview-host JavaScript *after* the outer target already
// exists, and that gap was measured anywhere between ~2s and ~24s on a loaded machine — so a
// one-shot `Page.getFrameTree` right after attaching regularly finds only the outer wrapper (or
// throws outright). Worse, the wrapper is itself a leaf frame with a `vscode-webview://` URL
// while it has no children, so a naive "first leaf with that URL prefix" pick silently returns
// the empty wrapper and every later selector times out. `findContentFrame` therefore only
// accepts a *nested* (depth >= 1) leaf frame that already has a live execution context with a
// real `document.body`, and keeps re-polling until one appears.

const TARGET_DISCOVERY_TIMEOUT_MS = 15000;
const CONTENT_FRAME_TIMEOUT_MS = 45000;
const CDP_REQUEST_TIMEOUT_MS = 10000;
const POLL_INTERVAL_MS = 250;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

interface CdpMessage {
  id?: number;
  method?: string;
  params?: unknown;
  result?: unknown;
  error?: { message: string };
  sessionId?: string;
}

interface CdpErrorDetails {
  text: string;
  exception?: { description?: string };
}

class CdpConnection {
  private nextId = 1;
  private closed = false;
  private readonly pending = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (error: Error) => void; timer: NodeJS.Timeout }
  >();
  private readonly eventListeners: Array<(message: CdpMessage) => void> = [];

  private constructor(private readonly ws: WebSocket) {
    ws.addEventListener("message", (event: MessageEvent) => {
      const message = JSON.parse(String(event.data)) as CdpMessage;
      if (message.id !== undefined && this.pending.has(message.id)) {
        const waiter = this.pending.get(message.id);
        this.pending.delete(message.id);
        if (!waiter) return;
        clearTimeout(waiter.timer);
        if (message.error) waiter.reject(new Error(message.error.message));
        else waiter.resolve(message.result);
      } else if (message.method) {
        for (const listener of this.eventListeners) listener(message);
      }
    });
    ws.addEventListener("close", () => {
      this.closed = true;
      this.rejectAllPending(new Error("CDP connection closed"));
    });
  }

  private rejectAllPending(error: Error): void {
    for (const [, waiter] of this.pending) {
      clearTimeout(waiter.timer);
      waiter.reject(error);
    }
    this.pending.clear();
  }

  static async connect(webSocketUrl: string): Promise<CdpConnection> {
    const ws = new WebSocket(webSocketUrl);
    await new Promise<void>((resolve, reject) => {
      ws.addEventListener("open", () => resolve(), { once: true });
      ws.addEventListener(
        "error",
        () => reject(new Error(`failed to connect to ${webSocketUrl}`)),
        { once: true },
      );
    });
    return new CdpConnection(ws);
  }

  // Every request carries its own timeout. A CDP call that never gets answered would otherwise
  // hang forever and blow the Mocha timeout instead of the caller's own deadline — and inside a
  // poll loop, a rejected call is simply retried on the next tick.
  send<T>(
    method: string,
    params: Record<string, unknown> = {},
    sessionId?: string,
    timeoutMs = CDP_REQUEST_TIMEOUT_MS,
  ): Promise<T> {
    if (this.closed) return Promise.reject(new Error("CDP connection closed"));
    const id = this.nextId++;
    const payload: CdpMessage = { id, method, params };
    if (sessionId) payload.sessionId = sessionId;
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`CDP request ${method} timed out after ${timeoutMs}ms`));
      }, timeoutMs);
      timer.unref?.();
      this.pending.set(id, {
        resolve: resolve as (value: unknown) => void,
        reject,
        timer,
      });
      this.ws.send(JSON.stringify(payload));
    });
  }

  onEvent(listener: (message: CdpMessage) => void): void {
    this.eventListeners.push(listener);
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.rejectAllPending(new Error("CDP connection closed"));
    try {
      this.ws.close();
    } catch {
      // Already closing/closed — nothing to do.
    }
  }
}

export interface WebviewSession {
  waitForSelector(selector: string, timeoutMs?: number): Promise<void>;
  click(selector: string): Promise<void>;
  fill(selector: string, value: string): Promise<void>;
  clickByText(tag: string, text: string): Promise<void>;
  /** Like `clickByText`, but matches elements whose text merely contains `text`. */
  clickByTextContaining(tag: string, text: string): Promise<void>;
  /** Clicks the `index`-th match (negative counts from the end) of `selector`. */
  clickBySelector(selector: string, index?: number): Promise<void>;
  contextMenuByText(tag: string, text: string): Promise<void>;
  waitForText(tag: string, text: string, timeoutMs?: number): Promise<void>;
  /** The page's visible text, for failure messages. */
  bodyText(): Promise<string>;
  /** The token colours the packaged highlight worker returns for a line of Rust (#843). */
  highlightWorkerColours(timeoutMs?: number): Promise<string[]>;
  /** The `name` attribute of every match of `selector` (missing attributes as ""). */
  attributes(selector: string, name: string): Promise<string[]>;
  /** The current `value` of every text field matching `selector` (what was typed, not the attribute). */
  values(selector: string): Promise<string[]>;
  /** Detaches from the webview target and closes the underlying CDP socket. Idempotent. */
  close(): Promise<void>;
}

class CdpWebviewSession implements WebviewSession {
  constructor(
    private readonly connection: CdpConnection,
    private readonly sessionId: string,
    private readonly contextId: number,
  ) {}

  private async evaluate<T>(expression: string): Promise<T> {
    const response = await this.connection.send<{
      result: { value: T };
      exceptionDetails?: CdpErrorDetails;
    }>(
      "Runtime.evaluate",
      { expression, contextId: this.contextId, returnByValue: true, awaitPromise: true },
      this.sessionId,
    );
    if (response.exceptionDetails) {
      const detail = response.exceptionDetails;
      throw new Error(`webview evaluate failed: ${detail.exception?.description ?? detail.text}`);
    }
    return response.result.value;
  }

  async waitForSelector(selector: string, timeoutMs = 15000): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const found = await this.evaluate<boolean>(
        `!!document.querySelector(${JSON.stringify(selector)})`,
      );
      if (found) return;
      await sleep(POLL_INTERVAL_MS);
    }
    throw new Error(`selector ${selector} not found within ${timeoutMs}ms`);
  }

  async click(selector: string): Promise<void> {
    await this.evaluate(`
      (function () {
        var el = document.querySelector(${JSON.stringify(selector)});
        if (!el) throw new Error(${JSON.stringify(`not found: ${selector}`)});
        el.click();
      })()
    `);
  }

  async fill(selector: string, value: string): Promise<void> {
    // Assigning `.value` directly bypasses React's synthetic event system, so the app's
    // controlled input never sees the change. Use the native value setter plus a dispatched
    // `input` event instead — the same technique `e2e/specs/first-flow.spec.ts`'s WebdriverIO
    // version uses via `browser.execute`.
    await this.evaluate(`
      (function () {
        var el = document.querySelector(${JSON.stringify(selector)});
        if (!el) throw new Error(${JSON.stringify(`not found: ${selector}`)});
        var setter = Object.getOwnPropertyDescriptor(
          window.HTMLTextAreaElement.prototype,
          "value",
        ).set;
        setter.call(el, ${JSON.stringify(value)});
        el.dispatchEvent(new Event("input", { bubbles: true }));
      })()
    `);
  }

  async clickByText(tag: string, text: string): Promise<void> {
    await this.evaluate(`
      (function () {
        var els = Array.from(document.querySelectorAll(${JSON.stringify(tag)}));
        var el = els.find(function (candidate) {
          return candidate.textContent && candidate.textContent.trim() === ${JSON.stringify(text)};
        });
        if (!el) throw new Error(${JSON.stringify(`no <${tag}> with text ${text}`)});
        el.click();
      })()
    `);
  }

  async clickByTextContaining(tag: string, text: string): Promise<void> {
    await this.evaluate(`
      (function () {
        var els = Array.from(document.querySelectorAll(${JSON.stringify(tag)}));
        var el = els.find(function (candidate) {
          return candidate.textContent && candidate.textContent.indexOf(${JSON.stringify(text)}) !== -1;
        });
        if (!el) throw new Error(${JSON.stringify(`no <${tag}> containing ${text}`)});
        el.click();
      })()
    `);
  }

  async clickBySelector(selector: string, index = 0): Promise<void> {
    await this.evaluate(`
      (function () {
        var els = Array.from(document.querySelectorAll(${JSON.stringify(selector)}));
        var i = ${index} < 0 ? els.length + ${index} : ${index};
        var el = els[i];
        if (!el) throw new Error(${JSON.stringify(`no match ${index} for ${selector}`)});
        el.click();
      })()
    `);
  }

  async contextMenuByText(tag: string, text: string): Promise<void> {
    await this.evaluate(`
      (function () {
        var els = Array.from(document.querySelectorAll(${JSON.stringify(tag)}));
        var el = els.find(function (candidate) {
          return candidate.textContent && candidate.textContent.indexOf(${JSON.stringify(text)}) !== -1;
        });
        if (!el) throw new Error(${JSON.stringify(`no <${tag}> containing ${text}`)});
        el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
      })()
    `);
  }

  async waitForText(tag: string, text: string, timeoutMs = 15000): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const found = await this.evaluate<boolean>(`
        Array.from(document.querySelectorAll(${JSON.stringify(tag)})).some(function (el) {
          return el.textContent && el.textContent.indexOf(${JSON.stringify(text)}) !== -1;
        })
      `);
      if (found) return;
      await sleep(POLL_INTERVAL_MS);
    }
    throw new Error(`no <${tag}> containing "${text}" found within ${timeoutMs}ms`);
  }

  /** The page's visible text, for failure messages. */
  /**
   * Starts the packaged syntax highlighting worker the way the app does (#843): fetches
   * `highlight-worker.js` next to the bundle's script, starts it from a blob: URL and asks it
   * to tokenize a line of Rust. Returns the tokens' colours; throws if the CSP or the package
   * stops the worker.
   */
  async highlightWorkerColours(timeoutMs = 30000): Promise<string[]> {
    return this.evaluate<string[]>(`
      (async () => {
        const script = document.querySelector('script[src*="vscode-main.js"]');
        if (!script) throw new Error("no vscode-main.js script element");
        const response = await fetch(new URL("highlight-worker.js", script.src).href);
        if (!response.ok) throw new Error("highlight-worker.js: HTTP " + response.status);
        const url = URL.createObjectURL(new Blob([await response.text()], { type: "text/javascript" }));
        const worker = new Worker(url);
        try {
          return await new Promise((resolve, reject) => {
            const timer = setTimeout(() => reject(new Error("no reply from the highlight worker")), ${timeoutMs});
            worker.addEventListener("error", (event) => {
              clearTimeout(timer);
              reject(new Error("highlight worker failed: " + (event.message || "blocked")));
            });
            worker.addEventListener("message", (event) => {
              clearTimeout(timer);
              if (event.data.error) reject(new Error(event.data.error));
              else resolve(event.data.tokens.flat().map((t) => String(t.color)));
            });
            worker.postMessage({ type: "tokenize", id: 1, lang: "rust", code: "fn main() {}" });
          });
        } finally {
          worker.terminate();
          URL.revokeObjectURL(url);
        }
      })()
    `);
  }

  async bodyText(): Promise<string> {
    return this.evaluate<string>("document.body ? document.body.innerText : ''");
  }

  async attributes(selector: string, name: string): Promise<string[]> {
    return this.evaluate<string[]>(`
      Array.from(document.querySelectorAll(${JSON.stringify(selector)})).map(function (el) {
        return el.getAttribute(${JSON.stringify(name)}) || "";
      })
    `);
  }

  async values(selector: string): Promise<string[]> {
    return this.evaluate<string[]>(`
      Array.from(document.querySelectorAll(${JSON.stringify(selector)})).map(function (el) {
        return el.value === undefined ? "" : String(el.value);
      })
    `);
  }

  async close(): Promise<void> {
    try {
      await this.connection.send("Target.detachFromTarget", { sessionId: this.sessionId });
    } catch {
      // The target may already be gone (window closed, extension host tearing down) — the
      // socket close below is what actually matters.
    }
    this.connection.close();
  }
}

interface TargetInfo {
  targetId: string;
  type: string;
  url: string;
}

interface FrameInfo {
  id: string;
  url: string;
}

interface FrameTreeNode {
  frame: FrameInfo;
  childFrames?: FrameTreeNode[];
}

// Leaf frames *below* the tree's root. The attached target's root frame is VSCode's outer
// sandbox wrapper, which shares the `vscode-webview://` origin but holds no application DOM;
// while the real content frame has not been created yet, that wrapper is itself a childless leaf
// and would match any URL-prefix-only filter. Excluding the root makes the pick unambiguous.
function nestedLeafFrames(root: FrameTreeNode): FrameInfo[] {
  const acc: FrameInfo[] = [];
  const visit = (node: FrameTreeNode): void => {
    if (!node.childFrames || node.childFrames.length === 0) {
      acc.push(node.frame);
      return;
    }
    for (const child of node.childFrames) visit(child);
  };
  for (const child of root.childFrames ?? []) visit(child);
  return acc;
}

async function fetchWebSocketDebuggerUrl(cdpHttpUrl: string, timeoutMs: number): Promise<string> {
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${cdpHttpUrl}/json/version`);
      const { webSocketDebuggerUrl } = (await response.json()) as {
        webSocketDebuggerUrl?: string;
      };
      if (webSocketDebuggerUrl) return webSocketDebuggerUrl;
    } catch (error) {
      lastError = error;
    }
    await sleep(POLL_INTERVAL_MS);
  }
  throw new Error(
    `CDP endpoint ${cdpHttpUrl} did not report a websocket debugger url within ${timeoutMs}ms` +
      (lastError ? ` (last error: ${String(lastError)})` : ""),
  );
}

// The `src` of every iframe in VSCode's workbench windows (the "page" targets). Each webview
// panel is one such iframe, and its CDP target's URL is that `src`. Closing a panel removes its
// iframe from the workbench at once, while the browser can still list the closed panel's target
// for a moment afterwards (#917), so a webview target is only live if its URL is in this set.
// One flattened session per workbench window, attached on first use and reused. A window that
// can't be read (closing, reloading, or not a workbench) is skipped for this poll, so it can't
// hide the others; its session is detached and attached afresh next time. No CDP call starts
// after `deadline`, and none is allowed to run past it.
async function workbenchIframeSources(
  connection: CdpConnection,
  targetInfos: TargetInfo[],
  pageSessions: Map<string, string>,
  deadline: number,
  onPageError: (error: unknown) => void,
): Promise<Set<string>> {
  const sources = new Set<string>();
  const callTimeout = () => Math.min(CDP_REQUEST_TIMEOUT_MS, deadline - Date.now());
  for (const page of targetInfos.filter((t) => t.type === "page")) {
    if (callTimeout() <= 0) break;
    let sessionId = pageSessions.get(page.targetId);
    try {
      if (sessionId === undefined) {
        ({ sessionId } = await connection.send<{ sessionId: string }>(
          "Target.attachToTarget",
          { targetId: page.targetId, flatten: true },
          undefined,
          callTimeout(),
        ));
        pageSessions.set(page.targetId, sessionId);
      }
      if (callTimeout() <= 0) break;
      const response = await connection.send<{ result: { value?: string[] } }>(
        "Runtime.evaluate",
        {
          expression: "Array.from(document.querySelectorAll('iframe'), (f) => f.src)",
          returnByValue: true,
        },
        sessionId,
        callTimeout(),
      );
      for (const src of response.result.value ?? []) sources.add(src);
    } catch (error) {
      onPageError(error);
      pageSessions.delete(page.targetId);
      if (sessionId !== undefined) {
        // Best effort: a target that is already gone has detached the session itself.
        connection.send("Target.detachFromTarget", { sessionId }).catch(() => undefined);
      }
    }
  }
  return sources;
}

async function findWebviewTarget(
  connection: CdpConnection,
  timeoutMs: number,
  goneTargets: ReadonlySet<string>,
  pageSessions: Map<string, string>,
): Promise<string> {
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown;
  while (Date.now() < deadline) {
    // Only the last poll's error explains a timeout; an earlier one (a window that was
    // closing) would point at the wrong cause.
    lastError = undefined;
    try {
      const { targetInfos } = await connection.send<{ targetInfos: TargetInfo[] }>(
        "Target.getTargets",
        {},
        undefined,
        Math.min(CDP_REQUEST_TIMEOUT_MS, Math.max(1, deadline - Date.now())),
      );
      const candidates = targetInfos.filter(
        (t) =>
          t.type === "iframe" &&
          t.url.startsWith("vscode-webview://") &&
          !goneTargets.has(t.targetId),
      );
      if (candidates.length > 0) {
        const live = await workbenchIframeSources(
          connection,
          targetInfos,
          pageSessions,
          deadline,
          (error) => {
            lastError = error;
          },
        );
        const target = candidates.find((t) => live.has(t.url));
        if (target) return target.targetId;
      }
    } catch (error) {
      // Transient — retry until the deadline.
      lastError = error;
    }
    await sleep(POLL_INTERVAL_MS);
  }
  throw new Error(
    `No live vscode-webview:// target found within ${timeoutMs}ms` +
      (lastError ? ` (last error: ${String(lastError)})` : ""),
  );
}

// Polls until the webview's real content frame exists *and* has a usable execution context.
// These two conditions are polled together on purpose: the frame tree and the
// `Runtime.executionContextCreated` stream are separate signals that settle at different times,
// and either one alone can be satisfied by a frame that is not yet scriptable.
async function findContentFrameContext(
  connection: CdpConnection,
  sessionId: string,
  executionContextsByFrame: Map<string, number>,
  timeoutMs: number,
  targetGone: () => boolean,
): Promise<number> {
  const deadline = Date.now() + timeoutMs;
  // `Page.createIsolatedWorld` is the fallback for a frame whose default-world
  // `Runtime.executionContextCreated` event never reaches this session (observed when the frame
  // is hosted out-of-process, in which case its context events belong to a different target).
  // An isolated world shares the frame's DOM — which is all this module ever touches — so it is
  // equivalent for our purposes, and it is created on demand rather than awaited. Cached per
  // frame so the poll loop doesn't create a new world every 250ms.
  const isolatedWorldsByFrame = new Map<string, number>();
  const contextForFrame = async (frameId: string): Promise<number | undefined> => {
    const defaultWorld = executionContextsByFrame.get(frameId);
    if (defaultWorld !== undefined) return defaultWorld;
    const cached = isolatedWorldsByFrame.get(frameId);
    if (cached !== undefined) return cached;
    try {
      const { executionContextId } = await connection.send<{ executionContextId: number }>(
        "Page.createIsolatedWorld",
        { frameId, worldName: "browsitory-e2e" },
        sessionId,
      );
      isolatedWorldsByFrame.set(frameId, executionContextId);
      return executionContextId;
    } catch {
      return undefined;
    }
  };

  let lastError: unknown;
  while (Date.now() < deadline) {
    // The target closed (its session detached): no frame will ever appear in it.
    if (targetGone()) throw new WebviewTargetGoneError(`session ${sessionId} detached`);
    try {
      const { frameTree } = await connection.send<{ frameTree: FrameTreeNode }>(
        "Page.getFrameTree",
        {},
        sessionId,
      );
      const candidates = nestedLeafFrames(frameTree).filter((f) =>
        f.url.startsWith("vscode-webview://"),
      );
      for (const frame of candidates) {
        const contextId = await contextForFrame(frame.id);
        if (contextId === undefined) continue;
        // A context id alone isn't enough — an about:blank placeholder document gets one too.
        // Require a real body before declaring this the content frame.
        const probe = await connection.send<{
          result: { value?: boolean };
          exceptionDetails?: CdpErrorDetails;
        }>(
          "Runtime.evaluate",
          { expression: "!!(document.body)", contextId, returnByValue: true },
          sessionId,
        );
        if (!probe.exceptionDetails && probe.result.value === true) return contextId;
        // A stale isolated world (its document navigated away) can never recover — drop it so
        // the next poll makes a fresh one.
        if (probe.exceptionDetails) isolatedWorldsByFrame.delete(frame.id);
      }
    } catch (error) {
      // `Page.getFrameTree` throws outright if called before the target's page domain is ready,
      // and `Runtime.evaluate` throws if a context is destroyed mid-poll (navigation). Both are
      // expected transients here — retry until the deadline rather than failing the run.
      lastError = error;
    }
    await sleep(POLL_INTERVAL_MS);
  }
  throw new Error(
    `No scriptable vscode-webview:// content frame found within ${timeoutMs}ms` +
      (lastError ? ` (last error: ${String(lastError)})` : ""),
  );
}

// The webview target went away while this module was attaching to it (#917). Not a failure of
// the webview under test: `connectToWebview` drops the target and looks again.
class WebviewTargetGoneError extends Error {}

export async function connectToWebview(
  cdpHttpUrl: string,
  targetTimeoutMs = TARGET_DISCOVERY_TIMEOUT_MS,
  contentFrameTimeoutMs = CONTENT_FRAME_TIMEOUT_MS,
): Promise<WebviewSession> {
  const webSocketDebuggerUrl = await fetchWebSocketDebuggerUrl(cdpHttpUrl, targetTimeoutMs);
  const connection = await CdpConnection.connect(webSocketDebuggerUrl);

  // #917: a spec that closes the panel and opens a new one (`workbench.action.closeAllEditors`,
  // then `browsitory.open`) can see the old panel's target still listed while the browser tears
  // it down. Attaching to it then fails with "No target with given id found", or worse, succeeds
  // and drives a panel that is about to go away. So discovery only picks a target whose iframe
  // is still in the workbench (`workbenchIframeSources`), and a target that goes away anyway
  // between that check and the attach, or while its content frame is polled (its session
  // detaches), is skipped and discovery runs again, within the same overall budget the two
  // stages had before.
  const deadline = Date.now() + targetTimeoutMs + contentFrameTimeoutMs;
  const goneTargets = new Set<string>();
  const pageSessions = new Map<string, string>();
  const detachedSessions = new Set<string>();
  connection.onEvent((message) => {
    if (message.method !== "Target.detachedFromTarget") return;
    const { sessionId } = message.params as { sessionId?: string };
    if (sessionId) detachedSessions.add(sessionId);
  });

  try {
    for (;;) {
      const remaining = deadline - Date.now();
      if (remaining <= 0) {
        throw new Error(
          `every vscode-webview:// target went away before its content frame was found ` +
            `(${goneTargets.size} tried)`,
        );
      }
      // 1. Find the webview's own CDP target (never a top-level "page" target — see the module
      // comment above). Own timeout budget.
      const webviewTargetId = await findWebviewTarget(
        connection,
        Math.min(targetTimeoutMs, remaining),
        goneTargets,
        pageSessions,
      );
      try {
        return await attachToWebview(
          connection,
          webviewTargetId,
          detachedSessions,
          Math.min(contentFrameTimeoutMs, deadline - Date.now()),
        );
      } catch (error) {
        if (!(error instanceof WebviewTargetGoneError)) throw error;
        goneTargets.add(webviewTargetId);
      }
    }
  } catch (error) {
    connection.close();
    throw error;
  }
}

async function targetExists(connection: CdpConnection, targetId: string): Promise<boolean> {
  const { targetInfos } = await connection.send<{ targetInfos: TargetInfo[] }>("Target.getTargets");
  return targetInfos.some((t) => t.targetId === targetId);
}

async function attachToWebview(
  connection: CdpConnection,
  webviewTargetId: string,
  detachedSessions: Set<string>,
  contentFrameTimeoutMs: number,
): Promise<WebviewSession> {
  // 2. Attach to it directly with a dedicated flattened CDP session. A target that is no
  // longer listed went away between `Target.getTargets` and this call.
  let sessionId: string;
  try {
    ({ sessionId } = await connection.send<{ sessionId: string }>("Target.attachToTarget", {
      targetId: webviewTargetId,
      flatten: true,
    }));
  } catch (error) {
    if (!(await targetExists(connection, webviewTargetId))) {
      throw new WebviewTargetGoneError(String(error));
    }
    throw error;
  }
  const gone = () => detachedSessions.has(sessionId);

  // Register the context listener *before* enabling Runtime: `Runtime.enable` replays the
  // already-existing contexts as `executionContextCreated` events, and those would otherwise
  // be missed.
  const executionContextsByFrame = new Map<string, number>();
  connection.onEvent((message) => {
    if (message.sessionId !== sessionId) return;
    const params = message.params as {
      context?: { id: number; auxData?: { frameId?: string; isDefault?: boolean } };
      executionContextId?: number;
    };
    if (message.method === "Runtime.executionContextCreated" && params.context) {
      const frameId = params.context.auxData?.frameId;
      // Prefer the frame's default world context (not an isolated one) — that's the one the
      // page's own script (React, etc.) actually runs in.
      if (frameId && params.context.auxData?.isDefault) {
        executionContextsByFrame.set(frameId, params.context.id);
      }
    } else if (message.method === "Runtime.executionContextDestroyed") {
      for (const [frameId, contextId] of executionContextsByFrame) {
        if (contextId === params.executionContextId) executionContextsByFrame.delete(frameId);
      }
    } else if (message.method === "Runtime.executionContextsCleared") {
      executionContextsByFrame.clear();
    }
  });

  try {
    await connection.send("Page.enable", {}, sessionId);
    await connection.send("Runtime.enable", {}, sessionId);
  } catch (error) {
    if (gone()) throw new WebviewTargetGoneError(String(error));
    throw error;
  }

  // 3. VSCode's webview-host JS creates the real content frame inside this target's root frame
  // some time *after* the target itself exists (observed: ~2s to 24s). Poll the frame tree and
  // the execution-context stream together, against this stage's own timeout budget.
  const contextId = await findContentFrameContext(
    connection,
    sessionId,
    executionContextsByFrame,
    contentFrameTimeoutMs,
    gone,
  );

  return new CdpWebviewSession(connection, sessionId, contextId);
}
