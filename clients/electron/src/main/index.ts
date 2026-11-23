// The main process: the window, the pipe to the local daemon, the session in the OS keychain. The
// page has no Node, reaches only the few calls below, and is checked on every one of them.

import { execFile } from "node:child_process";
import { createHmac, randomBytes } from "node:crypto";
import {
  closeSync,
  existsSync,
  fchmodSync,
  fsyncSync,
  mkdirSync,
  openSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
  writeSync,
} from "node:fs";
import { hostname } from "node:os";
import { basename, dirname, isAbsolute, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { type Link, LinkError } from "@aurelius/client";
import {
  app,
  BrowserWindow,
  clipboard,
  dialog,
  ipcMain,
  net,
  protocol,
  safeStorage,
  screen,
  session,
  shell,
  type WebContents,
} from "electron";
import {
  ARTIFACT_SCHEME,
  type ArtifactExport,
  ArtifactStore,
  captureHeight,
  EXPORT_HEIGHT_DEFAULT,
  exportFailure,
  exportName,
  exportWidth,
} from "./artifact";
import { ArtifactViews, artifactSession, pageHeight, renderArtifact, viewSlot } from "./artifact-session";
import { connectOrStart, daemonProgram, installDir, startDaemon } from "./daemon";
import { appendLinkLog, linkLine, PageWatch } from "./link-life";
import { linkInArguments, linkOf } from "./links";
import { runnable } from "./open-file";
import { daemonEndpoint, dataDir, openPipe } from "./pipe";
import { type Box, MIN_HEIGHT, MIN_WIDTH, placement } from "./placement";
import { openRemote, openSession, requestPairing, uploadFile } from "./remote";
import { DeviceSecrets, sessionRef, validHostId } from "./secrets";
import {
  loadServers,
  type PairedServer,
  pairingPlatform,
  parsePairing,
  type ServerList,
  saveServers,
  urlProblem,
  withoutServer,
  withPaired,
} from "./servers";
import { siteFile } from "./site";

const here = dirname(fileURLToPath(import.meta.url));
const SITE = join(here, "../renderer");
const ORIGIN = "app://aurelius";
const CSP =
  "default-src 'self'; style-src 'self' 'sha256-K63bSR1LetGtaTb/2K6EIsLtf78i2//qEkOoBZCNcBQ=' 'sha256-0KUt1GaAbAKiIczMgRikTmhynZoYPcw+0bnDtqcsh14='; img-src 'self' data: blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'";
const DEV_URL = (!app.isPackaged && process.env.ELECTRON_RENDERER_URL) || undefined;
const ENVIRONMENT =
  process.env.AURELIUS_ENV === "production" || (app.isPackaged && process.env.AURELIUS_ENV !== "dev")
    ? "production"
    : "dev";

/** One second process per environment gives its link to the first and ends. */
if (!app.requestSingleInstanceLock()) app.exit(0);

// No DNS prefetch anywhere: CSP has no directive for `<link rel="dns-prefetch">`, so an artifact
// could leak data in a host name it has resolved. The app itself never needs it. Before ready.
app.commandLine.appendSwitch("blink-settings", "dnsPrefetchingEnabled=false");

const ROOT = dataDir(process.env, ENVIRONMENT);
const INSTALL = installDir(process.execPath, app.isPackaged);
const PROGRAM = daemonProgram(process.env, INSTALL);
const UPLOAD_MAX_BYTES = 25 * 1024 * 1024;

const osCrypt = {
  available: encryptedStorage,
  encrypt: (text: string) => safeStorage.encryptString(text),
  decrypt: (data: Buffer) => safeStorage.decryptString(data),
};

const deviceSecrets = () =>
  new DeviceSecrets(join(app.getPath("userData"), "servers"), ROOT ?? app.getPath("userData"), ENVIRONMENT, osCrypt);

function encryptedStorage(): boolean {
  return safeStorage.isEncryptionAvailable() && safeStorage.getSelectedStorageBackend?.() !== "basic_text";
}

const serverList = (): ServerList => (ROOT === null ? { active: null, servers: [] } : loadServers(ROOT));

/** The server this device works with now; null: this computer's own service. */
function activeServer(): PairedServer | null {
  const list = serverList();
  return list.servers.find((s) => s.host_id === list.active) ?? null;
}

const remoteFetch: typeof fetch = (input, init) => net.fetch(input instanceof URL ? input.href : input, init);

protocol.registerSchemesAsPrivileged([
  { scheme: "app", privileges: { standard: true, secure: true, supportFetchAPI: true } },
  // No fetch, no CORS, no service workers: an artifact only ever loads as a document.
  { scheme: ARTIFACT_SCHEME, privileges: { standard: true, secure: true } },
]);

/** The artifacts the pages handed over, in memory only (artifact.ts). */
const artifacts = new ArtifactStore();

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

/** The artifact session and the viewers in it; artifacts never load in the app's own session. */
function serveArtifacts(): void {
  const ready = artifactSession(artifacts);
  const views = new ArtifactViews(artifacts, ready);
  const watched = new Set<number>();
  handle("artifact:serve", (sender, html: unknown) => {
    if (typeof html !== "string") throw new Error("Not an artifact.");
    // A window that closes or reloads lets go of all it asked for.
    if (!watched.has(sender.id)) {
      watched.add(sender.id);
      const id = sender.id;
      sender.on("did-navigate", () => artifacts.releaseAll(id));
      sender.once("destroyed", () => {
        artifacts.releaseAll(id);
        watched.delete(id);
      });
    }
    return artifacts.serve(sender.id, html);
  });
  handle("artifact:release", (sender, url: unknown) => {
    if (typeof url === "string") artifacts.release(sender.id, url);
  });
  // The viewer: a view over the page's stage, which the page keeps up to date with its bounds.
  handle("artifact:show", (sender, url: unknown, bounds: unknown, slot: unknown) => {
    if (typeof url !== "string") throw new Error("Not an artifact.");
    return views.show(sender, url, bounds, viewSlot(slot));
  });
  handle("artifact:bounds", (sender, bounds: unknown, slot: unknown) => views.place(sender, bounds, viewSlot(slot)));
  // Without a slot: every view of the page (as before compare existed).
  handle("artifact:hide", (sender, slot: unknown) =>
    views.hide(sender.id, slot === undefined ? undefined : viewSlot(slot)),
  );
  // Comments: main runs its own code in an isolated world of the view and checks what comes back
  // (artifact-world.ts, artifact-pick.ts). The artifact itself gets no channel.
  // A design being written: its newer HTML, shown in the same document (no new view).
  handle("artifact:update", (sender, html: unknown) => views.update(sender, html));
  handle("artifact:pick", (sender) => views.pick(sender));
  handle("artifact:pick-stop", (sender) => views.stopPick(sender));
  handle("artifact:export", async (sender, url: unknown, kind: unknown, name: unknown, width: unknown) => {
    // Only an artifact this window holds: another window's is not its to export.
    if (typeof url !== "string" || !artifacts.owns(sender.id, url))
      throw new Error("This artifact is not open any more.");
    if (kind !== "pdf" && kind !== "png") throw new Error("Unknown format.");
    const format: ArtifactExport = kind;
    const window = BrowserWindow.fromWebContents(sender);
    const options = {
      defaultPath: join(app.getPath("downloads"), exportName(typeof name === "string" ? name : "", format)),
      filters: [format === "pdf" ? { name: "PDF", extensions: ["pdf"] } : { name: "PNG image", extensions: ["png"] }],
    };
    const chosen = await (window ? dialog.showSaveDialog(window, options) : dialog.showSaveDialog(options));
    if (chosen.canceled || !chosen.filePath) return null;
    const wide = exportWidth(width);
    try {
      const bytes = await renderArtifact(ready, url, wide, async (hidden) => {
        if (format === "pdf") return hidden.webContents.printToPDF({ printBackground: true, preferCSSPageSize: true });
        // The whole page, not only the first screen. The number comes from the artifact: checked.
        // The width is the one asked for: a window nobody sees does not always report its own.
        const scale = screen.getDisplayMatching(hidden.getBounds()).scaleFactor;
        const tall = captureHeight(await pageHeight(hidden.webContents), EXPORT_HEIGHT_DEFAULT, wide, scale);
        hidden.setContentSize(wide, tall);
        await new Promise((resolve) => setTimeout(resolve, 150));
        return (await hidden.webContents.capturePage({ x: 0, y: 0, width: wide, height: tall })).toPNG();
      });
      writeFileSync(chosen.filePath, bytes);
      return chosen.filePath;
    } catch (error) {
      // The page gets a clear sentence; Chromium's or the file system's own text goes to the log.
      const detail = error instanceof Error ? `${error.name}: ${error.message}` : String(error);
      appendLinkLog(
        join(app.getPath("logs"), "artifacts.log"),
        `${new Date().toISOString()} export failed format=${format} width=${wide} error=${JSON.stringify(detail)}`,
      );
      throw new Error(exportFailure(error));
    }
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
  const links = new Map<number, { link: Link; owner: number; openedAt: number; detach: () => void }>();
  let nextId = 1;
  // When and why each link ended: a dropped link can only be pieced together afterwards.
  const logFile = join(app.getPath("logs"), "links.log");
  const log = (id: number, event: "opened" | "closed", detail: Record<string, string | number>) =>
    appendLinkLog(logFile, linkLine(new Date(), id, event, detail));

  /** Takes the link out first, so its own close event finds nothing to report twice. */
  const forget = (id: number, reason: string) => {
    const entry = links.get(id);
    if (entry === undefined) return null;
    links.delete(id);
    entry.detach();
    log(id, "closed", { reason, open_ms: Date.now() - entry.openedAt });
    return entry;
  };
  const close = (id: number, reason: string) => forget(id, reason)?.link.close();

  const connect = async (root: string): Promise<Link> => {
    const server = activeServer();
    if (server === null) {
      const endpoint = daemonEndpoint(root);
      return connectOrStart(() => openPipe(endpoint), PROGRAM === null ? null : () => startDaemon(PROGRAM, root));
    }
    const problem = urlProblem(server.url);
    if (problem !== null) throw new LinkError("unreachable", problem);
    const secret = deviceSecrets().get(server.host_id);
    if (secret === null) {
      throw new LinkError("revoked", `This computer has no key for ${server.host_name} any more. Pair it again.`);
    }
    return openRemote(server.url, await openSession(server, secret, remoteFetch));
  };

  handle("link:open", async (sender) => {
    const root = ROOT;
    if (root === null) {
      return { ok: false as const, kind: "unreachable", message: "The data folder of this user was not found." };
    }
    try {
      const link = await connect(root);
      const id = nextId++;
      // A reload makes a new page in the same contents; the old page's link goes with it. Only a page
      // change that goes through counts: one that will-navigate blocks must not cut the link.
      const watch = new PageWatch();
      const started = (details: { isSameDocument: boolean; isMainFrame: boolean }) => watch.started(details);
      const committed = () => {
        if (watch.committed()) close(id, "page changed");
      };
      const destroyed = () => close(id, "window closed");
      sender.on("did-start-navigation", started);
      sender.on("did-navigate", committed);
      sender.once("destroyed", destroyed);
      const detach = () => {
        if (sender.isDestroyed()) return;
        sender.off("did-start-navigation", started);
        sender.off("did-navigate", committed);
        sender.off("destroyed", destroyed);
      };
      links.set(id, { link, owner: sender.id, openedAt: Date.now(), detach });
      log(id, "opened", { server: activeServer() === null ? "local" : "remote" });
      link.listen(
        (text) => {
          if (!sender.isDestroyed()) sender.send("link:frame", id, text);
        },
        (code) => {
          if (forget(id, code === null ? "server closed" : `server closed (${code})`) === null) return;
          if (!sender.isDestroyed()) sender.send("link:closed", id, code);
        },
      );
      return { ok: true as const, id };
    } catch (error) {
      return {
        ok: false as const,
        kind: error instanceof LinkError ? error.kind : "unreachable",
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
  handle("link:close", (sender, id: unknown, reason: unknown) => {
    if (own(sender, id)) close(id as number, typeof reason === "string" ? reason.slice(0, 80) : "page asked");
  });
}

/** A secret on the clipboard goes after a minute, and at quit, unless something else was copied since. */
function serveDesktop(): void {
  const salt = randomBytes(32);
  const digest = (text: string) => createHmac("sha256", salt).update(text).digest();
  let held: Buffer | null = null;
  let timer: NodeJS.Timeout | null = null;
  const clearIfOurs = async () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
    const ours = held;
    held = null;
    if (ours !== null && digest(await clipboard.readText()).equals(ours)) await clipboard.writeText("");
  };
  handle("clipboard:write", (_sender, text: unknown) => {
    if (typeof text === "string") return clipboard.writeText(text);
  });
  handle("clipboard:write-secret", async (_sender, text: unknown) => {
    if (typeof text !== "string") throw new Error("Not text.");
    if (timer !== null) clearTimeout(timer);
    await clipboard.writeText(text);
    held = digest(text);
    timer = setTimeout(() => void clearIfOurs(), 60_000);
    return { sure: true };
  });
  app.on("before-quit", (event) => {
    if (held === null) return;
    event.preventDefault();
    void clearIfOurs().finally(() => app.quit());
  });

  handle("file:save-secret", async (sender, name: unknown, content: unknown) => {
    if (typeof name !== "string" || typeof content !== "string") throw new Error("Not a file.");
    const window = BrowserWindow.fromWebContents(sender);
    const options = { defaultPath: join(app.getPath("home"), basename(name)) };
    const chosen = await (window ? dialog.showSaveDialog(window, options) : dialog.showSaveDialog(options));
    if (chosen.canceled || !chosen.filePath) return null;
    const file = openSync(chosen.filePath, "w", 0o600);
    try {
      // Also over a file that was there, with other rights.
      fchmodSync(file, 0o600);
      writeSync(file, `${content}\n`);
      fsyncSync(file);
    } finally {
      closeSync(file);
    }
    return { path: chosen.filePath, private: true };
  });

  handle("folder:pick", async (sender, title: unknown) => {
    const window = BrowserWindow.fromWebContents(sender);
    const options = {
      title: typeof title === "string" ? title : "Choose a folder",
      properties: ["openDirectory", "createDirectory"] as ("openDirectory" | "createDirectory")[],
    };
    const chosen = await (window ? dialog.showOpenDialog(window, options) : dialog.showOpenDialog(options));
    return chosen.canceled ? null : (chosen.filePaths[0] ?? null);
  });
  handle("files:pick", async (sender) => {
    const window = BrowserWindow.fromWebContents(sender);
    const options = {
      title: "Add files",
      properties: ["openFile", "multiSelections"] as ("openFile" | "multiSelections")[],
    };
    const chosen = await (window ? dialog.showOpenDialog(window, options) : dialog.showOpenDialog(options));
    if (chosen.canceled) return [];
    for (const path of chosen.filePaths) {
      const size = statSync(path).size;
      if (size > UPLOAD_MAX_BYTES) {
        throw new Error(`${basename(path)} is ${Math.ceil(size / 1024 / 1024)} MB; files can be at most 25 MB.`);
      }
    }
    const server = activeServer();
    if (server === null) return chosen.filePaths.map((path) => ({ path, name: basename(path) }));
    const secret = deviceSecrets().get(server.host_id);
    if (secret === null) throw new Error(`This computer has no key for ${server.host_name} any more.`);
    const token = await openSession(server, secret, remoteFetch);
    const uploaded = [];
    for (const path of chosen.filePaths) {
      uploaded.push(await uploadFile(server.url, token, basename(path), readFileSync(path), remoteFetch));
    }
    return uploaded;
  });
  // A pasted image or a dropped file with no path here: a copy, or straight to the server's inbox.
  const pasted = join(app.getPath("userData"), "pasted");
  rmSync(pasted, { recursive: true, force: true });
  handle("files:store", async (_sender, name: unknown, bytes: unknown) => {
    if (typeof name !== "string" || !(bytes instanceof ArrayBuffer)) throw new Error("Not a file.");
    if (bytes.byteLength > UPLOAD_MAX_BYTES) throw new Error(`${basename(name)} is larger than 25 MB.`);
    const safe = basename(name.replaceAll("\\", "/")) || "file";
    const server = activeServer();
    if (server === null) {
      const folder = join(pasted, randomBytes(8).toString("hex"));
      mkdirSync(folder, { recursive: true });
      const path = join(folder, safe);
      writeFileSync(path, Buffer.from(bytes), { mode: 0o600 });
      return { path, name: safe };
    }
    const secret = deviceSecrets().get(server.host_id);
    if (secret === null) throw new Error(`This computer has no key for ${server.host_name} any more.`);
    const token = await openSession(server, secret, remoteFetch);
    return uploadFile(server.url, token, safe, new Uint8Array(bytes), remoteFetch);
  });
  // Only a folder: this must never start a program.
  handle("folder:open", async (_sender, path: unknown) => {
    if (typeof path !== "string" || !statSync(path).isDirectory()) throw new Error("Not a folder.");
    const failure = await shell.openPath(path);
    if (failure) throw new Error(failure);
  });
  // A file a run wrote opens in its own app; one that could run is only shown in its folder.
  handle("file:open", async (_sender, path: unknown): Promise<"opened" | "shown"> => {
    if (typeof path !== "string" || !isAbsolute(path)) throw new Error("Not a file.");
    const info = statSync(path, { throwIfNoEntry: false });
    if (info === undefined) throw new Error("That file is no longer there.");
    if (!info.isFile()) throw new Error("Not a file.");
    if (runnable(path, info.mode, process.platform)) {
      shell.showItemInFolder(path);
      return "shown";
    }
    const failure = await shell.openPath(path);
    if (failure) throw new Error(failure);
    return "opened";
  });
}

/** Per server, like a device secret. The Rust client's key for this computer's own service is `local`. */
function serveSessions(): void {
  const sessions = new DeviceSecrets(
    join(app.getPath("userData"), "sessions"),
    ROOT ?? app.getPath("userData"),
    ENVIRONMENT,
    osCrypt,
    process.platform,
    sessionRef,
  );
  const target = () => activeServer()?.host_id ?? "local";

  handle("session:get", () => sessions.get(target()));
  handle("session:set", (_sender, token: unknown) => {
    if (typeof token === "string") sessions.set(target(), token);
    else sessions.delete(target());
  });
  handle("app:info", (sender) => {
    const server = activeServer();
    return {
      version: app.getVersion(),
      deviceName: hostname(),
      os: process.platform,
      target: server && { hostId: server.host_id, environment: server.environment },
      windowProject: projectOfWindow(sender),
    };
  });
  handle("app:desktop-prefs", () => {
    const root = ROOT;
    return {
      uiJson: root === null ? null : readSmallFile(join(root, "ui.json")),
      screenUseJson: root === null ? null : readSmallFile(join(root, "screen_use.json")),
    };
  });
  handle("app:quit", () => app.quit());
}

function reloadWindows(): void {
  for (const window of BrowserWindow.getAllWindows()) window.webContents.reload();
}

function serveServers(): void {
  let pairing: AbortController | null = null;
  const need = (): string => {
    if (ROOT === null) throw new Error("The data folder of this user was not found.");
    return ROOT;
  };

  handle("servers:list", () => {
    const list = serverList();
    return {
      active: list.active,
      localName: list.local_name ?? null,
      localAvailable: !app.isPackaged || PROGRAM !== null,
      servers: list.servers.map((s) => ({
        hostId: s.host_id,
        hostName: s.host_name,
        url: s.url,
        lastUsedMs: s.last_used_ms,
      })),
    };
  });
  handle("servers:use", (_sender, hostId: unknown) => {
    const root = need();
    const list = loadServers(root);
    if (hostId !== null && !list.servers.some((s) => s.host_id === hostId)) throw new Error("Unknown server.");
    if (hostId === null && app.isPackaged && PROGRAM === null) {
      throw new Error("This install has no background service of its own. Pair a server.");
    }
    saveServers(root, {
      ...list,
      active: hostId as string | null,
      servers: list.servers.map((s) => (s.host_id === hostId ? { ...s, last_used_ms: Date.now() } : s)),
    });
    reloadWindows();
  });
  handle("servers:pair", async (_sender, text: unknown) => {
    const root = need();
    const payload = typeof text === "string" ? parsePairing(text) : null;
    if (payload === null || !validHostId(payload.host_id)) throw new Error("This is not a pairing text.");
    if (payload.env !== ENVIRONMENT) {
      throw new Error(`This pairing text is for the ${payload.env} Aurelius; this app is ${ENVIRONMENT}.`);
    }
    const problem = urlProblem(payload.url);
    if (problem !== null) throw new Error(problem);
    pairing?.abort();
    const abort = new AbortController();
    pairing = abort;
    try {
      const approved = await requestPairing(payload, hostname(), pairingPlatform(), abort.signal, remoteFetch);
      deviceSecrets().set(approved.host_id, approved.device_secret);
      saveServers(
        root,
        withPaired(loadServers(root), {
          host_id: approved.host_id,
          host_name: approved.host_name,
          url: payload.url,
          client_id: approved.client_id,
          environment: approved.environment,
          last_used_ms: Date.now(),
        }),
      );
    } catch (error) {
      if (abort.signal.aborted) return { cancelled: true };
      throw error;
    } finally {
      if (pairing === abort) pairing = null;
    }
    reloadWindows();
    return { cancelled: false };
  });
  handle("servers:cancel-pair", () => pairing?.abort());
  handle("servers:forget", (_sender, hostId: unknown) => {
    const root = need();
    if (!validHostId(hostId)) throw new Error("Unknown server.");
    const list = loadServers(root);
    saveServers(root, withoutServer(list, hostId));
    deviceSecrets().delete(hostId);
    rmSync(join(app.getPath("userData"), "sessions", `${hostId}.bin`), { force: true });
    if (list.active === hostId) reloadWindows();
  });
}

/** A link waits here until a page is signed in and asks; it also survives a change of server. */
let heldLink: string | null = linkInArguments(process.argv);

function receiveLink(url: string): void {
  const link = linkOf(url);
  if (link === null) return;
  heldLink = link;
  const window = mainWindow;
  if (window === null || window.isDestroyed()) return;
  if (window.isMinimized()) window.restore();
  window.focus();
  window.webContents.send("links:arrived");
}

function serveOsLinks(): void {
  handle("links:take", () => {
    const link = heldLink;
    heldLink = null;
    return link;
  });
  handle("links:hold", (_sender, url: unknown) => {
    if (typeof url === "string" && linkOf(url) !== null) heldLink = url;
  });
}

function serveStartup(): void {
  const ctl = INSTALL && process.platform === "win32" ? join(INSTALL, "aurelius-ctl.exe") : null;
  const run = (word: "on" | "off" | "status") =>
    new Promise<boolean>((resolve, reject) => {
      if (ctl === null) {
        reject(new Error("Start at sign-in is only for an installed service on Windows."));
        return;
      }
      execFile(ctl, ["autostart", word], { windowsHide: true, timeout: 10_000 }, (error, stdout) => {
        if (error) reject(new Error(String(stdout).trim() || "This did not work."));
        else resolve(/: on\b/.test(stdout));
      });
    });
  handle("startup:available", () => ctl !== null && existsSync(ctl));
  handle("startup:get", () => run("status"));
  handle("startup:set", (_sender, on: unknown) => run(on === true ? "on" : "off"));
}

/** The contents of a small text file, or null: these are preferences, never worth failing over. */
function readSmallFile(path: string): string | null {
  try {
    return statSync(path).size <= 1024 * 1024 ? readFileSync(path, "utf8") : null;
  } catch {
    return null;
  }
}

let mainWindow: BrowserWindow | null = null;
const projectWindows = new Map<string, BrowserWindow>();

function projectOfWindow(sender: WebContents): string | null {
  for (const [id, window] of projectWindows) {
    if (!window.isDestroyed() && window.webContents.id === sender.id) return id;
  }
  return null;
}

const windowsFile = () => join(app.getPath("userData"), "windows.json");

type SavedWindows = Record<string, { bounds: Box; maximized: boolean }>;

function savedWindows(): SavedWindows {
  try {
    return JSON.parse(readFileSync(windowsFile(), "utf8")) as SavedWindows;
  } catch {
    return {};
  }
}

function makeWindow(extra: Electron.BrowserWindowConstructorOptions, path: string): BrowserWindow {
  // Windows and macOS draw their window controls over the page; Linux keeps its window manager's.
  const titleBar: Electron.BrowserWindowConstructorOptions =
    process.platform === "win32"
      ? { titleBarStyle: "hidden", titleBarOverlay: { color: "#00000000", symbolColor: "#A1A1AA", height: 43 } }
      : process.platform === "darwin"
        ? { titleBarStyle: "hidden", trafficLightPosition: { x: 14, y: 14 } }
        : {};
  const window = new BrowserWindow({
    show: false,
    // The design's own background, so no white flash before the first paint.
    backgroundColor: "#08090A",
    autoHideMenuBar: true,
    title: "Aurelius",
    icon: app.isPackaged
      ? join(process.resourcesPath, "aurelius-512.png")
      : join(here, "../../assets/aurelius-512.png"),
    ...titleBar,
    ...extra,
    webPreferences: {
      preload: join(here, "../preload/index.cjs"),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      webSecurity: true,
    },
  });
  window.once("ready-to-show", () => window.show());
  // A link in a report or a message opens in the browser; only a web address leaves the app.
  // Artifacts never reach this: they run in views of their own, whose handler denies everything.
  window.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https?:\/\//i.test(url)) void shell.openExternal(url);
    return { action: "deny" };
  });
  // Never away from the app: a plain web link opens in the browser instead, anything else stays put.
  window.webContents.on("will-navigate", (event, url) => {
    if (fromOurPage(url)) return;
    event.preventDefault();
    if (/^https?:\/\//i.test(url)) void shell.openExternal(url);
  });
  window.loadURL(`${DEV_URL ?? ORIGIN}${path}`).catch((error: unknown) => {
    dialog.showErrorBox("Aurelius could not start", String(error));
    app.quit();
  });
  return window;
}

function openWindow(): void {
  const window = makeWindow({ width: 1440, height: 900, minWidth: MIN_WIDTH, minHeight: 560 }, "/");
  mainWindow = window;
  // Closing the main window closes the project windows too; the service keeps running.
  window.on("closed", () => {
    mainWindow = null;
    for (const other of projectWindows.values()) other.close();
  });
}

function openProjectWindow(id: string, name: string): void {
  const open = projectWindows.get(id);
  if (open && !open.isDestroyed()) {
    if (open.isMinimized()) open.restore();
    open.focus();
    return;
  }
  const saved = savedWindows()[id];
  const areas = screen.getAllDisplays().map((d) => d.workArea);
  const here = mainWindow ? screen.getDisplayMatching(mainWindow.getBounds()) : screen.getPrimaryDisplay();
  const place = saved ? placement(saved.bounds, areas, here.workArea) : null;
  const window = makeWindow(
    {
      ...(place ? place.bounds : { width: 1180, height: 780 }),
      minWidth: MIN_WIDTH,
      minHeight: MIN_HEIGHT,
      title: `${name} · Aurelius`,
    },
    // The window knows its project by projectWindows, so it starts where the main window does.
    "/",
  );
  // The window's name is the project's, whatever screen the page shows.
  window.on("page-title-updated", (event) => event.preventDefault());
  if (place?.own && saved?.maximized) window.maximize();
  projectWindows.set(id, window);
  let timer: NodeJS.Timeout | null = null;
  const remember = () => {
    if (timer !== null) clearTimeout(timer);
    timer = setTimeout(() => {
      if (window.isDestroyed()) return;
      const all = savedWindows();
      all[id] = { bounds: window.getNormalBounds(), maximized: window.isMaximized() };
      writeFileSync(windowsFile(), JSON.stringify(all));
    }, 400);
  };
  for (const change of ["resize", "move", "maximize", "unmaximize"] as const) window.on(change as "resize", remember);
  window.on("closed", () => {
    if (projectWindows.get(id) === window) projectWindows.delete(id);
  });
}

function serveWindows(): void {
  handle("window:open-project", (_sender, id: unknown, name: unknown) => {
    if (typeof id !== "string" || !/^[A-Za-z0-9-]{1,64}$/.test(id)) throw new Error("Unknown project.");
    openProjectWindow(id, typeof name === "string" ? name.slice(0, 200) : "Project");
  });
  handle("window:title-bar", (sender, dark: unknown) => {
    if (process.platform !== "win32") return;
    BrowserWindow.fromWebContents(sender)?.setTitleBarOverlay({ symbolColor: dark === true ? "#A1A1AA" : "#52525B" });
  });
}

/** The service reads which app version is installed from this file, so it can offer a newer one. */
function recordVersion(): void {
  if (INSTALL === null) return;
  const file = join(INSTALL, "aurelius.version");
  if (readSmallFile(file)?.trim() !== app.getVersion()) {
    try {
      writeFileSync(file, app.getVersion());
    } catch {
      // An update will write it; nothing depends on it now.
    }
  }
}

app.on("second-instance", (_event, args) => {
  const link = linkInArguments(args);
  if (link !== null) receiveLink(link);
  else if (mainWindow) {
    if (mainWindow.isMinimized()) mainWindow.restore();
    mainWindow.focus();
  } else if (app.isReady()) openWindow();
});
app.on("open-url", (event, url) => {
  event.preventDefault();
  receiveLink(url);
});

void app.whenReady().then(() => {
  session.defaultSession.setPermissionRequestHandler((_contents, _permission, callback) => callback(false));
  recordVersion();
  serveSite();
  serveArtifacts();
  serveLinks();
  serveOsLinks();
  serveSessions();
  serveServers();
  serveStartup();
  serveWindows();
  serveDesktop();
  openWindow();
  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) openWindow();
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});
