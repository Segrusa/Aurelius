// What must never run against a real server here (power, updates, Claude login, MCP on the PC),
// and the screens around a run: the writes are checked against a script.

import { expect, type Page, test } from "@playwright/test";
import { serveStatic, WEB_DIST } from "./daemon";
import { notification, scriptServer } from "./fake-server";

const FEATURES = ["ticket_follow", "ticket_discussion"];
const orion = {
  id: "e1",
  name: "Orion",
  role: "Lead",
  status: "active",
  working: true,
  agents: [],
  projects_active: ["p1"],
  agent_mode: "ask",
  version: 3,
};
const pixel = { ...orion, id: "e2", name: "Pixel" };
const atlas = {
  id: "p1",
  name: "Atlas",
  status: "active",
  folder: "/srv/atlas",
  version: 1,
  created_at_ms: 0,
  employees_active: 1,
};

async function scripted(page: Page, script: Parameters<typeof scriptServer>[1], path: string) {
  const server = await serveStatic(WEB_DIST);
  const fake = await scriptServer(page, {
    features: FEATURES,
    employees: [orion, pixel],
    projects: [atlas],
    ...script,
  });
  await page.goto(`${server.origin}${path}`);
  return { ...fake, stop: server.close };
}

const sent = (writes: Record<string, unknown>[], method: string) => writes.filter((w) => w.method === method);

