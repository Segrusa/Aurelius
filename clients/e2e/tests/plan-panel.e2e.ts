// A plan opens in the dock at the right of its conversation (ticket #80): a compact card in the
// feed, the text with comments made from a selection, and the decision in a footer that stays.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { approval, message, messages, scriptServer } from "./fake-server";

const SHOTS = join(REPO, "target/plan-panel");

const PLAN = [
  "# Add a resizable plan panel to the conversation",
  "",
  "A plan is long; it should be read beside the feed, not in it.",
  "",
  "## Steps",
  "",
  "1. Add the plan target to `clients/ts/app/src/screens/artifact-dock.tsx`.",
  "2. Build `PlanDock` in `clients/ts/app/src/screens/plan-dock.tsx` and keep the dock width.",
  "3. Make the feed card compact in `chat-message.tsx`.",
  "4. Move the decision into a hook shared by the card and the dock.",
  "5. Open a waiting plan by itself only in an empty dock.",
  "6. Test it in `plan-logic.test.ts`.",
  "",
  "## Risks",
  "",
  "- A long plan must keep the decision buttons in sight.",
  ...Array.from({ length: 30 }, (_, i) => `- Detail line ${i + 1} that makes the plan long enough to scroll.`),
].join("\n");

const session = {
  conversation_id: "s1",
  title: "Plan panel",
  employee_id: "e1",
  employee_name: "Titan",
  project_id: "p",
  project_name: "Aurelius",
  branch: "session/pp",
  worktree: "/data/worktrees/session-pp",
  running: false,
  created_at_ms: Date.now() - 600_000,
  last_activity_ms: Date.now() - 60_000,
};

async function open(page: Page, width: number) {
  const site = await serveStatic(WEB_DIST);
  const pending = approval("ap1", {
    tool: "claude.ExitPlanMode",
    arguments: { plan: PLAN },
    conversation_id: "s1",
    requester: "Titan",
  });
  const server = await scriptServer(page, {
    features: ["work_sessions"],
    employees: [{ id: "e1", name: "Titan", role: "Tech Lead", status: "active" }],
    projects: [{ id: "p", name: "Aurelius" }],
    approvals: [pending],
    conversations: {
      s1: messages("s1", [
        message("m1", "user", "Plan the plan panel."),
        message("m2", "assistant", PLAN, { update: "plan", approval_id: "ap1", author: "Titan", run_id: "r1" }),
      ]),
    },
    reads: {
      "work_session.list": () => ({ type: "work_sessions", value: { sessions: [session], as_of_seq: 1 } }),
    },
  });
  await page.setViewportSize({ width, height: 900 });
  await page.goto(site.origin);
  await page.goto(`${site.origin}/session/s1`);
  return { site, ...server };
}

test.describe("the plan panel", () => {
  test.beforeAll(() => mkdirSync(SHOTS, { recursive: true }));

  test("a waiting plan opens beside the feed, takes comments from a selection and is decided from its footer", async ({
    page,
  }) => {
    const { site, writes } = await open(page, 1440);
    try {
      // The feed keeps a compact card; the plan opened by itself in the empty dock.
      await expect(page.getByText("PLAN · WAITING FOR YOU")).toBeVisible();
      await expect(page.getByText("6 steps · 4 files").first()).toBeVisible();
      const panel = page.getByRole("complementary", { name: /^Plan: Add a resizable plan panel/ });
      await expect(panel).toBeVisible();
      await expect(page.getByRole("button", { name: "Showing" })).toBeDisabled();
      await expect(panel.getByText("Waiting for you")).toBeVisible();

      // The decision stays in sight: the footer's buttons are inside the viewport.
      const approve = panel.getByRole("button", { name: "Approve + Auto" });
      await expect(approve).toBeInViewport();
      await page.screenshot({ path: join(SHOTS, "1-open.png") });

      // A selection gets a Comment button; the comment marks its quote and lists it.
      const step = panel.getByText("keep the dock width");
      await step.scrollIntoViewIfNeeded();
      const box = await step.boundingBox();
      if (!box) throw new Error("No step text.");
      await page.mouse.move(box.x + box.width - 2, box.y + box.height / 2);
      await page.mouse.down();
      await page.mouse.move(box.x + box.width - 120, box.y + box.height / 2, { steps: 6 });
      await page.mouse.up();
      await panel.getByRole("button", { name: "Comment", exact: true }).click();
      await panel.getByRole("textbox", { name: "Your comment" }).fill("Also for plans, yes.");
      await panel.getByRole("textbox", { name: "Your comment" }).press("Enter");
      const comments = panel.getByRole("region", { name: "Your comments" });
      await expect(comments.getByText("Also for plans, yes.")).toBeVisible();
      await expect(panel.getByRole("button", { name: "Comment 1", exact: true })).toBeVisible();
      await page.screenshot({ path: join(SHOTS, "2-comment.png") });

      // Request changes sends the comment as feedback, exactly as the card did.
      await panel.getByRole("button", { name: "Request changes" }).click();
      await expect.poll(() => writes.some((w) => w.method === "approval.decide_plan")).toBe(true);
      const sent = writes.find((w) => w.method === "approval.decide_plan")?.params as Record<string, unknown>;
      expect(sent.approval_id).toBe("ap1");
      expect(sent.decision).toBe("revise");
      expect(String(sent.feedback)).toContain("Also for plans, yes.");

      // Esc closes the panel, Alt+Shift+P opens the latest plan again.
      await panel.getByRole("heading", { level: 2 }).first().click();
      await page.keyboard.press("Escape");
      await expect(panel).toHaveCount(0);
      await page.keyboard.press("Alt+Shift+P");
      await expect(panel).toBeVisible();
    } finally {
      site.close();
    }
  });

  test("a narrow window opens the plan in a dialog", async ({ page }) => {
    const { site } = await open(page, 900);
    try {
      const dialog = page.getByRole("dialog", { name: /Add a resizable plan panel/ });
      await expect(dialog).toBeVisible();
      await expect(dialog.getByRole("button", { name: "Approve + Auto" })).toBeVisible();
      await page.screenshot({ path: join(SHOTS, "3-narrow.png") });
    } finally {
      site.close();
    }
  });
});
