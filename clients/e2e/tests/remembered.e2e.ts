// A sign-in survives closing the app: against a real daemon and with whatever this machine's keychain
// does. The token the old Rust client left behind counts as well.

import { mkdtempSync, readdirSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { _electron, expect, test } from "@playwright/test";
import { REPO, TestDaemon } from "./daemon";

test.skip(Boolean(process.env.AURELIUS_JOURNEY), "One ordinary daemon is enough.");

test("closing and opening the app keeps you signed in; signing out does not", async () => {
  test.setTimeout(120_000);
  const daemon = new TestDaemon();
  await daemon.start();
  const profile = mkdtempSync(join(tmpdir(), "aurelius-electron-"));
  const open = () =>
    _electron.launch({
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
    let app = await open();
    let p = await app.firstWindow();
    await p.getByLabel("Name").fill("sam");
    await p.getByLabel("Password").fill("correct horse battery");
    await p.getByRole("button", { name: "Create" }).click();
    await expect(p.getByRole("heading", { name: "Inbox" })).toBeVisible();
    await app.close();

    app = await open();
    p = await app.firstWindow();
    await expect(p.getByRole("heading", { name: "Inbox" })).toBeVisible({ timeout: 30_000 });
    await expect(p.getByLabel("Password")).toHaveCount(0);

    const secrets = join(daemon.data, "secrets");
    const kept = () => readdirSync(secrets).filter((name) => name.includes("desktop-session-local"));
    for (const name of kept()) expect(statSync(join(secrets, name)).mode & 0o777).toBe(0o600);

    await p
      .getByRole("link", { name: /^Settings/ })
      .first()
      .click();
    await p.getByRole("button", { name: "Sign out", exact: true }).first().click();
    await expect(p.getByLabel("Password")).toBeVisible({ timeout: 30_000 });
    expect(kept()).toEqual([]);
    await app.close();

    app = await open();
    p = await app.firstWindow();
    await expect(p.getByLabel("Password")).toBeVisible({ timeout: 30_000 });
    await app.close();
  } finally {
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
  }
});
