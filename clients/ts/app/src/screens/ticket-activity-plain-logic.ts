// The ticket's Activity and Timeline in plain words (ticket #83, design/ticket-activity-plain.html):
// one card per delegated task with its state, its plan and what it does now; the work in the
// Timeline as one block per employee per plan step, with a few facts instead of every call. All
// of it is decided here without a model call, so it costs nothing and can be tested.

import type { ActivityView, ApprovalView, MessageView, TaskView } from "@aurelius/protocol";
import { answerParts } from "./answer-steps-logic";
import { isProgressTool, type Todo } from "./chat-logic";
import { rowFiles } from "./ticket-activity-logic";
import { todosAcross } from "./ticket-overview-logic";
import { busy } from "./ticket-page-logic";

const bare = (tool: string) => tool.replace(/^claude\./, "");
const running = (a: Pick<ActivityView, "ok">) => a.ok === null || a.ok === undefined;
const progress = (a: Pick<ActivityView, "tool">) => isProgressTool(bare(a.tool));

// ---- What a call is for ------------------------------------------------------------

/** What a call does, in the words a board member uses; lookups are never named on their own. */
export type Intent = "test" | "build" | "commit" | "command" | "edit" | "read" | "lookup" | "web" | "helper" | "other";

const COMMIT = /\bgit\s+(-C\s+\S+\s+)?commit\b/;
const TEST = /\b(test|tests|vitest|jest|pytest|nextest|playwright|e2e)\b/i;
const BUILD = /\b(build|tsc|cargo\s+(check|clippy)|flutter\s+analyze)\b/i;
const LOOKUP =
  /^\s*(ls|cat|head|tail|find|grep|rg|pwd|wc|tree|echo|git\s+(-C\s+\S+\s+)?(status|log|diff|show|branch))\b/;

const TOOL_INTENT: Record<string, Intent> = {
  Edit: "edit",
  MultiEdit: "edit",
  Write: "edit",
  NotebookEdit: "edit",
  Read: "read",
  NotebookRead: "read",
  Grep: "lookup",
  Glob: "lookup",
  LS: "lookup",
  WebSearch: "web",
  WebFetch: "web",
  Task: "helper",
  Agent: "helper",
};

const commandOf = (a: Pick<ActivityView, "input">): string => {
  const command = (a.input as { command?: unknown } | null)?.command;
  return typeof command === "string" ? command : "";
};

/** What a call is for: a test run, a build, a commit, another command, an edit, a read or a lookup. */
export function intentOf(a: Pick<ActivityView, "tool" | "input">): Intent {
  const tool = bare(a.tool);
  if (tool === "Bash" || tool === "PowerShell" || tool === "BashOutput") {
    const command = commandOf(a);
    if (COMMIT.test(command)) return "commit";
    if (TEST.test(command)) return "test";
    if (BUILD.test(command)) return "build";
    if (LOOKUP.test(command)) return "lookup";
    return "command";
  }
  return TOOL_INTENT[tool] ?? "other";
}

const shortName = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** The fixed phrase for a call that runs while the employee wrote nothing new: "Running the tests…". */
export function runningPhrase(a: Pick<ActivityView, "tool" | "input">): string {
  switch (intentOf(a)) {
    case "test":
      return "Running the tests…";
    case "build":
      return "Building…";
    case "commit":
      return "Saving a commit…";
    case "command":
      return "Running a command…";
    case "edit": {
      const file = rowFiles([a as ActivityView])[0];
      const verb = bare(a.tool) === "Write" ? "Writing" : "Changing";
      return file ? `${verb} ${shortName(file)}…` : `${verb} a file…`;
    }
    case "read":
    case "lookup":
      return "Looking through the code…";
    case "web":
      return "Searching the web…";
    case "helper":
      return "Working with a helper…";
    default:
      return "Working…";
  }
}

// ---- Facts ---------------------------------------------------------------------------

