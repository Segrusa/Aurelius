import type { ActivityView, Event, StreamAppend, StreamDelta } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import { StreamMerge } from "./stream";

const ids = { run_id: "r", conversation_id: "c", message_id: "m" };
const append = (offset: number, text: string, more: Partial<StreamAppend> = {}): StreamAppend => ({
  ...ids,
  offset,
  text,
  ...more,
});
const call = (call_id: string, offset = 0) => ({ call_id, offset }) as unknown as ActivityView;
const event = (kind: string) => ({ kind }) as unknown as Event;

describe("appends", () => {
  it("become the full text again", () => {
    const merge = new StreamMerge();
    expect(merge.append(append(0, "Hi 👋"))?.text).toBe("Hi 👋");
    // "Hi 👋" is 5 UTF-16 units: the emoji is a surrogate pair.
    expect(merge.append(append(5, " there"))?.text).toBe("Hi 👋 there");
    expect(merge.append(append(0, "Hello"))?.text).toBe("Hello");
    expect(merge.append(append(2, "y"))?.text).toBe("Hey");
  });

  it("wait for the next full text after a gap", () => {
    const merge = new StreamMerge();
    merge.append(append(0, "abc"));
    expect(merge.append(append(9, "x"))).toBeNull();
    expect(merge.append(append(3, "d"))?.text).toBe("abcd");
  });

  it("refuse an offset inside a surrogate pair", () => {
    const merge = new StreamMerge();
    merge.append(append(0, "a👋"));
    expect(merge.append(append(2, "x"))).toBeNull();
  });

  it("start over after a message or run event", () => {
    const merge = new StreamMerge();
    merge.append(append(0, "abc"));
    merge.onEvent(event("task.updated"));
    expect(merge.append(append(3, "d"))?.text).toBe("abcd");
    merge.onEvent(event("message.assistant_committed"));
    expect(merge.append(append(4, "e"))).toBeNull();
  });
});

describe("tool calls", () => {
  it("merge changes by call id", () => {
    const merge = new StreamMerge();
    const a = call("a");
    const b = call("b");
    expect(merge.append(append(0, "x", { activity: [a] }))?.activity).toEqual([a]);
    const changed = call("a", 7);
    expect(merge.append(append(1, "y", { activity: [changed, b], activity_changes: true }))?.activity).toEqual([
      changed,
      b,
    ]);
  });

  it("keep the list when an update has none", () => {
    const merge = new StreamMerge();
    const a = call("a");
    merge.append(append(0, "x", { activity: [a] }));
    expect(merge.append(append(1, "y"))?.activity).toEqual([a]);
  });

  it("merge the same way over a pipe", () => {
    const merge = new StreamMerge();
    const delta = (text: string, activity: ActivityView[], activity_changes = false): StreamDelta => ({
      ...ids,
      text,
      activity,
      activity_changes,
    });
    const a = call("a");
    const b = call("b");
    expect(merge.delta(delta("x", [a])).activity).toEqual([a]);
    const update = merge.delta(delta("xy", [b], true));
    expect(update.text).toBe("xy");
    expect(update.activity).toEqual([a, b]);
  });
});

describe("designs being written", () => {
  const draft = (content: string) => ({ call_id: "w1", path: "/p/a.html", content });
  const delta = (more: Partial<StreamDelta> = {}): StreamDelta => ({ ...ids, text: "t", activity: [], ...more });

  it("stay until their call is whole", () => {
    const merge = new StreamMerge();
    expect(merge.delta(delta()).draft).toBeNull();
    expect(merge.delta(delta({ draft: draft("<p>") })).draft).toEqual(draft("<p>"));
    // A text update says nothing of the design: the last one holds.
    expect(merge.delta(delta({ text: "tt" })).draft).toEqual(draft("<p>"));
    expect(merge.append(append(0, "x", { draft: draft("<p>Hi") }))?.draft).toEqual(draft("<p>Hi"));
    expect(merge.delta(delta({ activity: [call("w1")] })).draft).toBeNull();
    expect(merge.delta(delta()).draft).toBeNull();
  });

  it("are one per answer and go with a new connection", () => {
    const merge = new StreamMerge();
    merge.delta(delta({ draft: draft("<p>") }));
    expect(merge.delta({ ...delta(), message_id: "other" }).draft).toBeNull();
    merge.reset();
    expect(merge.delta(delta()).draft).toBeNull();
  });
});
