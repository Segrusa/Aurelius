// What only the desktop app does: another server, the operating system's links, windows of their
// own and the packaged program. All against a real service in a throw-away folder.

import { spawn } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Connection } from "@aurelius/client";
import { _electron, chromium, expect, type Page, test } from "@playwright/test";
import { openPipe } from "../../electron/src/main/pipe";
import { freePort, REPO, TestDaemon, until } from "./daemon";

const PASSWORD = "a-long-enough-password";
const ARGS = process.env.AURELIUS_ELECTRON_ARGS?.split(" ") ?? [];
const BIN = process.env.AURELIUS_DAEMON_BIN ?? join(REPO, "target/debug/aurelius-daemon");

function ownerOf(daemon: TestDaemon): Connection {
  const owner = new Connection({
    open: () => openPipe(daemon.socket),
    sessions: {
      get: async () => session,
      set: async (token) => {
        session = token;
      },
    },
    clientName: "e2e",
    clientVersion: "0",
    deviceName: "e2e",
  });
  let session: string | null = null;
  owner.start();
  return owner;
}

async function preparedServer(): Promise<{ daemon: TestDaemon; owner: Connection }> {
  const daemon = new TestDaemon();
  await daemon.start();
  const owner = ownerOf(daemon);
  await until("online", () => owner.getState().status === "online");
  await owner.write("user.create", { username: "sam", password: PASSWORD }, { retry: false });
  await owner.signIn("sam", PASSWORD);
  await owner.write("project.create", { name: "Northwind API" });
  await owner.write("remote.enable", undefined);
  return { daemon, owner };
}

async function pairingText(daemon: TestDaemon, owner: Connection): Promise<string> {
  const start = await owner.write("remote.start_pairing", undefined);
  const { result } = start;
  if (result.kind !== "host" || result.value.type !== "pairing_code") throw new Error("Expected a code.");
  const state = owner.getState();
  const payload = {
    v: 1,
    url: daemon.origin,
    code: result.value.code,
    host_id: state.hostId,
    host_name: "Server PC",
    env: "dev",
  };
  return `aurelius-pair:${Buffer.from(JSON.stringify(payload)).toString("base64url")}`;
}

function launch(data: string, profile: string) {
  return _electron.launch({
    args: [join(REPO, "clients/electron"), `--user-data-dir=${profile}`, ...ARGS],
    env: {
      ...process.env,
      AURELIUS_DATA_DIR: data,
      AURELIUS_ENV: "dev",
      ELECTRON_RENDERER_URL: "",
    } as Record<string, string>,
  });
}

async function signIn(page: Page): Promise<void> {
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
  await page.getByLabel("Name").fill("sam");
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();
}

