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
  todosAcross,
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

  it("builds the list from TaskCreate and TaskUpdate (Claude Code 2.1.318)", () => {
    const made = (offset: number, n: number, subject: string, activeForm?: string) => ({
      ...call("TaskCreate", { subject, description: subject, activeForm }, offset),
      call_id: `c${n}`,
      output: `Task #${n} created successfully: ${subject}`,
    });
    const update = (offset: number, taskId: string, more: Record<string, unknown>) => ({
      ...call("TaskUpdate", { taskId, ...more }, offset),
      call_id: `u${offset}-${taskId}`,
    });
    const todos = todosOf([
      made(0, 1, "Find causes"),
      made(0, 2, "Run the tests", "Running the tests"),
      made(0, 3, "Release"),
      made(0, 4, "Dropped"),
      update(3, "1", { status: "completed" }),
      update(5, "2", { status: "in_progress" }),
      update(5, "4", { status: "deleted" }),
      update(6, "9", { status: "completed" }),
      call("Bash", { command: "ls" }, 7),
    ]);
    expect(todos).toEqual([
      { text: "Find causes", state: "done" },
      { text: "Running the tests", state: "doing" },
      { text: "Release", state: "open" },
    ]);
  });

  it("numbers the items in order when a result is not there yet", () => {
    const todos = todosOf([
      call("TaskCreate", { subject: "a" }, 0, null),
      call("TaskCreate", { subject: "b" }, 0, null),
      { ...call("TaskUpdate", { taskId: "2", status: "completed" }, 1), call_id: "u" },
      { ...call("TaskCreate", { subject: "failed" }, 2, false), call_id: "f" },
    ]);
    expect(todos).toEqual([
      { text: "a", state: "open" },
      { text: "b", state: "done" },
    ]);
  });

  describe("over several answers (Claude Code 2.1.318)", () => {
    const made = (n: number, subject: string, activeForm?: string) => ({
      ...call("TaskCreate", { subject, activeForm }, 0),
      call_id: `c${n}-${subject}`,
      output: `Task #${n} created successfully: ${subject}`,
    });
    const update = (offset: number, taskId: string, more: Record<string, unknown>) => ({
      ...call("TaskUpdate", { taskId, ...more }, offset),
      call_id: `u${offset}-${taskId}`,
    });
    const reply = (id: string, activity: ActivityView[]) =>
      ({ id, role: "assistant", activity }) as unknown as MessageView;
    const first = [made(1, "Read the design"), made(2, "Build the page", "Building the page"), made(3, "Test it")];

    it("an update in a later answer changes the list an earlier one made", () => {
      const later = [update(0, "1", { status: "completed" }), update(1, "2", { status: "in_progress" })];
      expect(todosAcross([first, later])).toEqual([
        { text: "Read the design", state: "done" },
        { text: "Building the page", state: "doing" },
        { text: "Test it", state: "open" },
      ]);
      expect(latestTodos([], [reply("m1", first), reply("m2", later)])).toEqual(todosAcross([first, later]));
    });

    it("items added in a later answer join the earlier list", () => {
      const later = [made(4, "Add line comments"), update(1, "3", { status: "completed" })];
      expect(todosAcross([first, later])?.map((t) => [t.text, t.state])).toEqual([
        ["Read the design", "open"],
        ["Build the page", "open"],
        ["Test it", "done"],
        ["Add line comments", "open"],
      ]);
    });

    it("numbering that starts again at 1 is a new list", () => {
      expect(todosAcross([first, [made(1, "Fresh start")]])).toEqual([{ text: "Fresh start", state: "open" }]);
    });

    it("a later TodoWrite replaces the list", () => {
      const later = [todoCall(0, [{ content: "Snapshot", status: "pending" }])];
      expect(todosAcross([first, later])).toEqual([{ text: "Snapshot", state: "open" }]);
    });

    it("what streams for a stored answer counts once, in its place", () => {
      const live = { message_id: "m2", activity: [update(0, "1", { status: "completed" })] };
      const todos = latestTodos([live], [reply("m1", first), reply("m2", [])]);
      expect(todos?.[0]).toEqual({ text: "Read the design", state: "done" });
      expect(todos).toHaveLength(3);
    });
  });

  it("prefers what streams, then the newest answer", () => {
    const answer = (n: number, text: string) =>
      ({
        id: `m${n}`,
        role: "assistant",
        activity: [todoCall(0, [{ content: text, status: "pending" }])],
      }) as unknown as MessageView;
    const plain = { id: "m3", role: "assistant", activity: [] } as unknown as MessageView;
    const streaming = (id: string, text: string) => ({
      message_id: id,
      activity: [todoCall(0, [{ content: text, status: "pending" }])],
    });
    expect(latestTodos([], [answer(1, "a"), answer(2, "b"), plain])).toEqual([{ text: "b", state: "open" }]);
    expect(latestTodos([streaming("m9", "live")], [answer(1, "a")])).toEqual([{ text: "live", state: "open" }]);
    expect(latestTodos([streaming("m1", "newer")], [answer(1, "a")])).toEqual([{ text: "newer", state: "open" }]);
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
