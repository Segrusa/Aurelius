import type { AppError, ErrorCode, GuidelineTooLarge } from "@aurelius/protocol";

/** An error answer of the server. */
export class ProtocolError extends Error {
  readonly code: ErrorCode;
  /** `too_many_attempts`: how long until the server takes the next try. */
  readonly retryAfterMs: number | null;
  /** `looks_like_secret`: the names of the vault items the text holds the value of, never a value. */
  readonly matchedItems: string[];
  /** `guideline_too_large`: the combination that would get too many characters. */
  readonly tooLarge: GuidelineTooLarge | null;

  constructor(error: AppError) {
    super(error.message);
    this.name = "ProtocolError";
    this.code = error.code;
    this.retryAfterMs = error.retry_after_ms ?? null;
    this.matchedItems = error.matched_items ?? [];
    this.tooLarge = error.too_large ?? null;
  }
}

/** The connection dropped before an answer came. A write may or may not have been applied. */
export class DisconnectedError extends Error {
  constructor(message = "The connection to the server was lost.") {
    super(message);
    this.name = "DisconnectedError";
  }
}

/** The link stayed, but no answer came in time: the server may still be working on it. */
export class TimedOutError extends DisconnectedError {
  constructor() {
    super("The server did not answer in time.");
    this.name = "TimedOutError";
  }
}

/** Why a write ended without its answer. */
export type NotSentReason = "unreachable" | "dropped" | "timed_out";

/** A write that will not be retried. Resend with the same key to apply it at most once. */
export class NotSentError extends Error {
  readonly idempotencyKey: string;
  /** It went out before the connection dropped: the server may have applied it. */
  readonly sent: boolean;
  readonly reason: NotSentReason;
  /** What the server said when asked afterwards (`command.applied`); null: it could not be asked. */
  readonly applied: boolean | null;

  constructor(idempotencyKey: string, sent = false, reason?: NotSentReason, applied: boolean | null = null) {
    const why = reason ?? (sent ? "dropped" : "unreachable");
    super(notSentMessage(why, applied));
    this.name = "NotSentError";
    this.idempotencyKey = idempotencyKey;
    this.sent = sent;
    this.reason = why;
    this.applied = applied;
  }
}

function notSentMessage(reason: NotSentReason, applied: boolean | null): string {
  if (reason === "unreachable") return "Not sent. The server was not reachable.";
  if (applied === true) return "This arrived, but the server's answer was lost.";
  if (reason === "timed_out") {
    return applied === false
      ? "Not sent. The server did not answer in time; try again."
      : "The server did not answer in time. It may still have arrived.";
  }
  return applied === false
    ? "Not sent. The connection dropped before the server got it; try again."
    : "The connection dropped before the server answered. It may still have arrived.";
}

/** The write took effect, even though its own answer was lost on the way back. */
export function arrived(error: unknown): boolean {
  return error instanceof NotSentError && error.applied === true;
}

/** Opening the connection failed. */
export class LinkError extends Error {
  /** `session`: ask for a new session. `revoked`: pair again. `unreachable`: try again later. */
  readonly kind: "session" | "revoked" | "unreachable";

  constructor(kind: LinkError["kind"], message: string) {
    super(message);
    this.name = "LinkError";
    this.kind = kind;
  }
}
