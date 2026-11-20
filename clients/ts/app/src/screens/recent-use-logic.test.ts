import { describe, expect, it } from "vitest";
import { lastUsed, parseUses, sessionUseKey, ticketUseKey, touchUse, useKeyOf } from "./recent-use-logic";

describe("recently used sessions and tickets", () => {
  it("knows a session or a ticket from the screen's path, and nothing else", () => {
    expect(useKeyOf("/session/abc-1")).toBe(sessionUseKey("abc-1"));
    expect(useKeyOf("/ticket/72")).toBe(ticketUseKey(72));
    expect(useKeyOf("/ticket/72/files")).toBe(ticketUseKey(72));
    expect(useKeyOf("/board")).toBeNull();
    expect(useKeyOf("/ticket/new")).toBeNull();
  });

  it("reads back what it stored and ignores anything unreadable", () => {
    expect(parseUses(JSON.stringify({ a: 1 }))).toEqual({ a: 1 });
    expect(parseUses(JSON.stringify({ a: "x", b: 2 }))).toEqual({ b: 2 });
    expect(parseUses("not json")).toEqual({});
    expect(parseUses("[1]")).toEqual({});
    expect(parseUses(null)).toEqual({});
  });

  it("records an open and forgets the oldest past the limit", () => {
    expect(touchUse({ a: 1, b: 2 }, "a", 3)).toEqual({ a: 3, b: 2 });
    expect(touchUse({ a: 1, b: 2 }, "c", 3, 2)).toEqual({ c: 3, b: 2 });
  });

  it("takes the later of the activity and the open time", () => {
    expect(lastUsed({ a: 50 }, "a", 10)).toBe(50);
    expect(lastUsed({ a: 5 }, "a", 10)).toBe(10);
    expect(lastUsed({}, "a", 10)).toBe(10);
  });
});
