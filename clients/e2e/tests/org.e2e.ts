// The organisation screens against a real daemon: what the writes send has to be what the server takes.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { _electron, expect, type Page, test } from "@playwright/test";
import { REPO, serveStatic, TestDaemon, WEB_DIST } from "./daemon";
import { message, messages, scriptServer } from "./fake-server";

const SECRET = "ghp_a-very-secret-value-for-the-test";
const PASSWORD = "a-long-enough-password";

const toast = (page: Page, text: string | RegExp) =>
  page.getByRole("region", { name: "Notifications" }).getByText(text);

const go = (page: Page, name: string) => page.getByRole("navigation").getByRole("link", { name }).first().click();

test("employees, projects, guidelines, ideas, memory and the vault over a real daemon", async () => {
  test.setTimeout(120_000);
  const daemon = new TestDaemon();
  await daemon.start();
  const profile = mkdtempSync(join(tmpdir(), "aurelius-org-"));
  const folder = mkdtempSync(join(tmpdir(), "aurelius-org-project-"));
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
    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByRole("button", { name: "Create" }).click();
    await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();

    // A project, opened on creation.
    await page.getByRole("link", { name: "Projects", exact: true }).click();
    await page.getByRole("button", { name: "New project" }).first().click();
    await page.getByPlaceholder("Name, for example Atlas").fill("Atlas");
    await page.getByLabel("Project folder (required)").fill(folder);
    await page.getByRole("button", { name: "Create project" }).click();
    await expect(page.getByText("WHO WORKS HERE · 0")).toBeVisible();

    // A boss and a report, so the org chart has a line.
    await go(page, "Employees");
    await page.getByRole("button", { name: "Add employee" }).click();
    await page.getByPlaceholder("Name, for example Alex").fill("Orion");
    await page.getByPlaceholder(/Free-form role/).fill("Lead");
    await page.getByRole("button", { name: "Add", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Orion", exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Edit", exact: true }).click();
    await page.getByPlaceholder(/Optional, e.g./).fill("Head of things");
    await page.getByLabel("Description").fill("Plans the work and reviews it.");
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await expect(page.getByText("Head of things · Lead")).toBeVisible();
    await expect(page.getByText("Plans the work and reviews it.")).toBeVisible();

    // A provider, model and effort of its own, and a level of access.
    await page.getByRole("button", { name: "Edit", exact: true }).click();
    await page.getByRole("combobox", { name: "Provider" }).click();
    await page.getByRole("option", { name: /^Inherit/ }).click();
    await page.getByRole("radio", { name: "High", exact: true }).click();
    await page.getByRole("radio", { name: "Read only" }).click();
    await expect(page.getByText(/No edits, commands, web/)).toBeVisible();
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await expect(page.getByRole("dialog")).toBeHidden();
    await expect(page.getByText("Read only", { exact: true })).toBeVisible();
    await expect(page.getByText("High", { exact: true })).toBeVisible();

    await go(page, "Employees");
    await page.getByRole("button", { name: "Add employee" }).click();
    await page.getByPlaceholder("Name, for example Alex").fill("Pixel");
    await page.getByPlaceholder(/Free-form role/).fill("Designer");
    await page.getByRole("combobox", { name: "Reports to" }).click();
    await page.getByRole("option", { name: "Orion" }).click();
    // Ctrl+Enter on a closed selector still submits the dialog.
    await page.getByRole("combobox", { name: "Reports to" }).focus();
    await page.keyboard.press("Control+Enter");
    await expect(page.getByText("Designer · reports to Orion")).toBeVisible();
    await page.getByRole("link", { name: "Employees", exact: true }).first().click();
    await page.getByRole("link", { name: "Org chart" }).click();
    await expect(page.getByRole("link", { name: /Pixel/ })).toBeVisible();

    // A chat of one's own: the server takes the opening and the message, and the sidebar lists it.
    await go(page, "Employees");
    await page.getByRole("row", { name: /Orion/ }).getByRole("button", { name: "Chat", exact: true }).click();
    await expect(page.getByRole("region", { name: "Chat with Orion" })).toBeVisible();
    await expect(page.getByText("Talk with Orion here.", { exact: true }).first()).toBeVisible();
    await page.getByPlaceholder("Message Orion").fill("Hallo Orion");
    await page.keyboard.press("Enter");
    const talk = page.getByRole("region", { name: "Chat with Orion" });
    await expect(talk.getByText("Hallo Orion", { exact: true }).first()).toBeVisible();
    await expect(page.getByRole("navigation").getByRole("link", { name: "Orion" }).last()).toHaveAttribute(
      "href",
      /\/talk\//,
    );
    await expect(page.locator('[role="alert"]:not([data-astryx-live-region])')).toHaveCount(0);

    // A guideline: made, linked, changed to a second version, then archived and restored.
    await go(page, "Knowledge");
    await expect(page.getByRole("heading", { name: "No guidelines yet" })).toBeVisible();
    await page.getByRole("button", { name: "New guideline" }).first().click();
    await page.getByLabel("Title *").fill("Rust");
    await page.getByRole("textbox", { name: "Text" }).fill("## Rules\n- Keep it small");
    await page.getByRole("button", { name: "Links…" }).click();
    await page.getByRole("dialog").getByRole("checkbox", { name: /Atlas/ }).check();
    await page.getByRole("dialog").getByRole("button", { name: "Done" }).click();
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await expect(page.getByRole("link", { name: "Rust" })).toBeVisible();

    await page.getByRole("link", { name: "Rust" }).click();
    await expect(page.getByLabel("Title *")).toHaveValue("Rust");
    await page.getByRole("textbox", { name: "Text" }).fill("## Rules\n- Keep it smaller");
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await expect(page.getByText("v2")).toBeVisible();

    // An idea: made, then dropped.
    await page.getByRole("link", { name: "Ideas" }).click();
    await page.getByRole("button", { name: "New idea" }).click();
    await page.getByPlaceholder(/The idea, for example/).fill("Dark mode");
    await page.getByRole("button", { name: "Add idea" }).click();
    const idea = page.getByRole("region", { name: "Ideas" }).getByText("Dark mode");
    await expect(idea).toBeVisible();
    await page.getByRole("button", { name: "Drop", exact: true }).click();
    await expect(idea).toBeHidden();

    // A memory, in the context of the project.
    await page.getByRole("link", { name: "Memory" }).click();
    await page.getByLabel("Remember for this project").fill("Publishing needs a review first");
    await page.getByRole("button", { name: "Remember", exact: true }).click();
    await expect(toast(page, /Saved to project memory/)).toBeVisible();
    await page.getByRole("tab", { name: "In context" }).click();
    await expect(page.getByText("Publishing needs a review first").first()).toBeVisible();

    // A vault secret: never in the page until asked, hidden again on Hide, and it copies.
    await page.getByRole("link", { name: "Vault" }).click();
    await page.getByRole("button", { name: "New item" }).first().click();
    await page.getByRole("button", { name: "Secret" }).click();
    await page.getByPlaceholder("GitHub token").fill("GitHub token");
    await page.getByRole("dialog").locator('input[type="password"]').fill(SECRET);
    await page.getByRole("dialog").getByRole("button", { name: "Save" }).click();
    await expect(page.getByRole("button", { name: "Show GitHub token" })).toBeVisible();
    expect(await page.content()).not.toContain(SECRET);
    await page.getByRole("button", { name: "Show GitHub token" }).click();
    await expect(page.getByText(SECRET)).toBeVisible();
    await page.getByRole("button", { name: "Hide GitHub token" }).click();
    await expect(page.getByText(SECRET)).toBeHidden();
    expect(await page.content()).not.toContain(SECRET);

    await page.getByRole("button", { name: "Copy GitHub token" }).click();
    await expect(toast(page, "Copied. Clears in 60 s.")).toBeVisible();
    expect(await app.evaluate(({ clipboard }) => clipboard.readText())).toBe(SECRET);
    expect(await page.content()).not.toContain(SECRET);

    await page.getByRole("tab", { name: "Audit log" }).click();
    await expect(page.getByText("The audit log is read-only")).toBeVisible();
    await expect(page.getByText("GitHub token").first()).toBeVisible();
    await page.getByRole("tab", { name: "Items" }).click();

    // The key: a wrong password stays out, the right one shows it once.
    await page.getByRole("button", { name: "Export key…" }).click();
    await page.getByPlaceholder("Your password").fill("not-the-password");
    await page.getByRole("dialog").getByRole("button", { name: "Export key", exact: true }).click();
    await expect(page.getByRole("alert")).toBeVisible();
    await page.getByPlaceholder("Your password").fill(PASSWORD);
    await page.getByRole("dialog").getByRole("button", { name: "Export key", exact: true }).click();
    await expect(page.getByLabel("The vault key, shown once")).toBeVisible();
    await page.getByRole("button", { name: "Done" }).click();
    await expect(page.getByLabel("The vault key, shown once")).toBeHidden();
    // The Scheduled page over the real daemon: what an employee planned (no real run here,
    // so a row in the store), then taken back through the real write.
    const store = new DatabaseSync(join(daemon.data, "data", "aurelius.sqlite"));
    store
      .prepare(
        `INSERT INTO scheduled_messages(id, from_employee_id, to_employee_id, content, thread_key, place, planned_by_run, due_at_ms, created_at_ms)
         SELECT '00000000-0000-7000-8000-00000000e701', id, id, 'Check the deploy again.', 'chat:x', 'its own chat', 'r1', ?, ? FROM employees LIMIT 1`,
      )
      .run(Date.now() + 3_600_000, Date.now());
    store.close();
    await go(page, "Scheduled");
    const waiting = page.getByRole("region", { name: "Scheduled messages" });
    await expect(waiting.getByText("Check the deploy again.")).toBeVisible();
    await waiting.getByRole("button", { name: "Cancel" }).click();
    await page.getByRole("button", { name: "Take back" }).click();
    await expect(page.getByRole("region", { name: "What came of them" }).getByText("Taken back")).toBeVisible();
    await expect(waiting.getByText("No messages are scheduled.")).toBeVisible();
    await expect(
      page.getByRole("region", { name: "Recurring tasks" }).getByText("No recurring tasks yet."),
    ).toBeVisible();
    await expect(page.getByRole("region", { name: "Heartbeats" })).toHaveCount(0);
  } finally {
    await app.close().catch(() => undefined);
    await daemon.stop();
    rmSync(profile, { recursive: true, force: true });
    rmSync(folder, { recursive: true, force: true });
  }
});

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
const pixel = {
  id: "e2",
  name: "Pixel",
  role: "Designer",
  reports_to: "e1",
  status: "active",
  working: false,
  agents: [],
  projects_active: [],
  agent_mode: "ask",
  version: 1,
};
const atlas = {
  id: "p1",
  name: "Atlas",
  status: "active",
  folder: "/srv/atlas",
  version: 1,
  created_at_ms: 0,
  employees_active: 1,
};
const me = { id: "u1", username: "sam" };
const rust = {
  id: "g1",
  title: "Rust",
  chars: 18,
  version: 2,
  archived: false,
  links: [],
  open_proposals: 0,
  updated_at_ms: 1_700_000_000_000,
  updated_by: me,
};
const versionOf = (version: number, text: string) => ({
  by: me,
  created_at_ms: 1_700_000_000_000,
  guideline_id: "g1",
  source: "manual",
  text,
  title: "Rust",
  version,
});
const token = {
  id: "v1",
  name: "GitHub token",
  env_name: "GITHUB_TOKEN",
  kind: "secret",
  version: 1,
  created_at_ms: 0,
  updated_at_ms: 0,
  created_by: me,
  updated_by: me,
};

