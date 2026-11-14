import type { ActivityView, MessageView, TaskView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  branchOf,
  cardShown,
  elapsed,
  latestTodos,
  nowLine,
  openTasks,
  taskTally,
  todosOf,
  todoTally,
  workingLine,
} from "./ticket-overview-logic";

const call = (tool: string, input: unknown, offset = 0, ok: boolean | null = true): ActivityView => ({
  call_id: `${tool}-${offset}`,
  tool,
  input,
  offset,
  ok,
});

const todoCall = (offset: number, todos: unknown[]) => call("TodoWrite", { todos }, offset);

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

describe("the todo list", () => {
  it("reads Claude's TodoWrite, with the running item in its active form", () => {
    const todos = todosOf([
      todoCall(1, [
        { content: "Find causes", activeForm: "Finding causes", status: "completed" },
        { content: "Run the tests", activeForm: "Running the tests", status: "in_progress" },
        { content: "Release", activeForm: "Releasing", status: "pending" },
      ]),
    ]);
    expect(todos).toEqual([
      { text: "Find causes", state: "done" },
      { text: "Running the tests", state: "doing" },
      { text: "Release", state: "open" },
    ]);
  });

  it("takes the latest list and skips what is not a todo", () => {
    const todos = todosOf([
      todoCall(5, [{ content: "New", status: "pending" }, null, { status: "pending" }, { content: "  " }]),
      todoCall(1, [{ content: "Old", status: "pending" }]),
      call("Bash", { command: "ls" }, 9),
    ]);
    expect(todos).toEqual([{ text: "New", state: "open" }]);
  });

  it("is null without a list", () => {
    expect(todosOf([call("Bash", {}, 0)])).toBeNull();
    expect(todosOf([call("TodoWrite", { todos: "x" }, 0)])).toBeNull();
    expect(todosOf([todoCall(0, [])])).toBeNull();
  });

  it("prefers what streams, then the newest answer", () => {
    const answer = (n: number, text: string) =>
      ({ id: `m${n}`, activity: [todoCall(0, [{ content: text, status: "pending" }])] }) as unknown as MessageView;
    const plain = { id: "m3", activity: [] } as unknown as MessageView;
    expect(latestTodos([], [answer(1, "a"), answer(2, "b"), plain])).toEqual([{ text: "b", state: "open" }]);
    expect(latestTodos([[todoCall(0, [{ content: "live", status: "pending" }])]], [answer(1, "a")])).toEqual([
      { text: "live", state: "open" },
    ]);
    expect(latestTodos([], [])).toBeNull();
  });

  it("counts what is done", () => {
    expect(
      todoTally([
        { text: "a", state: "done" },
        { text: "b", state: "doing" },
      ]),
    ).toBe("1 of 2");
  });
});

describe("the line of what runs now", () => {
  it("says the time gone", () => {
    expect(elapsed(-5)).toBe("0s");
    expect(elapsed(45_000)).toBe("45s");
    expect(elapsed(96_000)).toBe("1m 36s");
    expect(elapsed(2 * 3_600_000 + 5 * 60_000)).toBe("2h 05m");
  });

  it("names the step that runs, or thinking between steps", () => {
    expect(nowLine([call("Bash", {}, 0, null)], 1_000, 97_000)).toBe("Running a command · 1m 36s");
    expect(nowLine([call("Bash", {}, 0, true)], 1_000, 2_000)).toBe("Thinking · 1s");
    expect(nowLine([], null, 0)).toBe("Thinking");
  });
});

describe("the ticket's tasks", () => {
  const tasks = [
    task("root"),
    task("a", { parent_task_id: "root", created_at_ms: 1 }),
    task("b", { parent_task_id: "a", created_at_ms: 2 }),
    task("done", { parent_task_id: "root", status: "succeeded" }),
  ];

  it("lists the open ones without the main task, as a tree", () => {
    expect(openTasks(tasks, "root").map((r) => [r.task.id, r.depth])).toEqual([
      ["a", 0],
      ["b", 1],
    ]);
  });

  it("names who is at work", () => {
    expect(workingLine([task("a", { employee_name: "Flux" }), task("b", { employee_name: "Fiona" })])).toBe(
      "Working: Flux, Fiona",
    );
    expect(workingLine([task("a", { status: "succeeded", live_run: "running", employee_name: "Flux" })])).toBe(
      "Working: Flux",
    );
    expect(workingLine([task("a")])).toBe("Working");
    expect(workingLine([task("a", { status: "succeeded" })])).toBeNull();
  });

  it("counts the delegated ones", () => {
    expect(taskTally(tasks, "root")).toBe("1 of 3 done");
    expect(taskTally([task("root")], "root")).toBeNull();
  });
});

describe("the card", () => {
  it("shows while something runs, a todo is left or a task is open", () => {
    expect(cardShown(true, null, 0)).toBe(true);
    expect(cardShown(false, [{ text: "x", state: "open" }], 0)).toBe(true);
    expect(cardShown(false, [{ text: "x", state: "done" }], 0)).toBe(false);
    expect(cardShown(false, null, 1)).toBe(true);
    expect(cardShown(false, null, 0)).toBe(false);
  });

  it("names a quick ticket's branch", () => {
    expect(branchOf({ kind: "quick", number: 64 })).toBe("quick/ticket-64");
    expect(branchOf({ kind: "normal", number: 64 })).toBeNull();
  });
});
