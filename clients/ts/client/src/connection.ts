// One live link to one server: hello, sign-in gate, subscription, reconnect with backoff and an
// outbox of unconfirmed writes. It knows nothing of how the bytes travel: a `Link` does that (pipe
// in Electron, WebSocket in Electron and the browser). Follows crates/client and the mobile app's
// lib/core/net/connection.dart.

import {
  type Capability,
  type ClaudeLoginPrompt,
  type ClaudeLoginView,
  type Event,
  encodeRequest,
  type LoadSnapshot,
  type LoginResult,
  type MethodName,
  type MethodParams,
  type PowerState,
  PROTOCOL_MAJOR,
  parseServerMessage,
  type Receipt,
  type RequestInput,
  type ResponseBody,
  type ServerMessage,
  type SideAnswer,
  type SignOutReason,
  type UserView,
  type WriteName,
} from "@aurelius/protocol";
import { DisconnectedError, LinkError, NotSentError, ProtocolError, TimedOutError } from "./errors";
import { StreamMerge, type StreamUpdate } from "./stream";

/** One open connection. Frames are JSON text, whatever carries them. */
export interface Link {
  send(text: string): void;
  /** `reason`: a few words for the transport's log of why this side closed it. */
  close(reason?: string): void;
  /** Set once, right after opening. `code`: the close code, when the transport has one. */
  listen(onFrame: (text: string) => void, onClosed: (code: number | null) => void): void;
}

/** Throws `LinkError`. */
export type OpenLink = () => Promise<Link>;

/** Where the board member's session is kept: a secret of this device. */
export interface SessionStore {
  get(): Promise<string | null>;
  set(token: string | null): Promise<void>;
}

export type LinkStatus = "connecting" | "online" | "offline" | "revoked" | "incompatible";

/** What the sign-in gate shows. */
export type BoardMemberStep = "unknown" | "create_first" | "sign_in" | "change_password" | "ready";

export interface LinkState {
  status: LinkStatus;
  message: string | null;
  lastOnlineMs: number | null;
  retryAtMs: number | null;
  capabilities: Capability[];
  features: string[];
  daemonVersion: string | null;
  hostId: string | null;
  hostName: string | null;
  environment: string | null;
  remote: boolean;
  /** Null until the first hello. */
  boardMembersExist: boolean | null;
  user: UserView | null;
  mustChangePassword: boolean;
  claudeLogin: ClaudeLoginView | null;
  /** The session ended without this device asking; shown once on the sign-in screen. */
  signedOut: SignOutReason | null;
}

export function boardMemberStep(state: LinkState): BoardMemberStep {
  if (state.user) return state.mustChangePassword ? "change_password" : "ready";
  if (state.boardMembersExist === null) return "unknown";
  return state.boardMembersExist ? "sign_in" : "create_first";
}

const INITIAL: LinkState = {
  status: "connecting",
  message: null,
  lastOnlineMs: null,
  retryAtMs: null,
  capabilities: [],
  features: [],
  daemonVersion: null,
  hostId: null,
  hostName: null,
  environment: null,
  remote: false,
  boardMembersExist: null,
  user: null,
  mustChangePassword: false,
  claudeLogin: null,
  signedOut: null,
};

/** What this client can take that older clients cannot (`client_feature`). */
const ACCEPTS = [
  "activity_changes",
  "team_channels",
  "server_name",
  "ticket_channels",
  "chat_threads",
  "chat_reactions",
  "ticket_follow",
  "message_edits",
  "artifact_drafts",
];

const BACKOFF_MS = [250, 500, 1000, 2000, 5000];
const CALL_TIMEOUT_MS = 60_000;
/** Unconfirmed writes are not replayed silently much later. */
const RETRY_WINDOW_MS = 60_000;
/** A browser sees no WebSocket pings, so this client asks something itself when all is quiet. */
const HEARTBEAT_MS = 20_000;
const SILENCE_LIMIT_MS = 45_000;
/** How long a write that lost its answer waits for the link to come back and the server to say whether it arrived. */
const CONFIRM_WAIT_MS = 15_000;
/** The server feature of `command.applied`. */
const COMMAND_APPLIED = "command_applied";

