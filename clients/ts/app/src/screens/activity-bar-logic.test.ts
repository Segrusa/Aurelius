import type { ActivityOverview, ActivityWaiting, ActivityWork, LoadSnapshot, TicketActivity } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import { areasOf } from "../data";
import {
  activityCounts,
  machineFigures,
  PRIVATE_CHAT,
  serverLine,
  ticketActivityLine,
  waitLabel,
  workHref,
  workLabel,
  workPlaces,
} from "./activity-bar-logic";

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

describe("where a row leads", () => {
  const places = workPlaces(
    [{ conversation_id: "s1" }],
    [
      { conversation_id: "ch7", ticket_id: "k7", ticket_number: 7 },
      { conversation_id: "ch9", ticket_id: "k9", ticket_number: 9 },
    ],
  );
  // Every kind of row the server sends (storage/activity.rs work_of), each with and without a place.
  const rows: ActivityWork[] = [
    { ticket_id: "k7", task_id: "t", chat_id: "c" },
    { ticket_id: "k7" },
    { ticket_id: "gone" },
    { ticket_id: "gone", task_id: "t" },
    { task_id: "t", chat_id: "c" },
    { task_id: "t" },
    { channel_id: "ch9" },
    { channel_id: "ch-general" },
    { chat_id: "s1" },
    { chat_id: "c" },
    { private: true, chat_id: "c" },
    { label: "x" },
  ];

  it("opens a ticket as its session, by the number from the channel list", () => {
    expect(workHref({ ticket_id: "k7", task_id: "t", chat_id: "c" }, places)).toBe("/ticket/7");
    expect(workHref({ ticket_id: "k7" }, places)).toBe("/ticket/7");
  });

  it("opens a ticket's channel as that ticket's session", () => {
    expect(workHref({ channel_id: "ch9" }, places)).toBe("/ticket/9");
  });

  it("opens a task as its run, also when its ticket is not known", () => {
    expect(workHref({ task_id: "t", chat_id: "c" }, places)).toBe("/tasks/t");
    expect(workHref({ ticket_id: "gone", task_id: "t" }, places)).toBe("/tasks/t");
  });

  it("opens a chat that is a work session as that session", () => {
    expect(workHref({ chat_id: "s1" }, places)).toBe("/session/s1");
  });

  it("leads nowhere for a channel or chat without a screen, a private chat or work without a place", () => {
    expect(workHref({ channel_id: "ch-general" }, places)).toBeNull();
    expect(workHref({ chat_id: "c" }, places)).toBeNull();
    expect(workHref({ ticket_id: "gone" }, places)).toBeNull();
    expect(workHref({ private: true, chat_id: "c" }, places)).toBeNull();
    expect(workHref({ label: "x" }, places)).toBeNull();
  });

  it("gives every row a ticket, session or task run, or nothing, never the removed chat or talk", () => {
    for (const work of rows) {
      const href = workHref(work, places);
      if (href !== null) expect(href).toMatch(/^\/(ticket\/\d+|session\/[^/?]+|tasks\/[^/?]+)$/);
      expect(href ?? "").not.toMatch(/\/chat|\/talk|\/board/);
    }
  });
});

describe("the server and its machine", () => {
  const load = (over: Partial<LoadSnapshot> = {}): LoadSnapshot => ({
    checks_active: true,
    cpu_pct: 12.4,
    gate: "open",
    held: 0,
    limits: { cpu_high: 90, cpu_low: 70, enabled: true, max_runs: 4, mem_min_mb: 1024 },
    mem_free_mb: 4096,
    mem_total_mb: 16384,
    running: 0,
    sampled_at_ms: 1,
    ...over,
  });

  it("names the server with its version", () => {
    expect(serverLine("atlas-box", "0.9.3")).toBe("atlas-box · v0.9.3");
    expect(serverLine("atlas-box", null)).toBe("atlas-box");
    expect(serverLine(null, null)).toBe("Aurelius");
  });

  it("shows CPU and the memory in use as whole percentages, with the amounts in the hint", () => {
    expect(machineFigures(load())).toEqual({
      cpu: "12%",
      memory: "75%",
      cpuHint: "CPU 12% (all cores, last ~10 s)",
      memoryHint: "Memory 75% in use: 12.0 of 16.0 GB",
    });
  });

  it("shows nothing where the server does not measure the machine", () => {
    expect(machineFigures(load({ checks_active: false }))).toBeNull();
    expect(machineFigures(load({ mem_total_mb: 0 }))).toBeNull();
  });
});
