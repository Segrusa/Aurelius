// The ticket page after the plain design (ticket #83, design/ticket-activity-plain.html): a ticket
// opens on Activity (the main task's plan, one card per delegation, a question inside the card of the
// employee who asks it); Timeline is the talk newest first with the composer on top and the work as
// plain blocks. A ticket with a main task, a delegation that asks and one that is done, against a
// scripted server; the screenshots go into `AURELIUS_SHOTS`, else target/ticket-83.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { approval, boardOf, channel, message, messages, scriptServer, task, ticket } from "./fake-server";

const SHOTS = process.env.AURELIUS_SHOTS ?? join(REPO, "target/ticket-83");
const FEATURES = ["ticket_channels", "ticket_discussion", "chat_threads", "ticket_responsible", "run_cards"];
const flux = { id: "e1", name: "Flux", role: "engineer", title: "Frontend Developer", status: "active", agents: [] };
const nexus = { id: "e2", name: "Nexus", role: "engineer", title: "API Engineer", status: "active", agents: [] };
const project = { id: "p", name: "Aurelius", status: "active", created_at_ms: 0, employees_active: 2, version: 1 };
const sam = { author_user: { id: "u", username: "Sam", created_at_ms: 0 } };

const started = Date.now() - 372_000;
// What Flux wrote between its calls; the calls after it start at its end (UTF-8 bytes, all ASCII here).
const FINDING = "Found the likely cause: the tile's rounded corners fall outside its hit area.";
const AFTER = FINDING.length;
const NEW_LINES = Array.from({ length: 10 }, (_, i) => `line ${i + 1}`).join("\n");
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

