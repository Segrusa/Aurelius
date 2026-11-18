// A day's work through the screens only, against a real daemon whose agent is the test runtime
// (a build with `--features testing`, in AURELIUS_DAEMON_BIN): nothing is seeded by a second client.

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { _electron, expect, type Page, test } from "@playwright/test";
import { REPO, TestDaemon } from "./daemon";
import { expectNewSession } from "./new-session";

const PASSWORD = "correct horse battery";
const SHOTS = process.env.AURELIUS_JOURNEY_SHOTS;

test.skip(
  !process.env.AURELIUS_JOURNEY,
  "Needs AURELIUS_DAEMON_BIN built with --features testing, and AURELIUS_JOURNEY=1.",
);

test("a ticket from nothing to Done, a chat, and a restart of the server", async () => {
  test.setTimeout(240_000);
  const daemon = new TestDaemon();
  await daemon.start();
  mkdirSync(join(daemon.dir, "atlas"));
  const profile = mkdtempSync(join(tmpdir(), "aurelius-electron-"));
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
  const log: { step: string; ms: number; actions: number }[] = [];
  let page: Page | null = null;
  let actions = 0;
  const consoleErrors: string[] = [];
  const act = async (what: () => Promise<unknown>) => {
    actions += 1;
    await what();
  };
  const step = async (name: string, body: () => Promise<void>) => {
    const started = Date.now();
    actions = 0;
    await body();
    log.push({ step: name, ms: Date.now() - started, actions });
    if (SHOTS && page) {
      mkdirSync(SHOTS, { recursive: true });
      await page.screenshot({
        path: join(SHOTS, `${String(log.length).padStart(2, "0")}-${name.replace(/\W+/g, "-")}.png`),
      });
    }
  };
  try {
    page = await app.firstWindow();
    const p = page;
    p.on("console", (m) => {
      if (m.type() === "error") consoleErrors.push(m.text());
    });
    p.on("pageerror", (e) => consoleErrors.push(String(e)));
    await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]?.setSize(1600, 950));
    const nav = (name: string) => p.getByRole("link", { name: new RegExp(`^${name}`) }).first();

    await step("first board member", async () => {
      await act(() => p.getByLabel("Name").fill("sam"));
      await act(() => p.getByLabel("Password").fill(PASSWORD));
      await act(() => p.getByRole("button", { name: "Create" }).click());
      await expect(p.getByRole("heading", { name: "Inbox" })).toBeVisible();
    });

    await step("add an employee", async () => {
      await act(() => nav("Employees").click());
      await act(() => p.getByRole("button", { name: "Add employee" }).click());
      const dialog = p.getByRole("dialog");
      await act(() => dialog.getByLabel("Name").fill("Builder"));
      await act(() => dialog.getByLabel("Role").fill("Developer"));
      await act(() => dialog.getByRole("button", { name: /^(Create|Add|Save)/ }).click());
      await expect(p.getByRole("dialog")).toHaveCount(0);
      await expect(p.getByText("Builder").first()).toBeVisible();
    });

    await step("add a project", async () => {
      await act(() => nav("Knowledge").click());
      await act(() => p.getByRole("link", { name: "Projects", exact: true }).click());
      await act(() => p.getByRole("button", { name: "New project" }).first().click());
      const dialog = p.getByRole("dialog");
      await act(() => dialog.getByLabel("Name").fill("Atlas"));
      await act(() => dialog.getByLabel("Project folder (required)").fill(join(daemon.dir, "atlas")));
      await act(() => dialog.getByRole("button", { name: /^Create/ }).click());
      await expect(p.getByRole("dialog")).toHaveCount(0);
    });

    await step("new ticket with an owner, started", async () => {
      await act(() => nav("Board").click());
      // The key the button shows.
      await expect(p.getByRole("button", { name: /New ticket/ }).first()).toBeVisible();
      await act(() => p.keyboard.press("n"));
      const dialog = p.getByRole("dialog");
      await act(() => dialog.getByLabel("Title").fill("Login page"));
      await expect(dialog.getByRole("button", { name: /^Project/ })).toBeVisible();
      await act(() => dialog.getByLabel("Description").fill("A login page with a password field."));
      await act(() => dialog.getByRole("button", { name: "Owner" }).press("Enter"));
      await act(() => p.getByRole("menuitem", { name: "Builder" }).click());
      await act(() => dialog.getByRole("button", { name: /Create and start/ }).click());
      await expect(p.getByRole("dialog")).toHaveCount(0);
    });

    await step("the agent comes back with a plan", async () => {
      await expect(p.getByRole("region", { name: "Plan review", exact: true }).getByText("Login page")).toBeVisible({
        timeout: 40_000,
      });
    });

    await step("what the Inbox shows for it", async () => {
      await act(() => nav("Inbox").click());
      await expect(p.getByRole("heading", { name: "Inbox" })).toBeVisible();
      // One decision, one row, readable.
      await expect(p.getByRole("button", { name: /Login page/ })).toHaveCount(1);
      await expect(p.getByText("#1 #1")).toHaveCount(0);
      await expect(p.getByText("plan_review")).toHaveCount(0);
    });

    await step("the plan in the ticket", async () => {
      await act(() => nav("Board").click());
      await act(() => p.getByRole("region", { name: "Plan review", exact: true }).getByText("Login page").click());
      await expect(p.getByRole("button", { name: "Approve", exact: true })).toBeVisible();
    });

    await step("approve the plan, the agent works and reports", async () => {
      await act(() => p.getByRole("button", { name: "Approve", exact: true }).click());
      await act(() => p.getByRole("button", { name: "Close" }).first().click());
      await expect(p.getByRole("region", { name: "Review", exact: true }).getByText("Login page")).toBeVisible({
        timeout: 40_000,
      });
    });

    await step("read the report in the ticket", async () => {
      await act(() => p.getByRole("region", { name: "Review", exact: true }).getByText("Login page").click());
      await expect(p.getByRole("button", { name: "Send back to Doing" })).toBeVisible();
    });

    await step("send back with feedback", async () => {
      await act(() => p.getByRole("button", { name: "Send back to Doing" }).click());
      await act(() => p.getByLabel("What needs to change?").fill("Add a forgot-password link"));
      await act(() => p.getByRole("button", { name: "Send back to Doing" }).click());
      await expect(p.getByRole("region", { name: "Doing", exact: true }).getByText("Login page")).toBeVisible();
      await expect(p.getByRole("region", { name: "Review", exact: true }).getByText("Login page")).toBeVisible({
        timeout: 40_000,
      });
    });

    await step("accept to Done", async () => {
      await act(() => p.getByRole("button", { name: "Mark done" }).click());
      await expect(p.getByRole("region", { name: "Done", exact: true }).getByText("Login page")).toBeVisible({
        timeout: 40_000,
      });
    });

    await step("a new session with the keyboard", async () => {
      // Ctrl+N is New session (ticket #60): the composer is ready, Alt+M goes from the Dispatcher to Builder.
      await act(() => p.keyboard.press("Control+n"));
      await expectNewSession(p);
      await act(() => p.keyboard.type("Compare three hosting options"));
      await act(() => p.keyboard.press("Alt+m"));
      await expect(p.getByRole("button", { name: /^Who/ })).toContainText("Builder");
      await act(() => p.keyboard.press("Control+Enter"));
      // The new ticket's chat opens, as after New ticket and Give work (#58); its plan shows on the Board.
      await expect(p).toHaveURL(/\/chat\?channel=/);
      await act(() => nav("Board").click());
      // The first employee is management, which starts in Plan: this stops at a plan first.
      await expect(
        p.getByRole("region", { name: "Plan review", exact: true }).getByText(/hosting options/),
      ).toBeVisible({
        timeout: 40_000,
      });
    });

    await step("approve a plan from the Inbox", async () => {
      await act(() => nav("Inbox").click());
      await expect(p.getByRole("heading", { name: "Inbox" })).toBeVisible();
      const row = p.getByRole("button", { name: /hosting options/ }).first();
      await act(() => row.click());
      const approve = p.getByRole("button", { name: /^Approve/ }).first();
      await expect(approve).toBeVisible();
      if (SHOTS) await p.screenshot({ path: join(SHOTS, "plan-in-inbox.png") });
      await act(() => p.keyboard.press("Control+Enter"));
      await expect(approve).toHaveCount(0, { timeout: 15_000 });
    });

    await step("chat with the employee", async () => {
      await act(() => p.keyboard.press("Control+8"));
      const box = p.getByPlaceholder(/Message the team/);
      await act(() => box.fill("What did you do today?"));
      await act(() => box.press("Enter"));
      await expect(p.getByText(/local test runtime/).first()).toBeVisible({ timeout: 30_000 });
      await expect(p.getByText("Writing…")).toHaveCount(0, { timeout: 30_000 });
    });

    await step("a server restart during an answer", async () => {
      const box = p.getByPlaceholder(/Message the team/);
      await act(() => box.fill("Second question"));
      await act(() => box.press("Enter"));
      await expect(p.getByText("Writing…")).toBeVisible({ timeout: 20_000 });
      await daemon.stop();
      await expect(p.getByText("Reconnecting…")).toBeVisible({ timeout: 20_000 });
      if (SHOTS) await p.screenshot({ path: join(SHOTS, "offline-during-answer.png") });
      await daemon.start();
      await expect(p.getByText("Reconnecting…")).toHaveCount(0, { timeout: 30_000 });
      // Without leaving the screen: the answer ended with the server.
      await expect(p.getByText("Writing…")).toHaveCount(0, { timeout: 15_000 });
      if (SHOTS) await p.screenshot({ path: join(SHOTS, "after-restart.png") });
    });

    await step("server restart while looking at the board", async () => {
      await act(() => nav("Board").click());
      await daemon.stop();
      await expect(p.getByText("Reconnecting…")).toBeVisible({ timeout: 20_000 });
      if (SHOTS) await p.screenshot({ path: join(SHOTS, "offline.png") });
      await daemon.start();
      await expect(p.getByRole("region", { name: "Done", exact: true }).getByText("Login page")).toBeVisible({
        timeout: 40_000,
      });
    });

    await step("switch to the dark theme", async () => {
      await act(() => nav("Settings").click());
      await act(() =>
        p.getByRole("navigation", { name: "Settings" }).getByRole("link", { name: "App", exact: true }).click(),
      );
      await act(() => p.getByText("Dark", { exact: true }).click());
      await expect(p.locator("html")).toHaveAttribute("data-theme", "dark");
    });

    await step("back from Settings", async () => {
      await act(() => p.getByRole("link", { name: "Back to the app" }).click());
      await expect(nav("Inbox")).toBeVisible();
    });

    for (const screen of ["Inbox", "Board", "Employees", "Knowledge"]) {
      await step(`dark ${screen}`, async () => {
        await act(() => nav(screen).click());
        await p.waitForTimeout(300);
      });
    }

    await step("the run in dark", async () => {
      await act(() => p.keyboard.press("Control+7"));
      await act(() => p.getByText("Compare three hosting options").first().click());
      await p.waitForTimeout(400);
    });

    await step("a message to the run, on a real server", async () => {
      await act(() => p.getByRole("button", { name: /Open conversation/ }).click());
      await expect(p.getByText("Assignment", { exact: true }).first()).toBeVisible();
      const box = p.getByLabel("Message Builder");
      await act(() => box.fill("Also compare prices"));
      await act(() => box.press("Enter"));
      await expect(p.getByText(/does not exist/)).toHaveCount(0);
      await expect(p.getByText(/You asked.*Also compare prices/).first()).toBeVisible({ timeout: 30_000 });
    });

    expect(consoleErrors).toEqual([]);
  } finally {
    if (SHOTS && page) await page.screenshot({ path: join(SHOTS, "last.png") }).catch(() => undefined);
    if (SHOTS) writeFileSync(join(SHOTS, "log.json"), JSON.stringify(log, null, 2));
    console.log(JSON.stringify(log));
    if (consoleErrors.length) console.log("CONSOLE", JSON.stringify(consoleErrors).slice(0, 1500));
    await app.close();
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
  }
});
