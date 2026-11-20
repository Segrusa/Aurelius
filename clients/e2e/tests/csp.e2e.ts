// The web build under the policy the gateway really sends: Astryx puts a few <style> elements in the
// page, and only those with a hash in the policy may stay. A refused one is silent without this check.

import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Connection } from "@aurelius/client";
import { _electron, expect, type Page, test } from "@playwright/test";
import { openPipe } from "../../electron/src/main/pipe";
import { REPO, serveStatic, TestDaemon, until, WEB_DIST } from "./daemon";
import { boardOf, channel, message, messages, scriptServer, ticket } from "./fake-server";
import { expectNewSession, sessionComposer } from "./new-session";

const SOURCE = readFileSync(join(REPO, "crates/remote-gateway/src/web.rs"), "utf8");
const POLICY = /"content-security-policy",\s*"([^"]+)"/.exec(SOURCE)?.[1] as string;

const violations = (page: Page) => page.evaluate(() => (globalThis as unknown as { violations: string[] }).violations);

const watchViolations = (page: Page) =>
  page.addInitScript(() => {
    const seen: string[] = [];
    (globalThis as unknown as { violations: string[] }).violations = seen;
    document.addEventListener("securitypolicyviolation", (e) =>
      seen.push(`${e.violatedDirective} ${e.blockedURI} ${e.sample}`),
    );
  });

const open = ticket("t1", 35, "doing", { title: "Trim the texts", conversation_id: "d1" });
const talk = messages("d1", [message("m1", "assistant", "```ts\nconst answer: number = 42;\n```")]);
const script = {
  employees: [{ id: "e1", name: "Flux", role: "engineer", title: "Engineer", status: "active" }],
  features: ["ticket_channels", "ticket_discussion"],
  tickets: [open],
  board: boardOf([open]),
  channels: [channel("d1", "#35-trim", { kind: "ticket", ticket_id: "t1", ticket_number: 35, ticket_column: "doing" })],
  details: { t1: { ticket: open, root_task: null, tasks: [], as_of_seq: 1 } },
  feeds: { d1: talk },
  conversations: { d1: talk },
};

/** What makes Astryx put a <style> in the page: a code block, a dialog and a tooltip. */
async function drawEverything(page: Page, newTicketKey = "Alt+t") {
  await expect(page.locator(".astryx-code-block code").first()).toBeVisible();
  await expect(page.locator("style:not([data-astryx-highlight-dynamic])").first()).toBeAttached();
  // The sidebar's New session row opens it; New ticket is a dialog on its key (Alt+T on the web).
  await page.getByRole("link", { name: "New session" }).click();
  await expectNewSession(page);
  await page.keyboard.press(newTicketKey);
  await expect(page.getByLabel("Title")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("navigation", { name: "Main" }).getByRole("link").first().hover();
  await page.waitForTimeout(600);
}

test("nothing the app draws is refused by the gateway's policy", async ({ page }) => {
  expect(POLICY).toContain("style-src 'self' 'sha256-");
  const site = await serveStatic(WEB_DIST, { "content-security-policy": POLICY });
  try {
    await watchViolations(page);
    await scriptServer(page, script);
    await page.goto(`${site.origin}/ticket/35`);
    await drawEverything(page);
    expect(await violations(page)).toEqual([]);
  } finally {
    site.close();
  }
});

test("nothing is refused on the page a real gateway serves", async ({ page }) => {
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
    await owner.write("user.create", { username: "sam", password: "a-long-enough-password" }, { retry: false });
    await owner.signIn("sam", "a-long-enough-password");
    await owner.write("remote.enable", undefined);
    await watchViolations(page);
    // The page and its files are the gateway's; only what the page asks it afterwards is scripted.
    await scriptServer(page, script);
    const response = await page.goto(`${daemon.origin}/ticket/35`);
    expect(response?.headers()["content-security-policy"]).toBe(POLICY);
    await drawEverything(page);
    expect(await violations(page)).toEqual([]);
  } finally {
    owner.dispose();
    await daemon.dispose();
  }
});

test("nothing is refused in the built Electron app", async () => {
  test.setTimeout(120_000);
  const daemon = new TestDaemon();
  await daemon.start();
  const profile = mkdtempSync(join(tmpdir(), "aurelius-csp-"));
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
    await watchViolations(page);
    await page.reload();
    await expect(page.getByRole("heading", { name: "Create the first board member" })).toBeVisible();
    expect(page.url()).toBe("app://aurelius/");
    await page.getByLabel("Name").fill("sam");
    await page.getByLabel("Password").fill("a-long-enough-password");
    await page.getByRole("button", { name: "Create" }).click();
    await expectNewSession(page);
    // A new session's first words show as a code block in the session it opens.
    await sessionComposer(page).fill("```ts\nconst answer: number = 42;\n```");
    await page.keyboard.press("Control+Enter");
    await expect(page).not.toHaveURL(/\/new$/);
    await drawEverything(page, "Control+t");
    expect(await violations(page)).toEqual([]);
  } finally {
    await app.close();
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
  }
});
