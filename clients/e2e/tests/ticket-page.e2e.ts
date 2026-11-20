// The ticket page (ticket #74): the description and what every task does now above the talk, the
// talk as Activity with a filter, what runs this second above the composer, and the plan of the main
// task with its delegations on the right. A ticket with a main task and a delegation at work at once,
// against a scripted server; the screenshot goes into `AURELIUS_SHOTS`, else target/ticket-74.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { boardOf, channel, message, messages, scriptServer, task, ticket } from "./fake-server";

const SHOTS = process.env.AURELIUS_SHOTS ?? join(REPO, "target/ticket-74");
const FEATURES = ["ticket_channels", "ticket_discussion", "chat_threads", "ticket_responsible", "run_cards"];
const flux = { id: "e1", name: "Flux", role: "engineer", title: "Frontend Developer", status: "active", agents: [] };
const nexus = { id: "e2", name: "Nexus", role: "engineer", title: "API Engineer", status: "active", agents: [] };
const project = { id: "p", name: "Aurelius", status: "active", created_at_ms: 0, employees_active: 2, version: 1 };
const sam = { author_user: { id: "u", username: "Sam", created_at_ms: 0 } };

const started = Date.now() - 372_000;
const todos = (states: string[], words: string[]) =>
  words.map((content, i) => ({ content, activeForm: content.replace(/^Fix/, "Fixing"), status: states[i] }));

const SHORT = "Clicking a workspace in the sidebar sometimes does nothing. Is it the radius animation?";
const LONG = [
  "Clicking a workspace in the sidebar sometimes does nothing.",
  "It happens on Windows and on macOS.",
  "Mostly right after the app starts.",
  "Is it the radius animation?",
  "Check the corners of every tile.",
  "The last line of the description.",
].join("\n\n");

function script(description = SHORT) {
  const t = ticket("t73", 73, "doing", {
    owner_employee_id: "e1",
    owner_name: "Flux",
    root_task_id: "k73",
    conversation_id: "d73",
    responsible_user_id: "u",
    responsible_username: "Sam",
    project_name: "Aurelius",
    root_task_status: "running",
    title: "Clicking a workspace in the sidebar sometimes does nothing",
    description,
  });
  const root = task("k73", {
    title: t.title,
    employee_id: "e1",
    employee_name: "Flux",
    conversation_id: "c73",
    ticket_id: "t73",
    status: "running",
  });
  const sub = task("s73", {
    title: "Check the hit area on Windows",
    employee_id: "e2",
    employee_name: "Nexus",
    parent_task_id: "k73",
    conversation_id: "cs73",
    ticket_id: "t73",
    status: "running",
    pending_approvals: 1,
  });
  const done = task("d1", {
    title: "Reproduce on macOS",
    employee_id: "e2",
    employee_name: "Nexus",
    parent_task_id: "k73",
    conversation_id: "cd1",
    ticket_id: "t73",
    status: "succeeded",
  });
  return {
    features: FEATURES,
    employees: [flux, nexus],
    projects: [project],
    company: { company: {}, members: [] },
    board: boardOf([t]),
    tickets: [t],
    channels: [
      channel("d73", "#73-ticket", { kind: "ticket", ticket_id: "t73", ticket_number: 73, ticket_column: "doing" }),
    ],
    feeds: {
      d73: messages("d73", [
        message("a1", "assistant", "Ticket moved from Todo to Doing.", {
          created_at_ms: started - 60_000,
          system: { kind: "moved" },
        }),
        message("a2", "user", "@Flux is it the radius animation?", { ...sam, created_at_ms: started - 30_000 }),
        message("a3", "assistant", "Likely: the corners fall outside the hit area while the tile is round.", {
          author: "Flux",
          created_at_ms: started,
        }),
      ]),
    },
    conversations: {
      c73: messages("c73", [
        message("w1", "assistant", "Working on it.", {
          run_id: "r1",
          status: "partial",
          created_at_ms: started,
          activity: [
            {
              call_id: "t1",
              tool: "TodoWrite",
              offset: 0,
              ok: true,
              input: {
                todos: todos(
                  ["completed", "completed", "in_progress", "pending"],
                  [
                    "Reproduce the missed clicks",
                    "Find the cause in WorkspaceRail",
                    "Fix the hit area of the tiles",
                    "Verify in the desktop app",
                  ],
                ),
              },
            },
            { call_id: "r1", tool: "Read", offset: 1, ok: true, input: { file_path: "sidebar.tsx" } },
            { call_id: "r2", tool: "Read", offset: 2, ok: true, input: { file_path: "rail.css" } },
            {
              call_id: "e1",
              tool: "Edit",
              offset: 3,
              ok: true,
              input: { file_path: "sidebar.tsx", old_string: "a\nb", new_string: "a\nb\nc\nd" },
            },
            { call_id: "b1", tool: "Bash", offset: 4, ok: null, input: { command: "pnpm test rail --run" } },
          ],
        }),
      ]),
      cs73: messages("cs73", [
        message("w2", "assistant", "Checking.", {
          run_id: "r2",
          status: "partial",
          created_at_ms: started + 60_000,
          activity: [
            {
              call_id: "t2",
              tool: "TodoWrite",
              offset: 0,
              ok: true,
              input: { todos: todos(["in_progress", "pending"], ["Open the app on Windows", "Click every corner"]) },
            },
          ],
        }),
      ]),
      cd1: messages("cd1", []),
    },
    details: { t73: { as_of_seq: 1, ticket: t, root_task: root, tasks: [root, sub, done] } },
    reads: {
      "usage.get_totals": () => ({
        type: "usage_totals",
        value: {
          own: {
            input_tokens: 0,
            output_tokens: 0,
            cached_tokens: 0,
            cache_creation_tokens: 0,
            estimate: true,
            runs: 0,
          },
          subtasks: [],
          total: {
            input_tokens: 0,
            output_tokens: 0,
            cached_tokens: 0,
            cache_creation_tokens: 0,
            estimate: true,
            runs: 0,
          },
        },
      }),
      "user.list": () => ({ type: "users", value: { users: [{ id: "u", username: "Sam", created_at_ms: 0 }] } }),
      "activity.overview": () => ({ type: "activity_overview", value: { tickets: [], employees: [], as_of_seq: 1 } }),
      "ticket.list": () => ({ type: "tickets", value: { tickets: [t], as_of_seq: 1 } }),
    },
  };
}

