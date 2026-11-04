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

/** A write that will not be retried. Resend with the same key to apply it at most once. */
export class NotSentError extends Error {
  readonly idempotencyKey: string;
  /** It went out before the connection dropped: the server may have applied it. */
  readonly sent: boolean;

  constructor(idempotencyKey: string, sent = false) {
    super(sent ? "The connection dropped before the server answered." : "Not sent. The server was not reachable.");
    this.name = "NotSentError";
    this.idempotencyKey = idempotencyKey;
    this.sent = sent;
  }
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
