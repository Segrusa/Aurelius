// The review-first Code tab (ticket #84, design/code-review.html) against a scripted server: the queue of a
// ticket in Review and two idle sessions, what is worked on, Tidy up, a comment, Finish review with Request
// changes, Ask Trace to review, Ask the owner to commit, and Merge into main.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { message, messages, scriptServer, task, ticket } from "./fake-server";

const SHOTS = join(REPO, "target/ticket-84");
const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;
const HOME = "/home/sam/projects/aurelius";
const WT = "/home/sam/.aurelius/worktrees";
const hash = (short: string) => short.padEnd(40, "0");

const flux = { employee_id: "e-flux", name: "Flux" };
const titan = { employee_id: "e-titan", name: "Titan" };

function view(now: number) {
  const at = (ago: number) => now - ago;
  const branch = (over: Record<string, unknown>) => ({
    ahead: 0,
    behind: 0,
    subject: "",
    author: "Sam",
    commits: [],
    ...over,
  });
  return {
    project_id: "p",
    repo_path: HOME,
    main_branch: "main",
    has_remote: true,
    fetched_at_ms: at(12 * MIN),
    worktrees: [
      {
        path: HOME,
        branch: "session/35f38c72",
        head: hash("f7aee92"),
        is_main: true,
        changed_count: 5,
        changed_files: ["crates/application/src/run/approvals.rs", "crates/storage/src/work_sessions.rs"],
      },
      { path: `${WT}/83`, branch: "feat/ticket-83-activity-plain", head: hash("c7d2a91"), changed_count: 0 },
      { path: `${WT}/a1`, branch: "session/a1c93e07", head: hash("df307b2"), changed_count: 0 },
      {
        path: `${WT}/84`,
        branch: "feat/ticket-84-code-review",
        head: hash("aa11bb2"),
        changed_count: 4,
        working: true,
      },
      { path: `${WT}/81`, branch: "feat/ticket-81-workspace-settings", head: hash("8181818"), changed_count: 0 },
    ],
    branches: [
      branch({
        name: "main",
        head: hash("93851c1"),
        is_main: true,
        upstream: "origin/main",
        upstream_ahead: 4,
        at_ms: at(HOUR),
      }),
      branch({
        name: "feat/ticket-83-activity-plain",
        head: hash("c7d2a91"),
        ahead: 3,
        behind: 2,
        at_ms: at(HOUR),
        files_changed: 9,
        insertions: 612,
        deletions: 148,
        worktree_path: `${WT}/83`,
        commits: [
          { hash: hash("c7d2a91"), subject: "desktop: Timeline tab, newest first, with filters" },
          { hash: hash("b41e07a"), subject: "desktop: Activity tab with the plan and one card per employee" },
          { hash: hash("93851c1"), subject: "design: ticket-activity-plain.html mock for the ticket page" },
        ],
        ticket: { ticket_id: "t83", number: 83, title: "Ticket page: Activity and Timeline" },
        owner: flux,
        open: true,
      }),
      branch({
        name: "session/a1c93e07",
        head: hash("df307b2"),
        ahead: 2,
        at_ms: at(3 * HOUR),
        files_changed: 3,
        insertions: 74,
        deletions: 39,
        worktree_path: `${WT}/a1`,
        commits: [
          { hash: hash("df307b2"), subject: "desktop: All workspaces is the list page beside the rail" },
          { hash: hash("f8f8176"), subject: "desktop: New session is a green button on top of the sidebar panel" },
        ],
        owner: flux,
        open: true,
      }),
      branch({
        name: "session/35f38c72",
        head: hash("f7aee92"),
        ahead: 1,
        at_ms: at(40 * MIN),
        files_changed: 5,
        insertions: 61,
        deletions: 1,
        worktree_path: HOME,
        commits: [
          { hash: hash("f7aee92"), subject: "design sessions always start in Auto; no Mode choice for a design" },
        ],
        owner: titan,
        open: true,
      }),
      branch({
        name: "feat/ticket-84-code-review",
        head: hash("aa11bb2"),
        ahead: 1,
        at_ms: at(5 * MIN),
        worktree_path: `${WT}/84`,
        ticket: { ticket_id: "t84", number: 84, title: "Code tab: review first" },
        owner: titan,
        working: true,
        open: true,
      }),
      branch({
        name: "feat/ticket-81-workspace-settings",
        head: hash("8181818"),
        merged: true,
        at_ms: at(2 * DAY),
        worktree_path: `${WT}/81`,
        ticket: { ticket_id: "t81", number: 81, title: "Workspace settings" },
      }),
      branch({ name: "feat/plan-panel", head: hash("9191919"), merged: true, at_ms: at(3 * DAY) }),
    ],
    commits: [],
  };
}

