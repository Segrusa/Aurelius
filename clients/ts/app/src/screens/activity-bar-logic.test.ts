import type { ActivityOverview, ActivityWaiting, TicketActivity } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import { areasOf } from "../data";
import { activityCounts, PRIVATE_CHAT, ticketActivityLine, waitLabel, workLabel } from "./activity-bar-logic";

const wait = (waits_for: string): ActivityWaiting => ({
  employee_name: "Ann",
  work: { label: "#1 A" },
  waits_for,
  since_ms: 1,
});
const ticket = (state: string, waits_for?: string): TicketActivity => ({
  ticket_id: "t",
  state,
  employee_name: "Ann",
  since_ms: 1,
  ...(waits_for ? { waits_for } : {}),
});

describe("the status bar", () => {
  it("counts only what waits for a slot as waiting for one", () => {
    const o: ActivityOverview = {
      as_of_seq: 1,
      busy: [{ employee_name: "Bo", work: { label: "x" }, since_ms: 1 }],
      waiting: [wait("slot"), wait("machine"), wait("paused"), wait("question")],
      tickets: [],
    };
    expect(activityCounts(o)).toEqual({ busy: 1, waitingForSlot: 2 });
  });

  it("shows another board member's chat only as a private chat", () => {
    expect(workLabel({ label: "", private: true })).toBe(PRIVATE_CHAT);
    expect(workLabel({ label: "#2 Work" })).toBe("#2 Work");
  });

  it("is refreshed by the events of a run, a task, a ticket and an employee", () => {
    for (const kind of ["run.started", "task.waiting", "ticket.updated", "employee.paused", "approval.requested"]) {
      expect(areasOf(kind)).toContain("activity");
    }
  });
});

describe("a ticket's line", () => {
  it("tells running, waiting for a slot, paused and waiting for something else apart", () => {
    const lines = [
      ticket("running"),
      ticket("waiting_slot", "slot"),
      ticket("paused", "paused"),
      ticket("waiting_other", "board_member"),
    ].map(ticketActivityLine);
    expect(lines).toEqual([
      "Running: Ann",
      "Waiting for a free slot: Ann is busy",
      "Paused: Ann",
      "Waiting for a board member",
    ]);
    expect(new Set(lines).size).toBe(4);
  });

  it("has words for a reason it does not know", () => {
    expect(waitLabel("something_new", "Ann")).toBe("Waiting");
  });
});
