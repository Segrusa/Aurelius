// A task that cannot start: this runs against the ordinary daemon, which has no connection until
// someone adds one. The test build always has one.

import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { _electron, expect, test } from "@playwright/test";
import { REPO, TestDaemon } from "./daemon";

test.skip(Boolean(process.env.AURELIUS_JOURNEY), "The test build always has a connection.");

test("a ticket that cannot start says so everywhere, and offers a way out", async () => {
  test.setTimeout(120_000);
  const daemon = new TestDaemon();
  await daemon.start();
  mkdirSync(join(daemon.dir, "atlas"));
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
  try {
    const p = await app.firstWindow();
    const nav = (name: string) => p.getByRole("link", { name: new RegExp(`^${name}`) }).first();
    await p.getByLabel("Name").fill("sam");
    await p.getByLabel("Password").fill("correct horse battery");
    await p.getByRole("button", { name: "Create" }).click();
    await expect(p.getByRole("heading", { name: "Inbox" })).toBeVisible();

    await nav("Employees").click();
    await p.getByRole("button", { name: "Add employee" }).click();
    // A role is no longer required.
    await p.getByRole("dialog").getByLabel("Name").fill("Builder");
    await p
      .getByRole("dialog")
      .getByRole("button", { name: /^(Create|Add|Save)/ })
      .click();
    await expect(p.getByRole("dialog")).toHaveCount(0);

    await nav("Knowledge").click();
    await p.getByRole("link", { name: "Projects", exact: true }).click();
    await p.getByRole("button", { name: "New project" }).first().click();
    await p.getByRole("dialog").getByLabel("Name").fill("Atlas");
    await p.getByRole("dialog").getByLabel("Project folder (required)").fill(join(daemon.dir, "atlas"));
    await p
      .getByRole("dialog")
      .getByRole("button", { name: /^Create/ })
      .click();
    await expect(p.getByRole("dialog")).toHaveCount(0);

    // Said before starting.
    await nav("Board").click();
    await expect(p.getByText("No connection is set up").first()).toBeVisible();
    await p.keyboard.press("n");
    const dialog = p.getByRole("dialog");
    await expect(dialog.getByText("No connection is set up")).toBeVisible();
    await dialog.getByLabel("Title").fill("Login page");
    await dialog.getByRole("button", { name: "Owner" }).press("Enter");
    await p.getByRole("menuitem", { name: "Builder" }).click();
    await dialog.getByRole("button", { name: /Create and start/ }).click();
    await expect(dialog).toHaveCount(0);

    // And after: on the card, in the panel, in the Inbox.
    const card = p.getByRole("region", { name: "Doing", exact: true }).getByRole("button", { name: /Login page/ });
    await expect(card.getByText(/stopped/i)).toBeVisible({ timeout: 30_000 });
    await card.click();
    await expect(p.getByText("Builder stopped and waits for you.")).toBeVisible();
    await expect(p.getByText("is working on it")).toHaveCount(0);
    await expect(p.getByText(/No connection is configured/).first()).toBeVisible();
    await expect(p.getByRole("button", { name: "Try again" })).toBeVisible();

    await nav("Inbox").click();
    await p
      .getByRole("button", { name: /Login page/ })
      .first()
      .click();
    await expect(p.getByText("Why it stopped")).toBeVisible();
    await expect(p.getByRole("button", { name: "Try again" })).toBeVisible();
    await expect(p.getByRole("link", { name: "Set one up" })).toBeVisible();

    // Trying again asks again: it reaches the server, and still says why, without a connection.
    await p.getByRole("button", { name: "Try again" }).click();
    await expect(p.getByText("Open a chat with a question first.")).toHaveCount(0);
    await expect(p.getByText("This did not work.")).toHaveCount(0);
    await p.getByRole("button", { name: /Open ticket/ }).click();
    await p
      .getByRole("link", { name: /Open run/ })
      .first()
      .click();
    await expect(p.getByText("Please try again.")).toBeVisible();
    await expect(p.getByText("Assignment", { exact: true })).toBeVisible();
  } finally {
    await app.close();
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
  }
});
