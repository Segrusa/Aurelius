import type { TaskView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import { filesWritten, filterTasks, foldTree, stepLine, stepsOf, subtasksOf, taskStateLine, treeOrder } from "./tasks-logic";

const task = (id: string, over: Partial<TaskView> = {}): TaskView => ({
  id,
  title: id,
  goal: "",
  status: "running",
  attachments: [],
  children: [],
  created_at_ms: 0,
  criteria: [],
  depends_on: [],
  handoffs: [],
  pending_approvals: 0,
  project_id: "p",
  run: {},
  updated_at_ms: 0,
  ...over,
});

describe("the task filter", () => {
  const tasks = [
    task("run", { updated_at_ms: 1 }),
    task("wait", { status: "waiting", updated_at_ms: 3 }),
    task("appr", { pending_approvals: 1, updated_at_ms: 2 }),
    task("done", { status: "succeeded" }),
    task("live", { status: "succeeded", live_run: "running" }),
  ];
  const ids = (state: "open" | "needs_you" | "ended" | "all") =>
    filterTasks(tasks, { state, employee: null, project: null }).map((t) => t.id);
  it("sorts into the states, newest change first", () => {
    expect(ids("open")).toEqual(["wait", "appr", "run", "live"]);
    expect(ids("needs_you")).toEqual(["wait", "appr"]);
    expect(ids("ended")).toEqual(["done"]);
    expect(ids("all")).toHaveLength(5);
  });

  it("filters on employee and project", () => {
    const list = [task("a", { employee_id: "e" }), task("b", { project_id: "q" })];
    expect(filterTasks(list, { state: "all", employee: "e", project: null }).map((t) => t.id)).toEqual(["a"]);
    expect(filterTasks(list, { state: "all", employee: null, project: "q" }).map((t) => t.id)).toEqual(["b"]);
  });
});

describe("the task tree", () => {
  it("puts delegated tasks under their parent, and survives a cycle", () => {
    const rows = [
      task("kid2", { parent_task_id: "top", created_at_ms: 2 }),
      task("kid1", { parent_task_id: "top", created_at_ms: 1 }),
      task("top"),
      task("orphan", { parent_task_id: "gone" }),
    ];
    expect(treeOrder(rows).map((r) => [r.task.id, r.depth])).toEqual([
      ["top", 0],
      ["kid1", 1],
      ["kid2", 1],
      ["orphan", 0],
    ]);
    const cycle = [task("a", { parent_task_id: "b" }), task("b", { parent_task_id: "a" })];
    expect(treeOrder(cycle)).toHaveLength(2);
  });

  it("lists every task below one task, nested, and nothing else", () => {
    const rows = [
      task("top"),
      task("kid", { parent_task_id: "top", created_at_ms: 1 }),
      task("grandkid", { parent_task_id: "kid" }),
      task("other"),
      task("otherkid", { parent_task_id: "other" }),
    ];
    expect(subtasksOf(rows, "top").map((r) => [r.task.id, r.depth])).toEqual([
      ["kid", 1],
      ["grandkid", 2],
    ]);
    expect(subtasksOf(rows, "grandkid")).toEqual([]);
  });

  it("folds what is finished, unless asked otherwise", () => {
    const rows = [task("top", { status: "succeeded" }), task("kid", { parent_task_id: "top", status: "succeeded" })];
    const tree = treeOrder(rows);
    expect(foldTree(tree, {}).map((r) => [r.task.id, r.below, r.open])).toEqual([["top", 1, false]]);
    expect(foldTree(tree, { top: true }).map((r) => r.task.id)).toEqual(["top", "kid"]);
    const running = treeOrder([task("top"), task("kid", { parent_task_id: "top" })]);
    expect(foldTree(running, {}).map((r) => r.task.id)).toEqual(["top", "kid"]);
    expect(foldTree(running, { top: false }).map((r) => r.task.id)).toEqual(["top"]);
  });

  it("says a task's state", () => {
    expect(taskStateLine(task("a", { pending_approvals: 1 })).text).toBe("Paused · needs approval");
    expect(taskStateLine(task("a", { status: "failed" })).text).toBe("Failed");
    expect(taskStateLine(task("a", { status: "queued", waiting_reason: "in_line" })).text).toBe("Waiting");
  });
});

describe("a run's steps", () => {
  it("are one line with the failures", () => {
    expect(stepsOf([])).toBeNull();
    const steps = stepsOf([
      { tool: "Bash", input: { command: "ls\n-la" }, ok: false },
      { tool: "Read", input: { file_path: "a.rs" }, ok: true },
    ]);
    expect(steps).toEqual({ count: 2, failed: 1, last: "a.rs" });
    expect(stepLine({ tool: "claude.Thing", input: {} })).toBe("Thing");
    expect(stepLine({ tool: "Bash", input: { command: "ls\n-la" } })).toBe("ls");
  });

  it("list the files written, not those that failed", () => {
    expect(
      filesWritten([
        {
          activity: [
            { tool: "Write", input: { file_path: "a" }, ok: true },
            { tool: "claude.Edit", input: { file_path: "a" } },
            { tool: "Edit", input: { file_path: "b" }, ok: false },
            { tool: "Read", input: { file_path: "c" } },
          ],
        },
      ]),
    ).toEqual(["a"]);
  });
});
