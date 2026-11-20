// The ticket page of ticket #74: what each task of a ticket does now, how far its plan is, and
// which part of the ticket's talk the Activity filter shows. Pure, so the Now card, the Plan
// panel and the live bar above the composer say the same.

import type { ActivityView, MessageView, TaskView } from "@aurelius/protocol";
import type { Todo } from "./chat-logic";
import { runLabel } from "./chat-logic";
import { stepLine } from "./tasks-logic";
import { elapsed } from "./ticket-overview-logic";

/** What the Activity list of a ticket shows. */
export type ActivityFilter = "all" | "changes" | "messages";

export const ACTIVITY_FILTERS: { key: ActivityFilter; label: string }[] = [
  { key: "all", label: "All" },
  { key: "changes", label: "Changes" },
  { key: "messages", label: "Messages" },
];

const tool = (a: { tool: string }) => a.tool.replace(/^claude\./, "");
const WRITING = new Set(["Write", "Edit", "MultiEdit", "NotebookEdit"]);
const COMMANDS = new Set(["Bash", "PowerShell"]);
const READING = new Set(["Read"]);

const writes = (activity: readonly ActivityView[]) => activity.some((a) => WRITING.has(tool(a)) && a.ok !== false);

/**
 * Whether a message of the ticket's talk shows under a filter. Changes: the system's lines (moved,
 * assigned, files added), task updates and answers that changed files. Messages: what people and
 * employees wrote.
 */
export function inFilter(
  m: Pick<MessageView, "system" | "update" | "activity" | "role">,
  filter: ActivityFilter,
): boolean {
  const line = Boolean(m.system) || Boolean(m.update);
  if (filter === "changes") return line || writes(m.activity);
  if (filter === "messages") return !line && (m.role === "user" || m.role === "assistant");
  return true;
}

/** What one run did so far, for the counters of the Now card. */
export interface RunCounts {
  read: number;
  changed: number;
  /** Lines added and removed over the edits; null when an input was cut short and the sum would lie. */
  added: number | null;
  removed: number | null;
  commands: number;
}

const lines = (v: unknown): number => (typeof v === "string" && v !== "" ? v.split("\n").length : 0);

/** Files read and changed (each counted once), commands run, and the lines the edits added and removed. */
export function runCounts(activity: readonly ActivityView[]): RunCounts {
  const read = new Set<string>();
  const changed = new Set<string>();
  let added = 0;
  let removed = 0;
  let exact = true;
  let commands = 0;
  for (const a of activity) {
    if (a.ok === false) continue;
    const name = tool(a);
    const input = (a.input ?? {}) as Record<string, unknown>;
    const path = typeof input.file_path === "string" ? input.file_path : null;
    if (READING.has(name) && path) read.add(path);
    else if (COMMANDS.has(name)) commands += 1;
    else if (WRITING.has(name)) {
      if (path) changed.add(path);
      if (a.truncated) exact = false;
      if (name === "Write") added += lines(input.content);
      else if (name === "Edit") {
        added += lines(input.new_string);
        removed += lines(input.old_string);
      } else if (name === "MultiEdit" && Array.isArray(input.edits)) {
        for (const e of input.edits as { new_string?: unknown; old_string?: unknown }[]) {
          added += lines(e?.new_string);
          removed += lines(e?.old_string);
        }
      } else exact = false;
    }
  }
  return {
    read: read.size,
    changed: changed.size,
    added: exact ? added : null,
    removed: exact ? removed : null,
    commands,
  };
}

/** The calls of one run: its finished answers in the task's conversation and what still streams. */
export function runActivity(
  messages: readonly Pick<MessageView, "run_id" | "activity">[],
  live: readonly { run_id: string; activity: ActivityView[] }[],
  runId: string | null,
): ActivityView[] {
  if (!runId) return [];
  return [
    ...messages.filter((m) => m.run_id === runId).flatMap((m) => m.activity),
    ...live.filter((u) => u.run_id === runId).flatMap((u) => u.activity),
  ];
}

/** Where a plan stands: the step at work (1-based), the steps done, and how many there are. */
export interface PlanStep {
  at: number;
  done: number;
  total: number;
}

export function planStep(todos: readonly Todo[] | null): PlanStep | null {
  if (!todos || todos.length === 0) return null;
  const done = todos.filter((t) => t.state === "done").length;
  const doing = todos.findIndex((t) => t.state === "doing");
  const at = doing >= 0 ? doing + 1 : Math.min(done + 1, todos.length);
  return { at, done, total: todos.length };
}

const clock = (ms: number) =>
  new Date(ms).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", hour12: false });

/** "Step 3 of 4 · working for 6m 12s · started 14:31"; the parts that are not known stay out. */
export function progressLine(step: PlanStep | null, startedAtMs: number | null, nowMs: number): string {
  const parts: string[] = [];
  if (step) parts.push(`Step ${step.at} of ${step.total}`);
  if (startedAtMs !== null) {
    parts.push(`working for ${elapsed(nowMs - startedAtMs)}`);
    parts.push(`started ${clock(startedAtMs)}`);
  }
  return parts.join(" · ");
}

/** The head of a task's Now row: who, and what they do ("Flux · Fixing the hit area"), else the task. */
export function nowTitle(name: string | null | undefined, todos: readonly Todo[] | null, task: string): string {
  const doing = todos?.find((t) => t.state === "doing")?.text ?? task;
  return name ? `${name} · ${doing}` : doing;
}

/** The call that runs right now, as the live bar shows it: "Running a command" and its command. */
export function currentCall(activity: readonly ActivityView[]): { label: string; detail: string } | null {
  const open = [...activity].reverse().find((a) => a.ok === null || a.ok === undefined);
  if (!open) return null;
  return { label: runLabel([open]), detail: stepLine(open) };
}

/** The last few calls of a run, newest last, for the open step of the Plan. */
export function lastSteps(
  activity: readonly ActivityView[],
  n: number,
): { id: string; text: string; state: "ok" | "failed" | "running" }[] {
  return activity.slice(-n).map((a) => ({
    id: a.call_id,
    text: stepLine(a),
    state: a.ok === false ? "failed" : a.ok === null || a.ok === undefined ? "running" : "ok",
  }));
}

/** A task that has something to say right now: at work, or waiting on someone. */
export function busy(t: TaskView): boolean {
  if (t.live_run) return true;
  if (t.pending_approvals > 0) return true;
  return t.status === "running" || t.status === "waiting" || (t.status === "queued" && !t.waiting_reason);
}

/** The tasks of the Now card: the main task first, then its delegations in the order they were made. */
export function nowTasks(tasks: readonly TaskView[], rootId: string | null | undefined): TaskView[] {
  const shown = tasks.filter(busy);
  return [...shown.filter((t) => t.id === rootId), ...shown.filter((t) => t.id !== rootId)];
}

/** "1 question for you", "2 questions for you". */
export const questionsText = (n: number) => (n === 1 ? "1 question for you" : `${n} questions for you`);
