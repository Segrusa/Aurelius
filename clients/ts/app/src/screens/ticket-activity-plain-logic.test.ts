import type { ActivityView, ApprovalView, MessageView, TaskView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  askOf,
  byWho,
  cardGroups,
  cardState,
  cut,
  factsOf,
  firstSentence,
  intentOf,
  minutes,
  nowSentence,
  optionLabel,
  plainTitle,
  runningPhrase,
  segments,
  stepResult,
  stepText,
  stepWindow,
  type TaskWork,
  workBlocks,
} from "./ticket-activity-plain-logic";

let n = 0;
const call = (tool: string, input: unknown, ok: boolean | null = true, offset = 0): ActivityView => ({
  call_id: `c${n++}`,
  tool,
  input,
  offset,
  ok,
});
const bash = (command: string, ok: boolean | null = true, offset = 0) => call("Bash", { command }, ok, offset);
const edit = (file: string, oldS = "a", newS = "b\nc", offset = 0) =>
  call("Edit", { file_path: file, old_string: oldS, new_string: newS }, true, offset);
const todo = (items: [string, string, string][], offset = 0) =>
  call(
    "TodoWrite",
    { todos: items.map(([content, activeForm, status]) => ({ content, activeForm, status })) },
    true,
    offset,
  );

const task = (over: Partial<TaskView>): TaskView =>
  ({
    id: "t1",
    title: "Desktop: Export CSV",
    status: "running",
    pending_approvals: 0,
    children: [],
    created_at_ms: 0,
    updated_at_ms: 0,
    employee_name: "Flux",
    conversation_id: "c-flux",
    ...over,
  }) as TaskView;

describe("intentOf", () => {
  it("tells tests, builds, commits and lookups apart from other commands", () => {
    expect(intentOf(bash("cargo test -p aurelius-application export"))).toBe("test");
    expect(intentOf(bash("pnpm exec tsc -p clients/ts/app"))).toBe("build");
    expect(intentOf(bash("git commit -m 'export'"))).toBe("commit");
    expect(intentOf(bash("git status --short"))).toBe("lookup");
    expect(intentOf(bash("ls -la"))).toBe("lookup");
    expect(intentOf(bash("pnpm install"))).toBe("command");
    expect(intentOf(call("Grep", { pattern: "x" }))).toBe("lookup");
    expect(intentOf(call("claude.Edit", { file_path: "a.ts" }))).toBe("edit");
  });
});

describe("runningPhrase", () => {
  it("names what runs in plain words and never names a lookup", () => {
    expect(runningPhrase(bash("pnpm vitest run"))).toBe("Running the tests…");
    expect(runningPhrase(edit("crates/app/src/board/export.rs"))).toBe("Changing export.rs…");
    expect(runningPhrase(bash("grep -rn foo ."))).toBe("Looking through the code…");
    expect(runningPhrase(call("Read", { file_path: "a.ts" }))).toBe("Looking through the code…");
  });
});

describe("factsOf", () => {
  it("counts changed files with their lines, and says how the tests went", () => {
    const facts = factsOf(
      [edit("a.ts"), edit("b.ts"), edit("a.ts"), bash("pnpm test", false), bash("pnpm test")],
      true,
    );
    expect(facts.map((f) => f.text)).toEqual(["Changed 2 files +6 −3", "Tests failed once, then passed"]);
    expect(facts[1]?.tone).toBe("good");
  });

  it("says the tests fail when the last run failed", () => {
    expect(factsOf([bash("pnpm test"), bash("pnpm test", false)])).toEqual([{ text: "Tests fail", tone: "bad" }]);
  });

  it("names builds and commits, but no lookups or other commands", () => {
    const facts = factsOf([bash("git status"), bash("pnpm install"), bash("pnpm exec tsc"), bash("git commit -m x")]);
    expect(facts.map((f) => f.text)).toEqual(["Builds", "Saved a commit"]);
  });

  it("only counts files read when nothing else happened", () => {
    expect(factsOf([call("Read", { file_path: "a.ts" }), call("Read", { file_path: "b.ts" })])).toEqual([
      { text: "Read 2 files", tone: "plain" },
    ]);
    expect(factsOf([call("Read", { file_path: "a.ts" }), edit("a.ts")]).map((f) => f.text)).toEqual(["Changed 1 file"]);
  });

  it("leaves the lines out when an edit's input was cut short", () => {
    const cutShort = { ...edit("a.ts"), truncated: true };
    expect(factsOf([cutShort], true).map((f) => f.text)).toEqual(["Changed 1 file"]);
  });

  it("ignores the plan's own calls and calls that still run", () => {
    expect(factsOf([todo([["A", "Doing A", "in_progress"]]), bash("pnpm test", null)])).toEqual([]);
  });
});

