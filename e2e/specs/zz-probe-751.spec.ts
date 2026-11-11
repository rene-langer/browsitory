// TEMPORARY probe for #751 (WebDriver clicks near the window's bottom edge). Logs only, never
// fails. Removed before the PR is ready.

type Hit = { y: number; got: number[] };

async function geometry(label: string) {
  const rect = await browser.getWindowRect();
  const page = await browser.execute(() =>
    JSON.stringify({
      inner: [window.innerWidth, window.innerHeight],
      outer: [window.outerWidth, window.outerHeight],
      screen: [screen.width, screen.height, screen.availWidth, screen.availHeight],
      screenXY: [window.screenX, window.screenY],
      dpr: window.devicePixelRatio,
      visual: window.visualViewport ? [window.visualViewport.width, window.visualViewport.height, window.visualViewport.offsetTop] : null,
      doc: [document.documentElement.clientHeight, document.documentElement.scrollHeight, document.body.scrollHeight],
      scroll: [window.scrollX, window.scrollY],
    }),
  );
  console.log(`[probe751] ${label} windowRect=${JSON.stringify(rect)} page=${page}`);
}

async function installOverlay() {
  await browser.execute(() => {
    const old = document.getElementById("probe751");
    old?.remove();
    const overlay = document.createElement("div");
    overlay.id = "probe751";
    overlay.style.cssText = "position:fixed;inset:0;z-index:2147483647;background:rgba(255,0,0,0.05)";
    const w = window as unknown as { probe751: number[] };
    w.probe751 = [];
    overlay.addEventListener("click", (event) => w.probe751.push(Math.round(event.clientY)));
    document.body.appendChild(overlay);
  });
}

async function sweep(label: string) {
  await installOverlay();
  const hits: Hit[] = [];
  for (const y of [10, 400, 700, 730, 740, 745, 750, 755, 757, 760, 765, 770, 780, 790, 799]) {
    await browser.execute(() => {
      (window as unknown as { probe751: number[] }).probe751 = [];
    });
    try {
      await browser.performActions([
        {
          type: "pointer",
          id: "probe-mouse",
          parameters: { pointerType: "mouse" },
          actions: [
            { type: "pointerMove", duration: 0, x: 600, y, origin: "viewport" },
            { type: "pointerDown", button: 0 },
            { type: "pointerUp", button: 0 },
          ],
        },
      ]);
      await browser.releaseActions();
    } catch (error) {
      console.log(`[probe751] ${label} y=${y} performActions error: ${String(error).slice(0, 200)}`);
    }
    await browser.pause(50);
    const got = await browser.execute(() => (window as unknown as { probe751: number[] }).probe751);
    hits.push({ y, got });
  }
  console.log(`[probe751] ${label} sweep=${JSON.stringify(hits)}`);

  // WebDriver's element click on small boxes at several heights.
  const results: string[] = [];
  for (const top of [700, 740, 750, 756, 760, 770, 780]) {
    await browser.execute((t: number) => {
      document.getElementById("probe751-box")?.remove();
      const box = document.createElement("button");
      box.id = "probe751-box";
      box.textContent = "probe";
      box.style.cssText = `position:fixed;left:500px;top:${t}px;width:100px;height:16px;z-index:2147483647`;
      (window as unknown as { probe751box: number }).probe751box = 0;
      box.addEventListener("click", () => {
        (window as unknown as { probe751box: number }).probe751box += 1;
      });
      document.body.appendChild(box);
    }, top);
    try {
      await (await $("#probe751-box")).click();
    } catch (error) {
      results.push(`${top}:error ${String(error).slice(0, 120)}`);
      continue;
    }
    await browser.pause(50);
    const n = await browser.execute(() => (window as unknown as { probe751box: number }).probe751box);
    results.push(`${top}:${n}`);
  }
  console.log(`[probe751] ${label} elementClick=${JSON.stringify(results)}`);
  await browser.execute(() => {
    document.getElementById("probe751")?.remove();
    document.getElementById("probe751-box")?.remove();
  });
}

describe("probe #751", () => {
  it("logs where WebDriver clicks arrive", async () => {
    await browser.refresh();
    await browser.pause(500);
    await geometry("start");
    await sweep("start");
    try {
      await browser.setWindowRect(0, 0, 1200, 800);
    } catch (error) {
      console.log(`[probe751] setWindowRect error ${String(error).slice(0, 200)}`);
    }
    await browser.pause(500);
    await geometry("moved to 0,0");
    await sweep("moved to 0,0");
  });
});
