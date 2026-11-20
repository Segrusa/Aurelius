// The Inbox is a bell in the sidebar's rail, and nothing else: its count, its dropdown, a row that
// leads to where it is decided, and the old /inbox link that now starts on New session.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { notification, scriptServer, task } from "./fake-server";

const SHOTS = join(REPO, "target/inbox-bell");

test("the bell counts what needs you, and a row leads to where it is decided", async ({ page }) => {
  mkdirSync(SHOTS, { recursive: true });
  const site = await serveStatic(WEB_DIST);
  try {
    const waiting = task("w1", {
      title: "Pick the login copy",
      status: "waiting",
      waiting_reason: "input",
      employee_name: "Pixel",
    });
    const { writes } = await scriptServer(page, {
      tasks: [waiting],
      notifications: [notification(1), notification(2)],
    });
    await page.setViewportSize({ width: 1440, height: 900 });
    // The app starts on New session; an old link to the Inbox does too.
    await page.goto(`${site.origin}/inbox`);
    await expect(page).toHaveURL(/\/new$/);
    await expect(page.getByRole("heading", { level: 1, name: "New session" })).toBeVisible();

    // No Inbox row in the sidebar; the bell in the rail says what needs you.
    await expect(page.getByRole("navigation").getByRole("link", { name: /^Inbox/ })).toHaveCount(0);
    const bell = page.getByRole("button", { name: "Inbox, 1 need you" });
    await expect(bell).toBeVisible();

    // The dropdown lists every group and its rows; an unread update is marked read once opened.
    await bell.click();
    const dropdown = page.getByRole("dialog", { name: "Inbox" });
    await expect(dropdown.getByRole("button", { name: /Pick the login copy/ })).toBeVisible();
    await expect(dropdown.getByText("Update 1 on this ticket")).toBeVisible();
    await expect(dropdown.getByRole("button", { name: "Mark all read" })).toBeVisible();
    await page.waitForLoadState("networkidle");
    await page.screenshot({ path: join(SHOTS, "dropdown.png"), animations: "disabled" });
    await dropdown.getByRole("button", { name: /^Update 1 on this ticket/ }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "notification.mark_read")?.params)
      .toMatchObject({ id: "n1" });
    await expect(page).toHaveURL(/\/ticket\/1$/);

    // A waiting task without a ticket leads to its run.
    await page.goto(`${site.origin}/new`);
    await page
      .getByRole("button", { name: /^Inbox/ })
      .first()
      .click();
    await page
      .getByRole("dialog", { name: "Inbox" })
      .getByRole("button", { name: /Pick the login copy/ })
      .click();
    await expect(page).toHaveURL(/\/tasks\/w1$/);
    await expect(page.getByRole("dialog", { name: "Inbox" })).toHaveCount(0);

    // Ctrl+I opens the bell from the keyboard.
    await page.keyboard.press("Control+i");
    await expect(page.getByRole("dialog", { name: "Inbox" })).toBeVisible();
  } finally {
    await site.close();
  }
});
