import type { ScheduledMessageView, ThreadWaiter } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import { areasOf } from "../data";
import { outcomeText, scheduledLine, shownWaiters, splitScheduled, waiterLine } from "./scheduled-logic";

const waiter = (over: Partial<ThreadWaiter> = {}): ThreadWaiter => ({
  employee_id: "e1",
  employee_name: "Flux",
  behind: 0,
  ...over,
});

const message = (over: Partial<ScheduledMessageView> = {}): ScheduledMessageView => ({
  id: "s1",
  from_employee: "e1",
  from_name: "Flux",
  to_employee: "e1",
  to_name: "Flux",
  text: "Check the CI",
  created_at_ms: 1,
  due_at_ms: 2,
  state: "scheduled",
  place: "the discussion of ticket #21",
  ...over,
});

describe("the waiting line", () => {
  it("says who waits and behind how much", () => {
    expect(waiterLine(waiter())).toBe("Flux is waiting to pick up this thread");
    expect(waiterLine(waiter({ behind: 1 }))).toBe("Flux is waiting to pick up this thread (behind 1 item)");
    expect(waiterLine(waiter({ behind: 2 }))).toBe("Flux is waiting to pick up this thread (behind 2 items)");
  });

  it("leaves out an employee already at work in the thread, whose next message only waits its turn", () => {
    const busy = waiter({ behind: 1 });
    const paused = waiter({ employee_name: "Bo", behind: 1, reason: "paused" });
    const idle = waiter({ employee_name: "Ann" });
    expect(shownWaiters([busy, paused, idle])).toEqual([paused, idle]);
  });

  it("names what holds it back other than its turn", () => {
    expect(waiterLine(waiter({ reason: "limit" }))).toContain("usage limit");
    expect(waiterLine(waiter({ reason: "question" }))).toContain("answer");
    expect(waiterLine(waiter({ reason: "newer" }))).toBe("Flux is waiting to pick up this thread");
  });

  it("tells a reminder from a message to a colleague", () => {
    expect(scheduledLine(message(), "14:30")).toBe("Flux has a reminder for 14:30");
    expect(scheduledLine(message({ to_employee: "e2", to_name: "Bo" }), "14:30")).toBe("Flux → Bo at 14:30");
  });
});

describe("the Scheduled page", () => {
  it("keeps what waits apart from what came of it", () => {
    const { waiting, past } = splitScheduled([
      message({ id: "a" }),
      message({ id: "b", state: "delivered" }),
      message({ id: "c", state: "cancelled" }),
    ]);
    expect(waiting.map((m) => m.id)).toEqual(["a"]);
    expect(past.map((m) => m.id)).toEqual(["b", "c"]);
  });

  it("says what came of a message, and shows a newer state as it is", () => {
    expect(outcomeText(message({ state: "delivered" }))).toBe("Delivered");
    expect(outcomeText(message({ state: "cancelled" }))).toBe("Taken back");
    expect(outcomeText(message({ state: "not_delivered", outcome: "Bo is paused." }))).toBe(
      "Not delivered: Bo is paused.",
    );
    expect(outcomeText(message({ state: "not_delivered" }))).toBe("Not delivered");
    expect(outcomeText(message({ state: "newer" }))).toBe("newer");
  });

  it("refetches on what changes a schedule and on runs", () => {
    expect(areasOf("recurring_task.changed")).toContain("scheduled");
    expect(areasOf("run.queued")).toContain("scheduled");
  });
});
