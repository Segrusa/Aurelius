import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { _electron, expect, test } from "@playwright/test";
import { REPO, TestDaemon } from "./daemon";

test("runs over the local socket: first board member, Inbox, reconnect, and no way out for the page", async () => {
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
    await expect(page.getByRole("heading", { name: "Create the first board member" })).toBeVisible();
    expect(page.url()).toBe("app://aurelius/");

    // The page has no Node and only the named calls.
    const reach = await page.evaluate(() => ({
      require: typeof (globalThis as { require?: unknown }).require,
      process: typeof (globalThis as { process?: unknown }).process,
      bridge: Object.keys((globalThis as unknown as { aurelius: object }).aurelius).sort(),
    }));
    expect(reach.require).toBe("undefined");
    expect(reach.process).toBe("undefined");
    expect(reach.bridge).toEqual([
      "cancelPairing",
      "closeLink",
      "copy",
      "copySecret",
      "desktopPrefs",
      "forgetServer",
      "getSession",
      "getStartup",
      "holdLink",
      "info",
      "listServers",
      "listenLink",
      "listenLinks",
      "openFolder",
      "openLink",
      "openProjectWindow",
      "pairServer",
      "pathOf",
      "pickFiles",
      "pickFolder",
      "quit",
      "saveSecretFile",
      "sendLink",
      "setSession",
      "setStartup",
      "setTitleBar",
      "startupAvailable",
      "storeFile",
      "takeLink",
      "useServer",
    ]);

    await page.getByLabel("Name").fill("sam");
    await page.getByLabel("Password").fill("a-long-enough-password");
    await page.getByRole("button", { name: "Create" }).click();
    await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();
    await expect(page.getByText("Nothing waits for you.")).toBeVisible();

    // Fonts and styles came through the CSP: the design's type and panel.
    await page.emulateMedia({ colorScheme: "dark" });
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    const look = await page.evaluate(async () => {
      await document.fonts.ready;
      return {
        family: getComputedStyle(document.body).fontFamily,
        loaded: document.fonts.check('13px "Inter Variable"'),
        bg: getComputedStyle(document.body).backgroundColor,
      };
    });
    expect(look.family).toContain("Inter Variable");
    expect(look.loaded).toBe(true);
    expect(look.bg).toBe("rgb(8, 9, 10)");

    await page.reload();
    await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();

    await daemon.stop();
    await expect(page.getByRole("status")).toHaveText("Reconnecting…");
    await daemon.start();
    await expect(page.getByRole("status")).toBeHidden({ timeout: 30_000 });
  } finally {
    await app.close();
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
  }
});