function script(description = SHORT, column = "doing") {
  const t = ticket("t73", 73, column, {
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
    status: column === "done" ? "succeeded" : "running",
    result: "Made the whole tile take the click. Tested on both platforms.",
  });
  const sub = task("s73", {
    title: "Check the hit area on Windows",
    employee_id: "e2",
    employee_name: "Nexus",
    parent_task_id: "k73",
    conversation_id: "cs73",
    ticket_id: "t73",
    status: column === "done" ? "succeeded" : "running",
    pending_approvals: column === "done" ? 0 : 1,
    result: "Every corner of every tile takes the click on Windows 11. Checked twice.",
  });
  const done = task("d1", {
    title: "Reproduce on macOS",
    employee_id: "e2",
    employee_name: "Nexus",
    parent_task_id: "k73",
    conversation_id: "cd1",
    ticket_id: "t73",
    status: "succeeded",
    result: "Reproduced it on macOS in two clicks. The second click is the one that is lost.",
  });
  const asks = approval("q1", {
    tool: "claude.AskUserQuestion",
    task_id: "s73",
    run_id: "r2",
    requester: "Nexus",
    arguments: {
      questions: [
        {
          question: "Which Windows build should I check?",
          header: "Build",
          multiSelect: false,
          options: [
            { label: "Windows 11 (Recommended)", description: "The build most of the team uses." },
            { label: "Windows 10", description: "Still on two machines." },
          ],
        },
      ],
    },
  });
  return {
    features: FEATURES,
    approvals: column === "done" ? [] : [asks],
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
        message("w1", "assistant", FINDING, {
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
            { call_id: "r1", tool: "Read", offset: 0, ok: true, input: { file_path: "src/sidebar.tsx" } },
            { call_id: "r2", tool: "Read", offset: 0, ok: true, input: { file_path: "src/rail.css" } },
            {
              call_id: "e1",
              tool: "Edit",
              offset: AFTER,
              ok: true,
              input: { file_path: "src/sidebar.tsx", old_string: "radius: 50%;\nclip: round;", new_string: NEW_LINES },
            },
            {
              call_id: "b0",
              tool: "Bash",
              offset: AFTER,
              ok: true,
              output: "Found 0 errors.",
              input: { command: "pnpm tsc --noEmit" },
            },
            { call_id: "b1", tool: "Bash", offset: AFTER, ok: null, input: { command: "pnpm test rail --run" } },
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

  const timeline = (page: Page) => page.getByRole("tab", { name: "Timeline" }).click();

  test("opens on Activity with the main task's plan; the properties keep Properties and Files", async ({ page }) => {
    await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    const activity = page.getByRole("tab", { name: /Activity/ });
    await expect(activity).toHaveAttribute("aria-selected", "true");
    await expect(activity).toContainText("1");
    const plan = page.getByRole("region", { name: "Plan" });
    await expect(plan).toContainText("Main task · Flux · step 3 of 4");
    await expect(plan).toContainText("Reproduce the missed clicks");
    await expect(plan).toContainText("Verify in the desktop app");
    // The step at work says in plain words what runs: a test run, not the command.
    await expect(plan).toContainText("Fixing the hit area of the tiles");
    await expect(plan).toContainText("Running the tests…");
    await expect(plan).not.toContainText("pnpm test rail --run");
    const aside = page.getByRole("complementary", { name: "Ticket properties" });
    await expect(aside.getByRole("region", { name: "Plan" })).toHaveCount(0);
    await expect(aside.getByRole("tab")).toHaveCount(2);
    await expect(aside.getByRole("tab", { name: "Activity" })).toHaveCount(0);
  });

  test("the employee who asks you comes first, with the question in its card", async ({ page }) => {
    const { writes } = await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    const cards = page.getByRole("article");
    await expect(cards.first()).toHaveAttribute("aria-label", "Nexus, needs your answer");
    const card = page.getByRole("article", { name: "Nexus, needs your answer" });
    await expect(card).toContainText("API Engineer");
    await expect(card).toContainText("Working on Check the hit area on Windows");
    await expect(card).toContainText("Step 1 of 2");
    await expect(card).toContainText("Which Windows build should I check?");
    const answer = card.getByRole("group", { name: "Answer Nexus" });
    // The employee's own pick is labelled, without "(Recommended)".
    await expect(answer.getByRole("button", { name: /Windows 11/ })).toContainText("Nexus's pick");
    await expect(answer).not.toContainText("(Recommended)");
    await answer.getByRole("button", { name: /Windows 11/ }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "approval.answer")?.params)
      .toEqual({ approval_id: "q1", answers: { "Which Windows build should I check?": "Windows 11" } });
    await expect(card.getByRole("status")).toContainText("You answered: Windows 11. Nexus goes on.");
  });

  test("the question can be answered in your own words", async ({ page }) => {
    const { writes } = await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    const card = page.getByRole("article", { name: "Nexus, needs your answer" });
    await card.getByLabel("Or answer in your own words").fill("Both, Windows 11 first");
    await card.getByRole("button", { name: "Send" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "approval.answer")?.params)
      .toEqual({ approval_id: "q1", answers: { "Which Windows build should I check?": "Both, Windows 11 first" } });
  });

  test("a finished delegation is one row, and opens the Timeline of its employee", async ({ page }) => {
    await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    await expect(page.getByText("Reproduced it on macOS in two clicks.")).toBeVisible();
    await page.getByRole("button", { name: "Timeline of Nexus" }).click();
    await expect(page.getByRole("tab", { name: "Timeline" })).toHaveAttribute("aria-selected", "true");
    await expect(page.getByRole("button", { name: "Nexus", exact: true })).toHaveAttribute("aria-pressed", "true");
    await expect(page.getByRole("article", { name: "Nexus · Open the app on Windows" })).toBeVisible();
    await expect(page.getByRole("article", { name: /^Flux · / })).toHaveCount(0);
    await expect(page.getByText("@Flux is it the radius animation?")).toHaveCount(0);
  });

  test("folds a long description to three lines until Show more", async ({ page }) => {
    await scriptServer(page, script(LONG));
    await page.goto(`${site.origin}/ticket/73`);
    const region = page.getByRole("region", { name: "Description" });
    await region.scrollIntoViewIfNeeded();
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
    await more.click();
    const less = region.getByRole("button", { name: "Show less" });
    await expect(less).toHaveAttribute("aria-expanded", "true");
    expect((await text.boundingBox())?.height ?? 0).toBeGreaterThan(folded * 1.5);
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

  test("the Timeline reads newest first, with the composer on top", async ({ page }) => {
    await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    await timeline(page);
    const top = async (text: string) => (await page.getByText(text).first().boundingBox())?.y ?? -1;
    const box = page.getByRole("textbox", { name: /^Message/ });
    await expect(box).toBeVisible();
    const composer = (await box.boundingBox())?.y ?? -1;
    const answer = await top("Likely: the corners fall outside the hit area");
    const asked = await top("@Flux is it the radius animation?");
    const moved = await top("Ticket moved from Todo to Doing.");
    expect(composer).toBeLessThan(answer);
    expect(answer).toBeLessThan(asked);
    expect(asked).toBeLessThan(moved);
    const order = await page.getByRole("article").evaluateAll((rows) => rows.map((r) => r.getAttribute("aria-label")));
    expect(order).toEqual(["Nexus · Open the app on Windows", "Flux · Fixing the hit area of the tiles"]);
  });

  test("a work block says what was done in plain words; its steps open on request", async ({ page }) => {
    await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    await timeline(page);
    const block = page.getByRole("article", { name: "Flux · Fixing the hit area of the tiles" });
    await expect(block).toContainText("Found the likely cause");
    const facts = block.getByRole("list", { name: "What it did" });
    await expect(facts.getByRole("listitem")).toHaveText(["Changed 1 file +10 −2", "Builds"]);
    // No file names or commands until the steps are opened.
    await expect(block).not.toContainText("pnpm test rail --run");
    await block.getByRole("button", { name: "› Show the 5 steps" }).click();
    const steps = block.getByRole("list", { name: "Steps" });
    await expect(steps.getByRole("listitem")).toHaveCount(5);
    await expect(steps).toContainText("pnpm test rail --run");
    await expect(steps).toContainText("running…");
    await expect(steps).toContainText("+10 −2");
  });

  test("the Timeline filter shows changes or messages", async ({ page }) => {
    await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    await timeline(page);
    const said = page.getByText("@Flux is it the radius animation?");
    const moved = page.getByText("Ticket moved from Todo to Doing.");
    const block = page.getByRole("article", { name: "Flux · Fixing the hit area of the tiles" });
    await expect(said).toBeVisible();
    await expect(moved).toBeVisible();
    const show = page.getByRole("group", { name: "Show" });
    await show.getByRole("button", { name: "Changes" }).click();
    await expect(said).toHaveCount(0);
    await expect(moved).toBeVisible();
    await expect(block).toBeVisible();
    await show.getByRole("button", { name: "Messages" }).click();
    await expect(moved).toHaveCount(0);
    await expect(block).toHaveCount(0);
    await expect(said).toBeVisible();
    await expect(show.getByRole("button", { name: "Messages" })).toHaveAttribute("aria-pressed", "true");
  });

  test("a message goes to the ticket's talk, where the owner answers", async ({ page }) => {
    const { writes } = await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    await timeline(page);
    const box = page.getByRole("textbox", { name: /^Message/ });
    await expect(box).toHaveAttribute("placeholder", "Message Flux (owner)… · @ to mention");
    await box.fill("@Nexus which Windows build?");
    await box.press("Enter");
    await expect
      .poll(() => writes.find((w) => w.method === "group_chat.send")?.params)
      .toMatchObject({ text: "@Nexus which Windows build?", channel: "d73" });
  });

  test("a Done ticket shows who did what", async ({ page }) => {
    await scriptServer(page, script(SHORT, "done"));
    await page.goto(`${site.origin}/ticket/73`);
    await expect(page.getByRole("heading", { name: "Who did what" })).toBeVisible();
    const flux = page.getByRole("article", { name: "Flux, done" });
    await expect(flux).toContainText("Made the whole tile take the click.");
    await expect(flux).not.toContainText("Tested on both platforms.");
    await expect(flux).toContainText("✓ Done");
    await expect(page.getByRole("article", { name: "Nexus, done" })).toHaveCount(2);
    await expect(page.getByRole("tab", { name: /Activity/ })).not.toContainText("1");
  });

  test("shots", async ({ page }) => {
    mkdirSync(SHOTS, { recursive: true });
    await scriptServer(page, script());
    await page.goto(`${site.origin}/ticket/73`);
    await expect(page.getByRole("article", { name: "Nexus, needs your answer" })).toBeVisible();
    await page.waitForLoadState("networkidle");
    await page.screenshot({ path: join(SHOTS, "activity.png"), animations: "disabled" });
    await timeline(page);
    await expect(page.getByRole("article", { name: "Flux · Fixing the hit area of the tiles" })).toBeVisible();
    await page.screenshot({ path: join(SHOTS, "timeline.png"), animations: "disabled" });
    await page.setViewportSize({ width: 720, height: 900 });
    await page.getByRole("tab", { name: /Activity/ }).click();
    await page.screenshot({ path: join(SHOTS, "activity-narrow.png"), animations: "disabled" });
    await scriptServer(page, script(SHORT, "done"));
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(`${site.origin}/ticket/73`);
    await expect(page.getByRole("heading", { name: "Who did what" })).toBeVisible();
    await page.screenshot({ path: join(SHOTS, "activity-done.png"), animations: "disabled" });
  });
});