describe("nowSentence", () => {
  it("gives the employee's latest own words", () => {
    const text = "Looking at the menu. The menu is built in board.tsx.";
    expect(nowSentence(text, [call("Read", { file_path: "a" }, true, 20)])).toBe("The menu is built in board.tsx.");
  });

  it("gives the phrase of a call that runs after those words", () => {
    expect(nowSentence("Fixed it, testing.", [bash("cargo test", null, 18)])).toBe("Running the tests…");
  });

  it("strips markdown and cuts long words to two lines' worth", () => {
    const long = `**Done:** ${"word ".repeat(80)}`;
    const said = nowSentence(long, []) ?? "";
    expect(said.startsWith("Done: word")).toBe(true);
    expect(said.length).toBeLessThanOrEqual(240);
    expect(said.endsWith("…")).toBe(true);
  });

  it("is null before anything happened", () => {
    expect(nowSentence("", [])).toBeNull();
  });
});

describe("firstSentence and cut", () => {
  it("takes the first sentence of a result", () => {
    expect(firstSentence("Added the export. Every field is quoted.")).toBe("Added the export.");
    expect(firstSentence(null)).toBeNull();
  });
  it("cuts at a word", () => {
    expect(cut("one two three four", 10)).toBe("one two…");
  });
});

describe("cardState", () => {
  it("asks, works, waits for a colleague, stops and is done", () => {
    const trace = task({ id: "t2", employee_name: "Trace", status: "waiting" });
    const flux = task({ parent_task_id: "t2", status: "running" });
    expect(cardState(flux, [trace, flux], true)).toEqual({ kind: "asking", text: "Needs your answer" });
    expect(cardState(flux, [trace, flux], false)).toEqual({ kind: "working", text: "Working" });
    expect(cardState(trace, [trace, flux], false)).toEqual({ kind: "waiting", text: "Waiting for Flux" });
    expect(cardState(task({ status: "waiting" }), [], false).text).toBe("Waiting for you");
    expect(cardState(task({ status: "queued", waiting_reason: "in_line" }), [], false).text).toBe(
      "Waiting for its turn",
    );
    expect(cardState(task({ status: "failed" }), [], false)).toEqual({ kind: "stopped", text: "Stopped" });
    expect(cardState(task({ status: "succeeded" }), [], false)).toEqual({ kind: "done", text: "Done" });
    expect(cardState(task({ status: "succeeded", live_run: "running" }), [], false).kind).toBe("working");
  });
});

describe("cardGroups", () => {
  it("puts asking first, then working, then waiting, ties by last activity; done apart", () => {
    const card = (id: string, kind: "asking" | "working" | "waiting" | "done", at: number) => ({
      task: task({ id, updated_at_ms: at }),
      state: { kind, text: kind },
    });
    const groups = cardGroups([
      card("wait", "waiting", 9),
      card("old", "working", 1),
      card("done", "done", 5),
      card("new", "working", 8),
      card("ask", "asking", 0),
    ]);
    expect(groups.now.map((c) => c.task.id)).toEqual(["ask", "new", "old", "wait"]);
    expect(groups.done.map((c) => c.task.id)).toEqual(["done"]);
  });
});

describe("segments, steps and time", () => {
  const todos = [
    { text: "A", state: "done" as const },
    { text: "B", state: "doing" as const },
    { text: "C", state: "open" as const },
  ];
  it("turns the step at work amber while the employee asks", () => {
    expect(segments(todos, false)).toEqual(["done", "now", "open"]);
    expect(segments(todos, true)).toEqual(["done", "ask", "open"]);
    expect(segments(null, false)).toEqual([]);
  });
  it("says the step, or that it is getting started", () => {
    expect(stepText(todos)).toBe("Step 2 of 3");
    expect(stepText(null)).toBe("Getting started");
    expect(stepText(todos.map((t) => ({ ...t, state: "done" as const })))).toBe("3 of 3 done");
  });
  it("shows at most five steps around the one at work", () => {
    const many = Array.from({ length: 9 }, (_, i) => ({
      text: `S${i}`,
      state: i < 6 ? ("done" as const) : i === 6 ? ("doing" as const) : ("open" as const),
    }));
    const w = stepWindow(many);
    expect(w.shown.map((t) => t.text)).toEqual(["S4", "S5", "S6", "S7", "S8"]);
    expect(w.earlier).toBe(4);
    expect(w.later).toBe(0);
  });
  it("writes time on a task in minutes", () => {
    expect(minutes(20_000)).toBe("<1 min");
    expect(minutes(38 * 60_000)).toBe("38 min");
    expect(minutes(125 * 60_000)).toBe("2 h 5 min");
  });
});

