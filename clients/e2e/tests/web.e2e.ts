import { Connection, expectBody } from "@aurelius/client";
import { expect, type Page, test } from "@playwright/test";
import { openPipe } from "../../electron/src/main/pipe";
import { openSettings, workspaceTile } from "./company";
import { serveStatic, TestDaemon, until, WEB_DIST } from "./daemon";
import { approval, scriptServer } from "./fake-server";

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
    // A new device is on All workspaces, so it starts on the list of workspaces.
    await expect(page.getByRole("heading", { level: 1, name: "All workspaces" })).toBeVisible();
    await expect(page).toHaveURL(/\/overview$/);
    await expect(workspaceTile(page, "Northwind API")).toBeVisible();
    await expect(page.getByRole("button", { name: "Inbox" }).first()).toBeVisible();

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
    const projectInSwitcher = async () => {
      await expect(workspaceTile(page, "Seen live")).toBeVisible();
    };
    await projectInSwitcher();

    // The server goes away and comes back.
    await daemon.stop();
    await expect(page.locator('p[role="status"]')).toHaveText("Reconnecting…");
    await daemon.start();
    await expect(page.locator('p[role="status"]')).toBeHidden({ timeout: 30_000 });
    await projectInSwitcher();

    // Settings in a browser: no section fails, and none allows other devices from here.
    await openSettings(page);
    for (const name of [
      "Account",
      "My defaults",
      "This device",
      "General",
      "Board members",
      "Overview",
      "Remote & phones",
      "Connections",
      "Tools & MCP",
      "Runs & load",
      "Backups",
      "Activity log",
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
      .getByRole("link", { name: "Remote & phones", exact: true })
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

test.describe("the Inbox bell", () => {
  let site: Awaited<ReturnType<typeof serveStatic>>;
  test.beforeAll(async () => {
    site = await serveStatic(WEB_DIST);
  });
  test.afterAll(() => site.close());

  // There is no Inbox page: an approval with no run or conversation is decided in the bell.
  const bell = (page: Page) => page.getByRole("button", { name: /^Inbox/ }).first();
  const dropdown = (page: Page) => page.getByRole("dialog", { name: "Inbox" });

  test("decides an approval in the bell with its own digest, never resent", async ({ page }) => {
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
    await page.goto(`${site.origin}/new`);
    await expect(page.getByRole("button", { name: "Inbox, 2 need you" })).toBeVisible();
    await bell(page).click();
    await dropdown(page)
      .getByRole("button", { name: /Flux wants to run a command.*kubectl/ })
      .click();
    await expect(dropdown(page).locator("pre")).toHaveText("kubectl -n prod rollout restart deploy/api");
    await dropdown(page)
      .getByRole("button", { name: /^Allow once/ })
      .click();
    await expect.poll(() => writes.length).toBe(1);
    expect(writes[0]).toMatchObject({
      method: "approval.decide",
      params: { approval_id: "a", action_digest: "digest-a", approve: true, always_allow: false },
    });

    await dropdown(page)
      .getByRole("button", { name: /Flux wants to run a command.*ls/ })
      .click();
    // The theme draws Deny as an outline. An app class that shares Astryx's atomic name for the red fill
    // once won over it and hid the label: red text on a red fill.
    const deny = dropdown(page).getByRole("button", { name: /^Deny/ });
    await expect(deny).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
    await expect(deny).toHaveCSS("border-top-width", "1px");
    await deny.click();
    await expect.poll(() => writes.length).toBe(2);
    expect(writes[1]).toMatchObject({ params: { approval_id: "b", approve: false } });
    await expect(dropdown(page).getByText("Nothing here. You are up to date.")).toBeVisible();
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
    await page.goto(`${site.origin}/new`);
    await bell(page).click();
    await dropdown(page)
      .getByRole("button", { name: /wants to run a command/ })
      .click();
    await expect(dropdown(page).locator("pre")).toHaveText("<script>alert(1)</script>");
    expect(await dropdown(page).locator("img").count()).toBe(0);
  });

  test("follows the chosen theme", async ({ page }) => {
    await page.addInitScript(() => localStorage.setItem("aurelius.appearance", "light"));
    await scriptServer(page, { approvals: [approval("a")] });
    await page.goto(`${site.origin}/new`);
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe("rgb(243, 243, 242)");
  });
});
