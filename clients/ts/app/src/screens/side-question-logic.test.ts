import { describe, expect, it } from "vitest";
import { answered, asked, canAsk, refused, started, stillAnswering, unclaimed } from "./side-question-logic";

const answer = (text: string, done = false, error?: string) => ({
  side_id: "s1",
  text,
  done,
  ...(error ? { error } : {}),
});

describe("side questions", () => {
  it("follow a question from asked to answered", () => {
    let list = asked([], 1, "What is it doing?");
    expect(list[0]).toMatchObject({ status: "asking", sideId: null, text: "" });
    list = started(list, 1, "s1");
    list = answered(list, answer("Reading"));
    expect(list[0]).toMatchObject({ status: "answering", text: "Reading" });
    list = answered(list, answer("Reading the export code.", true));
    expect(list[0]).toMatchObject({ status: "done", text: "Reading the export code.", error: null });
  });

  it("keep the text written before a failure and show why", () => {
    let list = started(asked([], 1, "q"), 1, "s1");
    list = answered(list, answer("Half an"));
    list = answered(list, answer("", true, "Stopped."));
    expect(list[0]).toMatchObject({ status: "failed", text: "Half an", error: "Stopped." });
  });

  it("take an answer that came before its id", () => {
    const list = asked([], 1, "q");
    const early = answer("Fast", true);
    expect(unclaimed(list, early)).toBe(true);
    expect(started(list, 1, "s1", early)[0]).toMatchObject({ status: "done", text: "Fast" });
  });

  it("show a refusal on its question only", () => {
    const list = refused(asked(asked([], 1, "a"), 2, "b"), 2, "Too long");
    expect(list.map((a) => a.status)).toEqual(["asking", "failed"]);
    expect(list[1]?.error).toBe("Too long");
  });

  it("stop only what is still being answered", () => {
    let list = started(asked([], 1, "a"), 1, "s1");
    list = started(asked(list, 2, "b"), 2, "s2");
    list = answered(list, { side_id: "s2", text: "done", done: true });
    expect(stillAnswering(list)).toEqual(["s1"]);
    expect(stillAnswering(asked([], 3, "no id yet"))).toEqual([]);
  });

  it("ask one at a time and never a blank question", () => {
    expect(canAsk([], "  ")).toBe(false);
    expect(canAsk([], "Why?")).toBe(true);
    const busy = started(asked([], 1, "a"), 1, "s1");
    expect(canAsk(busy, "Why?")).toBe(false);
    expect(canAsk(answered(busy, answer("ok", true)), "Why?")).toBe(true);
  });
});
