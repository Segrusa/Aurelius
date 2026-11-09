import { describe, expect, it } from "vitest";
import { areasOf } from "../data";
import {
  activityFilters,
  categoryLabel,
  detailText,
  firstInLine,
  parseActivitySearch,
  queueState,
  queueWhat,
  shortRun,
  stepLabel,
} from "./activity-logic";

describe("the activity log's filters", () => {
  it("reads only what makes sense from the URL", () => {
    expect(activityFilters({})).toEqual({});
    expect(activityFilters({ employee: "e1", thread: "ticket:t", run: "r", ticket: "21", category: "queue" })).toEqual({
      employee: "e1",
      thread: "ticket:t",
      run: "r",
      ticket: 21,
      category: "queue",
    });
    expect(activityFilters({ employee: "", ticket: "x", category: "gossip" })).toEqual({});
  });

  it("tells a ticket, a thread and a run apart in the search box", () => {
    expect(parseActivitySearch("#21")).toEqual({ ticket: 21 });
    expect(parseActivitySearch(" 21 ")).toEqual({ ticket: 21 });
    expect(parseActivitySearch("channel:abc")).toEqual({ thread: "channel:abc" });
    expect(parseActivitySearch("019A52F0-1111-7000-8000-000000000001")).toEqual({
      run: "019a52f0-1111-7000-8000-000000000001",
    });
    expect(parseActivitySearch("")).toEqual({});
    expect(parseActivitySearch("hello there")).toBeNull();
  });
});

describe("a step in words", () => {
  it("depends on its category", () => {
    expect(stepLabel({ category: "queue", step: "cancelled" })).toBe("Taken out of the queue");
    expect(stepLabel({ category: "run", step: "cancelled" })).toBe("Run stopped");
  });

  it("shows what a newer server wrote as it is", () => {
    expect(stepLabel({ category: "run", step: "went_for_lunch" })).toBe("went for lunch");
    expect(categoryLabel("later")).toBe("later");
  });

  it("names a usage limit and cuts a run id", () => {
    expect(detailText("quota")).toBe("usage limit");
    expect(detailText("Read")).toBe("Read");
    expect(detailText(undefined)).toBe("");
    expect(shortRun("abcdefghijkl")).toBe("ghijkl");
  });
});

describe("the employee's queue", () => {
  const time = () => "14:00";

  it("says what an item is", () => {
    expect(queueWhat({ kind: "task", source: "board" })).toBe("Task from a board member");
    expect(queueWhat({ kind: "message", source: "employee" })).toBe("Message from a colleague");
    expect(queueWhat({ kind: "newer", source: "system" })).toBe("newer");
  });

  it("says where it stands", () => {
    const item = { state: "queued", not_before_ms: 0 };
    expect(queueState({ ...item, state: "claimed" }, false, time)).toBe("Working on it");
    expect(queueState({ ...item, waits_for: "limit" }, false, time)).toBe("Usage limit, until 14:00");
    expect(queueState({ ...item, waits_for: "handoff" }, false, time)).toContain("sender");
    expect(queueState(item, true, time)).toBe("Next");
    expect(queueState(item, false, time)).toBe("In line");
  });

  it("finds what is next past what is held back", () => {
    expect(
      firstInLine([
        { id: "a", state: "claimed" },
        { id: "b", state: "queued", waits_for: "question" },
        { id: "c", state: "queued" },
      ]),
    ).toBe("c");
    expect(firstInLine([])).toBeNull();
  });

  it("an approval changes what the dot says", () => {
    expect(areasOf("approval.requested")).toContain("employees");
  });
});