const DIFFERENT_VERSIONS = "Update Aurelius on this device and on the server.";
const UNREACHABLE = "The server is not reachable.";

export type Response<T extends ResponseBody["type"]> = Extract<ResponseBody, { type: T }>;

/** `body` as the answer the method is known to give; anything else is a bug or a newer server. */
export function expectBody<T extends ResponseBody["type"]>(body: ResponseBody, type: T): Response<T> {
  if (body.type !== type) throw new Error(`Expected the answer ${type}, got ${body.type}.`);
  return body as Response<T>;
}

interface Waiting {
  resolve(body: ResponseBody): void;
  reject(error: Error): void;
  timer: ReturnType<typeof setTimeout>;
}

interface Pending {
  method: WriteName;
  params: unknown;
  key: string;
  expectedVersion: number | undefined;
  retry: boolean;
  timeoutMs: number;
  createdAt: number;
  inFlight: boolean;
  sent: boolean;
  resolve(receipt: Receipt): void;
  reject(error: Error): void;
}

export interface WriteOptions {
  /** The same key with the same request is answered from the receipt. */
  idempotencyKey?: string;
  expectedVersion?: number;
  /** False: never resent after a reconnect. Passwords and approvals. */
  retry?: boolean;
  timeoutMs?: number;
}

export interface ConnectionOptions {
  open: OpenLink;
  sessions: SessionStore;
  clientName: string;
  clientVersion: string;
  /** What the sessions list calls this device. */
  deviceName: string;
  /** Refuses a server of another environment, once it is known. */
  environment?: string;
  /** Refuses any other server, or this one without owner rights: a paired computer has full access. */
  expectedHostId?: string;
}

type Listener<T> = (value: T) => void;

class Emitter<T> {
  private readonly listeners = new Set<Listener<T>>();

  on(listener: Listener<T>): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  emit(value: T): void {
    for (const listener of this.listeners) listener(value);
  }
}

class Incompatible extends Error {}

