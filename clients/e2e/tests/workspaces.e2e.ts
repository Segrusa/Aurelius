// Switching workspace: the rail, what narrows, where a link takes it, and the reads a fake server
// would accept whatever they said, once against a real daemon.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { _electron, expect, type Page, test } from "@playwright/test";
import { chooseWorkspace, expectWorkspace, openCompanyPage, openSettings } from "./company";
import { REPO, serveStatic, TestDaemon, WEB_DIST } from "./daemon";
import { boardOf, channel, messages, notification, scriptServer, task, ticket } from "./fake-server";
import { expectNewSession } from "./new-session";

const SHOTS = process.env.AURELIUS_WORKSPACE_SHOTS;

const project = (id: string, name: string, status = "active") => ({
  id,
  name,
  status,
  created_at_ms: 0,
  employees_active: 0,
  version: 1,
});

const ATLAS = "11111111-1111-4111-8111-111111111111";
const ZEPHYR = "22222222-2222-4222-8222-222222222222";

const projects = [project(ATLAS, "Atlas"), project(ZEPHYR, "Zephyr")];
const inAtlas = ticket("t1", 21, "backlog", { title: "Atlas ticket", project_id: ATLAS, project_name: "Atlas" });
const inZephyr = ticket("t2", 22, "todo", { title: "Zephyr ticket", project_id: ZEPHYR, project_name: "Zephyr" });
const tickets = [inAtlas, inZephyr];
const of = (request: Record<string, unknown>) => (request.params as { project_id?: string } | undefined)?.project_id;

const noteIn = (i: number, projectId: string, subject: string) => ({
  ...notification(i),
  subject,
  project_id: projectId,
});
const notes = [noteIn(1, ATLAS, "Atlas news"), noteIn(2, ZEPHYR, "Zephyr news")];

const script = {
  features: ["workspace_filters", "ticket_channels", "chat_threads"],
  projects,
  tickets,
  tasks: [
    task("k1", { project_id: ATLAS, title: "Atlas task" }),
    task("k2", { project_id: ZEPHYR, title: "Zephyr task" }),
  ],
  details: {
    t1: { ticket: inAtlas, root_task: null, tasks: [], as_of_seq: 1 },
    t2: { ticket: inZephyr, root_task: null, tasks: [], as_of_seq: 1 },
  },
  channels: [
    channel("c-t1", "21-atlas-ticket", { kind: "ticket", project_id: ATLAS, ticket_id: "t1", ticket_number: 21 }),
  ],
  feeds: {
    "c-t1": messages("c-t1", []),
  },
  reads: {
    "board.get": (request: Record<string, unknown>) => ({
      type: "board",
      value: boardOf(tickets.filter((t) => !of(request) || t.project_id === of(request))),
    }),
    "notification.list": (request: Record<string, unknown>) => ({
      type: "notifications",
      value: { notifications: notes.filter((n) => !of(request) || n.project_id === of(request)), unread: 0 },
    }),
  },
};

const choose = chooseWorkspace;

async function open(page: Page, path: string, workspace?: string) {
  const site = await serveStatic(WEB_DIST);
  if (workspace) await page.addInitScript((id) => localStorage.setItem("aurelius.workspace:h", id), workspace);
  await scriptServer(page, script);
  await page.goto(`${site.origin}${path}`);
  return site;
}

