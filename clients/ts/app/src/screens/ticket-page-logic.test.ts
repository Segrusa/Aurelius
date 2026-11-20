import type { ActivityView, TaskView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import type { Todo } from "./chat-logic";
import {
  busy,
  currentCall,
  inFilter,
  lastSteps,
  nowTasks,
  nowTitle,
  planStep,
  progressLine,
  questionsText,
  runActivity,
  runCounts,
} from "./ticket-page-logic";

const call = (tool: string, input: unknown, ok: boolean | null = true, over: Partial<ActivityView> = {}): ActivityView => ({
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

const todos = (...states: Todo["state"][]): Todo[] => states.map((state, i) => ({ text: `Step ${i + 1}`, state }));

describe("inFilter", () => {
  const talk = { role: "user" as const, activity: [] };
  const line = { role: "system" as const, activity: [], system: { action: "moved" } as never };
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

describe("runCounts", () => {
  it("counts each file once, commands, and the lines the edits added and removed", () => {
    const c = runCounts([
      call("Read", { file_path: "a.ts" }),
      call("Read", { file_path: "a.ts" }, true, { call_id: "again" }),
      call("Read", { file_path: "b.ts" }),
      call("Edit", { file_path: "a.ts", old_string: "x\ny", new_string: "x\ny\nz" }),
      call("Write", { file_path: "c.ts", content: "one\ntwo" }),
      call("Bash", { command: "pnpm test" }),
      call("Bash", { command: "broken" }, false),
    ]);
    expect(c).toEqual({ read: 2, changed: 2, added: 5, removed: 2, commands: 1 });
  });

  it("gives no line counts when an edit's input was cut short", () => {
    const c = runCounts([call("Edit", { file_path: "a", old_string: "x", new_string: "y" }, true, { truncated: true })]);
    expect(c.added).toBeNull();
    expect(c.removed).toBeNull();
    expect(c.changed).toBe(1);
  });

  it("is all zero for a run that did nothing yet", () => {
    expect(runCounts([])).toEqual({ read: 0, changed: 0, added: 0, removed: 0, commands: 0 });
  });
});

describe("runActivity", () => {
  it("takes the run's finished answers and what still streams, and nothing of other runs", () => {
    const a = call("Read", { file_path: "a" });
    const b = call("Bash", { command: "x" });
    const other = call("Read", { file_path: "z" });
    const out = runActivity(
      [
        { run_id: "r1", activity: [a] },
        { run_id: "r0", activity: [other] },
      ],
      [{ run_id: "r1", activity: [b] }],
      "r1",
    );
    expect(out).toEqual([a, b]);
    expect(runActivity([{ run_id: "r1", activity: [a] }], [], null)).toEqual([]);
  });
});

describe("planStep and progressLine", () => {
  it("points at the step at work", () => {
    expect(planStep(todos("done", "done", "doing", "open"))).toEqual({ at: 3, done: 2, total: 4 });
  });

  it("points at the next step when none is at work, and stays on the last when all are done", () => {
    expect(planStep(todos("done", "open"))).toEqual({ at: 2, done: 1, total: 2 });
    expect(planStep(todos("done", "done"))).toEqual({ at: 2, done: 2, total: 2 });
  });

  it("has no step without a plan", () => {
    expect(planStep(null)).toBeNull();
    expect(planStep([])).toBeNull();
  });

  it("says the step, how long and since when, leaving out what is not known", () => {
    const start = new Date(2026, 10, 20, 14, 31).getTime();
    expect(progressLine({ at: 3, done: 2, total: 4 }, start, start + 372_000)).toBe(
      "Step 3 of 4 · working for 6m 12s · started 14:31",
    );
    expect(progressLine(null, null, 0)).toBe("");
    expect(progressLine({ at: 1, done: 0, total: 2 }, null, 0)).toBe("Step 1 of 2");
  });
});

describe("nowTitle", () => {
  it("names who and what they do now, else the task", () => {
    expect(nowTitle("Flux", [{ text: "Fixing the hit area", state: "doing" }], "Fix the rail")).toBe(
      "Flux · Fixing the hit area",
    );
    expect(nowTitle("Flux", null, "Fix the rail")).toBe("Flux · Fix the rail");
    expect(nowTitle(null, null, "Fix the rail")).toBe("Fix the rail");
  });
});

describe("currentCall and lastSteps", () => {
  it("names the call that runs now with its command", () => {
    expect(currentCall([call("Read", { file_path: "a" }), call("Bash", { command: "pnpm test rail" }, null)])).toEqual({
      label: "Running a command",
      detail: "pnpm test rail",
    });
    expect(currentCall([call("Read", { file_path: "a" })])).toBeNull();
  });

  it("gives the last calls with their state", () => {
    const steps = lastSteps(
      [call("Read", { file_path: "a" }), call("Edit", { file_path: "b" }, false), call("Bash", { command: "t" }, null)],
      2,
    );
    expect(steps.map((s) => [s.text, s.state])).toEqual([
      ["b", "failed"],
      ["t", "running"],
    ]);
  });
});

describe("nowTasks", () => {
  it("shows the main task first, then the delegations at work or waiting", () => {
    const tasks = [
      task("sub1"),
      task("root"),
      task("done", { status: "succeeded" }),
      task("asks", { status: "succeeded", pending_approvals: 1 }),
      task("inline", { status: "queued", waiting_reason: "in_line" }),
      task("waiting", { status: "waiting" }),
    ];
    expect(nowTasks(tasks, "root").map((t) => t.id)).toEqual(["root", "sub1", "asks", "waiting"]);
  });

  it("counts a finished task with a live run as busy", () => {
    expect(busy(task("x", { status: "succeeded", live_run: "running" as never }))).toBe(true);
    expect(busy(task("x", { status: "cancelled" }))).toBe(false);
  });
});

it("questionsText counts in words", () => {
  expect(questionsText(1)).toBe("1 question for you");
  expect(questionsText(0)).toBe("0 questions for you");
});
