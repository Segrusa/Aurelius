import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { _electron, expect, test } from "@playwright/test";
import { openCompanyPage } from "./company";
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
    // Reloaded, so that what the theme injects at the start is seen too.
    await page.addInitScript(() => {
      const seen: string[] = [];
      (globalThis as unknown as { violations: string[] }).violations = seen;
      document.addEventListener("securitypolicyviolation", (e) => seen.push(`${e.violatedDirective} ${e.sample}`));
    });
    await page.reload();
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
      "exportArtifact",
      "forgetServer",
      "getSession",
      "getStartup",
      "hideArtifact",
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
      "pickInArtifact",
      "placeArtifact",
      "quit",
      "releaseArtifact",
      "saveSecretFile",
      "sendLink",
      "serveArtifact",
      "setSession",
      "setStartup",
      "setTitleBar",
      "showArtifact",
      "startupAvailable",
      "stopArtifactPick",
      "storeFile",
      "takeLink",
      "updateArtifact",
      "useServer",
    ]);

    await page.getByLabel("Name").fill("sam");
    await page.getByLabel("Password").fill("a-long-enough-password");
    await page.getByRole("button", { name: "Create" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "New session" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Nothing waits for you." })).toBeVisible();

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
    await expect(page.getByRole("heading", { level: 1, name: "New session" })).toBeVisible();

    // The title bar moves the window on Windows and macOS, until a dialog or menu needs its clicks.
    await openCompanyPage(page, "Employees");
    const titleBar = page.locator("[data-titlebar]").first();
    await expect(titleBar).toBeVisible();
    const region = () => titleBar.evaluate((el) => getComputedStyle(el).getPropertyValue("app-region"));
    const os = await page.evaluate(() => document.documentElement.getAttribute("data-os"));
    await page.evaluate(() => document.documentElement.setAttribute("data-os", "win32"));
    expect(await region()).toBe("drag");
    await page.evaluate(() => {
      const menu = document.createElement("div");
      menu.id = "test-menu";
      menu.setAttribute("popover", "");
      document.body.append(menu);
    });
    expect(await region()).toBe("drag");
    await page.evaluate(() => (document.getElementById("test-menu") as HTMLElement).showPopover());
    expect(await region()).toBe("no-drag");
    await page.evaluate(() => document.getElementById("test-menu")?.remove());
    expect(await region()).toBe("drag");
    await page.keyboard.press("Control+k");
    await expect(page.getByRole("combobox", { name: "Command palette" })).toBeVisible();
    expect(await page.evaluate(() => (globalThis as unknown as { violations: string[] }).violations)).toEqual([]);
    expect(await region()).toBe("no-drag");
    await page.keyboard.press("Escape");
    await expect(page.getByRole("combobox", { name: "Command palette" })).toHaveCount(0);
    expect(await region()).toBe("drag");
    await page.evaluate((value) => {
      if (value === null) document.documentElement.removeAttribute("data-os");
      else document.documentElement.setAttribute("data-os", value);
    }, os);

    const reconnecting = page.locator('p[role="status"]');
    await daemon.stop();
    await expect(reconnecting).toHaveText("Reconnecting…");
    await daemon.start();
    await expect(reconnecting).toBeHidden({ timeout: 30_000 });
  } finally {
    await app.close();
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
  }
});