test.describe("the ticket page", () => {
  let site: { origin: string; close: () => void };
  test.beforeAll(async () => {
    site = await serveStatic(WEB_DIST);
  });
  test.afterAll(() => site.close());

  test("shows the description and what each task of the ticket does now", async ({ page }) => {
    await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    await expect(page.getByRole("region", { name: "Description" })).toContainText("Is it the radius animation?");
    const now = page.getByRole("region", { name: "Current status" });
    // The main task first, then the delegation that is at work; the finished one stays out.
    const main = now.locator("[data-now-task=k73]");
    await expect(main).toContainText("Flux · Fixing the hit area of the tiles");
    await expect(main).toContainText("Main task");
    await expect(main).toContainText("Step 3 of 4");
    await expect(main).toContainText("2 files read");
    await expect(main).toContainText("1 file changed +4 −2");
    await expect(main).toContainText("1 command run");
    await expect(main.getByRole("progressbar", { name: "Plan progress" })).toHaveAttribute("aria-valuenow", "2");
    const sub = now.locator("[data-now-task=s73]");
    await expect(sub).toContainText("Nexus · Open the app on Windows");
    await expect(sub).toContainText("Delegated task");
    await expect(sub.getByRole("button", { name: "1 question for you" })).toBeVisible();
    await expect(now.locator("[data-now-task=d1]")).toHaveCount(0);
  });

  test("folds a long description to three lines until Show more", async ({ page }) => {
    await scriptServer(page, script(LONG));
    await page.goto(`${site.origin}/ticket/73`);
    const region = page.getByRole("region", { name: "Description" });
    const more = region.getByRole("button", { name: "Show more" });
    await expect(more).toHaveAttribute("aria-expanded", "false");
    const text = region.getByRole("button", { name: "Edit Description" });
    const folded = (await text.boundingBox())?.height ?? 0;
    const line = await region
      .getByText("Clicking a workspace")
      .evaluate((el) => parseFloat(getComputedStyle(el).lineHeight));
    // Three lines (with the paragraphs' spacing inside them), never all six paragraphs.
    expect(folded).toBeGreaterThan(line * 2.5);
    expect(folded).toBeLessThan(line * 5);
    mkdirSync(SHOTS, { recursive: true });
    await region.screenshot({ path: join(SHOTS, "description-folded.png") });
    await more.click();
    const less = region.getByRole("button", { name: "Show less" });
    await expect(less).toHaveAttribute("aria-expanded", "true");
    expect((await text.boundingBox())?.height ?? 0).toBeGreaterThan(folded * 1.5);
    await region.screenshot({ path: join(SHOTS, "description-unfolded.png") });
    await less.click();
    await expect(region.getByRole("button", { name: "Show more" })).toBeVisible();
  });

  test("a short description has no Show more", async ({ page }) => {
    await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    const region = page.getByRole("region", { name: "Description" });
    await expect(region).toContainText("Is it the radius animation?");
    await expect(region.getByRole("button", { name: /Show (more|less)/ })).toHaveCount(0);
  });

  test("the live bar names the call each task runs, and Live output opens the task", async ({ page }) => {
    await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    const live = page.getByRole("region", { name: "Running now" });
    await expect(live).toContainText("Flux");
    await expect(live).toContainText("Running a command");
    await expect(live).toContainText("pnpm test rail --run");
    await expect(live).toContainText("Nexus");
    await live.getByRole("button", { name: "Live output" }).first().click();
    await expect(page).toHaveURL(/\/tasks\/k73$/);
  });

  test("Stop stops the task's run", async ({ page }) => {
    const { writes } = await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    await page.locator("[data-now-task=k73]").getByRole("button", { name: "Stop" }).click();
    await expect.poll(() => writes.find((w) => w.method === "run.stop")?.params).toEqual({ run_id: "r1" });
  });

  test("the Activity filter shows changes or messages", async ({ page }) => {
    await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    const said = page.getByText("@Flux is it the radius animation?");
    const moved = page.getByText("Ticket moved from Todo to Doing.");
    await expect(said).toBeVisible();
    await expect(moved).toBeVisible();
    const filter = page.getByRole("group", { name: "Filter activity" });
    await filter.getByRole("button", { name: "Changes" }).click();
    await expect(said).toHaveCount(0);
    await expect(moved).toBeVisible();
    await filter.getByRole("button", { name: "Messages" }).click();
    await expect(moved).toHaveCount(0);
    await expect(said).toBeVisible();
    await expect(filter.getByRole("button", { name: "Messages" })).toHaveAttribute("aria-pressed", "true");
  });

  test("the plan shows the main task's steps and every delegation, each opening to its own plan", async ({ page }) => {
    await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    const plan = page.getByRole("complementary", { name: "Ticket properties" }).getByRole("region", { name: "Plan" });
    await expect(plan).toContainText("Main task · Flux");
    await expect(plan).toContainText("2 / 4");
    await expect(plan).toContainText("Verify in the desktop app");
    // The step at work is open on its latest calls.
    await expect(plan.getByRole("list", { name: "Latest steps" })).toContainText("pnpm test rail --run");
    const tasks = plan.getByRole("region", { name: "Delegated tasks" });
    await expect(tasks).toContainText("Tasks (2)");
    await tasks.getByRole("button", { name: /Check the hit area on Windows/ }).click();
    await expect(tasks).toContainText("Click every corner");
    await expect(tasks.getByRole("link", { name: "Open the task's conversation" })).toHaveAttribute(
      "href",
      "/tasks/s73",
    );
  });

  test("a message goes to the ticket's talk, where the owner answers", async ({ page }) => {
    const { writes } = await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    const box = page.getByRole("textbox", { name: /^Message/ });
    await expect(box).toHaveAttribute("placeholder", "Message Flux (owner)… · @ to mention");
    await box.fill("@Nexus which Windows build?");
    await box.press("Enter");
    await expect
      .poll(() => writes.find((w) => w.method === "group_chat.send")?.params)
      .toMatchObject({ text: "@Nexus which Windows build?", channel: "d73" });
  });

  test("shot", async ({ page }) => {
    mkdirSync(SHOTS, { recursive: true });
    await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    await expect(page.getByRole("region", { name: "Current status" })).toBeVisible();
    await expect(page.getByText("Likely: the corners fall outside the hit area")).toBeVisible();
    await page.screenshot({ path: join(SHOTS, "ticket-page.png") });
  });
});
