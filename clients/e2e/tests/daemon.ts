// A real daemon with its own temporary data. No Claude, no Tailscale, nothing of this machine's.

import { type ChildProcess, spawn } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync, readlinkSync, rmSync, writeFileSync } from "node:fs";
import { createServer as createHttpServer } from "node:http";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, extname, join, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const BIN = process.env.AURELIUS_DAEMON_BIN ?? join(REPO, "target/debug/aurelius-daemon");

export const freePort = () =>
  new Promise<number>((done) => {
    const server = createServer();
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address() as { port: number };
      server.close(() => done(port));
    });
  });

export async function until<T>(
  what: string,
  check: () => T | undefined | null | false | Promise<T | undefined | null | false>,
  ms = 20_000,
): Promise<T> {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const value = await check();
    if (value) return value;
    await new Promise((r) => setTimeout(r, 25));
  }
  throw new Error(`Timed out waiting for ${what}.`);
}

const DIR_PREFIX = "aurelius-e2e-";
const OWNER_FILE = "owner.pid";
const PID_FILE = "daemon.pid";
const SETPRIV = "/usr/bin/setpriv";

/**
 * On Linux the daemon gets SIGTERM when the test process dies, through util-linux `setpriv`
 * (it execs, so the pid stays the daemon's). Without it a killed run (Ctrl-C, a tool's timeout)
 * skips every `finally`, and its daemon lives on under init.
 */
function withParentDeath(bin: string): [string, string[]] {
  if (process.platform !== "linux" || !existsSync(SETPRIV)) return [bin, []];
  return [SETPRIV, ["--pdeathsig", "TERM", "--", bin]];
}

function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function readPid(file: string): number | null {
  try {
    const pid = Number.parseInt(readFileSync(file, "utf8"), 10);
    return pid > 0 ? pid : null;
  } catch {
    return null;
  }
}

/** Whether `pid` still runs an aurelius-daemon, so a reused pid is never killed. */
function isDaemon(pid: number): boolean {
  if (process.platform !== "linux") return alive(pid);
  try {
    return /\/aurelius-daemon( \(deleted\))?$/.test(readlinkSync(`/proc/${pid}/exe`));
  } catch {
    return false;
  }
}

/**
 * Stops the daemons and removes the folders that earlier runs left behind: only those whose
 * owning test process is gone, so another session's run that is still going is left alone.
 * Returns how many folders it removed.
 */
export function cleanLeftovers(root = tmpdir()): number {
  let removed = 0;
  for (const name of readdirSync(root)) {
    if (!name.startsWith(DIR_PREFIX)) continue;
    const dir = join(root, name);
    const owner = readPid(join(dir, OWNER_FILE));
    if (owner !== null && alive(owner)) continue;
    const daemon = readPid(join(dir, PID_FILE));
    if (daemon !== null && isDaemon(daemon)) process.kill(daemon, "SIGTERM");
    rmSync(dir, { recursive: true, force: true });
    removed += 1;
  }
  return removed;
}

export class TestDaemon {
  readonly dir = mkdtempSync(join(tmpdir(), DIR_PREFIX));
  readonly data = join(this.dir, "data");
  readonly socket = join(this.data, "ipc", "daemon.sock");
  port = 0;
  private process: ChildProcess | null = null;

  async start(): Promise<void> {
    this.port ||= await freePort();
    // Who owns this daemon, so a later run can tell a leftover from another session's live test.
    writeFileSync(join(this.dir, OWNER_FILE), String(process.pid));
    const [program, args] = withParentDeath(BIN);
    this.process = spawn(program, args, {
      env: {
        PATH: process.env.PATH,
        HOME: join(this.dir, "home"),
        AURELIUS_DATA_DIR: this.data,
        AURELIUS_ENV: "dev",
        AURELIUS_NO_TAILSCALE: "1",
        AURELIUS_NO_TRAY: "1",
        AURELIUS_NO_AUTO_BACKUP: "1",
        AURELIUS_REMOTE_PORT: String(this.port),
        AURELIUS_WEB_DIR: join(REPO, "clients/web/dist"),
      },
      stdio: "ignore",
    });
    if (this.process.pid) writeFileSync(join(this.dir, PID_FILE), String(this.process.pid));
    await until("the socket", () => existsSync(this.socket));
  }

  get origin(): string {
    return `http://127.0.0.1:${this.port}`;
  }

  async stop(): Promise<void> {
    const running = this.process;
    this.process = null;
    if (!running) return;
    const gone = new Promise((r) => running.once("exit", r));
    running.kill();
    await gone;
  }

  async dispose(): Promise<void> {
    await this.stop();
    rmSync(this.dir, { recursive: true, force: true });
  }
}

const MIME: Record<string, string> = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".woff2": "font/woff2",
};

/** Serves a build as the gateway does: a path that is no file is the app's. */
export async function serveStatic(
  root: string,
  headers: Record<string, string> = {},
): Promise<{ origin: string; close: () => void }> {
  const server = createHttpServer((request, response) => {
    const path = normalize(new URL(request.url ?? "/", "http://x").pathname);
    const asked = join(root, path);
    const file = path !== "/" && existsSync(asked) ? asked : join(root, "index.html");
    response.setHeader("content-type", MIME[extname(file)] ?? "application/octet-stream");
    for (const [name, value] of Object.entries(headers)) response.setHeader(name, value);
    response.end(readFileSync(file));
  });
  await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
  const { port } = server.address() as { port: number };
  return { origin: `http://127.0.0.1:${port}`, close: () => server.close() };
}

export const WEB_DIST = join(REPO, "clients/web/dist");