test("a workspace and a ticket show their Claude Design link, and an older server shows no field", async ({ page }) => {
  const link = "https://claude.ai/artifact/9sfwWLbDbQerBZ3jmBSkXS";
  const linkedTicket = ticket("t1", 21, "backlog", {
    title: "Atlas ticket",
    project_id: ATLAS,
    project_name: "Atlas",
    design_url: link,
  });
  const linked = {
    ...script,
    features: [...script.features, "design_links"],
    projects: [{ ...project(ATLAS, "Atlas"), styleguide_url: link }, project(ZEPHYR, "Zephyr")],
    details: { t1: { ticket: linkedTicket, root_task: null, tasks: [], as_of_seq: 1 } },
    tickets: [linkedTicket, inZephyr],
  };
  const site = await serveStatic(WEB_DIST);
  try {
    await scriptServer(page, linked);
    await page.goto(`${site.origin}/projects/${ATLAS}`);
    await expect(page.getByText("Styleguide", { exact: true })).toBeVisible();
    await expect(page.getByLabel("Styleguide link")).toHaveValue(link);
    await expect(page.getByRole("button", { name: "Open", exact: true }).first()).toBeVisible();
    await expect(page.getByText("The link only works for people the artifact is shared with.")).toBeVisible();

    // The old Board link opens the ticket's session; the design link is in its properties.
    await page.goto(`${site.origin}/board?ticket=t1`);
    await expect(page).toHaveURL(/\/ticket\/21$/);
    const panel = page.getByRole("complementary", { name: "Ticket properties" });
    await expect(panel.getByRole("link", { name: "Open", exact: true })).toHaveAttribute("href", link);
    await expect(panel.getByText("The link only works for people the artifact is shared with.")).toBeVisible();

    await scriptServer(page, script);
    await page.goto(`${site.origin}/projects/${ATLAS}`);
    await expect(page.getByRole("heading", { name: "Workspace settings" })).toBeVisible();
    await expect(page.getByText("Styleguide", { exact: true })).toBeHidden();
    await page.goto(`${site.origin}/board?ticket=t1`);
    await expect(page.getByRole("complementary", { name: "Ticket properties" })).toBeVisible();
    await expect(page.getByText("The link only works for people the artifact is shared with.")).toBeHidden();
  } finally {
    await site.close();
  }
});

test("switching narrows the board and the inbox, and All workspaces shows everything", async ({ page }) => {
  const site = await open(page, "/board");
  try {
    await expect(page.getByRole("main").getByText("Atlas ticket")).toBeVisible();
    await expect(page.getByRole("main").getByText("Zephyr ticket")).toBeVisible();

    await choose(page, "Atlas");
    await expect(page.getByRole("main").getByText("Atlas ticket")).toBeVisible();
    await expect(page.getByRole("main").getByText("Zephyr ticket")).toBeHidden();

    await page
      .getByRole("button", { name: /^Inbox/ })
      .first()
      .click();
    const inbox = page.getByRole("dialog", { name: "Inbox" });
    await expect(inbox.getByText("Atlas news").first()).toBeVisible();
    await expect(inbox.getByText("Zephyr news")).toBeHidden();
    await page.keyboard.press("Escape");

    // Remembered per device.
    await choose(page, "Zephyr");
    await page.reload();
    await expectWorkspace(page, "Zephyr");

    await choose(page, "All workspaces");
    await expect(page.getByRole("main").getByText("Atlas ticket")).toBeVisible();
    await expect(page.getByRole("main").getByText("Zephyr ticket")).toBeVisible();
  } finally {
    site.close();
  }
});

test("a link to another project's ticket moves the workspace there", async ({ page }) => {
  const site = await open(page, "/new", ZEPHYR);
  try {
    await expectWorkspace(page, "Zephyr");
    await page.goto(`${new URL(page.url()).origin}/board?ticket=t1`);
    await expectWorkspace(page, "Atlas");
    await expect(page.getByText("Atlas ticket").first()).toBeVisible();
  } finally {
    site.close();
  }
});

test("new work starts in the workspace", async ({ page }) => {
  const site = await open(page, "/board");
  try {
    await choose(page, "Zephyr");
    // The Board's own New ticket.
    await page.getByRole("main").getByRole("button", { name: "New ticket" }).click();
    const dialog = page.getByRole("dialog");
    await expect(dialog.getByRole("button", { name: "Project" })).toContainText("Zephyr");
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    // The sidebar's New session row, in the same workspace.
    await page.getByRole("link", { name: "New session" }).click();
    await expectNewSession(page);
    await expect(page.getByRole("button", { name: /^Project/ })).toContainText("Zephyr");
  } finally {
    site.close();
  }
});

