import { Connection, expectBody } from "@aurelius/client";
import { expect, test } from "@playwright/test";
import { openPipe } from "../../electron/src/main/pipe";
import { serveStatic, TestDaemon, until, WEB_DIST } from "./daemon";
import { approval, notification, scriptServer } from "./fake-server";

const PASSWORD = "a-long-enough-password";

test.describe("against a real daemon", () => {
  let daemon: TestDaemon;
  let owner: Connection;

  test.beforeAll(async () => {
    daemon = new TestDaemon();
    await daemon.start();
    let token: string | null = null;
    owner = new Connection({
      open: () => openPipe(daemon.socket),
      sessions: {
        get: async () => token,
        set: async (next) => {
          token = next;
        },
      },
      clientName: "e2e",
      clientVersion: "0",
      deviceName: "e2e",
    });
    owner.start();
    await until("online", () => owner.getState().status === "online");
    await owner.write("user.create", { username: "sam", password: PASSWORD }, { retry: false });
    await owner.signIn("sam", PASSWORD);
    await owner.write("project.create", { name: "Northwind API" });
    await owner.write("remote.enable", undefined);
  });

  test.afterAll(async () => {
    owner?.dispose();
    await daemon?.dispose();
  });

  test("pairs, signs in, keeps secrets from scripts, follows events and survives a restart", async ({ page }) => {
    const refused: string[] = [];
    page.on("console", (m) => {
      if (m.type() === "error" && /Content Security Policy/i.test(m.text())) refused.push(m.text());
    });
    await page.goto(`${daemon.origin}/`);
    await expect(page.getByRole("heading", { name: "Pair this browser" })).toBeVisible();

    const start = await owner.write("remote.start_pairing", undefined);
    const result = start.result;
    if (result.kind !== "host" || result.value.type !== "pairing_code") throw new Error("Expected a code.");
    await page.getByLabel("Code").fill(result.value.code);
    await page.getByRole("button", { name: "Pair" }).click();

    const pending = await until("the pairing request", async () => {
      const body = await owner.call("remote.get_status");
      return expectBody(body, "remote").value.pending[0];
    });
    await owner.write(
      "remote.decide_pairing",
      { pairing_id: pending.pairing_id, approve: true, owner: false },
      { retry: false },
    );

    await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
    await page.getByLabel("Name").fill("sam");
    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();
    await expect(page).toHaveURL(/\/inbox$/);
    await expect(page.getByText("Northwind API")).toBeVisible();
    await expect(page.getByText("Nothing waits for you.")).toBeVisible();

    // No credential for a script, and none in the address.
    const cookies = await page.context().cookies();
    const named = cookies.filter((c) => c.name.startsWith("aurelius_"));
    expect(named.map((c) => c.name).sort()).toEqual(["aurelius_device", "aurelius_session"]);
    for (const cookie of named) {
      expect(cookie.httpOnly).toBe(true);
      expect(cookie.secure).toBe(true);
      expect(cookie.sameSite).toBe("Strict");
    }
    expect(await page.evaluate(() => document.cookie)).toBe("");
    expect(await page.evaluate(() => JSON.stringify(localStorage))).not.toContain("secret");
    expect(page.url()).not.toMatch(/token|secret/);

    // A change elsewhere reaches this page by an event.
    await owner.write("project.create", { name: "Seen live" });
    await expect(page.getByText("Seen live")).toBeVisible();

    // The server goes away and comes back.
    await daemon.stop();
    await expect(page.getByRole("status")).toHaveText("Reconnecting…");
    await daemon.start();
    await expect(page.getByRole("status")).toBeHidden({ timeout: 30_000 });
    await expect(page.getByText("Seen live")).toBeVisible();

    // Settings in a browser: no section fails, and none allows other devices from here.
    await page.getByRole("link", { name: "Settings" }).click();
    for (const name of [
      "Account",
      "App",
      "Company",
      "Servers & PC",
      "Remote access",
      "Connections",
      "MCP & tools",
      "Recurring schedules",
      "Backups",
      "Load limits",
      "Trace",
    ]) {
      await page.getByRole("navigation", { name: "Settings" }).getByRole("link", { name, exact: true }).click();
      await expect(page.getByRole("heading", { level: 1, name, exact: true })).toBeVisible();
      await page.waitForTimeout(250);
      expect({ name, alerts: await page.getByRole("main").getByRole("alert").allTextContents() }).toEqual({
        name,
        alerts: [],
      });
    }
    await page
      .getByRole("navigation", { name: "Settings" })
      .getByRole("link", { name: "Remote access", exact: true })
      .click();
    await expect(page.getByText("A browser can never allow other devices.")).toBeVisible();

    expect(refused).toEqual([]);
    const projects = expectBody(
      await until("the owner back", async () => owner.call("project.list").catch(() => null)),
      "projects",
    ).value;
    expect(projects.projects.length).toBe(2);
  });
});