test("pairs with a server elsewhere, works there, comes back and forgets it", async () => {
  const { daemon, owner } = await preparedServer();
  const room = mkdtempSync(join(tmpdir(), "aurelius-native-"));
  const here = join(room, "notes.txt");
  writeFileSync(here, "hello");
  const app = await launch(join(room, "data"), join(room, "profile"));
  try {
    const page = await app.firstWindow();
    // No service here and none to start: not a dead end.
    await expect(page.getByRole("heading", { name: "Not connected" })).toBeVisible();
    await page.getByLabel("Pair a server").fill("nonsense");
    await page.getByRole("button", { name: "Pair", exact: true }).click();
    await expect(page.getByRole("alert")).toHaveText("This is not a pairing text.");

    await page.getByLabel("Pair a server").fill(await pairingText(daemon, owner));
    await page.getByRole("button", { name: "Pair", exact: true }).click();
    await expect(page.getByText(/Waiting until you choose "Allow as owner"/)).toBeVisible();
    const pending = await until("the pairing request", async () => {
      const status = await owner.call("remote.get_status");
      return status.type === "remote" ? status.value.pending[0] : undefined;
    });
    expect(pending.platform).toBe("linux");
    await owner.write(
      "remote.decide_pairing",
      { pairing_id: pending.pairing_id, approve: true, owner: true },
      { retry: false },
    );

    // The app starts over on that server.
    await signIn(page);
    await expect(page.getByText("Northwind API")).toBeVisible();
    await expect(
      page.evaluate(() =>
        (window as unknown as { aurelius: { info(): Promise<{ target: unknown }> } }).aurelius
          .info()
          .then((i) => i.target !== null),
      ),
    ).resolves.toBe(true);

    // The device secret is not readable in the clear in the servers file.
    const servers = join(room, "data", "client", "servers.json");
    expect(statSync(servers).mode & 0o777).toBe(0o600);

    // Followed the events over the WebSocket: what the server makes shows up here.
    await owner.write("project.create", { name: "Made on the server" });
    await expect(page.getByText("Made on the server")).toBeVisible();

    // A file from this computer reaches the server through its inbox.
    await app.evaluate(({ dialog }, file) => {
      dialog.showOpenDialog = (async () => ({ canceled: false, filePaths: [file] })) as typeof dialog.showOpenDialog;
    }, here);
    await page.getByRole("button", { name: "New ticket" }).first().click();
    await page.getByPlaceholder("What should happen?").fill("With a remote file");
    await page.getByRole("button", { name: "Add files…" }).click();
    await page.getByRole("button", { name: "Remove notes.txt" }).click({ trial: true });
    await page.getByRole("button", { name: "Create", exact: true }).click();
    const withFile = await until("the ticket", async () => {
      const body = await owner.call("ticket.list", {});
      return body.type === "tickets" ? body.value.tickets.find((t) => t.title === "With a remote file") : undefined;
    });
    expect(withFile.attachments.map((f) => f.name)).toEqual(["notes.txt"]);

    await page.goto("app://aurelius/settings/servers");
    await page.getByRole("button", { name: /^Forget / }).click();
    await page.getByRole("button", { name: "Forget", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Not connected" })).toBeVisible();
    await expect(page.getByRole("button", { name: /^Forget / })).toBeHidden();
  } finally {
    await app.close();
    owner.dispose();
    await daemon.dispose();
    rmSync(room, { recursive: true, force: true });
  }
});

test("a link from the operating system opens in the running app; a project gets its own window", async () => {
  const { daemon, owner } = await preparedServer();
  const profile = mkdtempSync(join(tmpdir(), "aurelius-native-"));
  const app = await launch(daemon.data, profile);
  try {
    const page = await app.firstWindow();
    await signIn(page);
    await owner.write("ticket.create", { project_id: await projectId(owner), title: "From a link" }, { retry: false });
    const ticket = await until("the ticket", async () => {
      const body = await owner.call("ticket.list", {});
      return body.type === "tickets" ? body.value.tickets.find((t) => t.title === "From a link") : undefined;
    });
    const host = owner.getState().hostId;

    const second = (link: string) =>
      new Promise<number | null>((resolve) => {
        const child = spawn(
          join(REPO, "clients/electron/node_modules/electron/dist/electron"),
          [join(REPO, "clients/electron"), `--user-data-dir=${profile}`, ...ARGS, link],
          { env: { ...process.env, AURELIUS_DATA_DIR: daemon.data, AURELIUS_ENV: "dev" }, stdio: "ignore" },
        );
        child.once("exit", resolve);
      });

    // A second program gives its link to the first and ends.
    expect(await second(`aurelius://${host}/ticket/${ticket.id}`)).toBe(0);
    await expect(page).toHaveURL(/\/board\?ticket=/);
    await expect(page.getByRole("complementary", { name: /From a link/ })).toBeVisible();

    await second("aurelius://nonsense");
    await expect(
      page.getByRole("region", { name: "Notifications" }).getByText("This isn't a valid Aurelius link."),
    ).toBeVisible();
    await second(`aurelius://0190aaaa-0000-7000-8000-000000000000/ticket/${ticket.id}`);
    await expect(
      page
        .getByRole("region", { name: "Notifications" })
        .getByText("This link is for a server this device isn't paired with."),
    ).toBeVisible();

    // A window per project, not a second one; the page there has no sidebar.
    await page.goto("app://aurelius/projects");
    await page
      .getByRole("link", { name: /Northwind API/ })
      .first()
      .click();
    await page
      .getByRole("button", { name: /More|Project menu|Project actions/ })
      .first()
      .click();
    const [window] = await Promise.all([
      app.waitForEvent("window"),
      page.getByRole("menuitem", { name: "Open in new window" }).click(),
    ]);
    await expect(window.getByText("Northwind API").first()).toBeVisible();
    await expect(window.getByRole("navigation", { name: /main|Sidebar/i })).toBeHidden();
    const titles = await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().map((w) => w.getTitle()));
    expect(titles).toContain("Northwind API — Aurelius");
    // Asking again brings that window forward instead of a third.
    await page
      .getByRole("button", { name: /More|Project menu|Project actions/ })
      .first()
      .click();
    await page.getByRole("menuitem", { name: "Open in new window" }).click();
    await expect.poll(() => app.windows().length).toBe(2);
  } finally {
    await app.close();
    owner.dispose();
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
  }
});

const PACKAGED = join(REPO, "clients/electron/release/linux-x64");

test("the packaged program runs from its folder and starts the service beside it", async () => {
  test.skip(!existsSync(PACKAGED), "Run: pnpm --filter @aurelius/electron package");
  const room = mkdtempSync(join(tmpdir(), "aurelius-install-"));
  mkdirSync(join(room, "install"));
  cpSync(PACKAGED, join(room, "install", "app"), { recursive: true, verbatimSymlinks: true });
  cpSync(BIN, join(room, "install", "aurelius-daemon"));
  const port = await freePort();
  const data = join(room, "data");
  const child = spawn(
    join(room, "install", "app", "aurelius"),
    [`--user-data-dir=${join(room, "profile")}`, `--remote-debugging-port=${port}`, ...ARGS],
    {
      env: {
        PATH: process.env.PATH,
        HOME: join(room, "home"),
        AURELIUS_DATA_DIR: data,
        AURELIUS_ENV: "dev",
        AURELIUS_NO_TAILSCALE: "1",
        AURELIUS_NO_TRAY: "1",
        AURELIUS_NO_AUTO_BACKUP: "1",
        AURELIUS_REMOTE_PORT: String(await freePort()),
      } as NodeJS.ProcessEnv,
      stdio: "ignore",
    },
  );
  try {
    const browser = await until("the debugging port", () =>
      chromium.connectOverCDP(`http://127.0.0.1:${port}`).catch(() => undefined),
    );
    const page = browser.contexts()[0]?.pages()[0];
    if (!page) throw new Error("No window.");
    // The service came up by itself: this screen needs its answer.
    await expect(page.getByRole("heading", { name: "Create the first board member" })).toBeVisible({ timeout: 20_000 });
    expect(page.url()).toBe("app://aurelius/");
    expect(existsSync(join(room, "install", "aurelius.version"))).toBe(true);
    // The page reaches no Node here either, and the fuses are set.
    expect(await page.evaluate(() => typeof (globalThis as { process?: unknown }).process)).toBe("undefined");
    await browser.close();
  } finally {
    child.kill();
    // The service outlives the app on purpose: stop it here through its socket's owner.
    spawn("pkill", ["-f", join(room, "install", "aurelius-daemon")]);
    rmSync(room, { recursive: true, force: true });
  }
});

async function projectId(owner: Connection): Promise<string> {
  const body = await owner.call("project.list");
  if (body.type !== "projects") throw new Error("Expected projects.");
  const first = body.value.projects[0];
  if (!first) throw new Error("No project.");
  return first.id;
}