test("the rail switches with the sidebar folded, and the palette switches too", async ({ page }) => {
  const site = await open(page, "/board");
  try {
    await expect(page.getByRole("main").getByText("Zephyr ticket")).toBeVisible();
    await page.keyboard.press("Control+b");
    // Folded, only the rail stays.
    await expect(page.locator("[data-sidebar]").first()).toHaveJSProperty("offsetWidth", 56);
    await expect(page.getByRole("navigation", { name: "Main" })).toHaveCount(0);
    await choose(page, "Atlas");
    await expect(page.getByRole("main").getByText("Zephyr ticket")).toBeHidden();

    await page.keyboard.press("Control+k");
    await page.getByRole("combobox", { name: "Command palette" }).fill("Zephyr");
    await page.getByRole("option", { name: /Zephyr.*Switch workspace/ }).click();
    await expectWorkspace(page, "Zephyr");
  } finally {
    site.close();
  }
});

// The Board no longer opens a panel beside its columns: a card opens the ticket's session (ticket #70, O1).
test("a card opens its ticket's session instead of a panel beside the columns", async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 800 });
  const site = await open(page, "/board");
  try {
    await page.getByRole("main").getByText("Zephyr ticket").click();
    await expect(page).toHaveURL(/\/ticket\/\d+$/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Zephyr ticket");
    await expect(page.locator("[data-board-columns]")).toHaveCount(0);
  } finally {
    site.close();
  }
});

for (const theme of ["dark", "light"] as const) {
  test(`the board of a workspace, ${theme}`, async ({ page }) => {
    test.skip(!SHOTS, "only when pictures are asked for");
    const site = await open(page, "/board");
    try {
      await page.evaluate((t) => {
        localStorage.setItem("aurelius.appearance", t);
      }, theme);
      await page.reload();
      await choose(page, "Atlas");
      await expect(page.getByRole("main").getByText("Atlas ticket")).toBeVisible();
      await page.evaluate(() => document.fonts.ready);
      await page.screenshot({ path: join(SHOTS as string, `workspace-${theme}.png`), animations: "disabled" });
    } finally {
      site.close();
    }
  });
}

test("a real daemon takes the filters of a workspace", async () => {
  test.setTimeout(120_000);
  const daemon = new TestDaemon();
  await daemon.start();
  const profile = mkdtempSync(join(tmpdir(), "aurelius-workspace-"));
  const folder = mkdtempSync(join(tmpdir(), "aurelius-workspace-project-"));
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
  try {
    const page = await app.firstWindow();
    await page.getByLabel("Name").fill("sam");
    await page.getByLabel("Password").fill("a-long-enough-password");
    await page.getByRole("button", { name: "Create" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "New session" })).toBeVisible();
    await openCompanyPage(page, "Knowledge");
    await page.getByRole("link", { name: "Projects", exact: true }).click();
    await page.getByRole("button", { name: "New project" }).first().click();
    await page.getByPlaceholder("Name, for example Atlas").fill("Atlas");
    await page.getByLabel("Project folder (required)").fill(folder);
    await page.getByRole("button", { name: "Create project" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "Workspace settings" })).toBeVisible();

    await choose(page, "Atlas");
    // What refuses an unknown param answers with an error, and each screen would say so.
    const noFailure = () => expect(page.getByText(/did not load|not valid|is not valid/)).toHaveCount(0);
    await page
      .getByRole("button", { name: /^Inbox/ })
      .first()
      .click();
    await expect(
      page.getByRole("dialog", { name: "Inbox" }).getByText("Nothing here. You are up to date."),
    ).toBeVisible();
    await noFailure();
    await page.keyboard.press("Escape");
    await page.evaluate(() => {
      window.history.pushState({}, "", "/scheduled");
      window.dispatchEvent(new PopStateEvent("popstate"));
    });
    await expect(page.getByText("No messages are scheduled.").first()).toBeVisible();
    await noFailure();
    await openSettings(page);
    await page.getByRole("link", { name: "Activity log" }).click();
    await expect(page.getByText("Only Atlas.", { exact: false })).toBeVisible();
    await noFailure();
    await expect(page.getByRole("heading", { name: "Nothing here." }).or(page.getByRole("table"))).toBeVisible();
  } finally {
    await app.close();
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
    rmSync(folder, { recursive: true, force: true });
  }
});
