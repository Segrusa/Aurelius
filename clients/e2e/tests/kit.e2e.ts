import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Connection, expectBody } from "@aurelius/client";
import type { TicketView } from "@aurelius/protocol";
import { _electron, expect, type Page, test } from "@playwright/test";
import { daemonEndpoint, openPipe } from "../../electron/src/main/pipe";
import { REPO, serveStatic, TestDaemon, until, WEB_DIST } from "./daemon";
import { scriptServer } from "./fake-server";

const PASSWORD = "a-long-enough-password";

const palette = (page: Page) => page.locator("[cmdk-input]");

test("keyboard, palette, New ticket and Give work against a real daemon", async () => {
  const daemon = new TestDaemon();
  await daemon.start();
  // What the old desktop app leaves behind.
  writeFileSync(join(daemon.data, "ui.json"), JSON.stringify({ appearance: "light", panel_widths: { x: 1 } }));
  writeFileSync(
    join(daemon.data, "screen_use.json"),
    JSON.stringify({ screens: { board: { opens: 3, first_ms: 1, last_ms: 2 } } }),
  );
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
  let seed: Connection | null = null;
  try {
    const page = await app.firstWindow();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    await page.getByLabel("Name").fill("sam");
    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByRole("button", { name: "Create" }).click();
    await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();

    // No screen makes these yet: a second client does, and the app hears of it through events.
    let token: string | null = null;
    const endpoint = daemonEndpoint(daemon.data);
    seed = new Connection({
      open: () => openPipe(endpoint),
      sessions: {
        get: async () => token,
        set: async (next) => {
          token = next;
        },
      },
      clientName: "seed",
      clientVersion: "0",
      deviceName: "seed",
    });
    seed.start();
    const server = seed;
    await until("online", () => server.getState().status === "online");
    await server.signIn("sam", PASSWORD);
    await server.write("project.create", { name: "Northwind API" }, { retry: false });
    const ava = await server.write("employee.create", { name: "Ava", role: "CEO" }, { retry: false });
    const avaId = (ava.result as { value?: { id?: string } }).value?.id;
    const employees = async () => expectBody(await server.call("employee.list"), "employees").value.employees;
    const bossId = avaId ?? (await employees())[0]?.id;
    await server.write(
      "employee.create",
      { name: "Ben", role: "Engineer", reports_to: bossId ?? null },
      { retry: false },
    );
    await server.write(
      "vault.create",
      { name: "Deploy key", env_name: "DEPLOY_KEY", kind: "secret", value: "hunter2-very-secret-value" },
      { retry: false },
    );
    const tickets = async (): Promise<TicketView[]> =>
      expectBody(await server.call("ticket.list", {}), "tickets").value.tickets;

    // The palette: finds things, hides what is hidden, shuts out the keys behind it.
    await page.keyboard.press("Control+k");
    await expect(palette(page)).toBeFocused();
    await palette(page).fill("north");
    await expect(page.getByRole("option", { name: /Northwind API/ })).toBeVisible();
    await palette(page).fill("j");
    await expect(page).toHaveURL(/inbox/);
    await palette(page).fill("heartbeat");
    await expect(page.getByText("Nothing found.")).toBeVisible();
    await palette(page).fill("plugin");
    await expect(page.getByText("Nothing found.")).toBeVisible();
    await palette(page).fill("");
    await page.keyboard.press("Control+,");
    await expect(page).toHaveURL(/inbox/);
    await page.keyboard.press("Escape");
    await expect(palette(page)).toHaveCount(0);

    // A command runs from the palette, and Back comes home.
    await page.keyboard.press("Control+k");
    await palette(page).fill("settings");
    await page.keyboard.press("Enter");
    await expect(page.getByRole("heading", { level: 1, name: "Account" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();

    // Shortcuts go to the right screens; single keys stay out of a text field.
    await page.keyboard.press("Control+,");
    await expect(page.getByRole("heading", { level: 1, name: "Account" })).toBeVisible();
    await page.keyboard.press("Control+1");
    await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();

    // New ticket: the choices arrive, a wrong date is said, and the fields end up on the server.
    await page.getByRole("button", { name: "New ticket" }).click();
    const title = page.getByLabel("Title");
    await expect(title).toBeFocused();
    await title.fill("Add rate limits");
    await page.keyboard.press("j");
    await expect(title).toHaveValue("Add rate limitsj");
    await title.fill("Add rate limits");
    await page.getByLabel("Description").fill("Per **key**.");
    await page.getByRole("button", { name: "Priority" }).click();
    await page.getByRole("menuitem", { name: "High" }).click();
    await page.getByRole("button", { name: "Owner" }).click();
    await page.getByRole("menuitem", { name: "Ben" }).click();
    await page.getByLabel("Labels").fill("backend, ui, backend");
    await page.getByLabel("Deadline").fill("next week");
    await page.getByRole("button", { name: "Create", exact: true }).click();
    await expect(page.getByRole("alert")).toContainText("Write the date as");
    await page.getByLabel("Deadline").fill("2031-03-04 17:00");
    await page.getByRole("button", { name: "Create", exact: true }).click();
    await expect(page.getByText("Ticket created in Backlog.")).toBeVisible();
    await expect(title).toHaveCount(0);
    const made = (await tickets()).find((t) => t.title === "Add rate limits");
    expect(made?.column).toBe("backlog");
    expect(made?.priority).toBe("high");
    expect(made?.labels).toEqual(["backend", "ui"]);
    expect(made?.owner_name).toBe("Ben");
    expect(made?.deadline_ms).toBe(new Date(2031, 2, 4, 17, 0).getTime());
    expect(made?.responsible_username).toBe("sam");

    // A text with a vault value is held, and goes on purpose. Create and start stays Create and start.
    await page.keyboard.press("Control+t");
    await title.fill("Rotate the key");
    await page.getByLabel("Description").fill("The key is hunter2-very-secret-value");
    await page.keyboard.press("Control+Enter");
    await expect(page.getByRole("alert")).toContainText("Deploy key");
    expect((await tickets()).some((t) => t.title === "Rotate the key")).toBe(false);
    await page.getByRole("button", { name: "Send anyway" }).click();
    await expect(page.getByText("Ticket created and started.")).toBeVisible();
    await until("started", async () =>
      (await tickets()).find((t) => t.title === "Rotate the key" && t.column !== "backlog"),
    );

    // Give work: the top of the org chart by default, a ticket started at once.
    await page.keyboard.press("n");
    const goal = page.getByLabel("What needs doing");
    await expect(goal).toBeFocused();
    await expect(page.getByRole("button", { name: "Ava" })).toHaveAttribute("aria-pressed", "true");
    await goal.fill(`${"Compare three hosting options ".repeat(3)}\nwith prices`);
    await page.keyboard.press("Control+Enter");
    await expect(page.getByText("Created and started with Ava.")).toBeVisible();
    const given = (await tickets()).find((t) => t.owner_name === "Ava");
    expect([...(given?.title ?? "")]).toHaveLength(61);

    // Someone else, through the menu; the next time starts from whom it went to last.
    await page.keyboard.press("n");
    await page.getByRole("button", { name: "Someone else" }).click();
    await page.getByRole("menuitem", { name: "Ben" }).click();
    await goal.fill("Fix the build");
    await page.getByRole("button", { name: /^Give work/ }).click();
    await expect(page.getByText("Created and started with Ben.")).toBeVisible();
    expect((await tickets()).find((t) => t.title === "Fix the build")?.owner_name).toBe("Ben");
    expect(await page.evaluate(() => localStorage.getItem("aurelius.last_given_to"))).toBeTruthy();

    // The screen counter kept the desktop's numbers and adds its own, on this device only.
    await until(
      "the counter saved",
      () =>
        page.evaluate(() => {
          const screens = JSON.parse(localStorage.getItem("aurelius.screen_use") ?? "{}").screens ?? {};
          return screens.board?.opens === 3 && screens.inbox?.opens >= 2;
        }),
      10_000,
    );
  } finally {
    seed?.dispose();
    await app.close();
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
  }
});

test("on the web a browser's own keys stay the browser's, and the action moves", async ({ page }) => {
  const site = await serveStatic(WEB_DIST);
  try {
    const { writes } = await scriptServer(page, {
      projects: [
        { id: "p", name: "Northwind API", status: "active", created_at_ms: 0, employees_active: 0, version: 1 },
      ],
      employees: [{ id: "e", name: "Ava", role: "CEO", status: "active", agents: [], conversations: [] }],
    });
    await page.goto(`${site.origin}/inbox`);
    await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();
    await page.keyboard.press("Alt+t");
    await page.getByLabel("Title").fill("From the web");
    await page.keyboard.press("Control+Enter");
    await expect(page.getByText("Ticket created and started.")).toBeVisible();
    const sent = writes.find((w) => w.method === "ticket.create") as { params: { title: string; start: boolean } };
    expect(sent.params.title).toBe("From the web");
    expect(sent.params.start).toBe(true);

    // Right after a dialog closes, the next one has the keys; without motion too.
    await page.keyboard.press("Control+k");
    await expect(palette(page)).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(palette(page)).toHaveCount(0);
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.keyboard.press("Control+k");
    await expect(palette(page)).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(palette(page)).toHaveCount(0);
  } finally {
    site.close();
  }
});
