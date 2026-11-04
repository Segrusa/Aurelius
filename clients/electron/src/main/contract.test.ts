// The client against a real daemon, over its socket and as a browser through the gateway. Needs
// AURELIUS_DAEMON_BIN (`pnpm test:contract`); its own temporary data, no Claude, no Tailscale.

import { type ChildProcess, spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Connection, expectBody, type Link, type OpenLink, ProtocolError } from "@aurelius/client";
import type { Event } from "@aurelius/protocol";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { openPipe } from "./pipe";

const bin = process.env.AURELIUS_DAEMON_BIN;

const freePort = () =>
  new Promise<number>((resolve) => {
    const server = createServer();
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address() as { port: number };
      server.close(() => resolve(port));
    });
  });

async function until<T>(what: string, check: () => T | undefined | null | false, ms = 15_000): Promise<T> {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const value = check();
    if (value) return value;
    await new Promise((r) => setTimeout(r, 25));
  }
  throw new Error(`Timed out waiting for ${what}.`);
}

function memorySession() {
  let token: string | null = null;
  return {
    get: async () => token,
    set: async (next: string | null) => {
      token = next;
    },
  };
}

const client = (open: OpenLink) =>
  new Connection({
    open,
    sessions: memorySession(),
    clientName: "contract",
    clientVersion: "0",
    deviceName: "contract",
  });

const online = (conn: Connection) => until("online", () => conn.getState().status === "online");

/** A WebSocket as a page on `origin` opens it: the browser adds these two headers by itself. */
function browserSocket(url: string, origin: string, cookie: string): Promise<Link> {
  return new Promise((resolve, reject) => {
    const WS = WebSocket as unknown as new (url: string, options: { headers: Record<string, string> }) => WebSocket;
    const socket = new WS(url, { headers: { origin, cookie } });
    socket.onerror = () => reject(new Error("The upgrade was refused."));
    socket.onopen = () =>
      resolve({
        send: (text) => socket.send(text),
        close: () => socket.close(),
        listen: (onFrame, onClosed) => {
          socket.onmessage = (event) => onFrame(String(event.data));
          socket.onclose = (event) => onClosed(event.code);
        },
      });
  });
}

describe.skipIf(!bin)("a real daemon", () => {
  let daemon: ChildProcess;
  let dir: string;
  let port: number;
  let desktop: Connection;

  beforeAll(async () => {
    dir = mkdtempSync(join(tmpdir(), "aurelius-contract-"));
    port = await freePort();
    daemon = spawn(bin as string, [], {
      env: {
        PATH: process.env.PATH,
        HOME: join(dir, "home"),
        AURELIUS_DATA_DIR: join(dir, "data"),
        AURELIUS_ENV: "dev",
        AURELIUS_NO_TAILSCALE: "1",
        AURELIUS_REMOTE_PORT: String(port),
      },
      stdio: "ignore",
    });
    const socket = join(dir, "data", "ipc", "daemon.sock");
    await until("the socket", () => existsSync(socket));
    desktop = client(() => openPipe(socket));
    desktop.start();
    await online(desktop);
  }, 30_000);

  afterAll(() => {
    desktop?.dispose();
    daemon?.kill();
    if (dir) rmSync(dir, { recursive: true, force: true });
  });

  it("creates the first board member over its socket, signs in, and gets events", async () => {
    expect(desktop.getState().boardMembersExist).toBe(false);
    expect(desktop.getState().remote).toBe(false);
    await expect(desktop.call("project.list")).rejects.toBeInstanceOf(ProtocolError);

    await desktop.write("user.create", { username: "sam", password: "a-long-enough-password" }, { retry: false });
    const login = await desktop.signIn("sam", "a-long-enough-password");
    expect(login.user.username).toBe("sam");
    expect(desktop.getState().user?.username).toBe("sam");

    const events: Event[] = [];
    const stop = desktop.events.on((event) => events.push(event));
    await desktop.write("project.create", { name: "Contract" });
    await until("the event", () => events.some((e) => e.kind.startsWith("project.")));
    stop();
    const projects = expectBody(await desktop.call("project.list"), "projects").value;
    expect(projects.projects.map((p) => p.name)).toContain("Contract");
  });

  it("answers a repeated write from its receipt", async () => {
    const key = "01900000-0000-7000-8000-00000000abcd";
    const first = await desktop.write("project.create", { name: "Once" }, { idempotencyKey: key });
    const again = await desktop.write("project.create", { name: "Once" }, { idempotencyKey: key });
    expect(first.replayed).toBe(false);
    expect(again.replayed).toBe(true);
  });

  it("pairs a browser with cookies and works over the gateway", async () => {
    await desktop.write("remote.enable", undefined);
    const start = await desktop.write("remote.start_pairing", undefined);
    if (start.result.kind !== "host" || start.result.value.type !== "pairing_code") throw new Error("expected a code");
    const { code } = start.result.value;

    const origin = `http://127.0.0.1:${port}`;
    const pair = await fetch(`${origin}/v1/pair`, {
      method: "POST",
      headers: { "content-type": "application/json", origin },
      body: JSON.stringify({ code, device_name: "contract browser", platform: "web" }),
    });
    expect(pair.status).toBe(202);
    const { pairing_id } = (await pair.json()) as { pairing_id: string };
    await desktop.write("remote.decide_pairing", { pairing_id, approve: true, owner: false }, { retry: false });

    const approved = await fetch(`${origin}/v1/pair/${pairing_id}`);
    const body = (await approved.json()) as Record<string, unknown>;
    expect(body.status).toBe("approved");
    expect(body).not.toHaveProperty("device_secret");
    const device = (approved.headers.getSetCookie()[0] as string).split(";")[0] as string;

    const session = await fetch(`${origin}/v1/session`, { method: "POST", headers: { origin, cookie: device } });
    expect(session.status).toBe(200);
    expect(await session.json()).not.toHaveProperty("token");
    const cookie = (session.headers.getSetCookie()[0] as string).split(";")[0] as string;

    const url = `ws://127.0.0.1:${port}/v1/ws`;
    await expect(browserSocket(url, "https://evil.example", cookie)).rejects.toThrow(/refused/);

    const browser = client(() => browserSocket(url, origin, cookie));
    browser.start();
    await online(browser);
    expect(browser.getState().remote).toBe(true);
    expect(browser.getState().user).toBeNull();
    // The heartbeat must work before the sign-in, or the watchdog would drop a sign-in screen.
    await expect(browser.call("user.whoami")).resolves.toBeTruthy();
    await browser.signIn("sam", "a-long-enough-password");
    expect(browser.getState().capabilities).not.toContain("owner");

    const projects = expectBody(await browser.call("project.list"), "projects").value;
    expect(projects.projects.map((p) => p.name)).toContain("Contract");

    // A change on the desktop reaches the browser as an event.
    const events: Event[] = [];
    browser.events.on((event) => events.push(event));
    await desktop.write("project.create", { name: "Seen by the browser" });
    await until("the event in the browser", () => events.some((e) => e.kind.startsWith("project.")));
    browser.dispose();
  }, 30_000);
});
