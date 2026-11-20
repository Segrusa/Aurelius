import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { boardMemberStep, Connection, type Link, uuidv7 } from "./connection";
import { arrived, LinkError, NotSentError, ProtocolError } from "./errors";

const hello = (over: Record<string, unknown> = {}) => ({
  host_id: "h",
  daemon_version: "1",
  protocol: 5,
  environment: "dev",
  host_name: "PC",
  os: "linux",
  remote: true,
  client_id: null,
  capabilities: ["chat"],
  board_members_exist: true,
  user: { id: "u", username: "Sam" },
  ...over,
});

/** A server in a test: answers the handshake and records what else it gets. */
class FakeLink implements Link {
  sent: Record<string, unknown>[] = [];
  closed = false;
  private onFrame: (text: string) => void = () => {};
  private onClosed: (code: number | null) => void = () => {};
  answers: (request: Record<string, unknown>) => unknown = () => undefined;

  send(text: string): void {
    const request = JSON.parse(text) as Record<string, unknown>;
    this.sent.push(request);
    const ok = this.answers(request) ?? handshake(request);
    if (ok !== undefined)
      queueMicrotask(() => this.frame({ type: "response", request_id: request.request_id, ...(ok as object) }));
  }

  close(): void {
    this.closed = true;
  }

  listen(onFrame: (text: string) => void, onClosed: (code: number | null) => void): void {
    this.onFrame = onFrame;
    this.onClosed = onClosed;
  }

  frame(message: unknown): void {
    this.onFrame(JSON.stringify(message));
  }

  drop(code: number | null = 1006): void {
    this.onClosed(code);
  }

  methods(): unknown[] {
    return this.sent.map((r) => r.method);
  }
}

function handshake(request: Record<string, unknown>): unknown {
  switch (request.method) {
    case "session.hello":
      return { ok: { type: "hello", value: hello() } };
    case "server.get_health":
      return { ok: { type: "health", value: { last_seq: 41 } } };
    case "event.subscribe":
      return { ok: { type: "subscribed" } };
    default:
      return undefined;
  }
}

function connection(links: (FakeLink | Error)[], token: string | null = "tok") {
  const queue = [...links];
  let kept = token;
  const conn = new Connection({
    open: async () => {
      const next = queue.shift();
      if (next === undefined) throw new LinkError("unreachable", "No more links.");
      if (next instanceof Error) throw next;
      return next;
    },
    sessions: {
      get: async () => kept,
      set: async (value) => {
        kept = value;
      },
    },
    clientName: "test",
    clientVersion: "1",
    deviceName: "test device",
  });
  return { conn, token: () => kept };
}

const settle = () => vi.advanceTimersByTimeAsync(0);

