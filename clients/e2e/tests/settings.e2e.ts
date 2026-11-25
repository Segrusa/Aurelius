import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { _electron, expect, type Page, test } from "@playwright/test";
import { openSettings } from "./company";
import { REPO, TestDaemon } from "./daemon";

const PASSWORD = "a-long-enough-password";
/** Every entry of the Settings sidebar, in the order of design/settings-redesign.html. */
const ENTRIES = [
  "Account",
  "My defaults",
  "This device",
  "General",
  "Board members",
  "Employees",
  "Workspaces",
  "Guidelines",
  "Memory",
  "Overview",
  "Remote & phones",
  "Connections",
  "Tools & MCP",
  "Runs & load",
  "Backups",
  "Activity log",
];
/** The entries that are pages of Settings itself; the others open their own page. */
const SECTIONS = ENTRIES.filter((e) => !["Employees", "Workspaces", "Guidelines", "Memory"].includes(e));

async function open(page: Page, name: string) {
  await page.getByRole("navigation", { name: "Settings" }).getByRole("link", { name, exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name, exact: true })).toBeVisible();
}

const main = (page: Page) => page.getByRole("main");

test("every section works against a real daemon, and what is changed stays changed", async () => {
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
    await expect(page.getByRole("heading", { level: 1, name: "All workspaces" })).toBeVisible();

    await openSettings(page);
    await expect(page.getByRole("heading", { level: 1, name: "Account" })).toBeVisible();

    const nav = page.getByRole("navigation", { name: "Settings" });
    expect(await nav.getByRole("link").allTextContents()).toEqual(ENTRIES);
    await page.getByLabel("Search settings").fill("compaction");
    expect(await nav.getByRole("link").allTextContents()).toEqual(["Runs & load"]);
    await page.getByLabel("Search settings").fill("nothing like this");
    await expect(nav.getByText("Nothing found.")).toBeVisible();
    await page.getByLabel("Search settings").fill("");
    for (const name of SECTIONS) {
      await open(page, name);
      await page.waitForTimeout(250);
      expect({ name, alerts: await main(page).getByRole("alert").allTextContents() }).toEqual({ name, alerts: [] });
    }

    await open(page, "This device");
    await page.getByRole("radio", { name: "Light" }).click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    await page.getByRole("radio", { name: "Dark" }).click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

    await open(page, "General");
    const companyName = main(page).getByLabel("Company name");
    await companyName.fill("Northwind");
    await main(page).getByRole("button", { name: "Save" }).click();
    const mode = main(page).getByRole("switch", { name: "Management chooses the mode" });
    const was = await mode.isChecked();
    await mode.click();
    await expect(mode).toBeChecked({ checked: !was });
    await open(page, "Board members");
    await main(page).getByRole("button", { name: "Add board member" }).click();
    const add = page.getByRole("dialog", { name: "Add board member" });
    await add.getByLabel("Name").fill("alex");
    await add.getByLabel("First password").fill(PASSWORD);
    await add.getByLabel("Repeat password").fill(PASSWORD);
    await add.getByRole("button", { name: "Add", exact: true }).click();
    await expect(add).toBeHidden();
    await expect(main(page).getByText("alex", { exact: true })).toBeVisible();
    await main(page).getByRole("button", { name: "Disable", exact: true }).click();
    await main(page).getByRole("button", { name: "Yes, disable" }).click();
    await expect(main(page).getByText("Disabled", { exact: true })).toBeVisible();

    await open(page, "Overview");
    await main(page).getByLabel("Server name").fill("Testbak");
    await main(page).getByRole("button", { name: "Save" }).click();
    await expect(main(page).getByRole("button", { name: "Save" })).toBeHidden();

    await open(page, "Runs & load");
    const runs = main(page).getByLabel("Max runs at once");
    await expect(runs).not.toHaveValue("");
    const limit = String(Number(await runs.inputValue()) + 2);
    await runs.fill(limit);
    await main(page).getByRole("button", { name: "Save limits" }).click();
    await expect(main(page).getByRole("button", { name: "Save limits" })).toBeHidden();

    await open(page, "Backups");
    await main(page).getByRole("button", { name: "Back up now" }).click();
    await expect(main(page).getByRole("button", { name: "Restore" }).first()).toBeVisible({ timeout: 30_000 });

    // A connection comes and goes; nothing here needs a real key.
    await open(page, "Connections");
    await main(page).getByRole("button", { name: "Add connection" }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByRole("button", { name: /Anthropic API/ }).click();
    await dialog.getByRole("button", { name: "Add connection", exact: true }).click();
    await expect(main(page).getByRole("button", { name: /Anthropic API/ })).toBeVisible();
    await main(page).getByRole("button", { name: "Remove" }).last().click();
    await main(page).getByRole("button", { name: "Yes, remove" }).click();
    await expect(main(page).getByRole("button", { name: /Anthropic API/ })).toBeHidden();

    // Remote access is the server's switch; each board member pairs their own devices in Account.
    await open(page, "Remote & phones");
    await main(page).getByRole("switch", { name: "Remote access" }).click();
    await expect(main(page).getByRole("button", { name: "Show code" })).toBeHidden();
    await open(page, "Account");
    await expect(main(page).getByText("Devices", { exact: true })).toBeVisible();
    await expect(main(page).getByText("This device", { exact: true })).toBeVisible();
    await main(page).getByRole("button", { name: "Add a device" }).click();
    await main(page).getByRole("button", { name: "Show code" }).click();
    await expect(main(page).getByText(/Valid until/)).toBeVisible();

    // All of it still so after a reload, read back from the server.
    await page.reload();
    await expect(page.getByRole("heading", { level: 1, name: "Account" })).toBeVisible();
    await open(page, "General");
    await expect(main(page).getByLabel("Company name")).toHaveValue("Northwind");
    await expect(main(page).getByRole("switch", { name: "Management chooses the mode" })).toBeChecked({
      checked: !was,
    });
    await open(page, "Runs & load");
    await expect(main(page).getByLabel("Max runs at once")).toHaveValue(limit);
    await open(page, "Overview");
    await expect(main(page).getByLabel("Server name")).toHaveValue("Testbak");

    // Password, sign out, and back in with the new one only.
    const next = "another-long-enough-password";
    await open(page, "Account");
    await main(page).getByRole("button", { name: "Change", exact: true }).click();
    await main(page).getByLabel("Current password").fill(PASSWORD);
    await main(page).getByLabel("New password", { exact: true }).fill(next);
    await main(page).getByLabel("Repeat new password").fill(next);
    await main(page).getByRole("button", { name: "Save password" }).click();
    await expect(main(page).getByText("Your password is changed.")).toBeVisible();
    await main(page).getByRole("button", { name: "Sign out" }).first().click();
    await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
    await page.getByLabel("Name").fill("sam");
    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.getByRole("alert")).toBeVisible();
    await page.getByLabel("Password").fill(next);
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.getByRole("link", { name: "Back to the app" })).toBeVisible();
  } finally {
    await app.close();
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
  }
});
