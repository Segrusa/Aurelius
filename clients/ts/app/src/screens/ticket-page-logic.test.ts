import type { ActivityView, TaskView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import { busy, inFilter } from "./ticket-page-logic";

const call = (
  tool: string,
  input: unknown,
  ok: boolean | null = true,
  over: Partial<ActivityView> = {},
): ActivityView => ({
  call_id: `${tool}-${JSON.stringify(input)}`,
  tool,
  input,
  offset: 0,
  ok,
  ...over,
});

const task = (id: string, over: Partial<TaskView> = {}): TaskView => ({
  id,
  title: id,
  goal: "",
  status: "running",
  attachments: [],
  children: [],
  created_at_ms: 0,
  criteria: [],
  handoffs: [],
  pending_approvals: 0,
  project_id: "p",
  run: {},
  updated_at_ms: 0,
  ...over,
});

describe("inFilter", () => {
  const talk = { role: "user" as const, activity: [] };
  const line = { role: "assistant" as const, activity: [], system: { action: "moved" } as never };
  const update = { role: "assistant" as const, activity: [], update: "status" };
  const edit = { role: "assistant" as const, activity: [call("Edit", { file_path: "a.ts" })] };

  it("shows everything under All", () => {
    for (const m of [talk, line, update, edit]) expect(inFilter(m, "all")).toBe(true);
  });

  it("keeps the system's lines, updates and answers that changed files under Changes", () => {
    expect(inFilter(line, "changes")).toBe(true);
    expect(inFilter(update, "changes")).toBe(true);
    expect(inFilter(edit, "changes")).toBe(true);
    expect(inFilter(talk, "changes")).toBe(false);
  });

  it("keeps what people and employees wrote under Messages", () => {
    expect(inFilter(talk, "messages")).toBe(true);
    expect(inFilter(edit, "messages")).toBe(true);
    expect(inFilter(line, "messages")).toBe(false);
    expect(inFilter(update, "messages")).toBe(false);
  });

  it("does not count a failed edit as a change", () => {
    expect(inFilter({ role: "assistant", activity: [call("Edit", { file_path: "a" }, false)] }, "changes")).toBe(false);
  });
});

describe("busy", () => {
  it("counts a finished task with a live run as busy", () => {
    expect(busy(task("x", { status: "succeeded", live_run: "running" as never }))).toBe(true);
    expect(busy(task("x", { status: "succeeded", pending_approvals: 1 }))).toBe(true);
    expect(busy(task("x", { status: "queued", waiting_reason: "in_line" }))).toBe(false);
    expect(busy(task("x", { status: "cancelled" }))).toBe(false);
  });
});