/** One fact chip: "Changed 3 files", "Tests pass". `good` and `bad` colour it; the text says it too. */
export interface Fact {
  text: string;
  tone: "plain" | "good" | "bad";
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
const lines = (v: unknown): number => (typeof v === "string" && v !== "" ? v.split("\n").length : 0);

/** Lines added and removed by one edit; null when its input was cut short and a sum would lie. */
export function editLines(
  a: Pick<ActivityView, "tool" | "input" | "truncated">,
): { added: number; removed: number } | null {
  if (a.truncated) return null;
  const input = (a.input ?? {}) as Record<string, unknown>;
  switch (bare(a.tool)) {
    case "Write":
      return { added: lines(input.content), removed: 0 };
    case "Edit":
      return { added: lines(input.new_string), removed: lines(input.old_string) };
    case "MultiEdit": {
      if (!Array.isArray(input.edits)) return null;
      let added = 0;
      let removed = 0;
      for (const e of input.edits as { new_string?: unknown; old_string?: unknown }[]) {
        added += lines(e?.new_string);
        removed += lines(e?.old_string);
      }
      return { added, removed };
    }
    default:
      return null;
  }
}

/** How the test or build runs of some work went, as one fact. */
function outcome(runs: readonly ActivityView[], kind: "test" | "build"): Fact | null {
  const done = runs.filter((a) => !running(a));
  const last = done.at(-1);
  if (!last) return null;
  if (kind === "build") return last.ok ? { text: "Builds", tone: "good" } : { text: "Build fails", tone: "bad" };
  if (last.ok === false) return { text: "Tests fail", tone: "bad" };
  const failed = done.filter((a) => a.ok === false).length;
  if (failed === 0) return { text: "Tests pass", tone: "good" };
  const times = failed === 1 ? "once" : `${failed} times`;
  return { text: `Tests failed ${times}, then passed`, tone: "good" };
}

/**
 * The facts of some work: files changed (with the lines when `withLines`), how the tests and the
 * build went, commits saved. Files read only count when nothing else happened. Lookups and other
 * commands are never mentioned.
 */
export function factsOf(steps: readonly ActivityView[], withLines = false): Fact[] {
  const calls = steps.filter((a) => !progress(a));
  const facts: Fact[] = [];
  const edits = calls.filter((a) => intentOf(a) === "edit" && a.ok !== false && !running(a));
  const files = rowFiles(edits);
  if (files.length > 0) {
    let text = `Changed ${plural(files.length, "file", "files")}`;
    if (withLines) {
      const counts = edits.map(editLines);
      if (counts.every((c) => c !== null)) {
        const added = counts.reduce((s, c) => s + (c?.added ?? 0), 0);
        const removed = counts.reduce((s, c) => s + (c?.removed ?? 0), 0);
        text += ` +${added} −${removed}`;
      }
    }
    facts.push({ text, tone: "plain" });
  }
  const tests = outcome(
    calls.filter((a) => intentOf(a) === "test"),
    "test",
  );
  if (tests) facts.push(tests);
  const build = outcome(
    calls.filter((a) => intentOf(a) === "build"),
    "build",
  );
  if (build) facts.push(build);
  const commits = calls.filter((a) => intentOf(a) === "commit" && a.ok === true).length;
  if (commits > 0) facts.push({ text: commits === 1 ? "Saved a commit" : `Saved ${commits} commits`, tone: "plain" });
  if (facts.length === 0) {
    const read = rowFiles(calls.filter((a) => intentOf(a) === "read" && a.ok !== false));
    if (read.length > 0) facts.push({ text: `Read ${plural(read.length, "file", "files")}`, tone: "plain" });
  }
  return facts;
}

// ---- Now ------------------------------------------------------------------------------

/** How many characters of the employee's own words the "Now" line keeps (it shows two lines). */
export const NOW_CHARS = 240;

/** Text as one plain line: no markdown marks, no runs of spaces. */
export function plainLine(text: string): string {
  return text
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/^\s{0,3}(#{1,6}|[-*+]|\d+\.)\s+/gm, "")
    .replace(/[*_`>]+/g, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\s+/g, " ")
    .trim();
}

/** At most `max` characters, cut at a word, with an ellipsis when cut. */
export function cut(text: string, max: number): string {
  if (text.length <= max) return text;
  const head = text.slice(0, max - 1);
  const space = head.lastIndexOf(" ");
  return `${(space > max / 2 ? head.slice(0, space) : head).trimEnd()}…`;
}

/**
 * What an employee does now, from its latest answer: its own latest words; while a call runs after
 * those words, the fixed phrase of that call. Null when there is nothing to say yet.
 */
export function nowSentence(text: string, activity: readonly ActivityView[]): string | null {
  const parts = answerParts(
    text,
    activity.filter((a) => !progress(a)),
  );
  const last = parts.at(-1);
  if (last?.kind === "steps") {
    const open = [...last.steps].reverse().find(running);
    if (open) return runningPhrase(open);
  }
  const words = [...parts].reverse().find((p) => p.kind === "text");
  if (words?.kind === "text") {
    const line = plainLine(words.text);
    if (line !== "") return cut(line, NOW_CHARS);
  }
  const call = last?.kind === "steps" ? last.steps.at(-1) : undefined;
  return call ? runningPhrase(call) : null;
}

/** The first sentence of a task's result, for "Who did what". */
export function firstSentence(text: string | null | undefined, max = NOW_CHARS): string | null {
  const line = plainLine(text ?? "");
  if (line === "") return null;
  const end = /[.!?](\s|$)/.exec(line);
  return cut(end ? line.slice(0, end.index + 1) : line, max);
}

// ---- Cards ----------------------------------------------------------------------------

/** The state chip of a card. */
export type CardKind = "asking" | "working" | "waiting" | "stopped" | "done";

export interface CardState {
  kind: CardKind;
  text: string;
}

/**
 * The state of a task's card: it needs your answer, works, waits (for a colleague it gave work
 * to, for you, for its turn), stopped, or is done.
 */
export function cardState(task: TaskView, tasks: readonly TaskView[], asking: boolean): CardState {
  if (asking) return { kind: "asking", text: "Needs your answer" };
  if (task.live_run === "running" || task.status === "running") return { kind: "working", text: "Working" };
  if (task.live_run) return { kind: "waiting", text: "Starting" };
  switch (task.status) {
    case "waiting": {
      const child = tasks.find((c) => c.parent_task_id === task.id && busy(c) && c.employee_name);
      return { kind: "waiting", text: child ? `Waiting for ${child.employee_name}` : "Waiting for you" };
    }
    case "queued":
      return {
        kind: "waiting",
        text:
          task.waiting_reason === "scheduled" ? "Planned" : task.waiting_reason ? "Waiting for its turn" : "Starting",
      };
    case "draft":
      return { kind: "waiting", text: "Not started" };
    case "failed":
    case "cancelled":
      return { kind: "stopped", text: "Stopped" };
    default:
      return { kind: "done", text: "Done" };
  }
}

const ORDER: Record<CardKind, number> = { asking: 0, working: 1, waiting: 2, stopped: 3, done: 4 };

/**
 * The cards of the Activity tab: every task but the main one (that is the Plan). Working now holds
 * the ones that ask, work, wait or stopped, in that order and then by last activity; Done the rest.
 */
export function cardGroups<T extends { task: TaskView; state: CardState }>(
  cards: readonly T[],
): { now: T[]; done: T[] } {
  const sorted = [...cards].sort(
    (a, b) => ORDER[a.state.kind] - ORDER[b.state.kind] || b.task.updated_at_ms - a.task.updated_at_ms,
  );
  return { now: sorted.filter((c) => c.state.kind !== "done"), done: sorted.filter((c) => c.state.kind === "done") };
}

/** One segment of a status line: done, at work now, asking you, or still open. */
export type Segment = "done" | "now" | "ask" | "open";

/**
 * One segment per plan step. Every step at work is lit, since parts delegated in parallel run at
 * once; while the employee asks you, the first of them (or the next open step) turns amber.
 */
export function segments(todos: readonly Todo[] | null, asking: boolean): Segment[] {
  if (!todos || todos.length === 0) return [];
  const doing = todos.findIndex((t) => t.state === "doing");
  const ask = !asking ? -1 : doing >= 0 ? doing : todos.findIndex((t) => t.state !== "done");
  return todos.map((t, i) => (t.state === "done" ? "done" : i === ask ? "ask" : t.state === "doing" ? "now" : "open"));
}

/** How many plan steps a card lists before it folds the rest away. */
export const CARD_STEPS_SHOWN = 5;

/** The steps a card lists: a window of at most `max` around the step at work, and what it leaves out. */
export function stepWindow(
  todos: readonly Todo[],
  max = CARD_STEPS_SHOWN,
): { shown: Todo[]; first: number; earlier: number; later: number } {
  if (todos.length <= max) return { shown: [...todos], first: 0, earlier: 0, later: 0 };
  const doing = todos.findIndex((t) => t.state === "doing");
  const at = doing >= 0 ? doing : todos.findIndex((t) => t.state !== "done");
  const first = Math.max(0, Math.min((at < 0 ? todos.length : at) - 1, todos.length - max));
  return { shown: todos.slice(first, first + max), first, earlier: first, later: todos.length - first - max };
}

/**
 * "Step 3 of 4"; "Steps 2–4 of 7" when a run of steps is at work at once, "Steps 2, 5 of 7" when
 * they are apart; or "Getting started" before there is a plan.
 */
export function stepText(todos: readonly Todo[] | null): string {
  if (!todos || todos.length === 0) return "Getting started";
  const done = todos.filter((t) => t.state === "done").length;
  if (done === todos.length) return `${done} of ${done} done`;
  const doing = todos.flatMap((t, i) => (t.state === "doing" ? [i + 1] : []));
  if (doing.length === 0) return `Step ${done + 1} of ${todos.length}`;
  if (doing.length === 1) return `Step ${doing[0]} of ${todos.length}`;
  const first = doing[0];
  const last = doing[doing.length - 1];
  const run = last - first + 1 === doing.length;
  return `Steps ${run ? `${first}–${last}` : doing.join(", ")} of ${todos.length}`;
}

/** "<1 min", "38 min", "2 h 5 min": time on a task. */
export function minutes(ms: number): string {
  const m = Math.floor(Math.max(0, ms) / 60_000);
  if (m < 1) return "<1 min";
  if (m < 60) return `${m} min`;
  return m % 60 === 0 ? `${Math.floor(m / 60)} h` : `${Math.floor(m / 60)} h ${m % 60} min`;
}

/** How long a task has been on: until now while it is open, until its last change once it ended. */
export function taskTime(task: Pick<TaskView, "created_at_ms" | "updated_at_ms" | "status">, nowMs: number): string {
  const ended = task.status === "succeeded" || task.status === "failed" || task.status === "cancelled";
  return minutes((ended ? task.updated_at_ms : nowMs) - task.created_at_ms);
}

// ---- Questions ------------------------------------------------------------------------

/** What a task asks the board member: an open approval or question of its run, or a post that waits. */
export type Ask = { kind: "approval"; approval: ApprovalView } | { kind: "post"; message: MessageView };

/**
 * The question a task's card holds: first an open approval of the task, else the newest post in
 * the ticket's discussion that waits for a board member, by one of the task's runs (or, when its
 * runs are not known, by its employee) and not already asked by an approval.
 */
export function askOf(
  task: Pick<TaskView, "id" | "conversation_id" | "employee_name">,
  approvals: readonly ApprovalView[],
  discussion: readonly MessageView[],
  runIds: ReadonlySet<string>,
): Ask | null {
  const approval = approvals.find(
    (a) =>
      a.status === "pending" &&
      (a.task_id === task.id || (task.conversation_id && a.conversation_id === task.conversation_id)),
  );
  if (approval) return { kind: "approval", approval };
  const post = [...discussion]
    .reverse()
    .find(
      (m) =>
        m.awaiting_user &&
        !m.deleted_at_ms &&
        m.role === "assistant" &&
        (m.run_id && runIds.size > 0 ? runIds.has(m.run_id) : m.author === task.employee_name) &&
        !approvals.some((a) => a.status === "pending" && a.run_id === m.run_id),
    );
  return post ? { kind: "post", message: post } : null;
}

const PICK = /\s*\((recommended|aanbevolen)\)\s*$/i;

/** An option as the card shows it: without "(Recommended)", which becomes "<Name>'s pick". */
export function optionLabel(label: string): { label: string; pick: boolean } {
  return PICK.test(label) ? { label: label.replace(PICK, ""), pick: true } : { label, pick: false };
}

// ---- Timeline -------------------------------------------------------------------------

/** One task of the ticket with what its conversation holds. */
export interface TaskWork {
  taskId: string;
  who: string | null;
  messages: readonly Pick<MessageView, "id" | "role" | "run_id" | "text" | "activity" | "created_at_ms" | "update">[];
  /** What streams in the task's conversation now; it is newer than the message it belongs to. */
  live: readonly { message_id: string; run_id: string; text: string; activity: ActivityView[] }[];
}

/** One block of the Timeline: an employee's work on one step of its plan. */
export interface WorkBlock {
  id: string;
  taskId: string;
  who: string | null;
  /** When it started and when its latest answer was; a block that only streams is +Infinity. */
  atMs: number;
  endMs: number;
  /** The plan step ("Added the export to the server"), else a phrase from what it did. */
  title: string;
  /** The employee's own latest words in it. */
  note: string | null;
  state: "live" | "done" | "open";
  /** Its calls, without the plan's own. */
  steps: ActivityView[];
}

interface Building {
  block: WorkBlock;
  step: number | null;
  /** Whether the title came from the plan. */
  planned: boolean;
}

/** A title for work without a plan step, from what it did. */
export function plainTitle(steps: readonly ActivityView[], note: string | null): string {
  const kinds = new Set(steps.map(intentOf));
  if (kinds.has("edit")) return "Changed the code";
  if (kinds.has("test")) return "Ran the tests";
  if (kinds.has("build")) return "Built it";
  if (kinds.has("commit")) return "Saved a commit";
  const said = firstSentence(note, 80);
  if (said) return said;
  if (kinds.size > 0 && [...kinds].every((k) => k === "read" || k === "lookup")) return "Looked through the code";
  return "Worked on it";
}

/**
 * The work of every task as blocks, oldest first: one per employee per plan step (a step that runs
 * over several answers stays one block), with the employee's words between the calls as its note.
 * Work before there is a plan is one block per answer.
 */
export function workBlocks(tasks: readonly TaskWork[]): WorkBlock[] {
  const out: WorkBlock[] = [];
  for (const t of tasks) {
    const blocks: Building[] = [];
    let todos: Todo[] | null = null;
    let current: Building | null = null;
    // The plan's calls so far, per answer: a later answer updates the list an earlier one made.
    const progressCalls: ActivityView[][] = [];
    const liveOf = new Map(t.live.map((u) => [u.message_id, u]));
    const answers = [
      ...t.messages
        .filter((m) => m.role === "assistant" && m.run_id && !m.update)
        .map((m) => {
          const u = liveOf.get(m.id);
          return { id: m.id, atMs: m.created_at_ms, text: u?.text ?? m.text, activity: u?.activity ?? m.activity };
        }),
      ...t.live
        .filter((u) => !t.messages.some((m) => m.id === u.message_id))
        .map((u) => ({ id: u.message_id, atMs: Number.POSITIVE_INFINITY, text: u.text, activity: u.activity })),
    ];
    const stepNow = () => {
      const i = todos?.findIndex((x) => x.state === "doing") ?? -1;
      return i >= 0 ? i : null;
    };
    const open = (answer: { id: string; atMs: number }, key: string): Building => {
      const step = stepNow();
      const todo = step !== null ? todos?.[step] : undefined;
      const b: Building = {
        block: {
          id: `${answer.id}:${key}`,
          taskId: t.taskId,
          who: t.who,
          atMs: answer.atMs,
          endMs: answer.atMs,
          title: todo?.text ?? "",
          note: null,
          state: "open",
          steps: [],
        },
        step,
        planned: todo !== undefined,
      };
      blocks.push(b);
      return b;
    };
    for (const answer of answers) {
      // Work without a plan step does not run on over answers; a step does.
      if (current && current.step === null) current = null;
      if (current) current.block.endMs = Math.max(current.block.endMs, answer.atMs);
      const calls: ActivityView[] = [];
      progressCalls.push(calls);
      for (const part of answerParts(answer.text, answer.activity)) {
        if (part.kind === "text") {
          current ??= open(answer, `t${blocks.length}`);
          current.block.note = part.text;
          continue;
        }
        for (const call of part.steps) {
          if (progress(call)) {
            calls.push(call);
            const next = todosAcross(progressCalls);
            if (!next) continue;
            todos = next;
            for (const b of blocks) {
              const todo = b.step !== null ? todos[b.step] : undefined;
              if (b.planned && todo?.state === "done") {
                b.block.state = "done";
                b.block.title = todo.text;
              }
            }
            if (current && current.step !== stepNow()) current = null;
            continue;
          }
          current ??= open(answer, call.call_id);
          current.block.steps.push(call);
        }
      }
    }
    for (const b of blocks) {
      const block = b.block;
      if (block.steps.length === 0 && !block.note) continue;
      if (block.steps.some(running)) block.state = "live";
      else if (!b.planned && Number.isFinite(block.endMs)) block.state = "done";
      if (!b.planned || block.title === "") block.title = plainTitle(block.steps, block.note);
      out.push(block);
    }
  }
  // A stable sort keeps the order of the blocks of one task.
  return out.sort((a, b) => a.atMs - b.atMs);
}

/** How a call reads in a block's list of steps: "Edited", "Command", with its result. */
export function stepKind(a: Pick<ActivityView, "tool" | "input">): string {
  switch (intentOf(a)) {
    case "edit":
      return bare(a.tool) === "Write" ? "Wrote" : "Edited";
    case "read":
      return "Read";
    case "lookup":
      return "Looked up";
    case "web":
      return "Web";
    case "helper":
      return "Helper";
    case "other":
      return bare(a.tool);
    default:
      return "Command";
  }
}

/** The result of a step at the end of its line: "running…", "failed", "+6 −2", or nothing. */
export function stepResult(a: ActivityView): { text: string; tone: "plain" | "good" | "bad" } | null {
  if (running(a)) return { text: "running…", tone: "plain" };
  if (a.ok === false) return { text: "failed", tone: "bad" };
  if (intentOf(a) === "edit") {
    const n = editLines(a);
    return n ? { text: `+${n.added} −${n.removed}`, tone: "good" } : null;
  }
  if (intentOf(a) === "test" || intentOf(a) === "build") return { text: "passed", tone: "good" };
  return null;
}

/**
 * While the reader is scrolled away from the top of a newest-first list, its rows stay as they
 * were (with fresh content) and new rows are only counted, so nothing jumps under the reader.
 */
export function holdNew<T>(
  rows: readonly T[],
  idOf: (row: T) => string,
  held: ReadonlySet<string> | null,
): { shown: T[]; fresh: number } {
  if (!held) return { shown: [...rows], fresh: 0 };
  const shown = rows.filter((r) => held.has(idOf(r)));
  return { shown, fresh: rows.length - shown.length };
}

/** Whether a message of the ticket's talk belongs to an employee picked in the Timeline. */
export function byWho(m: Pick<MessageView, "author" | "role" | "system">, who: string | null): boolean {
  if (who === null) return true;
  if (m.system) return m.system.by === who || m.system.from === who || m.system.to === who;
  return m.role === "assistant" && m.author === who;
}
