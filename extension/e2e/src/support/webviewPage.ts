// Drives the VSCode extension's webview panel over a raw Chrome DevTools Protocol connection,
// bypassing Playwright entirely.
//
// Why not Playwright: `playwright-core`'s own frame discovery (`page.frames()` after
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
  private readonly pending = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (error: Error) => void }
  >();
  private readonly eventListeners: Array<(message: CdpMessage) => void> = [];

  private constructor(private readonly ws: WebSocket) {
    ws.addEventListener("message", (event: MessageEvent) => {
      const message = JSON.parse(String(event.data)) as CdpMessage;
      if (message.id !== undefined && this.pending.has(message.id)) {
        const waiter = this.pending.get(message.id);
        this.pending.delete(message.id);
        if (!waiter) return;
        if (message.error) waiter.reject(new Error(message.error.message));
        else waiter.resolve(message.result);
      } else if (message.method) {
        for (const listener of this.eventListeners) listener(message);
      }
    });
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

  send<T>(method: string, params: Record<string, unknown> = {}, sessionId?: string): Promise<T> {
    const id = this.nextId++;
    const payload: CdpMessage = { id, method, params };
    if (sessionId) payload.sessionId = sessionId;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
      this.ws.send(JSON.stringify(payload));
    });
  }

  onEvent(listener: (message: CdpMessage) => void): void {
    this.eventListeners.push(listener);
  }
}

export interface WebviewSession {
  waitForSelector(selector: string, timeoutMs?: number): Promise<void>;
  click(selector: string): Promise<void>;
  fill(selector: string, value: string): Promise<void>;
  clickByText(tag: string, text: string): Promise<void>;
  waitForText(tag: string, text: string, timeoutMs?: number): Promise<void>;
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
      await new Promise((resolve) => setTimeout(resolve, 250));
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

  async waitForText(tag: string, text: string, timeoutMs = 15000): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const found = await this.evaluate<boolean>(`
        Array.from(document.querySelectorAll(${JSON.stringify(tag)})).some(function (el) {
          return el.textContent && el.textContent.indexOf(${JSON.stringify(text)}) !== -1;
        })
      `);
      if (found) return;
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
    throw new Error(`no <${tag}> containing "${text}" found within ${timeoutMs}ms`);
  }
}

interface TargetInfo {
  targetId: string;
  type: string;
  url: string;
}

interface FrameTreeNode {
  frame: { id: string; url: string };
  childFrames?: FrameTreeNode[];
}

function leafFrames(node: FrameTreeNode, acc: FrameTreeNode["frame"][] = []): FrameTreeNode["frame"][] {
  if (!node.childFrames || node.childFrames.length === 0) {
    acc.push(node.frame);
  } else {
    for (const child of node.childFrames) leafFrames(child, acc);
  }
  return acc;
}

export async function connectToWebview(
  cdpHttpUrl: string,
  timeoutMs = 15000,
): Promise<WebviewSession> {
  const versionResponse = await fetch(`${cdpHttpUrl}/json/version`);
  const { webSocketDebuggerUrl } = (await versionResponse.json()) as {
    webSocketDebuggerUrl: string;
  };
  const connection = await CdpConnection.connect(webSocketDebuggerUrl);
  const deadline = Date.now() + timeoutMs;

  // 1. Find the webview's own CDP target (never a top-level "page" target — see the module
  // comment above).
  let webviewTargetId: string | undefined;
  while (Date.now() < deadline) {
    const { targetInfos } = await connection.send<{ targetInfos: TargetInfo[] }>(
      "Target.getTargets",
    );
    const target = targetInfos.find(
      (t) => t.type === "iframe" && t.url.startsWith("vscode-webview://"),
    );
    if (target) {
      webviewTargetId = target.targetId;
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  if (!webviewTargetId) {
    throw new Error(`No vscode-webview:// target found within ${timeoutMs}ms`);
  }

  // 2. Attach to it directly with a dedicated flattened CDP session.
  const { sessionId } = await connection.send<{ sessionId: string }>("Target.attachToTarget", {
    targetId: webviewTargetId,
    flatten: true,
  });

  const executionContextsByFrame = new Map<string, number>();
  connection.onEvent((message) => {
    if (message.sessionId !== sessionId || message.method !== "Runtime.executionContextCreated") {
      return;
    }
    const params = message.params as {
      context: { id: number; auxData?: { frameId?: string; isDefault?: boolean } };
    };
    const frameId = params.context.auxData?.frameId;
    // Prefer the frame's default world context (not an isolated one) — that's the one the
    // page's own script (React, etc.) actually runs in.
    if (frameId && params.context.auxData?.isDefault) {
      executionContextsByFrame.set(frameId, params.context.id);
    }
  });

  await connection.send("Page.enable", {}, sessionId);
  await connection.send("Runtime.enable", {}, sessionId);

  // 3. VSCode nests the webview's real content one level deeper than this attached target's own
  // root frame — walk this target's own frame tree and take the leaf.
  const { frameTree } = await connection.send<{ frameTree: FrameTreeNode }>(
    "Page.getFrameTree",
    {},
    sessionId,
  );
  const contentFrame = leafFrames(frameTree).find((f) => f.url.startsWith("vscode-webview://"));
  if (!contentFrame) {
    throw new Error("vscode-webview:// target has no nested content frame");
  }

  // 4. Wait for that frame's own execution context — it may not exist yet at attach time.
  let contextId: number | undefined;
  while (Date.now() < deadline) {
    contextId = executionContextsByFrame.get(contentFrame.id);
    if (contextId !== undefined) break;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  if (contextId === undefined) {
    throw new Error(`No execution context for the webview content frame found within ${timeoutMs}ms`);
  }

  return new CdpWebviewSession(connection, sessionId, contextId);
}
