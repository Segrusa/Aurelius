import type { ActivityView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  answerRows,
  diffCounts,
  mergeByTime,
  rowDiff,
  rowFiles,
  runningTitle,
  type TaskWork,
  type WorkRow,
  workInFilter,
  workRows,
  workTitle,
} from "./ticket-activity-logic";

const step = (tool: string, input: unknown, offset = 0, ok: boolean | null = true): ActivityView => ({
  call_id: `${tool}-${JSON.stringify(input)}`,
  tool,
  input,
  offset,
  ok,
});

const base = { id: "m1", taskId: "t1", who: "Flux", atMs: 100, messageId: "m1" };
const brief = (rows: WorkRow[]) =>
  rows.map((r) =>
    r.kind === "steps"
      ? `${r.work}:${r.steps.length}`
      : r.kind === "running"
        ? `running:${r.step.tool}`
        : `finding:${r.text}`,
  );

describe("answerRows", () => {
  it("groups calls of the same kind and splits on another kind", () => {
    const rows = answerRows(base, "", [
      step("Read", { file_path: "a.ts" }),
      step("Read", { file_path: "b.ts" }),
      step("Edit", { file_path: "a.ts", old_string: "x", new_string: "y" }),
      step("Bash", { command: "pnpm tsc" }),
      step("Bash", { command: "pnpm test" }),
    ]);
    expect(brief(rows)).toEqual(["read:2", "edit:1", "command:2"]);
  });

  it("gives the call that still runs a row of its own", () => {
    const rows = answerRows(base, "", [
      step("Read", { file_path: "a.ts" }),
      step("Bash", { command: "pnpm e2e" }, 0, null),
    ]);
    expect(brief(rows)).toEqual(["read:1", "running:Bash"]);
  });

  it("shows text before a call as a finding, but not the answer after the last call", () => {
    const text = "The hit area is too small.Fixed it.";
    const rows = answerRows(base, text, [
      step("Read", { file_path: "a.ts" }, 0),
      step("Edit", { file_path: "a.ts", old_string: "a", new_string: "b" }, 26),
    ]);
    expect(brief(rows)).toEqual(["read:1", "finding:The hit area is too small.", "edit:1"]);
  });

  it("leaves out the todo list and tools that are no read, edit or command", () => {
    const rows = answerRows(base, "", [
      step("TodoWrite", { todos: [] }),
      step("Grep", { pattern: "x" }),
      step("Read", { file_path: "a.ts" }),
    ]);
    expect(brief(rows)).toEqual(["read:1"]);
  });
});

describe("workRows", () => {
  const msg = (id: string, at: number, activity: ActivityView[], over = {}) => ({
    id,
    role: "assistant" as const,
    run_id: "r1",
    text: "",
    activity,
    created_at_ms: at,
    update: null,
    ...over,
  });

  it("orders the work of all tasks by time and skips what is no run's answer", () => {
    const tasks: TaskWork[] = [
      { taskId: "t1", who: "Flux", messages: [msg("a", 300, [step("Read", { file_path: "a.ts" })])], live: [] },
      {
        taskId: "t2",
        who: "Forge",
        messages: [
          msg("b", 200, [step("Bash", { command: "cargo test" })]),
          msg("c", 100, [step("Read", { file_path: "c.rs" })], { role: "user" }),
        ],
        live: [],
      },
    ];
    expect(workRows(tasks).map((r) => `${r.who}:${r.kind === "steps" ? r.work : r.kind}`)).toEqual([
      "Forge:command",
      "Flux:read",
    ]);
  });

  it("takes what streams over the stored answer and puts an unknown stream last", () => {
    const tasks: TaskWork[] = [
      {
        taskId: "t1",
        who: "Flux",
        messages: [msg("a", 100, [])],
        live: [
          { message_id: "a", run_id: "r1", text: "", activity: [step("Bash", { command: "pnpm e2e" }, 0, null)] },
          { message_id: "z", run_id: "r1", text: "", activity: [step("Read", { file_path: "z.ts" })] },
        ],
      },
    ];
    const rows = workRows(tasks);
    expect(brief(rows)).toEqual(["running:Bash", "read:1"]);
    expect(rows[1]?.atMs).toBe(Number.POSITIVE_INFINITY);
  });
});

describe("mergeByTime", () => {
  it("puts each work row before the first message after it", () => {
    const merged = mergeByTime(
      [{ created_at_ms: 100 }, { created_at_ms: 300 }],
      [{ atMs: 100 }, { atMs: 200 }, { atMs: Number.POSITIVE_INFINITY }],
    );
    expect(merged.map((r) => (r.kind === "message" ? `m${r.m.created_at_ms}` : `w${r.w.atMs}`))).toEqual([
      "m100",
      "w100",
      "w200",
      "m300",
      "wInfinity",
    ]);
  });
});

describe("the filter", () => {
  it("counts work as Changes and keeps it out of Messages", () => {
    expect(workInFilter("all")).toBe(true);
    expect(workInFilter("changes")).toBe(true);
    expect(workInFilter("messages")).toBe(false);
  });
});

describe("titles, files and diffs", () => {
  it("names a row by its files and commands", () => {
    const reads = [
      step("Read", { file_path: "src/a.ts" }),
      step("Read", { file_path: "src/a.ts" }),
      step("Read", { file_path: "b.ts" }),
    ];
    expect(workTitle("read", reads)).toBe("Read 2 files");
    expect(rowFiles(reads)).toEqual(["src/a.ts", "b.ts"]);
    expect(workTitle("edit", [step("Edit", { file_path: "a.ts" })])).toBe("Edited 1 file");
    expect(workTitle("command", [step("Bash", { command: "x" })])).toBe("Ran a command");
    expect(workTitle("command", [step("Bash", { command: "x" }), step("Bash", { command: "y" }, 0, false)])).toBe(
      "Ran 2 commands · 1 failed",
    );
    expect(runningTitle(step("Bash", { command: "x" }, 0, null))).toBe("Running a command");
    expect(runningTitle(step("WebSearch", { query: "x" }, 0, null))).toBe("Running WebSearch");
  });

  it("builds the diff from edits, multi-edits and writes", () => {
    const lines = rowDiff([
      step("Edit", { file_path: "src/a.ts", old_string: "const a = 1;", new_string: "const a = 2;\nconst b = 3;" }),
      step("MultiEdit", { file_path: "b.ts", edits: [{ old_string: "x", new_string: "y" }] }),
      step("Write", { file_path: "c.ts", content: "new" }),
      step("Edit", { file_path: "d.ts", old_string: "gone", new_string: "kept" }, 0, false),
    ]);
    expect(lines).toEqual([
      { sign: "file", text: "a.ts" },
      { sign: "-", text: "const a = 1;" },
      { sign: "+", text: "const a = 2;" },
      { sign: "+", text: "const b = 3;" },
      { sign: "file", text: "b.ts" },
      { sign: "-", text: "x" },
      { sign: "+", text: "y" },
      { sign: "file", text: "c.ts" },
      { sign: "+", text: "new" },
    ]);
    expect(diffCounts(lines)).toEqual({ added: 4, removed: 2 });
  });
});