const session = (id: string, branchName: string, title: string, over: Record<string, unknown> = {}) => ({
  conversation_id: id,
  project_id: "p",
  project_name: "Aurelius",
  employee_id: "e-flux",
  employee_name: "Flux",
  title,
  branch: branchName,
  created_at_ms: Date.now() - DAY,
  last_activity_ms: Date.now() - 3 * HOUR,
  running: false,
  ...over,
});

test("the Code tab opens on what waits for review and reviews a branch", async ({ page }) => {
  mkdirSync(SHOTS, { recursive: true });
  const site = await serveStatic(WEB_DIST);
  try {
    const now = Date.now();
    const report =
      "The ticket page gets an **Activity** tab as its default.\n\n- The properties panel keeps only Properties and Files.";
    const { writes } = await scriptServer(page, {
      features: ["workspace_filters", "code_view", "work_sessions"],
      projects: [{ id: "p", name: "Aurelius", status: "active", created_at_ms: 0, employees_active: 3, version: 1 }],
      employees: [
        {
          id: "e-trace",
          name: "Trace",
          role: "Code Review Engineer",
          conversation_id: "c-trace",
          agents: [],
          conversations: [],
          projects_active: [],
          agent_mode: "ask",
          browser_allowed: false,
          screen_allowed: false,
          sequential_tasks: false,
        },
      ],
      tickets: [
        ticket("t83", 83, "review", {
          title: "Ticket page: Activity and Timeline",
          owner_name: "Flux",
          owner_employee_id: "e-flux",
          project_name: "Aurelius",
          conversation_id: "c-t83",
          updated_at_ms: now - 2 * HOUR,
        }),
        ticket("t84", 84, "doing", {
          title: "Code tab: review first",
          owner_name: "Titan",
          owner_employee_id: "e-titan",
          project_name: "Aurelius",
        }),
      ],
      details: {
        t83: {
          as_of_seq: 1,
          ticket: ticket("t83", 83, "review", {
            title: "Ticket page: Activity and Timeline",
            conversation_id: "c-t83",
          }),
          root_task: task("r83", { status: "done", result: report, updated_at_ms: now - 2 * HOUR }),
          tasks: [],
        },
      },
      conversations: {
        "c-t83": messages("c-t83", [
          message("m1", "assistant", "Ready for your review. Two of the three minors are fixed.", {
            author: "Flux",
            created_at_ms: now - HOUR,
          }),
        ]),
        "c-a1": messages("c-a1", [
          message("a1", "assistant", "The sidebar has two sections again.", { author: "Flux" }),
        ]),
        "c-35": messages("c-35", [
          message("a2", "assistant", "A design session always runs in **Auto**.", { author: "Titan" }),
        ]),
      },
      reads: {
        "code.get": () => ({ type: "code", value: view(now) }),
        "work_session.list": () => ({
          type: "work_sessions",
          value: {
            as_of_seq: 1,
            sessions: [
              session("c-a1", "session/a1c93e07", "Sidebar: Sessions and Tickets fold per section"),
              session("c-35", "session/35f38c72", "Design sessions always start in Auto", {
                employee_id: "e-titan",
                employee_name: "Titan",
                last_activity_ms: now - 25 * MIN,
                own_branch: true,
                worktree: HOME,
              }),
            ],
          },
        }),
      },
      results: { "code.merge": () => ({ kind: "host", value: { type: "code", ...view(now) } }) },
    });
    await page.addInitScript(() => localStorage.setItem("aurelius.workspace:h", "p"));
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(`${site.origin}/code`);

    // The queue: oldest waiting first, the running ticket read-only, merged work under Tidy up.
    const queue = page.getByRole("complementary", { name: "Branches to review" });
    await expect(queue.getByRole("button", { name: /#83 Ticket page/ })).toBeVisible();
    const ready = queue.getByRole("button", { name: /waits|idle/ });
    await expect(ready).toHaveCount(3);
    await expect(ready.nth(0)).toContainText("Sidebar: Sessions and Tickets");
    await expect(ready.nth(1)).toContainText("#83");
    await expect(ready.nth(2)).toContainText("Design sessions always start in Auto");
    await expect(queue.getByText("Code tab: review first")).toBeVisible();
    await expect(queue.getByText("2 branches are merged into main, 1 worktree is left behind.")).toBeVisible();
    await expect(page.getByText("main · 4 ahead of origin · fetched 12 min ago")).toBeVisible();
    await expect(page.getByRole("link", { name: /^Code/ })).toContainText("3");

    // The ticket: its checks, the report and the comments after it.
    await queue.getByRole("button", { name: /#83 Ticket page/ }).click();
    const review = page.getByRole("region", { name: "Review" });
    await expect(review.getByRole("heading", { name: /#83 Ticket page: Activity and Timeline/ })).toBeVisible();
    await expect(review.getByText(/2 behind main/).first()).toBeVisible();
    await expect(review.getByText("Activity", { exact: true })).toBeVisible();
    await expect(review.getByText("Ready for your review. Two of the three minors are fixed.")).toBeVisible();
    await page.waitForLoadState("networkidle");
    await page.screenshot({ path: join(SHOTS, "code-review.png"), animations: "disabled" });

    // A comment goes to the ticket's discussion at once.
    await review
      .getByRole("textbox", { name: "Comment" })
      .fill("Hide the text from screen readers and keep the label.");
    await review.getByRole("button", { name: "Comment", exact: true }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "ticket.post")?.params)
      .toMatchObject({ ticket_id: "t83", text: "Hide the text from screen readers and keep the label." });

    // Commits tab.
    await review.getByRole("tab", { name: /Commits/ }).click();
    await expect(review.getByText("desktop: Timeline tab, newest first, with filters")).toBeVisible();

    // Finish review with Request changes sends the ticket back.
    await page.getByRole("button", { name: "Finish review" }).click();
    const finish = page.getByRole("dialog", { name: "Finish your review" });
    await finish.getByRole("textbox", { name: "Summary" }).fill("Deduplicate on the question id.");
    await finish.getByRole("radio", { name: /Request changes/ }).check();
    await page.screenshot({ path: join(SHOTS, "code-review-finish.png"), animations: "disabled" });
    await finish.getByRole("button", { name: "Submit review" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "ticket.send_back")?.params)
      .toMatchObject({
        ticket_id: "t83",
        feedback: "Changes requested on feat/ticket-83-activity-plain:\n\nDeduplicate on the question id.",
      });

    // Merge into main, after a confirm.
    await page.getByRole("button", { name: "Merge into main" }).click();
    await page
      .getByRole("dialog", { name: /Merge feat\/ticket-83-activity-plain into main\?/ })
      .getByRole("button", { name: "Merge into main" })
      .click();
    await expect
      .poll(() => writes.find((w) => w.method === "code.merge")?.params)
      .toMatchObject({ project_id: "p", branch: "feat/ticket-83-activity-plain" });

    // A session with uncommitted work: no Merge, Ask Titan to commit, and Ask Trace to review.
    await queue.getByRole("button", { name: /Design sessions always start in Auto/ }).click();
    await expect(review.getByText(/Not committed/).first()).toBeVisible();
    await expect(page.getByRole("button", { name: "Merge into main" })).toHaveCount(0);
    await page.screenshot({ path: join(SHOTS, "code-review-session.png"), animations: "disabled" });
    await review.getByRole("button", { name: "Ask Trace to review" }).first().click();
    await expect
      .poll(() => writes.find((w) => w.method === "employee.send_message")?.params)
      .toMatchObject({ employee_id: "e-trace", conversation_id: "c-trace" });
    await page.getByRole("button", { name: "Ask Titan to commit" }).click();
    await page.getByRole("dialog", { name: "Ask Titan to commit?" }).getByRole("button", { name: "Send" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "chat.send")?.params)
      .toMatchObject({ conversation_id: "c-35" });

    // The Sessions filter leaves only sessions; the old lists are one click away.
    await queue.getByRole("radio", { name: "Sessions" }).click();
    await expect(queue.getByRole("button", { name: /#83/ })).toHaveCount(0);
    await queue.getByRole("button", { name: "All branches and graph →" }).click();
    await expect(page.getByRole("region", { name: "Worktrees" })).toBeVisible();
    await page.getByRole("button", { name: "‹ Review" }).click();
    await expect(queue).toBeVisible();

    // Narrow: the queue first, the review after a pick, with a way back.
    await page.setViewportSize({ width: 420, height: 860 });
    await page.screenshot({ path: join(SHOTS, "code-review-narrow-queue.png"), animations: "disabled" });
    await queue.getByRole("button", { name: /Sidebar: Sessions/ }).click();
    await expect(page.getByRole("button", { name: "‹ To review" })).toBeVisible();
    await page.screenshot({ path: join(SHOTS, "code-review-narrow.png"), animations: "disabled" });

    // The dark theme, as the design.
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.addInitScript(() => localStorage.setItem("aurelius.appearance", "dark"));
    await page.reload();
    await queue.getByRole("button", { name: /#83 Ticket page/ }).click();
    await page.waitForLoadState("networkidle");
    await page.screenshot({ path: join(SHOTS, "code-review-dark.png"), animations: "disabled" });
  } finally {
    await site.close();
  }
});
