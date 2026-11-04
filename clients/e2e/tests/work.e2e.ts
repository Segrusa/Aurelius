import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Connection, expectBody } from "@aurelius/client";
import { _electron, expect, type Locator, type Page, test } from "@playwright/test";
import { daemonEndpoint, openPipe } from "../../electron/src/main/pipe";
import { REPO, serveStatic, TestDaemon, until, WEB_DIST } from "./daemon";
import { approval, boardOf, message, messages, notification, scriptServer, task, ticket } from "./fake-server";

const PASSWORD = "a-long-enough-password";

async function drag(page: Page, from: Locator, to: Locator) {
  await from.scrollIntoViewIfNeeded();
  const a = await from.boundingBox();
  const b = await to.boundingBox();
  if (!a || !b) throw new Error("Nothing to drag.");
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(a.x + a.width / 2 + 8, a.y + a.height / 2 + 8, { steps: 3 });
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2, { steps: 12 });
  await page.mouse.up();
}

const nav = (page: Page, name: string) =>
  page.getByRole("navigation").getByRole("link", { name, exact: false }).first().click();

test("the board, the ticket panel and My tickets against a real daemon", async () => {
  const daemon = new TestDaemon();
  await daemon.start();
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
    // Room for the columns the drags go between.
    await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]?.setSize(1800, 900));
    await page.getByLabel("Name").fill("sam");
    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByRole("button", { name: "Create" }).click();
    await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();

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
    const projects = expectBody(await server.call("project.list"), "projects").value.projects;
    const project = projects.find((p) => p.name === "Northwind API");
    if (!project) throw new Error("No project.");
    for (const title of ["First ticket", "Second ticket", "Third ticket"]) {
      await server.write("ticket.create", { project_id: project.id, title }, { retry: false });
    }
    const read = async () => {
      const board = expectBody(await server.call("board.get", { project_id: project.id }), "board").value;
      return Object.fromEntries(board.columns.map((c) => [c.column.kind, c.tickets.map((t) => t.title)]));
    };

    await nav(page, "Board");
    const backlog = page.getByRole("region", { name: "Backlog" });
    const card = (name: string) => page.getByRole("button", { name: new RegExp(`#\\d+ ${name}`) });
    await expect(card("First ticket")).toBeVisible();

    // Within a column: the order is saved.
    const before = (await read()).backlog as string[];
    const top = before[0] as string;
    const bottom = before.at(-1) as string;
    await drag(page, card(bottom), card(top));
    await until("reordered", async () => ((await read()).backlog as string[])[0] === bottom);

    // Backlog → Done is not a move the board allows.
    await drag(page, card(top), page.getByRole("region", { name: "Done" }));
    await expect(page.getByText(/Done/).last()).toBeVisible();
    expect((await read()).backlog).toContain(top);

    // Backlog → Todo starts it, and without a dispatcher it asks for an employee.
    await drag(page, card("Second ticket"), page.getByRole("region", { name: "Todo" }));
    await until("started", async () => ((await read()).todo as string[]).includes("Second ticket"));
    await expect(backlog.getByText("Second ticket")).toHaveCount(0);
    await card("Second ticket").click();
    const panel = page.getByRole("complementary", { name: /Second ticket/ });
    await expect(panel.getByText("No dispatcher is set. Choose an employee.")).toBeVisible();

    // Edit in place.
    await panel.getByRole("button", { name: "Edit Title" }).click();
    await panel.getByLabel("Title").fill("Second ticket, renamed");
    await page.keyboard.press("Enter");
    await until("renamed", async () => ((await read()).todo as string[]).includes("Second ticket, renamed"));

    // The discussion, and what the activity says of it.
    await panel.getByRole("tab", { name: /Discussion/ }).click();
    await panel.getByLabel("Reply in discussion").fill("Does this need a migration?");
    await page.keyboard.press("Enter");
    await expect(panel.getByText("Does this need a migration?")).toBeVisible();
    await panel.getByRole("tab", { name: "Activity" }).click();
    await expect(panel.getByText("Nothing happened yet.")).toHaveCount(0);

    // Back closes the panel.
    await page.getByRole("heading", { name: "Board" }).click();
    await page.keyboard.press("Escape");
    await expect(panel).toHaveCount(0);

    // By keyboard: pick up with Space, move with the arrows, drop with Space.
    await card("First ticket").focus();
    await page.keyboard.press("Space");
    await page.waitForTimeout(100);
    await page.keyboard.press("ArrowRight");
    await page.waitForTimeout(100);
    await page.keyboard.press("Space");
    await until("moved by keyboard", async () => ((await read()).todo as string[]).includes("First ticket"));

    await nav(page, "My tickets");
    await expect(page.getByRole("heading", { name: "My tickets" })).toBeVisible();
    await page.getByRole("button", { name: /^Created by me/ }).click();
    await expect(page.getByRole("button", { name: /Second ticket, renamed/ })).toBeVisible();
    await page.getByRole("button", { name: /Second ticket, renamed/ }).click();
    await expect(page.getByRole("complementary", { name: /Second ticket, renamed/ })).toBeVisible();

    await nav(page, "Tasks");
    await expect(page.getByRole("heading", { name: "Tasks" })).toBeVisible();
  } finally {
    seed?.dispose();
    await app.close();
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
  }
});

