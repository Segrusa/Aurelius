// Switching workspace: the rail, what narrows, where a link takes it, and the reads a fake server
// would accept whatever they said, once against a real daemon.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { _electron, expect, type Page, test } from "@playwright/test";
import { chooseWorkspace, expectWorkspace, openCompanyPage, openSettings, workspaceTile } from "./company";
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

/** A workspace tile opens its Overview (ticket #82): the tests of the Board open it again from the panel. */
async function openBoard(page: Page) {
  await page.getByRole("navigation", { name: "Main" }).getByRole("link", { name: "Board", exact: true }).click();
  await expect(page).toHaveURL(/\/board$/);
}

interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

async function open(page: Page, path: string, workspace?: string) {
  const site = await serveStatic(WEB_DIST);
  if (workspace) await page.addInitScript((id) => localStorage.setItem("aurelius.workspace:h", id), workspace);
  await scriptServer(page, script);
  await page.goto(`${site.origin}${path}`);
  return site;
}

test("a ticket shows its Claude Design link, and an older server shows no field", async ({ page }) => {
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
    // Workspace settings has no styleguide field since ticket #81; the link stays on the server.
    await page.goto(`${site.origin}/projects/${ATLAS}`);
    await expect(page.getByRole("heading", { name: "Workspace settings" })).toBeVisible();
    await expect(page.getByText("Styleguide", { exact: true })).toBeHidden();

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

test("workspace settings saves each field on its own, shows a save conflict and archives only by name", async ({
  page,
}) => {
  const site = await serveStatic(WEB_DIST);
  try {
    const { writes } = await scriptServer(page, {
      ...script,
      features: [...script.features, "project_description"],
      projects: [
        { ...project(ATLAS, "Atlas"), description: "The old one", folder: "/srv/atlas" },
        project(ZEPHYR, "Zephyr"),
      ],
      refuse: (request) =>
        request.method === "project.rename" ? { code: "version_conflict", message: "Changed elsewhere." } : null,
    });
    await page.goto(`${site.origin}/projects/${ATLAS}`);
    const main = page.getByRole("main");
    await expect(main.getByRole("heading", { level: 1, name: "Workspace settings" })).toBeVisible();
    for (const title of ["General", "Folder", "Work", "Memory and export", "Archive"]) {
      await expect(main.getByRole("heading", { level: 2, name: title, exact: true })).toBeVisible();
    }
    await expect(main.getByText("/srv/atlas")).toBeVisible();
    await expect(
      main.getByText("◆ No dispatcher. Started tickets wait in Todo until you choose an owner."),
    ).toBeVisible();

    // The description is sent with the version it was edited on.
    const description = main.getByLabel("Description");
    await description.fill("A new one");
    await expect(main.getByText("9 / 2000")).toBeVisible();
    await main.getByRole("button", { name: "Save", exact: true }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "project.set_description")?.params)
      .toEqual({ project_id: ATLAS, description: "A new one" });

    // A rename the server refuses as stale says so, with a way to load the other version.
    // (The fake server keeps the old description, so its Save still shows: Enter saves the name.)
    await main.getByLabel("Name", { exact: true }).fill("Atlas 2");
    await main.getByLabel("Name", { exact: true }).press("Enter");
    await expect(main.getByText("Someone else changed this workspace while you typed.")).toBeVisible();
    await expect(main.getByRole("button", { name: "Load their version" })).toBeVisible();

    // Status saves at once.
    await main.getByRole("radio", { name: "Paused" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "project.set_status")?.params)
      .toEqual({ project_id: ATLAS, status: "paused" });

    // Archive stays off until the exact name is typed.
    await main.getByRole("button", { name: "Archive…" }).click();
    const dialog = page.getByRole("dialog", { name: "Archive Atlas?" });
    const archive = dialog.getByRole("button", { name: "Archive", exact: true });
    await expect(archive).toBeDisabled();
    await dialog.getByLabel("Type the workspace name to confirm").fill("atlas");
    await expect(archive).toBeDisabled();
    await dialog.getByLabel("Type the workspace name to confirm").fill("Atlas");
    await archive.click();
    await expect
      .poll(() => writes.filter((w) => w.method === "project.set_status").map((w) => w.params))
      .toContainEqual({ project_id: ATLAS, status: "archived" });
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
    await openBoard(page);
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
    await openBoard(page);
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
    await openBoard(page);
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
    // The tile opens the workspace's Overview, also with the panel folded.
    await expect(page).toHaveURL(/\/overview$/);
    await expect(page.getByRole("heading", { level: 1, name: "Atlas" })).toBeVisible();
    await expect(page.getByRole("main").getByText("Zephyr ticket")).toBeHidden();

    await page.keyboard.press("Control+k");
    await page.getByRole("combobox", { name: "Command palette" }).fill("Zephyr");
    await page.getByRole("option", { name: /Zephyr.*Switch workspace/ }).click();
    await expectWorkspace(page, "Zephyr");
  } finally {
    site.close();
  }
});

// A tile is a circle until hovered, and the browser hit-tests the rounded shape: a click in the square's
// corner, before the hover had turned it into a rounded square, fell through to the rail (ticket #73).
test("a click in the corner of a workspace tile switches, also without hovering it first", async ({ page }) => {
  const site = await open(page, "/board");
  try {
    await expectWorkspace(page, "All workspaces");
    for (const [name, corner] of [
      ["Atlas", (b: Box) => ({ x: b.x + 2, y: b.y + 2 })],
      ["Zephyr", (b: Box) => ({ x: b.x + b.width - 2, y: b.y + b.height - 2 })],
    ] as const) {
      const box = await workspaceTile(page, name).boundingBox();
      if (!box) throw new Error(`no tile for ${name}`);
      // Straight from elsewhere to the corner, as a quick move does: no hover on the way.
      await page.mouse.move(0, 0);
      const { x, y } = corner(box);
      await page.mouse.click(x, y);
      await expectWorkspace(page, name);
    }
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
      await openBoard(page);
      await expect(page.getByRole("main").getByText("Atlas ticket")).toBeVisible();
      await page.evaluate(() => document.fonts.ready);
      await page.screenshot({ path: join(SHOTS as string, `workspace-${theme}.png`), animations: "disabled" });
    } finally {
      site.close();
    }
  });
}