describe("Connection", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("says hello with its session and subscribes from the server's last seq", async () => {
    const link = new FakeLink();
    const { conn } = connection([link]);
    conn.start();
    await settle();
    expect(link.methods()).toEqual(["session.hello", "server.get_health", "event.subscribe"]);
    expect(link.sent[0]?.params).toMatchObject({ session_token: "tok" });
    expect(link.sent[2]?.params).toEqual({ after_seq: 41 });
    expect(conn.getState().status).toBe("online");
    expect(boardMemberStep(conn.getState())).toBe("ready");
    conn.dispose();
  });

  it("does not subscribe before the sign-in, and forgets a session the server no longer has", async () => {
    const link = new FakeLink();
    link.answers = (r) =>
      r.method === "session.hello" ? { ok: { type: "hello", value: hello({ user: null }) } } : undefined;
    const { conn, token } = connection([link]);
    conn.start();
    await settle();
    expect(link.methods()).toEqual(["session.hello"]);
    expect(token()).toBeNull();
    expect(boardMemberStep(conn.getState())).toBe("sign_in");
    conn.dispose();
  });

  it("matches answers by request id and throws the server's errors", async () => {
    const link = new FakeLink();
    const { conn } = connection([link]);
    conn.start();
    await settle();
    link.answers = (r) => (r.method === "project.list" ? { error: { code: "forbidden", message: "No." } } : undefined);
    await expect(conn.call("project.list")).rejects.toBeInstanceOf(ProtocolError);
    conn.dispose();
  });

  it("resends an unconfirmed write with the same key after a reconnect", async () => {
    const first = new FakeLink();
    const second = new FakeLink();
    const { conn } = connection([first, second]);
    conn.start();
    await settle();
    const write = conn.write("power.cancel", undefined);
    await settle();
    const key = first.sent.find((r) => r.method === "power.cancel")?.idempotency_key;
    expect(key).toBeTruthy();

    second.answers = (r) =>
      r.method === "power.cancel" ? { ok: { type: "receipt", value: { committed_seq: 1 } } } : undefined;
    first.drop();
    await vi.advanceTimersByTimeAsync(1000);
    await expect(write).resolves.toEqual({ committed_seq: 1 });
    expect(second.sent.find((r) => r.method === "power.cancel")?.idempotency_key).toBe(key);
    conn.dispose();
  });

  it("never resends a write that says so", async () => {
    const link = new FakeLink();
    const { conn } = connection([link]);
    conn.start();
    await settle();
    const write = conn.write("power.cancel", undefined, { retry: false });
    const failed = expect(write).rejects.toMatchObject({ sent: true });
    await settle();
    link.drop();
    await failed;
    conn.dispose();
  });

  it("asks after a reconnect whether a no-retry write arrived before calling it failed", async () => {
    const withFeature = (r: Record<string, unknown>) =>
      r.method === "session.hello"
        ? { ok: { type: "hello", value: hello({ features: ["command_applied"] }) } }
        : undefined;
    const first = new FakeLink();
    const second = new FakeLink();
    first.answers = withFeature;
    const { conn } = connection([first, second]);
    conn.start();
    await settle();
    const write = conn.write("power.cancel", undefined, { retry: false });
    const failed = write.catch((e: unknown) => e);
    await settle();
    const key = first.sent.find((r) => r.method === "power.cancel")?.idempotency_key;
    second.answers = (r) =>
      r.method === "command.applied" ? { ok: { type: "command_applied", value: { applied: true } } } : withFeature(r);
    first.drop();
    await vi.advanceTimersByTimeAsync(1000);
    const error = (await failed) as NotSentError;
    expect(second.sent.find((r) => r.method === "command.applied")?.params).toEqual({ idempotency_key: key });
    expect(second.methods()).not.toContain("power.cancel");
    expect(error).toMatchObject({ reason: "dropped", applied: true });
    expect(arrived(error)).toBe(true);
    expect(error.message).toBe("This arrived, but the server's answer was lost.");
    conn.dispose();
  });

  it("says a dropped write did not arrive when the server has no receipt for it", async () => {
    const withFeature = (r: Record<string, unknown>) =>
      r.method === "session.hello"
        ? { ok: { type: "hello", value: hello({ features: ["command_applied"] }) } }
        : undefined;
    const first = new FakeLink();
    const second = new FakeLink();
    first.answers = withFeature;
    second.answers = (r) =>
      r.method === "command.applied" ? { ok: { type: "command_applied", value: { applied: false } } } : withFeature(r);
    const { conn } = connection([first, second]);
    conn.start();
    await settle();
    const failed = conn.write("power.cancel", undefined, { retry: false }).catch((e: unknown) => e);
    await settle();
    first.drop();
    await vi.advanceTimersByTimeAsync(1000);
    const error = (await failed) as NotSentError;
    expect(error).toMatchObject({ reason: "dropped", applied: false });
    expect(arrived(error)).toBe(false);
    expect(error.message).toContain("Not sent.");
    conn.dispose();
  });

  it("tells a timeout from a dropped link and asks on the same link", async () => {
    const link = new FakeLink();
    link.answers = (r) =>
      r.method === "session.hello"
        ? { ok: { type: "hello", value: hello({ features: ["command_applied"] }) } }
        : r.method === "command.applied"
          ? { ok: { type: "command_applied", value: { applied: true } } }
          : undefined;
    const { conn } = connection([link]);
    conn.start();
    await settle();
    const failed = conn.write("power.cancel", undefined, { retry: false, timeoutMs: 5000 }).catch((e: unknown) => e);
    await vi.advanceTimersByTimeAsync(5000);
    const error = (await failed) as NotSentError;
    expect(error).toMatchObject({ reason: "timed_out", applied: true });
    expect(link.closed).toBe(false);
    conn.dispose();
  });

  it("says a lost write may have arrived when the server cannot be asked", async () => {
    const link = new FakeLink();
    link.answers = (r) =>
      r.method === "session.hello"
        ? { ok: { type: "hello", value: hello({ features: ["command_applied"] }) } }
        : undefined;
    const { conn } = connection([link]);
    conn.start();
    await settle();
    const failed = conn.write("power.cancel", undefined, { retry: false }).catch((e: unknown) => e);
    await settle();
    link.drop();
    await vi.advanceTimersByTimeAsync(20_000);
    const error = (await failed) as NotSentError;
    expect(error).toMatchObject({ reason: "dropped", applied: null });
    expect(error.message).toBe("The connection dropped before the server answered. It may still have arrived.");
    conn.dispose();
  });

  it("does not take a no-retry write while offline, and gives up on the others after a minute", async () => {
    const { conn } = connection([new LinkError("unreachable", "Off.")]);
    conn.start();
    await settle();
    await expect(conn.write("power.cancel", undefined, { retry: false })).rejects.toBeInstanceOf(NotSentError);
    const write = conn.write("power.cancel", undefined);
    const failed = expect(write).rejects.toBeInstanceOf(NotSentError);
    await vi.advanceTimersByTimeAsync(66_000);
    await failed;
    conn.dispose();
  });

  it("stops at a revoked device and at another protocol", async () => {
    const revoked = connection([new LinkError("revoked", "Pair again.")]);
    revoked.conn.start();
    await settle();
    expect(revoked.conn.getState().status).toBe("revoked");
    revoked.conn.dispose();

    const link = new FakeLink();
    link.answers = (r) =>
      r.method === "session.hello" ? { error: { code: "incompatible_protocol", message: "Old." } } : undefined;
    const other = connection([link]);
    other.conn.start();
    await settle();
    expect(other.conn.getState().status).toBe("incompatible");
    other.conn.dispose();
  });

  it("backs off, then tries at once when woken", async () => {
    const link = new FakeLink();
    const { conn } = connection([new LinkError("unreachable", "Off."), new LinkError("unreachable", "Off."), link]);
    conn.start();
    await settle();
    expect(conn.getState().status).toBe("offline");
    expect(conn.getState().retryAtMs).not.toBeNull();
    await vi.advanceTimersByTimeAsync(400);
    conn.wake();
    await settle();
    expect(conn.getState().status).toBe("online");
    conn.dispose();
  });

  it("asks something when all is quiet and gives up on a silent link", async () => {
    const first = new FakeLink();
    const second = new FakeLink();
    const { conn } = connection([first, second]);
    conn.start();
    await settle();
    await vi.advanceTimersByTimeAsync(25_000);
    expect(first.methods()).toContain("user.whoami");
    await vi.advanceTimersByTimeAsync(25_000);
    await vi.advanceTimersByTimeAsync(1000);
    expect(first.closed).toBe(true);
    expect(second.methods()).toContain("session.hello");
    conn.dispose();
  });

  it("goes to the sign-in screen when the server ends the session", async () => {
    const link = new FakeLink();
    const { conn, token } = connection([link]);
    conn.start();
    await settle();
    link.frame({ type: "signed_out", reason: "revoked" });
    await settle();
    expect(token()).toBeNull();
    expect(conn.getState().signedOut).toBe("revoked");
    expect(boardMemberStep(conn.getState())).toBe("sign_in");
    conn.dispose();
  });

  it("follows the Claude login when the server says it changed", async () => {
    const link = new FakeLink();
    const login = { status: "linked", user_id: "u", background: false, email: "sam@example.com" };
    link.answers = (r) =>
      r.method === "claude_login.status" ? { ok: { type: "claude_login", value: login } } : undefined;
    const { conn } = connection([link]);
    conn.start();
    await settle();
    expect(conn.getState().claudeLogin?.status).not.toBe("linked");
    link.frame({ type: "event", seq: 2, kind: "claude_login.updated" });
    await settle();
    expect(conn.getState().claudeLogin).toEqual(login);
    conn.dispose();
  });

  it("passes events and merged streams on, and skips what it cannot read", async () => {
    const link = new FakeLink();
    const { conn } = connection([link]);
    const seen: string[] = [];
    conn.events.on((e) => seen.push(e.kind));
    conn.streams.on((s) => seen.push(s.text));
    conn.start();
    await settle();
    const ids = { run_id: "r", conversation_id: "c", message_id: "m" };
    link.frame({ type: "stream_append", ...ids, offset: 0, text: "Hi" });
    link.frame({ type: "stream_append", ...ids, offset: 2, text: "!" });
    link.frame({ type: "event", kind: "task.updated", seq: 42 });
    link.frame({ type: "from_a_newer_server" });
    expect(seen).toEqual(["Hi", "Hi!", "task.updated"]);
    conn.dispose();
  });
});

describe("uuidv7", () => {
  it("is a version 7 id in time order", () => {
    const id = uuidv7(1_700_000_000_000);
    expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(id.slice(0, 13).replace("-", "")).toBe((1_700_000_000_000).toString(16).padStart(12, "0"));
    expect(uuidv7(1_700_000_000_001) > id).toBe(true);
  });
});
