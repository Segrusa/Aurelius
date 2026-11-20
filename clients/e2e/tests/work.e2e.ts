import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Connection, expectBody } from "@aurelius/client";
import { _electron, expect, type Locator, type Page, test } from "@playwright/test";
import { daemonEndpoint, openPipe } from "../../electron/src/main/pipe";
import { REPO, serveStatic, TestDaemon, until, WEB_DIST } from "./daemon";
import { approval, boardOf, channel, message, messages, notification, scriptServer, task, ticket } from "./fake-server";

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

test("the board and a ticket's session against a real daemon", async () => {
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
    await expect(page.getByRole("heading", { level: 1, name: "New session" })).toBeVisible();

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
    await expect(page.getByRole("region", { name: "Notifications" }).getByText("Start first.")).toBeVisible();
    expect((await read()).backlog).toContain(top);

    // Backlog → Todo starts it, and without a dispatcher it asks for an employee.
    await drag(page, card("Second ticket"), page.getByRole("region", { name: "Todo" }));
    await until("started", async () => ((await read()).todo as string[]).includes("Second ticket"));
    await expect(backlog.getByText("Second ticket")).toHaveCount(0);
    // A card opens the ticket's session (ticket #70): the decision above the composer.
    await card("Second ticket").click();
    const gate = page.getByRole("region", { name: "Your decision" });
    await expect(gate.getByText("No dispatcher is set. Choose an employee.")).toBeVisible();

    // Edit the title in place, in the session's head.
    const head = page.locator("header[data-titlebar]").filter({ hasText: "Second ticket" });
    await head.getByRole("button", { name: "Edit Title" }).click();
    await head.getByLabel("Title").fill("Second ticket, renamed");
    await page.keyboard.press("Enter");
    await until("renamed", async () => ((await read()).todo as string[]).includes("Second ticket, renamed"));

    // The talk is the session's timeline; the activity tells what happened.
    const panel = page.getByRole("complementary", { name: "Ticket properties" });
    await panel.getByRole("tab", { name: "Activity" }).click();
    await expect(panel.getByText("Nothing happened yet.")).toHaveCount(0);

    // The reads behind the run card and the odometer, on the real server (fake-server answers anything).
    const onBoard = expectBody(await server.call("board.get", { project_id: project?.id ?? null }), "board").value;
    const second = onBoard.columns.flatMap((c) => c.tickets).find((t) => t.title === "Second ticket, renamed");
    const totals = expectBody(
      await server.call("usage.get_totals", { ticket_id: second?.id ?? null }),
      "usage_totals",
    ).value;
    expect(totals.total.input_tokens).toBeGreaterThanOrEqual(0);
    const none = "00000000-0000-7000-8000-000000000001";
    expect(expectBody(await server.call("run.get_activity", { run_id: none }), "run_activity").value.calls).toEqual([]);

    // Back to the Board for the keyboard moves.
    await nav(page, "Board");
    await expect(card("First ticket")).toBeVisible();

    // By keyboard: pick up with Space, move with the arrows, drop with Space.
    await card("First ticket").focus();
    await page.keyboard.press("Space");
    await page.waitForTimeout(100);
    await page.keyboard.press("ArrowRight");
    await page.waitForTimeout(100);
    await page.keyboard.press("Space");
    await until("moved by keyboard", async () => ((await read()).todo as string[]).includes("First ticket"));

    await page.keyboard.press("Control+7");
    await expect(page.getByRole("heading", { name: "Tasks", exact: true })).toBeVisible();
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

    // A card opens the ticket's session (ticket #70); its gate sits above the composer.
    await card("Screenshots in review").click();
    await expect(page).toHaveURL(/\/ticket\/21$/);
    const review = page.getByRole("region", { name: "Your decision" });
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

    await nav(page, "Board");
    await card("A plan to read").click();
    await expect(page).toHaveURL(/\/ticket\/22$/);
    const plan = page.getByRole("region", { name: "Your decision" });
    await expect(plan.getByText("First the migration.")).toBeVisible();
    // In the session the note opens on the first press; the second asks for it.
    await plan.getByRole("button", { name: "Request changes" }).click();
    await plan.getByRole("button", { name: "Request changes" }).click();
    await expect(plan.getByText("Write what needs to change.")).toBeVisible();
    await plan.getByRole("button", { name: "Approve", exact: true }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "ticket.approve_plan")?.params)
      .toMatchObject({ ticket_id: "t2", decision: "approve" });
    await plan.getByRole("button", { name: "Reject" }).click();
    await expect(page.getByRole("dialog", { name: "Reject the plan of #22?" })).toBeVisible();
    await page.keyboard.press("Escape");
    await nav(page, "Board");

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
      .poll(() => writes.find((w) => w.method === "employee.send_message")?.params)
      .toMatchObject({ text: "Stop and do this instead." });
  });

  test("a discussion: at work, interrupt card and odometer", async ({ page }) => {
    const used = (input: number) => ({
      runs: 1,
      input_tokens: input,
      output_tokens: 200,
      cached_tokens: 0,
      cache_creation_tokens: 0,
      cost_micro_usd: 120000,
      estimate: false,
      model_calls: 4,
    });
    const discussed = { ...doing, conversation_id: "d3" };
    const question = approval("q9", {
      tool: "claude.AskUserQuestion",
      conversation_id: "main",
      ticket_id: "t3",
      run_id: "r9",
      requester: "Rook",
      arguments: { questions: [{ question: "Which colour?", options: [{ label: "Green", description: "" }] }] },
    });
    const { writes } = await scriptServer(page, {
      ...script,
      features: ["ticket_discussion", "ticket_channels", "run_cards"],
      channels: [channel("d3", "#23-export", { kind: "ticket", ticket_id: "t3", ticket_number: 23 })],
      feeds: {
        d3: messages("d3", [
          message("m1", "assistant", "Started on it.", {
            update: "group",
            author: "Flux",
            run_id: "r1",
            run: { status: "running", tool_calls: 2, failed_calls: 1 },
          }),
          message("m2", "assistant", "Shall I migrate it?", {
            update: "group",
            author: "Bo",
            run_id: "r2",
            awaiting_user: true,
            run: { status: "succeeded", tool_calls: 0, failed_calls: 0 },
          }),
        ]),
      },
      board: boardOf([discussed]),
      tickets: [discussed],
      details: { t3: { ticket: discussed, root_task: k3, tasks: [k3], as_of_seq: 1 } },
      approvals: [question],
      conversations: {
        d3: messages("d3", [
          message("m1", "assistant", "Started on it.", {
            update: "group",
            author: "Flux",
            run_id: "r1",
            run: { status: "running", tool_calls: 2, failed_calls: 1 },
            usage: used(9000),
          }),
          message("m2", "assistant", "Shall I migrate it?", {
            update: "group",
            author: "Bo",
            run_id: "r2",
            awaiting_user: true,
            run: { status: "succeeded", tool_calls: 0, failed_calls: 0 },
          }),
        ]),
      },
      reads: {
        "usage.get_totals": () => ({
          type: "usage_totals",
          value: { own: used(9000), subtasks: [], total: used(9000) },
        }),
        "run.get_activity": () => ({
          type: "run_activity",
          value: {
            run_id: "r1",
            message_id: "x1",
            calls: [
              {
                call_id: "a",
                offset: 0,
                tool: "Bash",
                input: { command: "cargo test" },
                ok: false,
                output: "1 failed",
              },
            ],
          },
        }),
      },
    });
    await page.goto(site.origin);
    await nav(page, "Board");
    // The card opens the ticket's session: what its runs used is in its properties, not in its head.
    await page.getByRole("button", { name: /#\d+ Still at work/ }).click();
    const head = page.locator("header[data-titlebar]").filter({ hasText: "Still at work" });
    await expect(head).toBeVisible();
    await expect(head.getByText("Tokens in")).toHaveCount(0);
    const properties = page.getByRole("complementary", { name: "Ticket properties" });
    await properties.getByRole("button", { name: "Usage" }).hover();
    await expect(page.getByRole("tooltip")).toContainText("$0.12 API-equivalent");

    await page.goto(`${site.origin}/ticket/23`);
    const chat = page.getByRole("main");
    // A discussion is a chat: that someone is at work, and how to stop it, but not what they ran.
    const note = chat.locator("[data-run-note]").first();
    await expect(note.getByText("Working", { exact: true })).toBeVisible();
    await expect(chat.getByText(/tool calls/)).toHaveCount(0);
    await note.getByRole("button", { name: "Stop" }).click();
    await expect.poll(() => writes.find((w) => w.method === "run.stop")?.params).toMatchObject({ run_id: "r1" });

    await expect(chat.getByRole("radio", { name: "Green" })).toBeVisible();
    await chat.getByLabel("Your answer to Bo").fill("Yes, please");
    await chat.getByRole("button", { name: "Send answer" }).first().click();
    await expect
      .poll(() => writes.find((w) => w.method === "group_chat.send")?.params)
      .toMatchObject({ channel: "d3", text: "Yes, please" });

    await chat.getByRole("radio", { name: "Green" }).check();
    await chat.getByRole("button", { name: "Send answer" }).last().click();
    await expect
      .poll(() => writes.find((w) => w.method === "approval.answer")?.params)
      .toMatchObject({ approval_id: "q9", answers: { "Which colour?": "Green" } });
  });

  test("a question in the bell leads to its run, where it is answered", async ({ page }) => {
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
    await page.goto(`${site.origin}/new`);
    await page
      .getByRole("button", { name: /^Inbox/ })
      .first()
      .click();
    await page
      .getByRole("dialog", { name: "Inbox" })
      .getByRole("button", { name: /Which database|Flux/ })
      .first()
      .click();
    await expect(page).toHaveURL(/\/tasks\/k3$/);
    await page.getByRole("radio", { name: "SQLite" }).check();
    await page.getByRole("button", { name: "Send answer" }).first().click();
    await expect
      .poll(() => writes.find((w) => w.method === "approval.answer")?.params)
      .toMatchObject({ approval_id: "q1" });
  });

  test("Tasks and a run: approve inline, stop, and the properties", async ({ page }) => {
    const { writes } = await scriptServer(page, script);
    await page.goto(site.origin);
    await page.goto(`${site.origin}/tasks`);
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
    await properties.getByRole("button", { name: /Files written/ }).click();
    await expect(properties.getByText("src/api.rs")).toBeVisible();
    await properties.getByRole("button", { name: "Usage" }).hover();
    await expect(page.getByRole("tooltip")).toContainText("6.4k out");
    await expect(page.getByRole("tooltip")).toContainText("$0.43");

    await page.getByRole("button", { name: /Allow once/ }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "approval.decide")?.params)
      .toMatchObject({ approval_id: "a1", approve: true });
    await page.getByLabel("Message Flux").fill("Please also check the logs.");
    await page.keyboard.press("Enter");
    await expect
      .poll(() => writes.find((w) => w.method === "employee.send_message")?.params)
      .toMatchObject({ conversation_id: "c3", text: "Please also check the logs." });
    await page.getByRole("button", { name: "Stop run" }).click();
    await expect.poll(() => writes.find((w) => w.method === "run.stop")?.params).toMatchObject({ run_id: "r3" });
    await page.getByRole("link", { name: "#23", exact: true }).click();
    await expect(page).toHaveURL(/\/ticket\/23$/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Still at work");
  });

  test("a session shows the same properties as a task's run", async ({ page }) => {
    const session = {
      conversation_id: "s1",
      title: "Fix the login page",
      employee_id: "e1",
      employee_name: "Flux",
      project_id: "p",
      project_name: "Northwind API",
      branch: "session/ab12",
      worktree: "/data/worktrees/session-ab12",
      running: false,
      created_at_ms: Date.now() - 600_000,
      last_activity_ms: Date.now() - 60_000,
    };
    await scriptServer(page, {
      ...script,
      features: ["work_sessions"],
      conversations: {
        s1: {
          ...messages("s1", [
            message("m1", "user", "Fix the login page."),
            message("m2", "assistant", "The login page works again.", {
              run_id: "r9",
              activity: [{ call_id: "a", offset: 0, tool: "Edit", input: { file_path: "src/login.tsx" }, ok: true }],
              usage: {
                input_tokens: 12,
                output_tokens: 2100,
                cached_tokens: 50000,
                cache_creation_tokens: 3000,
                cost_micro_usd: 120000,
              },
            }),
          ]),
          agent_mode: "plan",
          run: { model: "claude-opus-5-5", effort: "high" },
        },
      },
      reads: {
        "work_session.list": () => ({ type: "work_sessions", value: { sessions: [session], as_of_seq: 1 } }),
        "run.guidelines": (request) => ({
          type: "run_guidelines",
          value: {
            run_id: (request.params as { run_id: string }).run_id,
            guidelines:
              (request.params as { run_id: string }).run_id === "r9"
                ? [{ guideline_id: "g1", title: "Rust style", version: 3 }]
                : [],
          },
        }),
      },
    });
    await page.goto(site.origin);
    await page.goto(`${site.origin}/session/s1`);
    const properties = page.getByRole("complementary", { name: "Session properties" });
    await expect(properties.getByText("Flux", { exact: true })).toBeVisible();
    await expect(properties.getByText("Waiting for you")).toBeVisible();
    await expect(properties.getByText("session/ab12")).toBeVisible();
    await expect(properties.getByText("Opus")).toBeVisible();
    await expect(properties.getByText("high", { exact: true })).toBeVisible();
    // The files and guidelines start folded; their headings say how many.
    const files = properties.getByRole("button", { name: /Files written/ });
    await expect(files).toHaveAttribute("aria-expanded", "false");
    await expect(properties.getByText("src/login.tsx")).toHaveCount(0);
    // What the runs used is behind the info icon of the Properties heading.
    await expect(properties.getByText(/2\.1k out/)).toBeHidden();
    await properties.getByRole("button", { name: "Usage" }).hover();
    await expect(page.getByRole("tooltip")).toContainText("2.1k out");
    await expect(page.getByText("The login page works again.")).toBeVisible();
    await page.screenshot({ path: `${REPO}/target/session-details/session-panel.png` });
    await files.click();
    await expect(properties.getByText("src/login.tsx")).toBeVisible();
    await properties.getByRole("button", { name: /Guidelines in this run/ }).click();
    await expect(properties.getByText("Rust style")).toBeVisible();
    await expect(properties.getByText("v3")).toBeVisible();
    // The head's button hides the panel beside the talk, and shows it again.
    await page.getByRole("button", { name: "Hide session properties" }).click();
    await expect(properties).toHaveCount(0);
    await page.getByRole("button", { name: "Show session properties" }).click();
    await expect(properties).toBeVisible();
  });

  test("a subagent opens as a chat log that cannot be answered, and ends with its report", async ({ page }) => {
    mkdirSync(`${REPO}/target/subagent-log`, { recursive: true });
    const session = {
      conversation_id: "s1",
      title: "Remove the Team chat",
      employee_id: "e1",
      employee_name: "Flux",
      project_id: "p",
      project_name: "Northwind API",
      branch: "session/ab12",
      worktree: "/data/worktrees/session-ab12",
      running: true,
      created_at_ms: Date.now() - 600_000,
      last_activity_ms: Date.now() - 60_000,
    };
    const agent = {
      kind: "subagent",
      name: "Server: remove employee and Team chat",
      detail: "general-purpose",
      prompt: "You implement the SERVER part of removing chat.\n\nWork only in the worktree.",
      since_ms: Date.now() - 3_600_000,
      steps: 23,
      doing: "Bash · cargo test -p storage",
      run_id: "r9",
      call_id: "t1",
      conversation_id: "s1",
    };
    let finished = false;
    const steps = Array.from({ length: 23 }, (_, i) => ({
      call_id: `c${i}`,
      parent_call_id: "t1",
      offset: 0,
      tool: i % 2 === 0 ? "Bash" : "Read",
      input: i % 2 === 0 ? { command: `cargo check step ${i}` } : { file_path: `crates/file_${i}.rs` },
      ok: finished || i < 22 ? true : null,
    }));
    await scriptServer(page, {
      ...script,
      features: ["work_sessions", "subagent_steps"],
      conversations: {
        s1: messages("s1", [message("m1", "user", "Remove the Team chat.")]),
      },
      reads: {
        "work_session.list": () => ({ type: "work_sessions", value: { sessions: [session], as_of_seq: 1 } }),
        "subagent.list": () => ({ type: "subagents", value: finished ? [] : [agent] }),
        "run.get_activity": () => ({
          type: "run_activity",
          value: {
            run_id: "r9",
            message_id: "m2",
            calls: [
              {
                call_id: "t1",
                offset: 0,
                tool: "Task",
                input: {},
                ok: finished ? true : null,
                output: finished ? "The Team chat is gone from the server; 12 tests pass." : null,
              },
              ...steps.map((s, i) => ({ ...s, ok: finished || i < 22 ? true : null })),
            ],
          },
        }),
      },
    });
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(site.origin);
    await page.goto(`${site.origin}/session/s1`);
    await page.getByRole("button", { name: /Server: remove employee and Team chat/ }).click();
    const dialog = page.getByRole("dialog", { name: "Server: remove employee and Team chat" });
    const log = dialog.getByRole("log");

    // The employee's question first, as a message; then the subagent's steps, folded in lines of ten.
    await expect(log.getByText("Flux", { exact: true })).toBeVisible();
    await expect(log.getByText("You implement the SERVER part of removing chat.")).toBeVisible();
    await expect(log.getByText("23 steps")).toBeVisible();
    const done = log.getByRole("button", { name: "Ran 5 commands, read 5 files" });
    await expect(done).toHaveCount(2);
    await expect(done.first()).toHaveAttribute("aria-expanded", "false");
    // The batch it is in stays open, so its newest step shows.
    const now = log.getByRole("button", { name: /^Running 2 commands, read file_21\.rs/ });
    await expect(now).toHaveAttribute("aria-expanded", "true");
    await expect(log.getByRole("status")).toHaveText("Working · Bash · cargo test -p storage");

    // Nothing to write in: it only reports to the employee.
    await expect(dialog.getByRole("textbox")).toHaveCount(0);
    await expect(dialog.getByText("Read only: a subagent reports to Flux; you cannot write to it.")).toBeVisible();
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${REPO}/target/subagent-log/running.png` });

    // Once it is done, the log ends with its report and nothing works any more.
    finished = true;
    await expect(log.getByText("The Team chat is gone from the server; 12 tests pass.")).toBeVisible({
      timeout: 10_000,
    });
    await expect(log.getByRole("status")).toHaveCount(0);
    await page.screenshot({ path: `${REPO}/target/subagent-log/done.png` });
  });

  test("the sidebar puts a working session under Running, and its workspace counts it at the right", async ({
    page,
  }) => {
    const working = {
      conversation_id: "s3",
      title: "Fix the login page",
      employee_id: "e1",
      employee_name: "Flux",
      project_id: "p",
      project_name: "Northwind API",
      running: true,
      created_at_ms: Date.now() - 60_000,
      last_activity_ms: Date.now() - 1_000,
    };
    await scriptServer(page, {
      ...script,
      features: ["work_sessions"],
      reads: {
        "work_session.list": () => ({ type: "work_sessions", value: { sessions: [working], as_of_seq: 1 } }),
      },
    });
    await page.goto(site.origin);
    const side = page.getByRole("navigation", { name: "Main" });
    // All workspaces: the session's workspace says how many run, its total at the right; Running is under it.
    await expect(side.getByRole("button", { name: /^Running/ })).toHaveText(/Running\s*1/);
    const header = side.getByRole("button").filter({ has: page.getByTitle("1 running") });
    await expect(header).toHaveAttribute("aria-expanded", "true");
    await expect(side.getByRole("link", { name: /Fix the login page/ })).toBeVisible();
    await expect(side.getByRole("img", { name: "Working" })).toBeVisible();
    // Right-aligned: the count ends at the header's padding, not just after the title.
    const gap = await header.evaluate((button) => {
      const count = button.lastElementChild as Element;
      return button.getBoundingClientRect().right - count.getBoundingClientRect().right;
    });
    expect(gap).toBeLessThan(12);
    await header.screenshot({ path: `${REPO}/target/sidebar-summary/sessions-header.png` });
  });

  test("a new session goes to the dispatcher, which chooses who takes it", async ({ page }) => {
    const choosing = {
      conversation_id: "s2",
      title: "The login page is blank",
      employee_id: "e0",
      employee_name: "Orion",
      project_id: "p",
      project_name: "Northwind API",
      running: true,
      dispatch: "choosing",
      created_at_ms: Date.now() - 1_000,
      last_activity_ms: Date.now() - 1_000,
    };
    let sessions: unknown[] = [];
    const { writes } = await scriptServer(page, {
      ...script,
      features: ["work_sessions", "session_dispatch"],
      employees: [
        { id: "e0", name: "Orion", role: "Project Manager", status: "active" },
        { id: "e1", name: "Flux", role: "Engineer", status: "active" },
      ],
      conversations: {
        s2: {
          ...messages("s2", [
            message("m1", "user", "The login page is blank."),
            message("m2", "assistant", "Orion is choosing who takes this…", { update: "system" }),
          ]),
          employee_id: "e0",
        },
      },
      reads: {
        "work_session.list": () => ({ type: "work_sessions", value: { sessions, as_of_seq: 1 } }),
        // A connection is set up, so Start can go.
        "connection.list": () => ({
          type: "connections",
          value: {
            connections: [{ id: "c1", display_name: "Claude", provider: "claude_code", status: "ready", models: [] }],
            defaults: [],
          },
        }),
      },
      results: {
        "work_session.dispatch": () => {
          sessions = [choosing];
          return {
            kind: "command",
            value: { type: "work_session_dispatching", conversation_id: "s2", project_id: "p", message_id: "m1" },
          };
        },
      },
    });
    await page.goto(`${site.origin}/new`);
    await page.getByRole("radio", { name: "Session" }).click();
    await expect(page.getByRole("button", { name: "Who" })).toHaveText(/Dispatcher/);
    await page.getByRole("textbox", { name: "What needs doing" }).fill("The login page is blank.");
    await page.getByRole("button", { name: /^Start/ }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "work_session.dispatch")?.params)
      .toMatchObject({
        project_id: "p",
        text: "The login page is blank.",
      });
    expect(writes.some((w) => w.method === "work_session.start")).toBe(false);

    // The session opens: the dispatcher is at it, and the board member can choose instead.
    await expect(page).toHaveURL(/\/session\/s2$/);
    await expect(page.getByRole("region", { name: /^Orion is choosing who takes this/ })).toBeVisible();
    const properties = page.getByRole("complementary", { name: "Session properties" });
    await expect(properties.getByText("Dispatcher is choosing")).toBeVisible();
    await expect(page.getByText(/You can also choose yourself/)).toBeVisible();
    await page.screenshot({ path: `${REPO}/target/session-dispatch/session-choosing.png` });
    await page.getByRole("button", { name: "Who takes this session" }).click();
    await page.getByRole("menuitem", { name: /Flux/ }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "work_session.assign")?.params)
      .toEqual({ conversation_id: "s2", employee_id: "e1" });
  });

  test("the dispatcher's lines show as cards: who chooses, and whom it chose with the reason", async ({ page }) => {
    const taken = {
      conversation_id: "s3",
      title: "The login page is blank",
      employee_id: "e1",
      employee_name: "Flux",
      project_id: "p",
      project_name: "Northwind API",
      running: false,
      created_at_ms: Date.now() - 60_000,
      last_activity_ms: Date.now() - 60_000,
    };
    await scriptServer(page, {
      ...script,
      features: ["work_sessions", "session_dispatch"],
      employees: [
        { id: "e0", name: "Orion", role: "Project Manager", status: "active" },
        { id: "e1", name: "Flux", role: "Engineer", status: "active" },
      ],
      conversations: {
        s3: {
          ...messages("s3", [
            message("m1", "user", "The login page is blank."),
            message("m2", "assistant", "Orion is choosing who takes this…", { update: "system" }),
            message("m3", "assistant", "Orion chose Flux: The login page is front-end work.", { update: "system" }),
          ]),
          employee_id: "e1",
        },
      },
      reads: { "work_session.list": () => ({ type: "work_sessions", value: { sessions: [taken], as_of_seq: 1 } }) },
    });
    await page.goto(`${site.origin}/session/s3`);
    const chose = page.getByRole("region", { name: /^Orion chose Flux: The login page is front-end work\./ });
    await expect(chose).toBeVisible();
    await expect(chose.getByText("The login page is front-end work.")).toBeVisible();
    // Once it chose, the line that it is choosing goes: the card says it all.
    await expect(page.getByRole("region", { name: /^Orion is choosing/ })).toHaveCount(0);
    await page.screenshot({ path: `${REPO}/target/dispatch-cards/session.png` });
  });

  test("the Inbox bell says what each row is, and leads to where it is acted on", async ({ page }) => {
    const { writes } = await scriptServer(page, script);
    await page.goto(site.origin);
    const bell = () => page.getByRole("button", { name: /^Inbox/ }).first();
    const inbox = page.getByRole("dialog", { name: "Inbox" });
    // A proposed hire is approved on the org chart.
    await bell().click();
    await inbox.getByRole("button", { name: /Lena/ }).click();
    await expect(page).toHaveURL(/\/employees\/org$/);
    await expect(page.getByText("Test everything.")).toBeVisible();
    await page.getByRole("button", { name: "Approve" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "proposal.decide")?.params)
      .toMatchObject({ proposal_id: "pr1", approve: true });
    // An update is dismissed in the bell itself.
    await bell().click();
    await inbox.getByRole("button", { name: "Dismiss Update 1 on this ticket" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "notification.dismiss")?.params)
      .toMatchObject({ id: "n1" });
  });

  test("the History lists work sessions next to tickets, and another's session opens read-only", async ({ page }) => {
    mkdirSync(`${REPO}/target/session-history`, { recursive: true });
    const now = Date.now();
    const done = {
      ticket_id: "t9",
      number: 9,
      title: "Export to CSV",
      project_id: "p",
      project_name: "Northwind API",
      owner_name: "Flux",
      done_at_ms: now - 120_000,
      summary: "The export writes CSV.",
      summary_state: "ready",
    };
    const base = {
      project_id: "p",
      project_name: "Northwind API",
      employee_id: "e1",
      employee_name: "Flux",
      created_at_ms: now - 3_600_000,
    };
    const mine = {
      ...base,
      conversation_id: "s1",
      title: "Fix the login page",
      started_by: "u",
      started_by_name: "sam",
      branch: "session/ab12",
      last_activity_ms: now - 60_000,
      summary: "Flux fixed the login page and added a test.",
      summary_state: "ready",
    };
    const theirs = {
      ...base,
      conversation_id: "s2",
      title: "Release notes",
      started_by: "u2",
      started_by_name: "ada",
      last_activity_ms: now - 180_000,
      summary_state: "before_history",
    };
    const { writes } = await scriptServer(page, {
      ...script,
      features: ["ticket_history", "session_history", "work_sessions"],
      conversations: {
        ...script.conversations,
        s2: {
          ...messages("s2", [message("m1", "user", "Write the release notes.")]),
          employee_id: "e1",
          project_id: "p",
        },
      },
      reads: {
        "ticket.history": () => ({ type: "history", value: { entries: [done], has_more: false, as_of_seq: 1 } }),
        "work_session.history": () => ({
          type: "session_history",
          value: { entries: [mine, theirs], has_more: false, as_of_seq: 1 },
        }),
        "work_session.list": () => ({
          type: "work_sessions",
          value: { sessions: [{ ...mine, running: false }], as_of_seq: 1 },
        }),
      },
    });
    await page.goto(`${site.origin}/history`);
    const today = page.getByRole("region", { name: "Today" });
    // One list, newest first: my session, the ticket, then Ada's session.
    await expect(today.getByRole("button", { name: /Fix the login page/ })).toBeVisible();
    const titles = await today
      .getByRole("button", { name: /Fix the login page|Export to CSV|Release notes/ })
      .allTextContents();
    expect(titles.map((t) => t.replace(/^(Session|#9)/, "").trim())).toEqual([
      "Fix the login page",
      "Export to CSV",
      "Release notes",
    ]);
    await expect(today.getByText("Flux fixed the login page and added a test.")).toBeVisible();
    await expect(today.getByText(/started by ada/)).toBeVisible();
    await expect(today.getByText("No summary yet")).toBeVisible();
    await page.screenshot({ path: `${REPO}/target/session-history/history.png` });

    // Ada's session has no summary yet: Summarize asks for it.
    await today.getByRole("button", { name: "Summarize" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "work_session.retry_summary")?.params)
      .toMatchObject({ conversation_id: "s2" });

    // Only sessions; the filter is in the URL.
    await page.getByRole("radio", { name: "Sessions" }).click();
    await expect(page).toHaveURL(/type=sessions/);
    await expect(page.getByRole("button", { name: /Export to CSV/ })).toHaveCount(0);

    // Ada's session opens to read, not to write.
    await page.getByRole("button", { name: /Release notes/ }).click();
    await expect(page).toHaveURL(/\/session\/s2$/);
    await expect(page.getByText("Write the release notes.")).toBeVisible();
    await expect(page.getByText(/Another board member started this session/)).toBeVisible();
    await expect(page.getByLabel("Message Flux")).toHaveCount(0);
    await page.screenshot({ path: `${REPO}/target/session-history/others-session.png` });
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
    await expect(page.getByRole("heading", { level: 1, name: "New session" })).toBeVisible();
    const started = Date.now();
    await nav(page, "Board");
    await expect(page.getByRole("button", { name: /Busy ticket 0$/ })).toBeVisible();
    const opened = Date.now() - started;
    expect(await page.locator("[data-card]").count()).toBeLessThan(120);
    // A card far down the column comes up when it is scrolled to.
    const todo = page.getByRole("region", { name: "Todo" }).locator("[data-column-scroll]");
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
  await page.goto(`${site.origin}/tasks`);
  await expect(page.getByText("Write the regression test").first()).toBeVisible();
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(out, "tasks.png") });
  await page.goto(`${site.origin}/tasks/k3`);
  await expect(page.getByRole("heading", { name: "Run by Flux" })).toBeVisible();
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(out, "run.png") });
  site.close();
});