test.describe("states a test daemon cannot produce", () => {
  let site: { origin: string; close: () => void };
  test.beforeAll(async () => {
    site = await serveStatic(WEB_DIST);
  });
  test.afterAll(async () => {
    site.close();
  });

  const inReview = ticket("t1", 21, "review", {
    title: "Screenshots in review",
    owner_employee_id: "e1",
    owner_name: "Flux",
    root_task_id: "k1",
    root_task_status: "review",
    responsible_user_id: "u",
    responsible_username: "Sam",
  });
  const inPlan = ticket("t2", 22, "plan_review", {
    title: "A plan to read",
    owner_employee_id: "e1",
    owner_name: "Flux",
    root_task_id: "k2",
    root_task_status: "review",
    responsible_user_id: "u",
    responsible_username: "Sam",
  });
  const doing = ticket("t3", 23, "doing", {
    title: "Still at work",
    owner_employee_id: "e1",
    owner_name: "Flux",
    root_task_id: "k3",
    root_task_status: "running",
    conversation_id: "c3",
    responsible_user_id: "u",
  });
  const quiet = ticket("t4", 24, "backlog", { title: "Nothing to do" });
  const k1 = task("k1", {
    status: "review",
    ticket_id: "t1",
    ticket_number: 21,
    result: "It works. **All** screens are done.",
    conversation_id: "c1",
  });
  const k2 = task("k2", { status: "review", ticket_id: "t2", ticket_number: 22, conversation_id: "c2" });
  const k3 = task("k3", {
    status: "running",
    ticket_id: "t3",
    ticket_number: 23,
    conversation_id: "c3",
    live_run: "running",
    pending_approvals: 1,
  });
  const k4 = task("k4", {
    title: "A sub task",
    parent_task_id: "k3",
    ticket_id: "t3",
    ticket_number: 23,
    delegated_by: "k3",
    status: "succeeded",
    result: "Done.",
  });
  const script = {
    board: boardOf([inReview, inPlan, doing, quiet]),
    tickets: [inReview, inPlan, doing, quiet],
    tasks: [k1, k2, k3, k4],
    employees: [{ id: "e1", name: "Flux", role: "Engineer", status: "active" }],
    projects: [{ id: "p", name: "Northwind API" }],
    details: {
      t1: { ticket: inReview, root_task: k1, tasks: [k1], as_of_seq: 1 },
      t2: { ticket: inPlan, root_task: k2, tasks: [k2], as_of_seq: 1 },
      t3: { ticket: doing, root_task: k3, tasks: [k3, k4], as_of_seq: 1 },
    },
    conversations: {
      c1: messages("c1", []),
      c2: messages("c2", [message("m0", "assistant", "1. First the migration.", { update: "plan" })]),
      c3: messages("c3", [
        message("m1", "assistant", "I restart the API now.", {
          run_id: "r3",
          activity: [
            { call_id: "a", offset: 0, tool: "Write", input: { file_path: "src/api.rs" }, ok: true },
            { call_id: "b", offset: 1, tool: "Bash", input: { command: "cargo test" }, ok: false },
          ],
          usage: {
            input_tokens: 44,
            output_tokens: 6400,
            cached_tokens: 1090000,
            cache_creation_tokens: 19000,
            cost_micro_usd: 430000,
          },
        }),
      ]),
    },
    approvals: [approval("a1", { task_id: "k3", conversation_id: "c3", run_id: "r3" })],
    notifications: [notification(1)],
    proposals: [
      {
        id: "pr1",
        kind: "hire",
        name: "Lena",
        role: "QA",
        reason: "Tests lag behind.",
        instructions: "Test everything.",
        proposed_by_name: "Ava",
        status: "pending",
        created_at_ms: Date.now() - 1000,
      },
    ],
  };

  test("the gates of a ticket, and a drop that needs an answer", async ({ page }) => {
    const { writes } = await scriptServer(page, script);
    await page.goto(site.origin);
    await nav(page, "Board");
    const card = (name: string) => page.getByRole("button", { name: new RegExp(`#\\d+ ${name}`) });

    await card("Screenshots in review").click();
    const review = page.getByRole("complementary", { name: /Screenshots in review/ });
    await expect(review.getByText("In review · your decision")).toBeVisible();
    await expect(review.getByText("All")).toBeVisible();
    // Send back needs feedback.
    await review.getByRole("button", { name: "Send back to Doing" }).click();
    await review.getByRole("button", { name: "Send back to Doing" }).click();
    await expect(review.getByText("Write what needs to change.")).toBeVisible();
    await review.getByLabel("What needs to change?").fill("The dark theme is off.");
    await review.getByRole("button", { name: "Send back to Doing" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "ticket.send_back")?.params)
      .toMatchObject({ ticket_id: "t1", feedback: "The dark theme is off." });
    await review.getByRole("button", { name: "Mark done" }).click();
    await expect.poll(() => writes.some((w) => w.method === "ticket.accept")).toBe(true);

    await card("A plan to read").click();
    const plan = page.getByRole("complementary", { name: /A plan to read/ });
    await expect(plan.getByText("First the migration.")).toBeVisible();
    await plan.getByRole("button", { name: "Pas aan" }).click();
    await expect(plan.getByText("Write what needs to change.")).toBeVisible();
    await plan.getByRole("button", { name: "Approve", exact: true }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "ticket.approve_plan")?.params)
      .toMatchObject({ ticket_id: "t2", decision: "approve" });
    await plan.getByRole("button", { name: "Afwijzen" }).click();
    await expect(page.getByRole("dialog", { name: "Reject the plan of #22?" })).toBeVisible();
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Close", exact: true }).click();

    // In review → Doing asks what needs to change.
    await drag(page, card("Screenshots in review"), page.getByRole("region", { name: "Doing" }));
    const dialog = page.getByRole("dialog");
    await expect(dialog.getByLabel("What needs to change?")).toBeVisible();
    await dialog.getByRole("button", { name: "Cancel" }).click();
    // In review → Done asks to confirm.
    await drag(page, card("Screenshots in review"), page.getByRole("region", { name: "Done" }));
    await expect(page.getByRole("dialog", { name: "Mark #21 as Done?" })).toBeVisible();
    await page.getByRole("dialog").getByRole("button", { name: "Cancel" }).click();
    // Needs you filters the board.
    await page.getByRole("button", { name: /Needs you/ }).click();
    await expect(card("Nothing to do")).toHaveCount(0);
  });

  test("a run: what its answer was given, its kept files, send now", async ({ page }) => {
    const PIXEL = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
    const answered = message("kept", "assistant", "Here is the chart.", {
      run_id: "r3",
      files: [{ id: "f1", media_type: "image/png", name: "chart.png", size: 68 }],
    });
    const { writes } = await scriptServer(page, {
      ...script,
      conversations: { ...script.conversations, c3: messages("c3", [answered]) },
      reads: {
        "run.get_context": () => ({
          type: "context",
          value: {
            run_id: "r3",
            budget_tokens: 1000,
            used_tokens: 120,
            runtime_managed: false,
            scopes: ["project"],
            items: [
              {
                id: "i1",
                hash: "h",
                included: true,
                label: "Project memory",
                reason: null,
                score: 0.9,
                section: "Memory",
                tokens: 100,
              },
              {
                id: "i2",
                hash: "h",
                included: false,
                label: "Old note",
                reason: "over budget",
                score: 0.2,
                section: "Memory",
                tokens: 20,
              },
            ],
          },
        }),
        "file.get": () => ({
          type: "file",
          value: { id: "f1", media_type: "image/png", name: "chart.png", data_base64: PIXEL },
        }),
      },
    });
    await page.goto(`${site.origin}/tasks/k3`);
    await page.getByRole("button", { name: "Conversation", exact: true }).click();
    await page.getByRole("menuitem", { name: "Show context of last answer" }).click();
    const context = page.getByRole("dialog");
    await expect(context.getByText("120 of 1000 tokens · project")).toBeVisible();
    await expect(context.getByText("Project memory")).toBeVisible();
    await expect(context.getByText(/Left out: Old note/)).toBeVisible();
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: /chart\.png/ }).click();
    await expect(page.getByRole("dialog", { name: "chart.png" }).getByRole("img")).toBeVisible();
    await page.keyboard.press("Escape");
    await page.getByLabel("Message Flux").fill("Stop and do this instead.");
    await page.keyboard.press("Alt+Enter");
    await expect.poll(() => writes.find((w) => w.method === "run.stop")?.params).toBeTruthy();
    await expect
      .poll(() => writes.find((w) => w.method === "chat.send")?.params)
      .toMatchObject({ text: "Stop and do this instead." });
  });

  test("a question in the Inbox: the number keys, then Enter", async ({ page }) => {
    const question = approval("q1", {
      tool: "claude.AskUserQuestion",
      task_id: "k3",
      conversation_id: "c3",
      arguments: {
        questions: [
          {
            question: "Which database?",
            header: "Storage",
            multiSelect: false,
            options: [
              { label: "Postgres", description: "" },
              { label: "SQLite", description: "" },
            ],
          },
        ],
      },
    });
    const { writes } = await scriptServer(page, { ...script, approvals: [question] });
    await page.goto(`${site.origin}/inbox`);
    await page
      .getByRole("button", { name: /Which database|Flux/ })
      .first()
      .click();
    await expect(page.getByRole("radio", { name: "SQLite" })).toBeVisible();
    await page.locator("body").click({ position: { x: 1, y: 1 } });
    await page.keyboard.press("2");
    await expect(page.getByRole("radio", { name: "SQLite" })).toBeChecked();
    await page.keyboard.press("Enter");
    await expect
      .poll(() => writes.find((w) => w.method === "approval.answer")?.params)
      .toMatchObject({ approval_id: "q1" });
  });

  test("Tasks and a run: approve inline, stop, and the properties", async ({ page }) => {
    const { writes } = await scriptServer(page, script);
    await page.goto(site.origin);
    await nav(page, "Tasks");
    const list = page.getByRole("region", { name: "Task list" });
    await expect(list.getByText("Task k3")).toBeVisible();
    await expect(list.getByText("A sub task")).toHaveCount(0);
    await page.getByRole("button", { name: /^All/ }).click();
    await expect(list.getByText("A sub task")).toBeVisible();
    await list.getByRole("button", { name: "Fold" }).first().click();
    await expect(list.getByText("A sub task")).toHaveCount(0);
    await list.getByRole("button", { name: /Task k3/ }).click();
    const preview = page.getByRole("complementary", { name: "Task preview" });
    await expect(preview.getByText("The goal of k3.")).toBeVisible();
    await preview.getByRole("button", { name: /Open conversation/ }).click();

    await expect(page.getByRole("heading", { name: "Run by Flux" })).toBeVisible();
    await expect(page.getByText("I restart the API now.")).toBeVisible();
    await expect(page.getByText("2 steps")).toBeVisible();
    await expect(page.getByText("1 failed")).toBeVisible();
    const properties = page.getByRole("complementary", { name: "Run properties" });
    await expect(properties.getByText("src/api.rs")).toBeVisible();
    await expect(properties.getByText(/6\.4k out/)).toBeVisible();
    await expect(properties.getByText(/\$0\.43/)).toBeVisible();

    await page.getByRole("button", { name: /Allow once/ }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "approval.decide")?.params)
      .toMatchObject({ approval_id: "a1", approve: true });
    await page.getByLabel("Message Flux").fill("Please also check the logs.");
    await page.keyboard.press("Enter");
    await expect
      .poll(() => writes.find((w) => w.method === "chat.send")?.params)
      .toMatchObject({ conversation_id: "c3", text: "Please also check the logs." });
    await page.getByRole("button", { name: "Stop run" }).click();
    await expect.poll(() => writes.find((w) => w.method === "run.stop")?.params).toMatchObject({ run_id: "r3" });
    await page.getByRole("link", { name: "#23", exact: true }).click();
    await expect(page.getByRole("complementary", { name: /Still at work/ })).toBeVisible();
  });

  test("the Inbox says what each row is, and acts on it", async ({ page }) => {
    const { writes } = await scriptServer(page, script);
    await page.goto(site.origin);
    await page.getByRole("button", { name: /Lena/ }).click();
    await expect(page.getByText("Test everything.")).toBeVisible();
    await page.getByRole("button", { name: "Approve" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "proposal.decide")?.params)
      .toMatchObject({ proposal_id: "pr1", approve: true });
    await page.getByRole("button", { name: /Update 1 on this ticket/ }).click();
    await page.getByRole("button", { name: "Dismiss" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "notification.dismiss")?.params)
      .toMatchObject({ id: "n1" });
  });

  test("a board with a thousand tickets opens and filters quickly", async ({ page }) => {
    const many = Array.from({ length: 1000 }, (_, i) =>
      ticket(`x${i}`, i + 1, i % 2 ? "backlog" : "todo", {
        owner_employee_id: "e1",
        owner_name: "Flux",
        title: `Busy ticket ${i}`,
      }),
    );
    await scriptServer(page, { ...script, board: boardOf(many), tickets: many, details: {}, conversations: {} });
    await page.goto(site.origin);
    await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();
    const started = Date.now();
    await nav(page, "Board");
    await expect(page.getByRole("button", { name: /Busy ticket 0$/ })).toBeVisible();
    const opened = Date.now() - started;
    expect(await page.locator("[data-card]").count()).toBeLessThan(120);
    // A card far down the column comes up when it is scrolled to.
    const todo = page.getByRole("region", { name: "Todo" }).locator(".overflow-y-auto");
    await todo.evaluate((el) => el.scrollTo(0, el.scrollHeight));
    await expect(page.getByRole("button", { name: /Busy ticket 998/ })).toBeVisible();
    const filtering = Date.now();
    await page.getByLabel("Find a ticket").fill("Busy ticket 999");
    await expect(page.getByRole("button", { name: /Busy ticket 998/ })).toHaveCount(0);
    const filtered = Date.now() - filtering;
    console.log(`board 1000 tickets: open ${opened} ms, filter ${filtered} ms`);
  });
});

