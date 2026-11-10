// The Changes screen against a real daemon and a real repository: what is not committed,
// the commits of a branch, a commit's files and its diff, and the branches.

import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { Connection, expectBody } from "@aurelius/client";
import { expect, test } from "@playwright/test";
import { openPipe } from "../../electron/src/main/pipe";
import { TestDaemon, until } from "./daemon";

const PASSWORD = "a-long-enough-password";
const SHOTS = process.env.AURELIUS_SHOTS;

function git(dir: string, ...args: string[]): void {
  execFileSync(
    "git",
    ["-C", dir, "-c", "user.name=Forge", "-c", "user.email=forge@example.com", "-c", "commit.gpgsign=false", ...args],
    { stdio: "ignore" },
  );
}

test("a project's changes, commits and branches are read from its repository", async ({ page }) => {
  const daemon = new TestDaemon();
  await daemon.start();
  let token: string | null = null;
  const owner = new Connection({
    open: () => openPipe(daemon.socket),
    sessions: {
      get: async () => token,
      set: async (next) => {
        token = next;
      },
    },
    clientName: "e2e",
    clientVersion: "0",
    deviceName: "e2e",
  });
  try {
    owner.start();
    await until("online", () => owner.getState().status === "online");
    await owner.write("user.create", { username: "sam", password: PASSWORD }, { retry: false });
    await owner.signIn("sam", PASSWORD);

    const repo = join(daemon.dir, "northwind");
    mkdirSync(join(repo, "src"), { recursive: true });
    git(repo, "init", "-q", "-b", "main");
    writeFileSync(join(repo, "README.md"), "# Northwind\n");
    writeFileSync(join(repo, "src/main.rs"), "fn main() {}\n");
    git(repo, "add", ".");
    git(repo, "commit", "-q", "-m", "Start the project\n\nThe first files of the API.");
    writeFileSync(join(repo, "src/main.rs"), 'fn main() {\n    println!("hi");\n}\n');
    git(repo, "commit", "-q", "-am", "Say hi at the start");
    git(repo, "checkout", "-q", "-b", "feat/login");
    writeFileSync(join(repo, "src/login.rs"), "pub fn login() {}\n");
    git(repo, "add", ".");
    git(repo, "commit", "-q", "-m", "Add the login");
    git(repo, "checkout", "-q", "main");
    writeFileSync(join(repo, "README.md"), "# Northwind\n\nAn API for orders.\n");
    writeFileSync(join(repo, "notes.txt"), "a new file\n");
    await owner.write("project.create", { name: "Northwind API", folder: repo });
    await owner.write("remote.enable", undefined);

    await page.goto(`${daemon.origin}/`);
    const start = await owner.write("remote.start_pairing", undefined);
    const result = start.result;
    if (result.kind !== "host" || result.value.type !== "pairing_code") throw new Error("Expected a code.");
    await page.getByLabel("Code").fill(result.value.code);
    await page.getByRole("button", { name: "Pair" }).click();
    const pending = await until("the pairing request", async () => {
      const body = await owner.call("remote.get_status");
      return expectBody(body, "remote").value.pending[0];
    });
    await owner.write(
      "remote.decide_pairing",
      { pairing_id: pending.pairing_id, approve: true, owner: false },
      { retry: false },
    );
    await page.getByLabel("Name").fill("sam");
    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();

    // Under All projects a project is chosen first.
    await page.getByRole("link", { name: "Changes" }).click();
    await expect(page.getByRole("heading", { name: "Changes" })).toBeVisible();
    await page.getByRole("button", { name: "Northwind API" }).click();

    // What is not committed: the edit and the new file, with their text.
    await expect(page.getByText("2 files changed")).toBeVisible();
    await expect(page.getByText("An API for orders.")).toBeVisible();
    await expect(page.getByText("a new file", { exact: true })).toBeVisible();
    if (SHOTS) await page.screenshot({ path: join(SHOTS, "changes-working.png"), animations: "disabled" });
    await page.getByRole("button", { name: /notes\.txt/ }).click();
    await expect(page.getByText("An API for orders.")).toBeHidden();

    // The commits where the folder is, one of them whole, and those of another branch.
    await page.getByRole("radio", { name: "Commits" }).click();
    await expect(page.getByRole("button", { name: /Say hi at the start/ })).toBeVisible();
    await expect(page.getByRole("button", { name: /Add the login/ })).toBeHidden();
    await page.getByRole("button", { name: /Start the project/ }).click();
    await expect(page.getByText("The first files of the API.")).toBeVisible();
    await expect(page.getByText("2 files changed")).toBeVisible();
    await page.getByRole("button", { name: /Say hi at the start/ }).click();
    await expect(page.getByText('println!("hi");')).toBeVisible();
    if (SHOTS) await page.screenshot({ path: join(SHOTS, "changes-commit.png"), animations: "disabled" });

    await page.getByRole("radio", { name: "Branches" }).click();
    await expect(page.getByText("Not pushed").first()).toBeVisible();
    if (SHOTS) await page.screenshot({ path: join(SHOTS, "changes-branches.png"), animations: "disabled" });
    await page.getByRole("button", { name: /feat\/login/ }).click();
    await expect(page.getByRole("button", { name: /Add the login/ })).toBeVisible();
  } finally {
    owner.dispose();
    await daemon.dispose();
  }
});
