// The targets of the plan's decision gate. Numbers go to perf-results.json next to the test output.

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { _electron, expect, type Page, test } from "@playwright/test";
import { REPO, serveStatic, TestDaemon, WEB_DIST } from "./daemon";
import { approval, notification, scriptServer } from "./fake-server";

const results: Record<string, unknown> = {};

/** Scrolls `selector` from top to bottom while recording every frame's length. */
async function scrollFrames(page: Page, selector: string, ms: number) {
  return page.evaluate(
    async ({ selector, ms }) => {
      const element = document.querySelector(selector) as HTMLElement;
      let longTasks = 0;
      let longest = 0;
      const observer = new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          longTasks++;
          longest = Math.max(longest, entry.duration);
        }
      });
      observer.observe({ entryTypes: ["longtask"] });
      const frames: number[] = [];
      const start = performance.now();
      let last = start;
      await new Promise<void>((done) => {
        const step = (now: number) => {
          frames.push(now - last);
          last = now;
          const t = (now - start) / ms;
          element.scrollTop = (element.scrollHeight - element.clientHeight) * Math.min(1, t);
          if (t >= 1) done();
          else requestAnimationFrame(step);
        };
        requestAnimationFrame(step);
      });
      frames.shift();
      observer.disconnect();
      const sorted = [...frames].sort((a, b) => a - b);
      return {
        frames: frames.length,
        over33ms: frames.filter((f) => f > 33).length,
        p50: Math.round(sorted[Math.floor(sorted.length * 0.5)] ?? 0),
        p99: Math.round(sorted[Math.floor(sorted.length * 0.99)] ?? 0),
        worst: Math.round(sorted[sorted.length - 1] ?? 0),
        longTasks,
        longestTaskMs: Math.round(longest),
      };
    },
    { selector, ms },
  );
}

test.afterAll(() => {
  const dir = join(REPO, "clients/e2e/test-results");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "perf-results.json"), JSON.stringify(results, null, 2));
  console.log(JSON.stringify(results, null, 2));
});

test("the Inbox with 5,000 rows, in a browser", async ({ page }) => {
  const site = await serveStatic(WEB_DIST);
  try {
    await scriptServer(page, {
      approvals: [approval("a")],
      notifications: Array.from({ length: 5000 }, (_, i) => notification(i)),
      projects: [
        { id: "p", name: "Northwind API", status: "active", created_at_ms: 0, employees_active: 0, version: 1 },
      ],
      limits: [{ label: "Weekly limit", percent: 58, resets: "Resets Mon 09:00" }],
    });
    await page.goto(`${site.origin}/inbox`);
    await expect(page.getByRole("heading", { name: "Flux wants to run a command" })).toBeVisible();
    results.inboxScroll5000Rows = await scrollFrames(page, 'section[aria-label="Inbox items"] > div', 3000);
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Emulation.setCPUThrottlingRate", { rate: 4 });
    await page.evaluate(() => {
      (document.querySelector('section[aria-label="Inbox items"] > div') as HTMLElement).scrollTop = 0;
    });
    results.inboxScroll5000RowsCpu4x = await scrollFrames(page, 'section[aria-label="Inbox items"] > div', 3000);
    await cdp.send("Emulation.setCPUThrottlingRate", { rate: 1 });
    await expect(page.locator('section[aria-label="Inbox items"] button').first()).toBeVisible();
    expect(await page.locator('section[aria-label="Inbox items"] button').count()).toBeLessThan(80);

    // Measured in the page, so it holds no round trips of the test itself: from the click to the
    // frame that shows the other screen.
    const switches = await page.evaluate(async () => {
      const times: number[] = [];
      for (let i = 0; i < 12; i++) {
        const name = i % 2 === 0 ? "Board" : "Inbox";
        const link = [...document.querySelectorAll("nav a")].find((a) =>
          a.textContent?.startsWith(name),
        ) as HTMLElement;
        const start = performance.now();
        link.click();
        while (document.querySelector("main h1")?.textContent !== name) await new Promise(requestAnimationFrame);
        await new Promise(requestAnimationFrame);
        times.push(Math.round(performance.now() - start));
      }
      return times;
    });
    results.screenSwitchMs = { worst: Math.max(...switches), each: switches };
  } finally {
    site.close();
  }
});

test("a chat of 2,000 markdown messages while one streams", async ({ page }) => {
  const site = await serveStatic(join(REPO, "clients/e2e/bench/dist"));
  try {
    await page.goto(`${site.origin}/`);
    await page.getByText("Message 0 ").first().waitFor();
    results.chatScroll2000Messages = await scrollFrames(page, '[data-testid="virtuoso-scroller"]', 6000);
    // The same on a machine four times slower: this machine's numbers say little about a weak laptop.
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Emulation.setCPUThrottlingRate", { rate: 4 });
    await page.evaluate(() => {
      const scroller = document.querySelector('[data-testid="virtuoso-scroller"]') as HTMLElement;
      scroller.scrollTop = 0;
    });
    await page.waitForTimeout(500);
    results.chatScroll2000MessagesCpu4x = await scrollFrames(page, '[data-testid="virtuoso-scroller"]', 6000);
    await cdp.send("Emulation.setCPUThrottlingRate", { rate: 1 });
    results.chatStreamingRenders = await page.evaluate(
      () => (window as unknown as { renders: { list: number; streaming: number } }).renders,
    );
  } finally {
    site.close();
  }
});

test("Electron's cold start until the Inbox is usable", async () => {
  const daemon = new TestDaemon();
  await daemon.start();
  const profile = mkdtempSync(join(tmpdir(), "aurelius-electron-"));
  const launch = async () => {
    const start = Date.now();
    const app = await _electron.launch({
      args: [
        join(REPO, "clients/electron"),
        `--user-data-dir=${profile}`,
        ...(process.env.AURELIUS_ELECTRON_ARGS?.split(" ") ?? []),
      ],
      env: { ...process.env, AURELIUS_DATA_DIR: daemon.data, AURELIUS_ENV: "dev", ELECTRON_RENDERER_URL: "" } as Record<
        string,
        string
      >,
    });
    return { app, start };
  };
  try {
    const first = await launch();
    const page = await first.app.firstWindow();
    await page.getByLabel("Name").fill("sam");
    await page.getByLabel("Password").fill("a-long-enough-password");
    await page.getByRole("button", { name: "Create" }).click();
    await page.getByRole("heading", { name: "Inbox" }).waitFor();
    await first.app.close();

    const times: number[] = [];
    for (let i = 0; i < 3; i++) {
      const run = await launch();
      const window = await run.app.firstWindow();
      // The session lasts only as long as the app without a keychain: the first usable screen counts.
      await window.getByRole("heading", { name: /^(Sign in|Inbox)$/ }).waitFor();
      times.push(Date.now() - run.start);
      await run.app.close();
    }
    results.electronColdStartMs = times;
  } finally {
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
  }
});
