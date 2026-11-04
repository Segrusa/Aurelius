// A real daemon with its own temporary data. No Claude, no Tailscale, nothing of this machine's.

import { type ChildProcess, spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
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

export class TestDaemon {
  readonly dir = mkdtempSync(join(tmpdir(), "aurelius-e2e-"));
  readonly data = join(this.dir, "data");
  readonly socket = join(this.data, "ipc", "daemon.sock");
  port = 0;
  private process: ChildProcess | null = null;

  async start(): Promise<void> {
    this.port ||= await freePort();
    this.process = spawn(BIN, [], {
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
export async function serveStatic(root: string): Promise<{ origin: string; close: () => void }> {
  const server = createHttpServer((request, response) => {
    const path = normalize(new URL(request.url ?? "/", "http://x").pathname);
    const asked = join(root, path);
    const file = path !== "/" && existsSync(asked) ? asked : join(root, "index.html");
    response.setHeader("content-type", MIME[extname(file)] ?? "application/octet-stream");
    response.end(readFileSync(file));
  });
  await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
  const { port } = server.address() as { port: number };
  return { origin: `http://127.0.0.1:${port}`, close: () => server.close() };
}

export const WEB_DIST = join(REPO, "clients/web/dist");