test.describe("settings actions", () => {
  test("a Claude login connects with the code, and shows when the server says so", async ({ page }) => {
    let linked = false;
    const view = () => ({
      status: linked ? "linked" : "missing",
      user_id: "u",
      background: false,
      email: linked ? "sam@example.com" : null,
    });
    const inputs: unknown[] = [];
    const { writes, push, stop } = await scripted(
      page,
      {
        reads: {
          "claude_login.status": () => ({ type: "claude_login", value: view() }),
          "claude_login.input": (r) => {
            inputs.push(r.params);
            return { type: "ok" };
          },
          "session.list": () => ({ type: "sessions", value: { sessions: [] } }),
        },
        results: {
          "claude_login.link": () => ({ kind: "host", value: { type: "claude_login_link", link_id: "L1" } }),
        },
      },
      "/settings/account",
    );
    // The hello knows no login: an event brings the state in.
    await expect(page.getByText("Unknown")).toBeVisible();
    push({ type: "event", seq: 2, kind: "claude_login.updated" });
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await expect.poll(() => sent(writes, "claude_login.link").length).toBe(1);
    push({ type: "claude_login_prompt", link_id: "L1", asks_code: true, url: "https://claude.com/oauth" });
    await expect(page.getByRole("button", { name: "Open sign-in page ↗" })).toBeVisible();
    await page.getByLabel("Code").fill("abc-123");
    await page.getByLabel("Code").press("Enter");
    await expect.poll(() => inputs).toEqual([{ link_id: "L1", text: "abc-123" }]);
    linked = true;
    push({ type: "event", seq: 3, kind: "claude_login.updated" });
    await expect(page.getByRole("dialog")).toBeHidden();
    await expect(page.getByText("sam@example.com")).toBeVisible();
    await stop();
  });

  test("the PC only shuts down after asking, and the shutdown can be cancelled", async ({ page }) => {
    let pending: unknown = null;
    const { writes, stop } = await scripted(
      page,
      {
        reads: {
          "server.get_name": () => ({ type: "server_name", value: { name: "Bak", computer_name: "bak" } }),
          "power.get": () => ({ type: "power", value: { pending } }),
          "update.get": () => ({
            type: "update",
            value: {
              available: true,
              checked_at_ms: 1,
              current: "1.0",
              enabled: true,
              latest: "1.1",
              notes: "Better.",
            },
          }),
        },
        onWrite: (r) => {
          if (r.method === "power.shutdown")
            pending = { when: "now", requested_at_ms: 1, deadline_ms: Date.now() + 60_000 };
          if (r.method === "power.cancel") pending = null;
        },
      },
      "/settings/servers",
    );
    await expect(page.getByText("Better.")).toBeVisible();
    await page.getByRole("button", { name: "Download update" }).click();
    await expect.poll(() => sent(writes, "update.prepare").length).toBe(1);
    await page.getByRole("button", { name: "Shut down now", exact: true }).click();
    expect(sent(writes, "power.shutdown")).toHaveLength(0);
    await page.getByRole("button", { name: "Yes, shut down now" }).click();
    await expect.poll(() => sent(writes, "power.shutdown").map((w) => w.params)).toEqual([{ when: "now" }]);
    await page.getByRole("button", { name: "Cancel shutdown" }).click();
    await expect.poll(() => sent(writes, "power.cancel").length).toBe(1);
    await expect(page.getByRole("button", { name: "When all work is done" })).toBeVisible();
    await stop();
  });

  test("an API key goes out once, and a refused key is gone from the field", async ({ page }) => {
    const connection = {
      id: "c1",
      display_name: "Anthropic API",
      provider: "anthropic_api",
      status: "needs_key",
      billing: "API",
      has_secret: false,
      models: ["claude-opus-5-5"],
    };
    const { writes, stop } = await scripted(
      page,
      {
        reads: {
          "connection.list": () => ({ type: "connections", value: { connections: [connection], defaults: [] } }),
        },
        refuse: (r) => (r.method === "secret.set" ? { code: "invalid", message: "This key was refused." } : null),
      },
      "/settings/connections",
    );
    await page.getByRole("button", { name: "API key", exact: true }).click();
    await page.getByLabel("API key").fill("sk-secret");
    await page.getByLabel("API key").press("Enter");
    await expect(page.getByText("This key was refused.")).toBeVisible();
    await expect(page.getByLabel("API key")).toHaveValue("");
    expect(sent(writes, "secret.set")).toHaveLength(1);
    await stop();
  });

  test("an MCP server is added with its arguments apart", async ({ page }) => {
    const { writes, stop } = await scripted(
      page,
      {
        reads: {
          "mcp.list": () => ({ type: "mcp_servers", value: { servers: [], missing: ["gone"] } }),
          "auto_allow.get_setting": () => ({ type: "auto_allow_setting", value: { enabled: false, tools: [] } }),
          "browser.get": () => ({ type: "browser", value: { choice: "automatic" } }),
          "mcp_always_allow.list": () => ({ type: "mcp_always_allow", value: { tools: ["mcp__x__y"] } }),
        },
      },
      "/settings/mcp",
    );
    await expect(page.getByText("Claude Code no longer has this server.")).toBeVisible();
    await page.getByRole("button", { name: "Add MCP server…" }).click();
    await page.getByLabel("Name").fill("files");
    await page.getByLabel("Command").fill("npx -y server-files /tmp");
    await page.getByRole("button", { name: "Add server" }).click();
    await expect
      .poll(() => sent(writes, "mcp.add").map((w) => w.params))
      .toEqual([
        { name: "files", transport: "stdio", enable: true, command: "npx", args: ["-y", "server-files", "/tmp"] },
      ]);
    await page.getByRole("button", { name: "Remove" }).last().click();
    await expect
      .poll(() => sent(writes, "mcp_always_allow.remove").map((w) => w.params))
      .toEqual([{ tool: "mcp__x__y" }]);
    await stop();
  });

  test("a server's folder is chosen from what the server lists", async ({ page }) => {
    const { writes, stop } = await scripted(
      page,
      {
        reads: {
          "fs.list_dir": (r) => {
            const path = (r.params as { path: string | null }).path;
            return {
              type: "dir_listing",
              value: path
                ? { path, parent: null, entries: [], roots: [], truncated: false }
                : { path: null, entries: [], roots: [{ name: "home", path: "/home" }], truncated: false },
            };
          },
        },
      },
      "/projects",
    );
    await page.getByRole("button", { name: "New project" }).first().click();
    await page.getByRole("button", { name: "Choose…" }).click();
    await expect(page.getByRole("button", { name: "Use this folder" })).toBeDisabled();
    await page.getByRole("button", { name: "home" }).click();
    await page.getByRole("button", { name: "Use this folder" }).click();
    await expect(page.getByLabel("Project folder (required)")).toHaveValue("/home");
    expect(writes).toHaveLength(0);
    await stop();
  });
});