describe("askOf", () => {
  const approval = (over: Partial<ApprovalView>) =>
    ({ id: "a1", status: "pending", run_id: "r1", ...over }) as ApprovalView;
  const post = (over: Partial<MessageView>) =>
    ({ id: "m1", role: "assistant", awaiting_user: true, run_id: "r9", author: "Flux", ...over }) as MessageView;
  const flux = task({});

  it("takes the task's open approval first", () => {
    expect(askOf(flux, [approval({ task_id: "t1" })], [post({})], new Set())).toEqual({
      kind: "approval",
      approval: approval({ task_id: "t1" }),
    });
  });

  it("takes a waiting post by one of the task's runs", () => {
    expect(askOf(flux, [], [post({ run_id: "r9" })], new Set(["r9"]))?.kind).toBe("post");
    expect(askOf(flux, [], [post({ run_id: "r8" })], new Set(["r9"]))).toBeNull();
  });

  it("falls back to the employee's name when the task's runs are not known", () => {
    expect(askOf(flux, [], [post({ author: "Flux" })], new Set())?.kind).toBe("post");
    expect(askOf(flux, [], [post({ author: "Forge" })], new Set())).toBeNull();
  });

  it("skips a post whose question an approval already asks, and a deleted one", () => {
    expect(askOf(flux, [approval({ run_id: "r9" })], [post({})], new Set(["r9"]))).toBeNull();
    expect(askOf(flux, [], [post({ deleted_at_ms: 5 })], new Set(["r9"]))).toBeNull();
  });
});

describe("optionLabel", () => {
  it("turns (Recommended) into the employee's pick", () => {
    expect(optionLabel("Ask me every time (Recommended)")).toEqual({ label: "Ask me every time", pick: true });
    expect(optionLabel("Downloads")).toEqual({ label: "Downloads", pick: false });
  });
});

describe("workBlocks", () => {
  const answer = (id: string, at: number, text: string, activity: ActivityView[]) => ({
    id,
    role: "assistant" as const,
    run_id: "r1",
    text,
    activity,
    created_at_ms: at,
    update: null,
  });
  const work = (messages: TaskWork["messages"], live: TaskWork["live"] = []): TaskWork => ({
    taskId: "t1",
    who: "Forge",
    messages,
    live,
  });

  it("makes one block per plan step, titled by the step and done once the step is", () => {
    const text = "Looking first.Quoting every field now.";
    const blocks = workBlocks([
      work([
        answer("m1", 100, text, [
          todo([
            ["Look at the board read", "Looking at the board read", "in_progress"],
            ["Add the export", "Adding the export", "pending"],
          ]),
          call("Read", { file_path: "board.rs" }),
          todo(
            [
              ["Look at the board read", "Looking at the board read", "completed"],
              ["Add the export", "Adding the export", "in_progress"],
            ],
            14,
          ),
          edit("export.rs", "a", "b", 38),
          bash("cargo test export", true, 38),
        ]),
      ]),
    ]);
    expect(blocks.map((b) => [b.title, b.state, b.steps.length])).toEqual([
      ["Look at the board read", "done", 1],
      ["Adding the export", "open", 2],
    ]);
    expect(blocks[1]?.note).toBe("Quoting every field now.");
  });

  it("keeps one step that runs over two answers as one block", () => {
    const plan = todo([["Add the export", "Adding the export", "in_progress"]]);
    const blocks = workBlocks([
      work([answer("m1", 100, "", [plan, edit("a.rs")]), answer("m2", 200, "", [bash("cargo test")])]),
    ]);
    expect(blocks).toHaveLength(1);
    expect(blocks[0]?.steps).toHaveLength(2);
    expect([blocks[0]?.atMs, blocks[0]?.endMs]).toEqual([100, 200]);
  });

  it("marks a block with a call that runs as live, and puts a streaming answer last", () => {
    const blocks = workBlocks([
      work(
        [answer("m1", 100, "", [edit("a.rs")])],
        [{ message_id: "m2", run_id: "r1", text: "", activity: [bash("cargo test", null)] }],
      ),
    ]);
    expect(blocks.map((b) => [b.title, b.state])).toEqual([
      ["Changed the code", "done"],
      ["Ran the tests", "live"],
    ]);
    expect(blocks[1]?.atMs).toBe(Number.POSITIVE_INFINITY);
  });

  it("leaves out answers without calls and without words", () => {
    expect(workBlocks([work([answer("m1", 100, "", [])])])).toEqual([]);
  });
});

describe("plainTitle, stepResult and byWho", () => {
  it("titles work without a plan by what it did", () => {
    expect(plainTitle([call("Read", { file_path: "a" }), bash("ls")], null)).toBe("Looked through the code");
    expect(plainTitle([], "The board read returns the owner. More.")).toBe("The board read returns the owner.");
  });
  it("gives a step's result", () => {
    expect(stepResult(edit("a.ts", "x", "y\nz"))).toEqual({ text: "+2 −1", tone: "good" });
    expect(stepResult(bash("cargo test", null))).toEqual({ text: "running…", tone: "plain" });
    expect(stepResult(bash("ls", false))).toEqual({ text: "failed", tone: "bad" });
  });
  it("filters the talk on one employee", () => {
    expect(byWho({ role: "assistant", author: "Forge" }, "Forge")).toBe(true);
    expect(byWho({ role: "user", author: null }, "Forge")).toBe(false);
    expect(byWho({ role: "assistant", author: "Flux" }, null)).toBe(true);
  });
});