// The Overview (ticket #82): a tile opens it; it shows the workspace's facts, what needs you, the board
// in five lanes and what got done; All workspaces shows a card per workspace.
const ZEPHYR_REVIEW = ticket("t3", 23, "review", {
  title: "Zephyr review",
  project_id: ZEPHYR,
  project_name: "Zephyr",
  owner_name: "Flux",
  owner_employee_id: "e-flux",
});
const ZEPHYR_DONE = ticket("t4", 24, "done", {
  title: "Zephyr shipped",
  project_id: ZEPHYR,
  project_name: "Zephyr",
  owner_name: "Forge",
  done_at_ms: Date.now() - 60_000,
});

async function openOverview(page: Page, path: string, workspace?: string) {
  const site = await serveStatic(WEB_DIST);
  if (workspace) await page.addInitScript((id) => localStorage.setItem("aurelius.workspace:h", id), workspace);
  await scriptServer(page, {
    ...script,
    projects: [
      project(ATLAS, "Atlas"),
      { ...project(ZEPHYR, "Zephyr"), description: "Room booking for coworking spaces.", folder: "/work/zephyr" },
    ],
    tickets: [...tickets, ZEPHYR_REVIEW, ZEPHYR_DONE],
  });
  await page.goto(`${site.origin}${path}`);
  return site;
}

test("a workspace tile opens its Overview, with its facts, needs, board and what got done", async ({ page }) => {
  const site = await openOverview(page, "/board");
  try {
    await choose(page, "Zephyr");
    await expect(page).toHaveURL(/\/overview$/);
    const main = page.getByRole("main");
    await expect(main.getByRole("heading", { level: 1, name: /Zephyr/ })).toBeVisible();
    await expect(main.getByText("Room booking for coworking spaces.")).toBeVisible();
    await expect(
      page.getByRole("navigation", { name: "Main" }).getByRole("link", { name: "Overview" }),
    ).toHaveAttribute("aria-current", "page");

    const facts = main.getByRole("region", { name: "Workspace" });
    await expect(facts.getByText("/work/zephyr")).toBeVisible();
    await expect(facts.getByText("Only each employee's own")).toBeVisible();

    const needs = main.getByRole("region", { name: "Needs you" });
    await expect(
      needs.getByRole("button", { name: /Ready for review · Flux.*#23 Zephyr review.*Review$/ }),
    ).toBeVisible();

    const board = main.getByRole("region", { name: "Board" });
    await expect(board.getByRole("button", { name: "Todo: 1" })).toBeVisible();
    await expect(board.getByRole("button", { name: "Review: 1" })).toBeVisible();
    await expect(board.getByRole("button", { name: "Done: 1" })).toBeVisible();
    await expect(main.getByRole("region", { name: "Running now" }).getByText("Nothing is running.")).toBeVisible();

    const done = main.getByRole("region", { name: "Recently done" });
    await expect(done.getByText("Today")).toBeVisible();
    await expect(done.getByRole("button", { name: /#24 Zephyr shipped.*Forge/ })).toBeVisible();

    // A Needs you card opens the ticket's session.
    await needs.getByRole("button", { name: /#23 Zephyr review/ }).click();
    await expect(page).toHaveURL(/\/ticket\/23$/);
  } finally {
    site.close();
  }
});

test("All workspaces' Overview has a card per workspace that opens it", async ({ page }) => {
  const site = await openOverview(page, "/overview");
  try {
    const main = page.getByRole("main");
    await expect(main.getByRole("heading", { level: 1, name: "All workspaces" })).toBeVisible();
    await expect(main.getByText("2 workspaces · 0 sessions running · 2 waiting on you")).toBeVisible();
    const cards = main.getByRole("region", { name: "Workspaces" });
    await expect(cards.getByRole("button", { name: "Atlas" })).toBeVisible();
    await expect(cards.getByRole("button", { name: "Zephyr" })).toContainText("2 need you");
    await cards.getByRole("button", { name: "Zephyr" }).click();
    await expectWorkspace(page, "Zephyr");
    await expect(main.getByRole("heading", { level: 1, name: /Zephyr/ })).toBeVisible();
  } finally {
    site.close();
  }
});

for (const theme of ["light", "dark"] as const) {
  test(`the Overview of a workspace, ${theme}`, async ({ page }) => {
    test.skip(!SHOTS, "only when pictures are asked for");
    const site = await openOverview(page, "/overview", ZEPHYR);
    try {
      await page.evaluate((t) => localStorage.setItem("aurelius.appearance", t), theme);
      await page.reload();
      await expect(page.getByRole("heading", { level: 1, name: /Zephyr/ })).toBeVisible();
      await page.evaluate(() => document.fonts.ready);
      await page.screenshot({ path: join(SHOTS as string, `overview-${theme}.png`), animations: "disabled" });
      await choose(page, "All workspaces");
      await expect(page.getByRole("heading", { level: 1, name: "All workspaces" })).toBeVisible();
      await page.screenshot({ path: join(SHOTS as string, `overview-all-${theme}.png`), animations: "disabled" });
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