test.describe("around the work", () => {
  test("files on a new ticket are uploaded by a browser, and sent by the path it got", async ({ page }) => {
    const { writes, stop } = await scripted(page, {}, "/new");
    await page.route("**/v1/upload", (route) =>
      route.fulfill({ json: { path: "/srv/inbox/1/notes.txt", name: "notes.txt", byte_length: 5 } }),
    );
    // The sidebar's + is New session now (ticket #60); New ticket is Alt+T on the web, once the app is up.
    await expect(page.getByRole("heading", { level: 1, name: "New session" })).toBeVisible();
    await page.keyboard.press("Alt+t");
    await page.getByPlaceholder("What should happen?").fill("With a file");
    const [chooser] = await Promise.all([
      page.waitForEvent("filechooser"),
      page.getByRole("button", { name: "Add files…" }).click(),
    ]);
    await chooser.setFiles({ name: "notes.txt", mimeType: "text/plain", buffer: Buffer.from("hello") });
    await expect(page.getByRole("button", { name: "Remove notes.txt" })).toBeVisible();
    await page.getByRole("button", { name: "Create", exact: true }).click();
    await expect
      .poll(() => sent(writes, "ticket.create").map((w) => (w.params as { files?: string[] }).files))
      .toEqual([["/srv/inbox/1/notes.txt"]]);
    await stop();
  });

  test("the inbox clears only after asking", async ({ page }) => {
    const { writes, stop } = await scripted(
      page,
      {
        notifications: [{ ...notification(1), read: false, needs_you: false }],
      },
      "/new",
    );
    await page
      .getByRole("button", { name: /^Inbox/ })
      .first()
      .click();
    const inbox = page.getByRole("dialog", { name: "Inbox" });
    await inbox.getByRole("button", { name: "Mark all read" }).click();
    await expect.poll(() => sent(writes, "notification.mark_all_read").length).toBe(1);
    await inbox.getByRole("button", { name: "Clear all" }).click();
    expect(sent(writes, "notification.dismiss_all")).toHaveLength(0);
    await expect(inbox.getByText("Clear every update?")).toBeVisible();
    await inbox.getByRole("button", { name: "Clear", exact: true }).click();
    await expect.poll(() => sent(writes, "notification.dismiss_all").length).toBe(1);
    await stop();
  });

  test("the board's columns are named, and two the same are refused", async ({ page }) => {
    const column = (id: string, kind: string, name: string, version: number) => ({
      column: { id, kind, name, position: 0, version },
      tickets: [],
    });
    const { writes, stop } = await scripted(
      page,
      { board: { as_of_seq: 1, columns: [column("a", "backlog", "Backlog", 3), column("b", "todo", "Todo", 7)] } },
      "/board",
    );
    await page.getByRole("button", { name: "Columns" }).click();
    await page.getByLabel("Name of Todo").fill("backlog");
    await page.getByRole("button", { name: "Save columns" }).click();
    await expect(page.getByText("Each column needs its own name.")).toBeVisible();
    await page.getByLabel("Name of Todo").fill("Next");
    await page.getByRole("button", { name: "Save columns" }).click();
    await expect
      .poll(() => sent(writes, "board.columns_update").map((w) => [w.params, w.expected_version]))
      .toEqual([
        [
          {
            columns: [
              { column_id: "a", name: "Backlog" },
              { column_id: "b", name: "Next" },
            ],
          },
          7,
        ],
      ]);
    await stop();
  });

  test("a usage page shows this member's limits", async ({ page }) => {
    const { stop } = await scripted(
      page,
      {
        limits: [{ label: "Week", percent: 40, resets: "Mon" }],
        reads: {
          "usage.get": () => ({
            type: "usage",
            value: { days: [], today: { input_tokens: 0, output_tokens: 0 }, per_employee: [], per_project: [] },
          }),
        },
      },
      "/usage",
    );
    await expect(page.getByRole("heading", { name: "Usage", level: 1 })).toBeVisible();
    await expect(page.getByRole("main").getByText("40%")).toBeVisible();
    await stop();
  });
});
