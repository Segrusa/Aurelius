import type { TaskView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import { filesWritten, stepLine, subtasksOf, taskBackPath, taskStateLine, treeOrder } from "./tasks-logic";

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

  it("says a task's state", () => {
    expect(taskStateLine(task("a", { pending_approvals: 1 })).text).toBe("Paused · needs approval");
    expect(taskStateLine(task("a", { status: "failed" })).text).toBe("Failed");
    expect(taskStateLine(task("a", { status: "queued", waiting_reason: "in_line" })).text).toBe("Waiting");
  });
});

describe("a run's steps", () => {
  it("are each one line", () => {
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

describe("leaving a task's run", () => {
  it("goes to the ticket the task works for, else to its employee, else to New session", () => {
    expect(taskBackPath(task("k1", { ticket_id: "t1", ticket_number: 21, employee_id: "e1" }))).toBe("/ticket/21");
    expect(taskBackPath(task("k1", { ticket_id: "t1", employee_id: "e1" }))).toBe("/board?ticket=t1");
    expect(taskBackPath(task("k1", { employee_id: "e1" }))).toBe("/employees/e1");
    expect(taskBackPath(task("k1"))).toBe("/new");
    expect(taskBackPath(undefined)).toBe("/new");
  });
});