/** A time-ordered id, as the server's own ids are. */
export function uuidv7(now = Date.now()): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  for (let i = 0; i < 6; i++) bytes[i] = Math.floor(now / 2 ** (8 * (5 - i))) & 0xff;
  bytes[6] = ((bytes[6] as number) & 0x0f) | 0x70;
  bytes[8] = ((bytes[8] as number) & 0x3f) | 0x80;
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export class Connection {
  readonly events = new Emitter<Event>();
  readonly streams = new Emitter<StreamUpdate>();
  readonly loads = new Emitter<LoadSnapshot>();
  readonly powers = new Emitter<PowerState>();
  readonly claudeLoginPrompts = new Emitter<ClaudeLoginPrompt>();
  /** Answers to this connection's side questions (`conversation.side_ask`), whole text each time. */
  readonly sideAnswers = new Emitter<SideAnswer>();

  private state: LinkState = INITIAL;
  private readonly stateListeners = new Set<() => void>();
  private readonly waiting = new Map<number, Waiting>();
  private readonly outbox: Pending[] = [];
  private readonly merge = new StreamMerge();
  private link: Link | null = null;
  private onClosed: ((code: number | null) => void) | null = null;
  private wakeUp: (() => void) | null = null;
  private nextId = 1;
  private started = false;
  private disposed = false;
  private subscribed = false;
  private signingOut = false;
  private signedOutNotice: SignOutReason | null = null;
  private lastFrameAt = 0;
  private timers: ReturnType<typeof setInterval>[] = [];

  constructor(private readonly options: ConnectionOptions) {}

  /** For `useSyncExternalStore`: the same object until something changed. */
  getState = (): LinkState => this.state;

  subscribe = (listener: () => void): (() => void) => {
    this.stateListeners.add(listener);
    return () => this.stateListeners.delete(listener);
  };

  start(): void {
    if (this.started) return;
    this.started = true;
    this.timers.push(setInterval(() => this.expireOutbox(), 5000));
    this.timers.push(setInterval(() => this.watch(), 5000));
    void this.run();
  }

  /** The user asked to retry, or the app came back: try now. */
  wake(): void {
    this.wakeUp?.();
  }

  dispose(): void {
    this.disposed = true;
    for (const timer of this.timers) clearInterval(timer);
    this.wake();
    this.link?.close("app closed the connection");
    this.failWaiting();
    for (const pending of this.outbox.splice(0)) pending.reject(new NotSentError(pending.key));
  }

  /** A read or session method. Throws `ProtocolError` or `DisconnectedError`. */
  call<M extends MethodName>(method: M, ...rest: CallArgs<M>): Promise<ResponseBody> {
    const [params, timeoutMs] = rest as [unknown, number | undefined];
    return this.request({ method, params } as never, timeoutMs ?? CALL_TIMEOUT_MS);
  }

  /** A write. Unconfirmed writes are resent with the same key after a reconnect, within a minute. */
  write<M extends WriteName>(method: M, params: MethodParams[M], options: WriteOptions = {}): Promise<Receipt> {
    return new Promise((resolve, reject) => {
      const pending: Pending = {
        method,
        params,
        key: options.idempotencyKey ?? uuidv7(),
        expectedVersion: options.expectedVersion,
        retry: options.retry ?? true,
        timeoutMs: options.timeoutMs ?? CALL_TIMEOUT_MS,
        createdAt: Date.now(),
        inFlight: false,
        sent: false,
        resolve,
        reject,
      };
      if (this.state.status === "online") {
        this.outbox.push(pending);
        void this.send(pending);
      } else if (pending.retry) {
        this.outbox.push(pending);
      } else {
        reject(new NotSentError(pending.key));
      }
    });
  }

  /** Sign in. The password is only sent, never kept. */
  async signIn(username: string, password: string): Promise<LoginResult> {
    const body = await this.call("user.login", {
      username: username.trim(),
      password,
      device_name: this.options.deviceName,
    });
    const login = expectBody(body, "login").value;
    await this.options.sessions.set(login.session_token || null);
    this.signedOutNotice = null;
    if (!login.must_change_password) await this.subscribeEvents();
    this.setState({
      boardMembersExist: true,
      user: login.user,
      mustChangePassword: login.must_change_password ?? false,
      signedOut: null,
    });
    return login;
  }

  /** Change this board member's password. Never resent; a forced change opens the app after it. */
  async setPassword(oldPassword: string | null, newPassword: string): Promise<void> {
    await this.write("user.set_password", { old_password: oldPassword, new_password: newPassword }, { retry: false });
    if (this.state.mustChangePassword) {
      await this.subscribeEvents();
      this.setState({ mustChangePassword: false });
    }
  }

  /** Sign out here: the token is forgotten even when the server cannot be told. */
  async signOut(): Promise<void> {
    this.signingOut = true;
    try {
      await this.call("user.logout");
    } catch (error) {
      const expected =
        error instanceof DisconnectedError || (error instanceof ProtocolError && error.code === "unauthenticated");
      if (!expected) throw error;
    } finally {
      this.signingOut = false;
    }
    await this.endSession(null);
  }

  /** The sign-in screen showed "You were signed out on this device.". */
  dismissSignedOut(): void {
    this.signedOutNotice = null;
    if (this.state.signedOut !== null) this.setState({ signedOut: null });
  }

  /** Reads this board member's Claude login again; everything that shows it follows. */
  async refreshClaudeLogin(): Promise<void> {
    const body = await this.call("claude_login.status");
    this.setState({ claudeLogin: expectBody(body, "claude_login").value });
  }

  private setState(change: Partial<LinkState>): void {
    if (this.disposed) return;
    this.state = { ...this.state, ...change };
    for (const listener of this.stateListeners) listener();
  }

  private async run(): Promise<void> {
    let attempt = 0;
    while (!this.disposed) {
      this.setState({ status: "connecting", retryAtMs: null });
      let closedCode: number | null = null;
      try {
        const closed = new Promise<number | null>((resolve) => {
          this.onClosed = resolve;
        });
        await this.openAndGreet();
        attempt = 0;
        closedCode = await closed;
        if (this.disposed) return;
        this.setState({ status: "offline", message: "Connection lost. Reconnecting…" });
        if (closedCode === 4401) continue; // Revoked or turned off: the next open says which, now.
      } catch (error) {
        this.link?.close(error instanceof Error ? `hello failed: ${error.message}` : "hello failed");
        this.failWaiting();
        if (this.disposed) return;
        if (error instanceof Incompatible) {
          this.setState({ status: "incompatible", message: error.message });
          return;
        }
        if (error instanceof LinkError && error.kind === "revoked") {
          this.setState({ status: "revoked", message: error.message });
          return;
        }
        this.setState({ status: "offline", message: error instanceof LinkError ? error.message : UNREACHABLE });
      }
      const step = BACKOFF_MS[Math.min(attempt++, BACKOFF_MS.length - 1)] as number;
      const delay = step + Math.floor(Math.random() * (step / 4));
      this.setState({ retryAtMs: Date.now() + delay });
      await new Promise<void>((resolve) => {
        const timer = setTimeout(resolve, delay);
        this.wakeUp = () => {
          clearTimeout(timer);
          resolve();
        };
      });
      this.wakeUp = null;
    }
  }

  private async openAndGreet(): Promise<void> {
    const link = await this.options.open();
    this.link = link;
    this.lastFrameAt = Date.now();
    this.subscribed = false;
    this.merge.reset();
    link.listen(
      (text) => this.onFrame(text),
      (code) => this.linkClosed(link, code),
    );

    const token = await this.options.sessions.get();
    let hello: Response<"hello">["value"];
    try {
      hello = await this.hello(token).catch(async (error) => {
        if (!(error instanceof ProtocolError) || error.code !== "unauthenticated" || token === null) throw error;
        // The server refused the kept session: forget it, hello without one.
        await this.options.sessions.set(null);
        return this.hello(null);
      });
    } catch (error) {
      if (error instanceof ProtocolError && error.code === "incompatible_protocol")
        throw new Incompatible(DIFFERENT_VERSIONS);
      throw error;
    }
    if (hello.protocol !== PROTOCOL_MAJOR) throw new Incompatible(DIFFERENT_VERSIONS);
    const { environment, expectedHostId } = this.options;
    if (expectedHostId !== undefined) {
      if (hello.host_id !== expectedHostId) {
        throw new Incompatible(`${hello.host_name} now answers as another Aurelius server. Pair this computer again.`);
      }
      if (!hello.capabilities.includes("owner")) {
        throw new Incompatible(
          `This computer is paired with ${hello.host_name} without full access. Pair it again and choose "Allow as owner".`,
        );
      }
    }
    if (environment !== undefined && hello.environment !== environment) {
      throw new Incompatible(
        `This device is paired with the ${environment} Aurelius, but the server is ${hello.environment}. Pair again.`,
      );
    }
    // A kept session the server no longer has: forget it; the sign-in screen comes.
    if (token !== null && !hello.user) {
      await this.options.sessions.set(null);
      if (this.state.user && hello.board_members_exist) this.signedOutNotice = "expired";
    }
    const user = hello.user ?? null;
    const mustChangePassword = user !== null && (hello.must_change_password ?? false);
    // Without a session the gate answers health and events only after the sign-in.
    if (user !== null && !mustChangePassword) await this.subscribeEvents();
    this.setState({
      status: "online",
      message: null,
      lastOnlineMs: Date.now(),
      retryAtMs: null,
      capabilities: hello.capabilities,
      features: hello.features ?? [],
      daemonVersion: hello.daemon_version,
      hostId: hello.host_id,
      hostName: hello.host_name,
      environment: hello.environment,
      remote: hello.remote,
      boardMembersExist: (hello.board_members_exist ?? false) || user !== null,
      user,
      mustChangePassword,
      claudeLogin: user === null ? null : (hello.claude_login ?? null),
      signedOut: user === null ? this.signedOutNotice : null,
    });
    if (user !== null) this.signedOutNotice = null;
    for (const pending of [...this.outbox]) void this.send(pending);
  }

  private async hello(token: string | null): Promise<Response<"hello">["value"]> {
    const { clientName, clientVersion } = this.options;
    const body = await this.call("session.hello", {
      client_name: clientName,
      client_version: clientVersion,
      accepts: ACCEPTS,
      ...(token === null ? {} : { session_token: token }),
    });
    return expectBody(body, "hello").value;
  }

  private async subscribeEvents(): Promise<void> {
    if (this.subscribed) return;
    const health = expectBody(await this.call("server.get_health"), "health").value;
    await this.call("event.subscribe", { after_seq: health.last_seq });
    this.subscribed = true;
  }

  private async endSession(reason: SignOutReason | null): Promise<void> {
    this.subscribed = false;
    await this.options.sessions.set(null);
    this.signedOutNotice = reason;
    if (this.state.user !== null || reason !== null) {
      this.setState({
        boardMembersExist: true,
        user: null,
        mustChangePassword: false,
        claudeLogin: null,
        signedOut: reason,
      });
    }
  }

  private request(
    input: { method: MethodName; params?: unknown; idempotencyKey?: string; expectedVersion?: number },
    timeoutMs: number,
  ): Promise<ResponseBody> {
    const link = this.link;
    if (link === null) return Promise.reject(new DisconnectedError());
    const requestId = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waiting.delete(requestId);
        reject(new TimedOutError());
      }, timeoutMs);
      this.waiting.set(requestId, { resolve, reject, timer });
      const encoded = encodeRequest({ ...input, requestId } as RequestInput<MethodName>);
      link.send(JSON.stringify(encoded));
    });
  }

  private async send(pending: Pending): Promise<void> {
    if (pending.inFlight) return;
    pending.inFlight = true;
    pending.sent = true;
    try {
      const body = await this.request(
        {
          method: pending.method,
          params: pending.params,
          idempotencyKey: pending.key,
          ...(pending.expectedVersion === undefined ? {} : { expectedVersion: pending.expectedVersion }),
        },
        pending.timeoutMs,
      );
      this.removeFromOutbox(pending);
      pending.resolve(expectBody(body, "receipt").value);
    } catch (error) {
      pending.inFlight = false;
      if (error instanceof DisconnectedError) {
        if (pending.retry) return;
        this.removeFromOutbox(pending);
        await this.rejectLost(pending, error instanceof TimedOutError ? "timed_out" : "dropped");
        return;
      }
      this.removeFromOutbox(pending);
      pending.reject(error as Error);
    }
  }

  /**
   * A write that is never resent lost its answer. Before it is called failed, the server is
   * asked whether it arrived: a message that was stored must not come back as "not sent".
   */
  private async rejectLost(pending: Pending, reason: "dropped" | "timed_out"): Promise<void> {
    let applied: boolean | null = null;
    try {
      // A server without the method (the last hello says so) is not waited for.
      const canAsk = () => this.state.features.includes(COMMAND_APPLIED);
      if (!this.disposed && canAsk() && (await this.online(CONFIRM_WAIT_MS)) && canAsk()) {
        const body = await this.request(
          { method: "command.applied", params: { idempotency_key: pending.key } },
          CONFIRM_WAIT_MS,
        );
        applied = expectBody(body, "command_applied").value.applied;
      }
    } catch {
      // Not known: the message says it may have arrived.
    }
    pending.reject(new NotSentError(pending.key, true, reason, applied));
  }

  /** True once this connection is online again with a link, false after `withinMs` or when it gives up. */
  private online(withinMs: number): Promise<boolean> {
    const ready = () => this.state.status === "online" && this.link !== null;
    if (ready()) return Promise.resolve(true);
    return new Promise((resolve) => {
      const done = (ok: boolean) => {
        clearTimeout(timer);
        off();
        resolve(ok);
      };
      const timer = setTimeout(() => done(false), withinMs);
      const off = this.subscribe(() => {
        if (ready()) done(true);
        else if (this.state.status === "revoked" || this.state.status === "incompatible") done(false);
      });
    });
  }

  private removeFromOutbox(pending: Pending): void {
    const at = this.outbox.indexOf(pending);
    if (at >= 0) this.outbox.splice(at, 1);
  }

  private onFrame(text: string): void {
    this.lastFrameAt = Date.now();
    const message = parseServerMessage(text);
    if (message !== null) this.handle(message);
  }

  private handle(message: ServerMessage): void {
    switch (message.type) {
      case "response": {
        const waiting = this.waiting.get(message.request_id);
        this.waiting.delete(message.request_id);
        if (waiting) clearTimeout(waiting.timer);
        if (message.error) {
          this.onErrorAnswer(message.error.code);
          waiting?.reject(new ProtocolError(message.error));
        } else if (message.ok) {
          waiting?.resolve(message.ok);
        }
        return;
      }
      case "event":
        this.merge.onEvent(message);
        this.events.emit(message);
        // The hello only told what it was then; this is how a login that just worked gets here.
        if (message.kind === "claude_login.updated") void this.refreshClaudeLogin().catch(() => undefined);
        if (this.state.status === "online") this.setState({ lastOnlineMs: Date.now() });
        return;
      case "stream":
        this.streams.emit(this.merge.delta(message));
        return;
      case "stream_append": {
        const update = this.merge.append(message);
        if (update !== null) this.streams.emit(update);
        return;
      }
      case "load_changed":
        this.loads.emit(message);
        return;
      case "power_changed":
        this.powers.emit(message);
        return;
      case "claude_login_prompt":
        this.claudeLoginPrompts.emit(message);
        return;
      case "side_answer":
        this.sideAnswers.emit(message);
        return;
      case "signed_out":
        // A sign-out this device asked for needs no message.
        void this.endSession(this.signingOut && message.reason === "logged_out" ? null : message.reason);
        return;
    }
  }

  private onErrorAnswer(code: string): void {
    if (this.state.user === null) return;
    if (code === "unauthenticated") void this.endSession("expired");
    else if (code === "board_member_required") void this.endSession(null);
    else if (code === "password_change_required") this.setState({ mustChangePassword: true });
  }

  private linkClosed(link: Link, code: number | null): void {
    if (this.link !== link) return;
    this.link = null;
    this.failWaiting();
    this.onClosed?.(code);
    this.onClosed = null;
  }

  private failWaiting(): void {
    const waiting = [...this.waiting.values()];
    this.waiting.clear();
    for (const { reject, timer } of waiting) {
      clearTimeout(timer);
      reject(new DisconnectedError());
    }
  }

  private watch(): void {
    const link = this.link;
    if (link === null || this.state.status !== "online") return;
    const quiet = Date.now() - this.lastFrameAt;
    if (quiet >= SILENCE_LIMIT_MS) {
      link.close(`no answer for ${Math.round(quiet / 1000)} s`);
      this.linkClosed(link, null);
    } else if (quiet >= HEARTBEAT_MS) {
      this.call("user.whoami").catch(() => {});
    }
  }

  private expireOutbox(): void {
    const now = Date.now();
    for (const pending of [...this.outbox]) {
      if (!pending.inFlight && now - pending.createdAt > RETRY_WINDOW_MS) {
        this.removeFromOutbox(pending);
        pending.reject(new NotSentError(pending.key, pending.sent));
      }
    }
  }
}

type CallArgs<M extends MethodName> = MethodParams[M] extends undefined
  ? [params?: undefined, timeoutMs?: number]
  : [params: MethodParams[M], timeoutMs?: number];
