import { describe, expect, it } from "vitest";
import { METHODS, type MethodName, PROTOCOL_MAJOR } from "./generated";
import samples from "./generated/wire-samples.json";
import { encodeRequest, parseServerMessage } from "./wire";

const parse = (sample: unknown) => parseServerMessage(JSON.stringify(sample));

describe("requests", () => {
  it("writes a read as the server does", () => {
    expect(encodeRequest({ requestId: 7, method: "user.whoami" })).toEqual(samples.request_read);
  });

  it("writes a write as the server does", () => {
    const { method, idempotency_key } = samples.request_write;
    const encoded = encodeRequest({
      requestId: 8,
      method,
      idempotencyKey: idempotency_key,
      expectedVersion: 3,
    } as never);
    expect(encoded).toEqual(samples.request_write);
  });

  it("refuses a write without a key", () => {
    const method = Object.keys(METHODS).find((m) => METHODS[m as MethodName] !== "read");
    expect(() => encodeRequest({ requestId: 1, method } as never)).toThrow(/idempotency key/);
  });

  it("speaks the server's major", () => {
    expect(samples.request_read.protocol).toBe(PROTOCOL_MAJOR);
  });
});

describe("server messages", () => {
  it("reads an error response", () => {
    const message = parse(samples.response_error);
    if (message?.type !== "response") throw new Error("expected a response");
    expect(message.request_id).toBe(7);
    expect(message.error?.code).toBe("forbidden");
    expect(message.ok ?? null).toBeNull();
  });

  it("reads a receipt", () => {
    const message = parse(samples.response_receipt);
    if (message?.type !== "response" || message.ok?.type !== "receipt") throw new Error("expected a receipt");
    expect(message.ok.value.committed_seq).toBe(91);
  });

  it("reads a stream append", () => {
    const message = parse(samples.stream_append);
    if (message?.type !== "stream_append") throw new Error("expected an append");
    expect(message.offset).toBe(5);
    expect(message.text).toBe(" there");
  });

  it("reads a sign-out", () => {
    const message = parse(samples.signed_out);
    if (message?.type !== "signed_out") throw new Error("expected a sign-out");
    expect(message.reason).toBe("revoked");
  });

  it("reads a side answer", () => {
    const message = parse({ type: "side_answer", side_id: "s1", text: "Working on the export", done: false });
    if (message?.type !== "side_answer") throw new Error("expected a side answer");
    expect(message.side_id).toBe("s1");
    expect(message.text).toBe("Working on the export");
    expect(message.done).toBe(false);
  });

  it("leaves what it cannot read", () => {
    expect(parseServerMessage("not json")).toBeNull();
    expect(parseServerMessage("[]")).toBeNull();
    expect(parseServerMessage('{"type":"from_a_newer_server"}')).toBeNull();
    expect(parseServerMessage('{"type":"constructor"}')).toBeNull();
  });
});
