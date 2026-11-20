// The Code tab of a workspace (ticket #77) against a scripted server: the design's repository (worktrees,
// branches, the commit graph), a merge that answers the new view, and a refused delete that shows inline.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { scriptServer } from "./fake-server";

const SHOTS = join(REPO, "target/T77-D");
const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;
const HOME = "/home/sam/projects/aurelius";
const WT = "/home/sam/.aurelius/worktrees";

const flux = { employee_id: "e-flux", name: "Flux" };
const bridge = { employee_id: "e-bridge", name: "Bridge" };
const hash = (short: string) => short.padEnd(40, "0");

function view(now: number, merged = false) {
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
    fetched_at_ms: at(2 * MIN),
    worktrees: [
      { path: HOME, branch: "main", head: hash("e02b7f3"), is_main: true },
      { path: `${WT}/73`, branch: "fix/73-rail-hit-area", head: hash("a3f9c21"), changed_count: 1 },
      { path: `${WT}/71`, branch: "feat/71-scroll-sessions", head: hash("91c4e5a"), changed_count: 0 },
      { path: `${WT}/68`, branch: "feat/68-ticket-activity-feed", head: hash("c71f2aa"), changed_count: 0 },
    ],
    branches: [
      branch({
        name: "main",
        head: hash("e02b7f3"),
        is_main: true,
        subject: "Merge fix/64-history-tickets",
        at_ms: at(2 * HOUR),
        worktree_path: HOME,
      }),
      branch({
        name: "fix/73-rail-hit-area",
        head: hash("a3f9c21"),
        ahead: 2,
        subject: "Move tile click target to square wrapper",
        author: "Flux",
        at_ms: at(4 * MIN),
        worktree_path: `${WT}/73`,
        files_changed: 3,
        insertions: 41,
        deletions: 12,
        commits: [
          { hash: hash("a3f9c21"), subject: "Move tile click target to square wrapper" },
          { hash: hash("7be0d14"), subject: "Reproduce missed clicks in rail test" },
        ],
        ticket: { ticket_id: "t73", number: 73, title: "Het klikken op een van de workspaces" },
        owner: flux,
        working: true,
      }),
      branch({
        name: "feat/71-scroll-sessions",
        head: hash("91c4e5a"),
        ahead: 3,
        behind: 1,
        subject: "Virtualize session list scroll container",
        author: "Flux",
        at_ms: at(25 * MIN),
        worktree_path: `${WT}/71`,
        files_changed: 5,
        insertions: 128,
        deletions: 40,
        commits: [
          { hash: hash("91c4e5a"), subject: "Virtualize session list scroll container" },
          { hash: hash("4f20b8e"), subject: "Keep scroll position per list" },
          { hash: hash("c83a1d2"), subject: "Add scroll container to ticket list" },
        ],
        ticket: { ticket_id: "t71", number: 71, title: "Scrollen in sessie- en tickets" },
        owner: flux,
        working: true,
      }),
      branch({
        name: "fix/70-sidebar-count",
        head: hash("b5e1a90"),
        ahead: merged ? 0 : 1,
        merged,
        subject: "Count worktrees in the sidebar",
        author: "Bridge",
        at_ms: at(HOUR),
        files_changed: merged ? 0 : 2,
        insertions: 18,
        deletions: 3,
        commits: merged ? [] : [{ hash: hash("b5e1a90"), subject: "Count worktrees in the sidebar" }],
        ticket: { ticket_id: "t70", number: 70, title: "Teller in de zijbalk" },
        owner: bridge,
      }),
      branch({
        name: "feat/68-ticket-activity-feed",
        head: hash("c71f2aa"),
        ahead: 5,
        behind: 4,
        subject: "Activity feed: group events by task",
        author: "Bridge",
        at_ms: at(5 * HOUR),
        worktree_path: `${WT}/68`,
        files_changed: 9,
        insertions: 312,
        deletions: 87,
        commits: [
          { hash: hash("c71f2aa"), subject: "Activity feed: group events by task" },
          { hash: hash("2b9e7d1"), subject: "Add task step model" },
        ],
        ticket: { ticket_id: "t68", number: 68, title: "Activity per task tonen" },
        owner: bridge,
      }),
      branch({
        name: "fix/64-history-tickets",
        head: hash("5d8a0c9"),
        merged: true,
        subject: "Show tickets in history",
        author: "Bridge",
        at_ms: at(3 * HOUR),
        ticket: { ticket_id: "t64", number: 64, title: "History bevat enkel tickets" },
        owner: bridge,
      }),
      branch({
        name: "spike/mobile-nav",
        head: hash("e44c0b9"),
        ahead: 1,
        behind: 23,
        subject: "Try bottom tab bar",
        at_ms: at(20 * DAY),
        files_changed: 4,
        insertions: 96,
        deletions: 30,
        commits: [{ hash: hash("e44c0b9"), subject: "Try bottom tab bar" }],
      }),
    ],
    commits: [
      ["a3f9c21", ["7be0d14"], "Move tile click target to square wrapper", "Flux", 4 * MIN, ["fix/73-rail-hit-area"]],
      ["7be0d14", ["e02b7f3"], "Reproduce missed clicks in rail test", "Flux", 12 * MIN, []],
      [
        "91c4e5a",
        ["4f20b8e"],
        "Virtualize session list scroll container",
        "Flux",
        25 * MIN,
        ["feat/71-scroll-sessions"],
      ],
      ["4f20b8e", ["0a9d3e4"], "Keep scroll position per list", "Flux", 50 * MIN, []],
      ["b5e1a90", ["e02b7f3"], "Count worktrees in the sidebar", "Bridge", HOUR, ["fix/70-sidebar-count"]],
      ["e02b7f3", ["0a9d3e4", "5d8a0c9"], "Merge fix/64-history-tickets into main", "Sam", 2 * HOUR, ["main"]],
      ["5d8a0c9", ["8e1f2c0"], "Show tickets in history", "Bridge", 3 * HOUR, ["fix/64-history-tickets"]],
      ["8e1f2c0", ["0a9d3e4"], "Add tickets to history query", "Bridge", 4 * HOUR, []],
      ["0a9d3e4", ["6c0a7b3"], "Bump daemon to 0.9.3", "Sam", 6 * HOUR, []],
      ["c71f2aa", ["2b9e7d1"], "Activity feed: group events by task", "Bridge", DAY, ["feat/68-ticket-activity-feed"]],
      ["2b9e7d1", ["6c0a7b3"], "Add task step model", "Bridge", DAY + HOUR, []],
      ["6c0a7b3", ["5f00000"], "Release 0.9.2", "Sam", 2 * DAY, []],
    ].map(([h, parents, subject, author, ago, branches]) => ({
      hash: hash(h as string),
      parents: (parents as string[]).map(hash),
      subject,
      author,
      at_ms: at(ago as number),
      branches,
    })),
  };
}

