// Account → Your devices: a board member pairs, sees and revokes their own devices there;
// Remote access keeps only what is the server's (feature `own_devices`).

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { _electron, expect, type Page, test } from "@playwright/test";
import { openSettings } from "./company";
import { REPO, TestDaemon } from "./daemon";

const PASSWORD = "a-long-enough-password";

const main = (page: Page) => page.getByRole("main");

async function open(page: Page, name: string) {
  await page.getByRole("navigation", { name: "Settings" }).getByRole("link", { name, exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name, exact: true })).toBeVisible();
}

test("a board member pairs, lists and revokes their own device in Account", async () => {
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
  try {
    const page = await app.firstWindow();
    await page.getByLabel("Name").fill("sam");
    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByRole("button", { name: "Create" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "New session" })).toBeVisible();
    await openSettings(page);

    // Remote access is the server's switch; it points to Account for your own devices.
    await open(page, "Remote access");
    await main(page).getByRole("switch", { name: "Remote access" }).click();
    await expect(main(page).getByText("Pair and revoke your own phones and computers under Account.")).toBeVisible();
    await expect(main(page).getByRole("button", { name: "Show code" })).toBeHidden();

    await open(page, "Account");
    await expect(main(page).getByText("No paired devices.")).toBeVisible();
    await main(page).getByRole("button", { name: "Show code" }).click();
    const code =
      (await main(page)
        .getByText(/^[A-Z2-9]{8}$/)
        .textContent()) ?? "";

    // A phone enters the code; sam is asked, in Account.
    const asked = await fetch(`${daemon.origin}/v1/pair`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ code, device_name: "sam-phone", platform: "android" }),
    });
    expect(asked.ok).toBe(true);
    await page.reload();
    await expect(page.getByRole("heading", { level: 1, name: "Account" })).toBeVisible();
    const waiting = main(page).getByText("Waiting for your answer");
    await expect(waiting).toBeVisible();
    await main(page).getByRole("button", { name: "Allow", exact: true }).click();

    const row = main(page).getByText("sam-phone", { exact: true });
    await expect(row).toBeVisible();
    await expect(waiting).toBeHidden();
    await expect(main(page).getByText("Only you see these devices.", { exact: false })).toBeVisible();

    // Not listed again under Remote access: it is sam's, not the server's.
    await open(page, "Remote access");
    await expect(main(page).getByText("sam-phone", { exact: true })).toBeHidden();

    await open(page, "Account");
    await main(page).getByRole("button", { name: "Revoke", exact: true }).click();
    await main(page).getByRole("button", { name: "Yes, revoke" }).click();
    await expect(main(page).getByText("No paired devices.")).toBeVisible();
    await expect(main(page).getByText("sam-phone", { exact: true })).toBeHidden();
  } finally {
    await app.close();
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
  }
});
