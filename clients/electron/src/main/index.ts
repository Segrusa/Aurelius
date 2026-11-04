// The main process: the window, the pipe to the local daemon, the session in the OS keychain. The
// page has no Node, reaches only the few calls below, and is checked on every one of them.

import { existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { hostname } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import type { Link } from "@aurelius/client";
import { app, BrowserWindow, dialog, ipcMain, net, protocol, safeStorage, session, type WebContents } from "electron";
import { daemonEndpoint, dataDir, openPipe } from "./pipe";
import { siteFile } from "./site";

const here = dirname(fileURLToPath(import.meta.url));
const SITE = join(here, "../renderer");
const ORIGIN = "app://aurelius";
const CSP =
  "default-src 'self'; img-src 'self' data: blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'";
const DEV_URL = (!app.isPackaged && process.env.ELECTRON_RENDERER_URL) || undefined;
const ENVIRONMENT =
  process.env.AURELIUS_ENV === "production" || (app.isPackaged && process.env.AURELIUS_ENV !== "dev")
    ? "production"
    : "dev";

protocol.registerSchemesAsPrivileged([
  { scheme: "app", privileges: { standard: true, secure: true, supportFetchAPI: true } },
]);

function serveSite(): void {
  protocol.handle("app", async (request) => {
    const url = new URL(request.url);
    // Not `origin`: Node has none for a scheme it does not know.
    if (url.protocol !== "app:" || url.host !== "aurelius") return new Response("Not found", { status: 404 });
    const file = siteFile(SITE, url.pathname);
    const response = await net.fetch(pathToFileURL(file).toString());
    const headers = new Headers(response.headers);
    headers.set("Content-Security-Policy", CSP);
    headers.set("X-Content-Type-Options", "nosniff");
    return new Response(response.body, { status: response.status, headers });
  });
}

function fromOurPage(frameUrl: string | undefined): boolean {
  if (frameUrl === undefined) return false;
  return frameUrl.startsWith(`${ORIGIN}/`) || (DEV_URL !== undefined && frameUrl.startsWith(DEV_URL));
}

/** Every call of the page goes through here. */
function handle<A extends unknown[], R>(
  channel: string,
  action: (sender: WebContents, ...args: A) => R | Promise<R>,
): void {
  ipcMain.handle(channel, (event, ...args) => {
    if (!fromOurPage(event.senderFrame?.url)) throw new Error("Not allowed.");
    return action(event.sender, ...(args as A));
  });
}

function serveLinks(): void {
  const links = new Map<number, { link: Link; owner: number }>();
  let nextId = 1;

  const close = (id: number) => {
    links.get(id)?.link.close();
    links.delete(id);
  };

  handle("link:open", async (sender) => {
    const root = dataDir(process.env, ENVIRONMENT);
    if (root === null) return { ok: false as const, message: "The data folder of this user was not found." };
    try {
      const link = await openPipe(daemonEndpoint(root));
      const id = nextId++;
      links.set(id, { link, owner: sender.id });
      link.listen(
        (text) => {
          if (!sender.isDestroyed()) sender.send("link:frame", id, text);
        },
        (code) => {
          links.delete(id);
          if (!sender.isDestroyed()) sender.send("link:closed", id, code);
        },
      );
      sender.once("destroyed", () => close(id));
      return { ok: true as const, id };
    } catch (error) {
      return {
        ok: false as const,
        message: error instanceof Error ? error.message : "The Aurelius service is not running.",
      };
    }
  });

  const own = (sender: WebContents, id: unknown) => {
    const entry = typeof id === "number" ? links.get(id) : undefined;
    return entry?.owner === sender.id ? entry : undefined;
  };
  handle("link:send", (sender, id: unknown, text: unknown) => {
    if (typeof text === "string") own(sender, id)?.link.send(text);
  });
  handle("link:close", (sender, id: unknown) => {
    if (own(sender, id)) close(id as number);
  });
}

/** Only ever kept encrypted by the OS. Without a keychain it lasts until the app closes. */
function serveSessions(): void {
  const file = join(app.getPath("userData"), "session.bin");
  let inMemory: string | null = null;
  const encrypted = () =>
    safeStorage.isEncryptionAvailable() && safeStorage.getSelectedStorageBackend?.() !== "basic_text";

  handle("session:get", () => {
    if (inMemory !== null) return inMemory;
    if (!encrypted() || !existsSync(file)) return null;
    try {
      inMemory = safeStorage.decryptString(readFileSync(file));
    } catch {
      rmSync(file, { force: true });
    }
    return inMemory;
  });
  handle("session:set", (_sender, token: unknown) => {
    inMemory = typeof token === "string" ? token : null;
    if (inMemory !== null && encrypted()) {
      mkdirSync(dirname(file), { recursive: true });
      writeFileSync(file, safeStorage.encryptString(inMemory), { mode: 0o600 });
    } else {
      rmSync(file, { force: true });
    }
  });
  handle("app:info", () => ({ version: app.getVersion(), deviceName: hostname() }));
  handle("app:desktop-prefs", () => {
    const root = dataDir(process.env, ENVIRONMENT);
    return {
      uiJson: root === null ? null : readSmallFile(join(root, "ui.json")),
      screenUseJson: root === null ? null : readSmallFile(join(root, "screen_use.json")),
    };
  });
}

/** The contents of a small text file, or null: these are preferences, never worth failing over. */
function readSmallFile(path: string): string | null {
  try {
    return statSync(path).size <= 1024 * 1024 ? readFileSync(path, "utf8") : null;
  } catch {
    return null;
  }
}

function openWindow(): void {
  const window = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 900,
    minHeight: 560,
    show: false,
    // The design's own background, so no white flash before the first paint.
    backgroundColor: "#08090A",
    autoHideMenuBar: true,
    title: "Aurelius",
    webPreferences: {
      preload: join(here, "../preload/index.cjs"),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      webSecurity: true,
    },
  });
  window.once("ready-to-show", () => window.show());
  window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  window.webContents.on("will-navigate", (event, url) => {
    if (!fromOurPage(url)) event.preventDefault();
  });
  window.loadURL(DEV_URL ?? `${ORIGIN}/`).catch((error: unknown) => {
    dialog.showErrorBox("Aurelius could not start", String(error));
    app.quit();
  });
}

void app.whenReady().then(() => {
  session.defaultSession.setPermissionRequestHandler((_contents, _permission, callback) => callback(false));
  serveSite();
  serveLinks();
  serveSessions();
  openWindow();
  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) openWindow();
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});