test("the Code tab shows the repository, merges a ready branch and shows a refusal inline", async ({ page }) => {
  mkdirSync(SHOTS, { recursive: true });
  const site = await serveStatic(WEB_DIST);
  try {
    const now = Date.now();
    let merged = false;
    const { writes } = await scriptServer(page, {
      features: ["workspace_filters", "code_view"],
      projects: [{ id: "p", name: "Aurelius", status: "active", created_at_ms: 0, employees_active: 2, version: 1 }],
      reads: { "code.get": () => ({ type: "code", value: view(now, merged) }) },
      results: {
        "code.merge": () => {
          merged = true;
          return { kind: "host", value: { type: "code", ...view(now, true) } };
        },
      },
      refuse: (request) =>
        request.method === "code.delete_branch"
          ? { code: "validation", message: "git refused: spike/mobile-nav is checked out somewhere else." }
          : null,
    });
    await page.addInitScript(() => localStorage.setItem("aurelius.workspace:h", "p"));
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(`${site.origin}/code`);

    // The head, the sidebar row with its count, the worktrees and the graph.
    await expect(page.getByRole("heading", { level: 1, name: "Code" })).toBeVisible();
    await expect(page.getByRole("link", { name: /^Code/ })).toContainText("3");
    const worktrees = page.getByRole("region", { name: "Worktrees" });
    await expect(worktrees.getByText("3 active")).toBeVisible();
    await expect(page.getByText("Fetched 2m ago")).toBeVisible();
    const details = page.getByRole("complementary", { name: "Branch details" });
    await expect(details.getByRole("heading", { name: "fix/73-rail-hit-area" })).toBeVisible();
    await page.waitForLoadState("networkidle");
    await page.screenshot({ path: join(SHOTS, "code.png"), animations: "disabled" });
    await page.getByRole("region", { name: "Commits" }).screenshot({ path: join(SHOTS, "code-graph.png") });

    // An employee works on fix/73: its worktree cannot be removed now.
    await expect(details.getByRole("button", { name: "Remove worktree" })).toBeDisabled();

    // Active hides Merged and Stale; All shows them.
    const branches = page.getByRole("region", { name: "Branches" });
    await expect(branches.getByRole("button", { name: /spike\/mobile-nav/ })).toHaveCount(0);
    await branches.getByRole("radio", { name: "All" }).click();
    await expect(branches.getByRole("button", { name: /spike\/mobile-nav.*Stale/ })).toBeVisible();

    // Merge a ready branch: confirm, then the answer is the new view.
    await branches.getByRole("button", { name: /fix\/70-sidebar-count/ }).click();
    await details.getByRole("button", { name: "Merge into main" }).click();
    const confirm = page.getByRole("dialog", { name: "Merge fix/70-sidebar-count into main?" });
    await confirm.getByRole("button", { name: "Merge" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "code.merge")?.params)
      .toMatchObject({
        project_id: "p",
        branch: "fix/70-sidebar-count",
      });
    await expect(branches.getByRole("button", { name: /fix\/70-sidebar-count.*Merged/ })).toBeVisible();

    // Deleting a branch main lacks commits of asks first, then forces; a refusal shows on the page.
    await branches.getByRole("button", { name: /spike\/mobile-nav/ }).click();
    await details.getByRole("button", { name: "Delete branch" }).click();
    const lose = page.getByRole("dialog", { name: "Delete spike/mobile-nav?" });
    await expect(lose).toContainText("1 commit that main does not have");
    await lose.getByRole("button", { name: "Delete branch" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "code.delete_branch")?.params)
      .toMatchObject({
        branch: "spike/mobile-nav",
        force: true,
      });
    await expect(page.getByRole("alert").filter({ hasText: "checked out somewhere else" })).toBeVisible();
    await page.screenshot({ path: join(SHOTS, "code-refused.png"), animations: "disabled" });

    // All workspaces has no single repository (this init script runs after the one that chose "p").
    await page.addInitScript(() => localStorage.setItem("aurelius.workspace:h", ""));
    await page.reload();
    await expect(page.getByRole("heading", { name: "Pick a workspace" })).toBeVisible();

    // The design is dark: the same page in the dark theme, for comparing.
    await page.addInitScript(() => {
      localStorage.setItem("aurelius.workspace:h", "p");
      localStorage.setItem("aurelius.appearance", "dark");
    });
    await page.reload();
    await expect(page.getByRole("region", { name: "Worktrees" })).toBeVisible();
    await page.waitForLoadState("networkidle");
    await page.screenshot({ path: join(SHOTS, "code-dark.png"), animations: "disabled" });
  } finally {
    await site.close();
  }
});