test.describe("the Inbox", () => {
  let site: Awaited<ReturnType<typeof serveStatic>>;
  test.beforeAll(async () => {
    site = await serveStatic(WEB_DIST);
  });
  test.afterAll(() => site.close());

  test("decides by keyboard with the approval's own digest, never resent", async ({ page }) => {
    const list = [
      approval("a"),
      approval("b", { requested_at_ms: Date.now() - 600_000, arguments: { command: "ls" } }),
    ];
    const { writes } = await scriptServer(page, {
      approvals: list,
      onWrite: (request) => {
        const params = request.params as { approval_id: string };
        list.splice(
          list.findIndex((a) => a.id === params.approval_id),
          1,
        );
      },
    });
    await page.goto(`${site.origin}/inbox`);
    await expect(page.getByRole("heading", { name: "Flux wants to run a command" })).toBeVisible();
    await expect(page.locator("pre")).toHaveText("kubectl -n prod rollout restart deploy/api");

    await page.keyboard.press("Enter");
    await expect.poll(() => writes.length).toBe(1);
    expect(writes[0]).toMatchObject({
      method: "approval.decide",
      params: { approval_id: "a", action_digest: "digest-a", approve: true, always_allow: false },
    });
    await expect(page.locator("pre")).toHaveText("ls");

    await page.keyboard.press("d");
    await expect.poll(() => writes.length).toBe(2);
    expect(writes[1]).toMatchObject({ params: { approval_id: "b", approve: false } });
    await expect(page.getByText("Nothing waits for you.")).toBeVisible();
  });

  test("J and K keep the selected row in view, and it fits a narrow window", async ({ page }) => {
    await page.setViewportSize({ width: 930, height: 700 });
    await scriptServer(page, {
      approvals: [approval("a")],
      notifications: Array.from({ length: 300 }, (_, i) => notification(i)),
    });
    await page.goto(`${site.origin}/inbox`);
    await expect(page.getByText(/^1 \/ 301/)).toBeVisible();
    for (let i = 0; i < 40; i++) await page.keyboard.press("j");
    await expect(page.getByText(/^41 \/ 301/)).toBeVisible();
    await expect(page.locator('[aria-current="true"]')).toBeInViewport({ ratio: 1 });
    for (let i = 0; i < 40; i++) await page.keyboard.press("k");
    await expect(page.locator('[aria-current="true"]')).toBeInViewport({ ratio: 1 });
    const counter = await page.getByText(/^1 \/ 301/).boundingBox();
    expect(counter?.height).toBeLessThan(24);
    const tops = await page
      .getByRole("button", { name: /Allow once|Always allow|Deny/ })
      .evaluateAll((all) => all.map((b) => b.getBoundingClientRect().top));
    expect(new Set(tops).size).toBe(1);
  });

  test("shows what an agent sent as text", async ({ page }) => {
    await scriptServer(page, {
      approvals: [
        approval("x", {
          requester: "<img src=x onerror=alert(1)>",
          arguments: { command: "<script>alert(1)</script>" },
        }),
      ],
    });
    await page.goto(`${site.origin}/inbox`);
    await expect(page.locator("pre")).toHaveText("<script>alert(1)</script>");
    expect(await page.locator("main img").count()).toBe(0);
  });

  test("walks with J and K, and filters", async ({ page }) => {
    await scriptServer(page, { approvals: [approval("a")], notifications: [notification(4), notification(5)] });
    await page.goto(`${site.origin}/inbox`);
    const position = page.getByText(/^\d+ \/ \d+/);
    await expect(position).toContainText("1 / 3");
    await page.keyboard.press("j");
    await expect(position).toContainText("2 / 3");
    await page.keyboard.press("k");
    await expect(position).toContainText("1 / 3");
    await page.getByRole("button", { name: "Reviews" }).click();
    await expect(position).toContainText("1 / 1");
    await expect(page.getByRole("button", { name: /Ready for your review/ })).toBeVisible();
    await page.getByRole("button", { name: "Mentions" }).click();
    await expect(page.getByRole("button", { name: /Update 5/ })).toBeVisible();
  });

  test("follows the chosen theme", async ({ page }) => {
    await page.addInitScript(() => localStorage.setItem("aurelius.appearance", "light"));
    await scriptServer(page, { approvals: [approval("a")] });
    await page.goto(`${site.origin}/inbox`);
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe("rgb(243, 243, 242)");
  });
});
