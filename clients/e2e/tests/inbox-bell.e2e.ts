// The Inbox behind a bell in every screen's head: its count, its dropdown, and a row that opens on
// the Inbox page. The sidebar has no Inbox row any more.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { notification, scriptServer, task } from "./fake-server";

const SHOTS = join(REPO, "target/inbox-bell");

test("the bell in a screen's head counts what needs you and opens a row on the Inbox page", async ({ page }) => {
  mkdirSync(SHOTS, { recursive: true });
  const site = await serveStatic(WEB_DIST);
  try {
    const waiting = task("w1", {
      title: "Pick the login copy",
      status: "waiting",
      waiting_reason: "input",
      employee_name: "Pixel",
    });
    await scriptServer(page, {
      tasks: [waiting],
      notifications: [notification(1), notification(2)],
    });
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(`${site.origin}/board`);
    await expect(page.getByRole("heading", { level: 1, name: "Board" })).toBeVisible();

    // No Inbox row in the sidebar; the bell in the head says what needs you.
    await expect(page.getByRole("navigation").getByRole("link", { name: /^Inbox/ })).toHaveCount(0);
    const bell = page.getByRole("button", { name: "Inbox, 1 need you" });
    await expect(bell).toBeVisible();

    // The dropdown lists the groups and their rows.
    await bell.click();
    const dropdown = page.getByRole("dialog", { name: "Inbox" });
    await expect(dropdown.getByRole("button", { name: /Pick the login copy/ })).toBeVisible();
    await expect(dropdown.getByText("Update 1 on this ticket")).toBeVisible();
    await page.waitForLoadState("networkidle");
    await page.screenshot({ path: join(SHOTS, "dropdown.png"), animations: "disabled" });

    // A row opens on the Inbox page, selected there.
    await dropdown.getByRole("button", { name: /Pick the login copy/ }).click();
    await expect(page).toHaveURL(/\/inbox\?row=task(%3A|:)w1$/);
    await expect(page.getByRole("heading", { level: 1, name: "Inbox" })).toBeVisible();
    await expect(page.getByRole("dialog", { name: "Inbox" })).toHaveCount(0);

    // Open Inbox goes to the whole page, from any head.
    await page.goto(`${site.origin}/board`);
    await page.getByRole("button", { name: /^Inbox/ }).click();
    await page.getByRole("button", { name: "Open Inbox" }).click();
    await expect(page).toHaveURL(/\/inbox$/);
  } finally {
    await site.close();
  }
});
