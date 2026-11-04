// What the schemas do not cover: the request envelope (hand-serialized in crates/protocol/src/rpc.rs)
// and the JSON of the gateway's HTTP routes (crates/remote-gateway/src/http.rs). The envelope is
// tested against wire-samples.json, which the server's own types write.

import {
  type Capability,
  METHODS,
  type MethodName,
  type MethodParams,
  PROTOCOL_MAJOR,
  type ServerMessage,
} from "./generated";

/** A method that changes something: the server needs an idempotency key for it. */
export type WriteName = { [K in MethodName]: (typeof METHODS)[K] extends "read" ? never : K }[MethodName];

type Params<M extends MethodName> = MethodParams[M] extends undefined
  ? { params?: undefined }
  : { params: MethodParams[M] };

type Write<M extends MethodName> = M extends WriteName
  ? {
      /** The same key with the same request is answered from the receipt. */
      idempotencyKey: string;
      /** Optimistic concurrency for an entity with a version. */
      expectedVersion?: number;
    }
  : { idempotencyKey?: undefined; expectedVersion?: undefined };

export type RequestInput<M extends MethodName> = { requestId: number; method: M } & Params<M> & Write<M>;

/** The object to send as JSON. Keys left out are absent on the wire, as the server writes them. */
export function encodeRequest<M extends MethodName>(input: RequestInput<M>): Record<string, unknown> {
  const { requestId, method, params, idempotencyKey, expectedVersion } = input as RequestInput<MethodName> & {
    params?: unknown;
    idempotencyKey?: string;
    expectedVersion?: number;
  };
  if (METHODS[method] !== "read" && idempotencyKey === undefined) {
    throw new Error(`The write ${method} needs an idempotency key.`);
  }
  return {
    protocol: PROTOCOL_MAJOR,
    request_id: requestId,
    ...(idempotencyKey === undefined ? {} : { idempotency_key: idempotencyKey }),
    ...(expectedVersion === undefined ? {} : { expected_version: expectedVersion }),
    method,
    ...(params === undefined ? {} : { params }),
  };
}

const MESSAGE_TYPES: Record<ServerMessage["type"], true> = {
  claude_login_prompt: true,
  event: true,
  load_changed: true,
  power_changed: true,
  response: true,
  signed_out: true,
  stream: true,
  stream_append: true,
  terminal_exit: true,
  terminal_output: true,
};

/** One message from the server, or null for what this client cannot read: a newer server may send more types. */
export function parseServerMessage(text: string): ServerMessage | null {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null) return null;
  const type = (value as { type?: unknown }).type;
  return typeof type === "string" && Object.hasOwn(MESSAGE_TYPES, type) ? (value as ServerMessage) : null;
}

/** `POST /v1/pair`. */
export interface PairRequest {
  code: string;
  device_name: string;
  platform: string;
  /** `owner`: a computer asking for full access. */
  role?: string;
}

/** The 202 of `POST /v1/pair`. */
export interface PairAccepted {
  pairing_id: string;
  expires_at_ms: number;
}

/** The 200 of `GET /v1/pair/{id}`. */
export type PairPoll =
  | { status: "pending" }
  | {
      status: "approved";
      client_id: string;
      device_secret: string;
      host_id: string;
      host_name: string;
      environment: string;
    };

/** The 200 of `POST /v1/session`. */
export interface GatewaySession {
  token: string;
  expires_at_ms: number;
  capabilities: Capability[];
  name: string;
}

/** The 200 of `POST /v1/upload`: the path to name in `files` or `images`. */
export interface GatewayUpload {
  path: string;
  name: string;
  byte_length: number;
}

/** The body of a gateway error. */
export interface GatewayError {
  error: string;
}
