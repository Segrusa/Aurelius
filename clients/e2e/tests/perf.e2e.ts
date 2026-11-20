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

test("the Inbox bell with 5,000 rows, in a browser", async ({ page }) => {
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
    await page.goto(`${site.origin}/new`);
    await expect(page.getByRole("heading", { level: 1, name: "New session" })).toBeVisible();
    // The bell draws a capped number of rows per group, however long the list is.
    const start = Date.now();
    await page
      .getByRole("button", { name: /^Inbox/ })
      .first()
      .click();
    const inbox = page.getByRole("dialog", { name: "Inbox" });
    await expect(inbox.getByRole("button", { name: /Flux wants to run a command/ })).toBeVisible();
    results.inboxBellOpen5000RowsMs = Date.now() - start;
    // Rows only: an update also has its Dismiss button.
    const rows = inbox.getByRole("button").filter({ hasNotText: /^$/ });
    expect(await rows.count()).toBeLessThan(120);
    await page.keyboard.press("Escape");

    // Measured in the page, so it holds no round trips of the test itself: from the click to the
    // frame that shows the other screen.
    const switches = await page.evaluate(async () => {
      const times: number[] = [];
      for (let i = 0; i < 12; i++) {
        const name = i % 2 === 0 ? "Board" : "New session";
        // Ctrl+1 goes to New session, the start.
        const link = [...document.querySelectorAll("nav a")].find((a) => a.textContent?.startsWith("Board"));
        const start = performance.now();
        if (name === "Board") (link as HTMLElement).click();
        else
          window.dispatchEvent(
            new KeyboardEvent("keydown", { key: "1", code: "Digit1", ctrlKey: true, bubbles: true }),
          );
        while (document.querySelector('[role="main"] h1')?.textContent !== name)
          await new Promise(requestAnimationFrame);
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

test("the palette with 3,000 tickets, in a browser", async ({ page }) => {
  const site = await serveStatic(WEB_DIST);
  try {
    await scriptServer(page, {
      projects: [
        { id: "p", name: "Northwind API", status: "active", created_at_ms: 0, employees_active: 0, version: 1 },
      ],
      tickets: Array.from({ length: 3000 }, (_, i) => ({
        id: `t${i}`,
        number: i + 1,
        title: `Ticket about subject ${i}`,
        column: "doing",
        project_id: "p",
        project_name: "Northwind API",
      })),
    });
    await page.goto(`${site.origin}/new`);
    await expect(page.getByRole("heading", { level: 1, name: "New session" })).toBeVisible();
    const pressK = () =>
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", code: "KeyK", ctrlKey: true, bubbles: true }));
    // The first open fetches the tickets; only after that counts as a real measurement of the data.
    await page.evaluate(pressK);
    await expect(page.getByRole("option", { name: /#1 Ticket about subject 0/ })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("combobox", { name: "Command palette" })).toHaveCount(0);

    const opens: number[] = [];
    for (let i = 0; i < 8; i++) {
      // From the key press to the frame that shows the palette, measured in the page.
      opens.push(
        await page.evaluate(async () => {
          const start = performance.now();
          window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", code: "KeyK", ctrlKey: true, bubbles: true }));
          while (!document.querySelector('input[role="combobox"][aria-label="Command palette"]'))
            await new Promise((r) => requestAnimationFrame(r));
          return performance.now() - start;
        }),
      );
      await page.keyboard.press("Escape");
      await expect(page.getByRole("combobox", { name: "Command palette" })).toHaveCount(0);
    }
    results.paletteOpen3000TicketsMs = { worst: Math.max(...opens), all: opens.map(Math.round) };
    expect(Math.max(...opens)).toBeLessThan(50);

    await page.evaluate(pressK);
    const typed = await page.evaluate(async () => {
      const input = document.querySelector('input[role="combobox"][aria-label="Command palette"]') as HTMLInputElement;
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      const start = performance.now();
      set?.call(input, "subject 2999");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      while (!document.querySelector('[role="option"][data-value="ticket:t2999"]')) {
        await new Promise((r) => requestAnimationFrame(r));
      }
      return performance.now() - start;
    });
    results.paletteFilter3000TicketsMs = typed;
    expect(typed).toBeLessThan(50);
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
    await page.getByRole("heading", { level: 1, name: "New session" }).waitFor();
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
    // Headless on this machine it is about 470 ms; a heavy import in the first chunk would show here.
    expect(Math.max(...times)).toBeLessThan(1000);
  } finally {
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
  }
});