async function scripted(page: Page, script: Parameters<typeof scriptServer>[1], path: string) {
  const server = await serveStatic(WEB_DIST);
  const fake = await scriptServer(page, {
    employees: [orion, pixel],
    projects: [atlas],
    features: ["guidelines", "vault", "instruction_proposals"],
    ...script,
  });
  await page.goto(`${server.origin}${path}`);
  return { ...fake, stop: server.close };
}

const vaultReads = (value: string) => ({
  "vault.list": () => ({ type: "vault", value: { items: [token], company_items: [], counts: [] } }),
  "vault.reveal": () => ({ type: "vault_value", value: { item_id: "v1", value } }),
  "vault.audit": () => ({ type: "vault_audit", value: { rows: [], more: false } }),
});

test.describe("scripted organisation", () => {
  test("the org chart shows the tree and a proposal, and the decision goes to the server", async ({ page }) => {
    const { writes, stop } = await scripted(
      page,
      {
        proposals: [
          {
            id: "pr1",
            kind: "colleague",
            status: "pending",
            name: "Vega",
            role: "Researcher",
            title: "Head of research",
            reports_to: "e1",
            reports_to_name: "Orion",
            proposed_by: "e1",
            proposed_by_name: "Orion",
            reason: "We need someone for the reading.",
            instructions: "Read papers.",
          },
        ],
      },
      "/employees/org",
    );
    await expect(page.getByRole("link", { name: /Pixel/ })).toBeVisible();
    await expect(page.getByRole("link", { name: /Orion/ })).toBeVisible();
    await expect(page.getByText("Orion proposes Vega")).toBeVisible();
    await expect(page.getByText("Head of research · Researcher · reports to Orion")).toBeVisible();
    await page.getByRole("button", { name: "Approve" }).click();
    await expect.poll(() => writes.filter((w) => w.method === "proposal.decide").length).toBe(1);
    expect(writes.find((w) => w.method === "proposal.decide")?.params).toMatchObject({
      proposal_id: "pr1",
      approve: true,
    });
    await stop();
  });

  test("a proposal for instructions that changed since cannot be approved", async ({ page }) => {
    const { stop } = await scripted(
      page,
      {
        proposals: [
          {
            id: "pr2",
            kind: "instructions",
            status: "pending",
            name: "Pixel",
            role: "Designer",
            target_employee_id: "e2",
            target_employee_name: "Pixel",
            proposed_by: "e1",
            proposed_by_name: "Orion",
            base_version: 1,
            current_version: 2,
            current_instructions: "Old line",
            instructions: "New line",
          },
        ],
      },
      "/employees/org",
    );
    await expect(page.getByRole("alert")).toContainText("version 1 → 2");
    await expect(page.getByRole("button", { name: "Approve" })).toBeHidden();
    await expect(page.getByRole("button", { name: "Decline" })).toBeVisible();
    await expect(page.getByText("+ New line")).toBeVisible();
    await stop();
  });

  test("a version conflict keeps what was typed and can compare it", async ({ page }) => {
    let served = 2;
    const { writes, stop } = await scripted(
      page,
      {
        guidelines: [rust],
        reads: {
          "guideline.get": () => ({
            type: "guideline",
            value: { guideline: { ...rust, version: served }, version: versionOf(served, `text of ${served}`) },
          }),
          "guideline.affected": () => ({
            type: "guideline_affected",
            value: { guideline_id: "g1", employees: [], projects: [], conversations: 0 },
          }),
        },
        refuse: (w) => {
          if (w.method !== "guideline.update") return null;
          served = 3;
          return { code: "version_conflict", message: "Changed." };
        },
      },
      "/knowledge/guidelines/g1",
    );
    const text = page.getByRole("textbox", { name: "Text" });
    await expect(text).toHaveValue("text of 2");
    await text.fill("what I typed");
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await expect(
      page.getByText("Version 3 was saved while you were editing. Nothing of yours was saved."),
    ).toBeVisible();
    await expect(text).toHaveValue("what I typed");
    expect(writes.find((w) => w.method === "guideline.update")?.expected_version).toBe(2);
    await page.getByRole("button", { name: "Compare" }).click();
    await expect(page.getByText("- text of 3")).toBeVisible();
    await expect(page.getByText("+ what I typed")).toBeVisible();
    await page.getByRole("button", { name: "Discard mine" }).click();
    await expect(text).toHaveValue("text of 3");
    await stop();
  });

  test("Edit and approve sends the edited text with the decision", async ({ page }) => {
    const { writes, stop } = await scripted(
      page,
      {
        reads: {
          "guideline.proposals": () => ({
            type: "guideline_proposals",
            value: {
              proposals: [
                {
                  id: "gp1",
                  title: "Reviews",
                  text: "Review it twice.",
                  reason: "Bugs got through.",
                  status: "pending",
                  proposed_by: "e1",
                  suggested_links: [{ kind: "employee", id: "e2" }],
                },
              ],
            },
          }),
        },
      },
      "/knowledge/guidelines/new?proposal=gp1",
    );
    const text = page.getByRole("textbox", { name: "Text" });
    await expect(text).toHaveValue("Review it twice.");
    await text.fill("Review it three times.");
    await page.getByRole("button", { name: "Approve" }).click();
    await expect.poll(() => writes.filter((w) => w.method === "guideline.decide_proposal").length).toBe(1);
    expect(writes.find((w) => w.method === "guideline.decide_proposal")?.params).toMatchObject({
      proposal_id: "gp1",
      approve: true,
      text: "Review it three times.",
      links: [{ kind: "employee", id: "e2" }],
    });
    await stop();
  });

  test("a shown secret hides when the window loses focus and when leaving the page", async ({ page }) => {
    const { stop } = await scripted(page, { reads: vaultReads(SECRET) }, "/knowledge/vault");
    const show = page.getByRole("button", { name: "Show GitHub token" });
    await show.click();
    await expect(page.getByText(SECRET)).toBeVisible();
    await page.evaluate(() => window.dispatchEvent(new Event("blur")));
    await expect(page.getByText(SECRET)).toBeHidden();
    expect(await page.content()).not.toContain(SECRET);

    await show.click();
    await expect(page.getByText(SECRET)).toBeVisible();
    await page.getByRole("link", { name: "Ideas" }).click();
    await expect(page.getByText(SECRET)).toBeHidden();
    expect(await page.content()).not.toContain(SECRET);
    await stop();
  });

  test("a secret disappears after its 30 seconds", async ({ page }) => {
    await page.clock.install();
    const { stop } = await scripted(page, { reads: vaultReads(SECRET) }, "/knowledge/vault");
    await page.getByRole("button", { name: "Show GitHub token" }).click();
    await expect(page.getByText(SECRET)).toBeVisible();
    await expect(page.getByText(/Hides in 0:(30|29)/)).toBeVisible();
    await page.clock.runFor(31_000);
    await expect(page.getByText(SECRET)).toBeHidden();
    await stop();
  });

  test("a refused secret is reported and not sent again, and in a browser Copy does not promise to clear", async ({
    page,
    context,
  }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    const { writes, stop } = await scripted(
      page,
      {
        reads: vaultReads(SECRET),
        refuse: (w) => (w.method === "vault.create" ? { code: "invalid", message: "Refused." } : null),
      },
      "/knowledge/vault",
    );
    await page.getByRole("button", { name: "Copy GitHub token" }).click();
    await expect(toast(page, /A browser may keep it on the clipboard/)).toBeVisible();
    await expect(toast(page, "Copied. Clears in 60 s.")).toBeHidden();

    await page.getByRole("button", { name: "New item" }).first().click();
    await page.getByRole("button", { name: "Secret" }).click();
    await page.getByPlaceholder("GitHub token").fill("Other");
    await page.getByRole("dialog").locator('input[type="password"]').fill("another-secret-value");
    await page.getByRole("dialog").getByRole("button", { name: "Save" }).click();
    await expect(page.getByRole("dialog").getByRole("alert")).toContainText("Refused.");
    expect(writes.filter((w) => w.method === "vault.create")).toHaveLength(1);
    await stop();
  });

  test("the sidebar shows the latest four chats, one unread, and a chat answers on chat.send", async ({ page }) => {
    const chat = (id: string, name: string, unread: number) => ({
      conversation_id: id,
      employee_id: "e1",
      employee_name: name,
      unread,
      last_activity_ms: 1,
    });
    const { writes, stop } = await scripted(
      page,
      {
        features: ["employee_chats"],
        conversations: {
          c1: messages("c1", [
            message("m1", "user", "Can you rename it?"),
            message("m2", "assistant", "Made a ticket for it."),
            message("m3", "assistant", "Which name?\n\nFrom ticket #7 Rename", {
              update: "ticket_note",
              author: "Orion",
            }),
          ]),
        },
        reads: {
          "employee.chats": () => ({
            type: "employee_chats",
            value: {
              as_of_seq: 1,
              chats: [
                chat("c1", "Orion", 0),
                chat("c2", "Mira", 2),
                chat("c3", "Bo", 0),
                chat("c4", "Cy", 0),
                chat("c5", "Fifth", 0),
              ],
            },
          }),
        },
        results: {
          "employee.open_chat": () => ({
            kind: "command",
            value: { type: "employee_chat_opened", conversation_id: "c1" },
          }),
        },
      },
      "/employees",
    );
    const nav = page.getByRole("navigation");
    await expect(nav.getByRole("link", { name: "Mira" })).toBeVisible();
    await expect(nav.getByRole("link", { name: "Cy" })).toBeVisible();
    await expect(nav.getByRole("link", { name: "Fifth" })).toHaveCount(0);

    await page.getByRole("row", { name: /Pixel/ }).getByRole("button", { name: "Chat", exact: true }).click();
    await expect.poll(() => writes.filter((w) => w.method === "employee.open_chat").length).toBe(1);
    expect(writes.find((w) => w.method === "employee.open_chat")?.params).toMatchObject({ employee_id: "e2" });
    const talk = page.getByRole("region", { name: "Chat with Orion" });
    await expect(talk.getByText("Made a ticket for it.")).toBeVisible();
    await expect(talk.getByText("FROM A TICKET")).toBeVisible();
    await expect(talk.getByText(/From ticket #7/)).toBeVisible();

    await page.getByPlaceholder("Message Orion").fill("Call it Vega");
    await page.keyboard.press("Enter");
    await expect.poll(() => writes.filter((w) => w.method === "chat.send").length).toBe(1);
    expect(writes.find((w) => w.method === "chat.send")?.params).toMatchObject({
      conversation_id: "c1",
      text: "Call it Vega",
    });
    expect(writes.filter((w) => w.method === "group_chat.send")).toHaveLength(0);
    await stop();
  });

  test("the Scheduled page lists what comes back and a chat shows who waits for it", async ({ page }) => {
    const planned = {
      from_employee: "e1",
      from_name: "Orion",
      to_employee: "e1",
      to_name: "Orion",
      created_at_ms: 1,
      due_at_ms: Date.now() + 3_600_000,
      place: "the discussion of ticket #21",
      ticket_id: "t1",
    };
    const { writes, stop } = await scripted(
      page,
      {
        features: ["scheduled_messages"],
        reads: {
          "scheduled_message.list": () => ({
            type: "scheduled_messages",
            value: {
              as_of_seq: 1,
              messages: [
                { ...planned, id: "s1", text: "Check the CI of PR 12.", state: "scheduled" },
                {
                  ...planned,
                  id: "s2",
                  text: "Look at the deploy.",
                  to_employee: "e2",
                  to_name: "Pixel",
                  state: "not_delivered",
                  outcome: "Pixel is paused.",
                },
              ],
            },
          }),
          "recurring_task.list": () => ({
            type: "task_schedules",
            value: {
              as_of_seq: 1,
              schedules: [
                {
                  id: "r1",
                  title: "Weekly report",
                  goal: "Report",
                  criteria: [],
                  employee_id: "e1",
                  employee_name: "Orion",
                  recurrence: { kind: "daily", at_minute: 540 },
                  paused: false,
                  run: {},
                  attachments: [],
                  created_at_ms: 1,
                },
              ],
            },
          }),
        },
      },
      "/scheduled",
    );
    await expect(page.getByRole("heading", { name: "Scheduled" })).toBeVisible();
    const waiting = page.getByRole("region", { name: "Scheduled messages" });
    await expect(waiting.getByRole("row", { name: /Check the CI of PR 12/ })).toContainText("Orion");
    await expect(waiting.getByRole("link", { name: "the discussion of ticket #21" })).toBeVisible();
    await expect(
      page.getByRole("region", { name: "What came of them" }).getByText("Not delivered: Pixel is paused."),
    ).toBeVisible();
    await expect(page.getByRole("region", { name: "Recurring tasks" }).getByText("Daily at 09:00")).toBeVisible();
    await expect(page.getByRole("region", { name: "Heartbeats" })).toHaveCount(0);

    await waiting.getByRole("button", { name: "Cancel" }).click();
    await page.getByRole("button", { name: "Take back" }).click();
    await expect.poll(() => writes.filter((w) => w.method === "scheduled_message.cancel").length).toBe(1);
    expect(writes.find((w) => w.method === "scheduled_message.cancel")?.params).toMatchObject({ message_id: "s1" });
    await stop();
  });

  test("under a chat's title stands who waits to pick it up and what is scheduled", async ({ page }) => {
    const { stop } = await scripted(
      page,
      {
        features: ["scheduled_messages", "employee_chats"],
        conversations: { c1: messages("c1", [message("m1", "assistant", "Noted.")]) },
        reads: {
          "thread.waiting": () => ({
            type: "thread_waiting",
            value: {
              as_of_seq: 1,
              waiters: [{ employee_id: "e1", employee_name: "Orion", behind: 2 }],
              scheduled: [],
            },
          }),
        },
      },
      "/talk/c1",
    );
    await expect(page.getByLabel("Who waits for this thread")).toHaveText(
      "Orion — waiting to pick up this thread (behind 2 items)",
    );
    await stop();
  });
});