test("shots", async ({ page }) => {
  test.skip(!process.env.AURELIUS_SHOTS, "Only when asked for.");
  const out = process.env.AURELIUS_SHOTS as string;
  const site = await serveStatic(WEB_DIST);
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.emulateMedia({ colorScheme: "dark" });
  const a = ticket("t1", 21, "review", {
    title: "Screenshots in review",
    owner_employee_id: "e1",
    owner_name: "Flux",
    root_task_id: "k1",
    root_task_status: "review",
    responsible_user_id: "u",
    responsible_username: "Sam",
    priority: "high",
    labels: ["ui"],
    conversation_id: "c0",
  });
  const b = ticket("t2", 22, "doing", {
    title: "Fix token refresh race",
    owner_employee_id: "e1",
    owner_name: "Flux",
    root_task_id: "k3",
    root_task_status: "running",
    responsible_user_id: "u",
  });
  const c = ticket("t3", 23, "backlog", { title: "Rate limit the export", deadline_ms: Date.now() + 86_400_000 });
  const d = ticket("t4", 24, "done", { title: "Retry webhook delivery", done_at_ms: Date.now() - 3_600_000 });
  const k1 = task("k1", {
    status: "review",
    ticket_id: "t1",
    ticket_number: 21,
    result: "All screens have a **screenshot** now.\n\n- Board\n- Task run",
    conversation_id: "c1",
    title: "Screenshots in review",
  });
  const k3 = task("k3", {
    status: "running",
    ticket_id: "t2",
    ticket_number: 22,
    conversation_id: "c3",
    live_run: "running",
    pending_approvals: 1,
    title: "Fix token refresh race",
    goal: "Two refreshes at once log the user out. Serialize them.",
  });
  const k4 = task("k4", {
    title: "Write the regression test",
    parent_task_id: "k3",
    ticket_id: "t2",
    ticket_number: 22,
    employee_name: "Ben",
    employee_id: "e2",
  });
  await scriptServer(page, {
    board: boardOf([a, b, c, d]),
    tickets: [a, b, c, d],
    tasks: [k1, k3, k4],
    myTickets: { tickets: [a, b], following: [] },
    features: ["my_tickets", "ticket_responsible", "ticket_follow", "ticket_discussion", "board_testing"],
    employees: [{ id: "e1", name: "Flux", role: "Engineer", status: "active" }],
    projects: [{ id: "p", name: "Northwind API" }],
    details: { t1: { ticket: a, root_task: k1, tasks: [k1], as_of_seq: 1 } },
    conversations: {
      c0: messages("c0", [message("d1", "user", "Is the dark theme included?", { author_user: { username: "Sam" } })]),
      c1: messages("c1", []),
      c3: messages("c3", [
        message("m0", "user", "Please look at the refresh first.", { author_user: { username: "Sam" } }),
        message(
          "m1",
          "assistant",
          "The refresh runs twice when two requests fail at once. I serialize it behind one lock and restart the API to check.",
          {
            run_id: "r3",
            activity: [
              { call_id: "a", offset: 0, tool: "Write", input: { file_path: "src/auth/refresh.rs" }, ok: true },
              { call_id: "b", offset: 1, tool: "Bash", input: { command: "cargo test -p auth" }, ok: false },
            ],
            usage: {
              input_tokens: 44,
              output_tokens: 6400,
              cached_tokens: 1090000,
              cache_creation_tokens: 19000,
              cost_micro_usd: 430000,
            },
          },
        ),
      ]),
    },
    approvals: [approval("a1", { task_id: "k3", conversation_id: "c3", run_id: "r3" })],
  });
  await page.goto(`${site.origin}/board?ticket=t1`);
  await expect(page.getByText("In review · your decision")).toBeVisible();
  await page.waitForTimeout(400);
  await page.screenshot({ path: join(out, "board.png") });
  await nav(page, "My tickets");
  await expect(page.getByRole("heading", { name: "My tickets" })).toBeVisible();
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(out, "my.png") });
  await nav(page, "Tasks");
  await expect(page.getByText("Write the regression test").first()).toBeVisible();
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(out, "tasks.png") });
  await page.goto(`${site.origin}/tasks/k3`);
  await expect(page.getByRole("heading", { name: "Run by Flux" })).toBeVisible();
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(out, "run.png") });
  site.close();
});
