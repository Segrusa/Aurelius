// The work of a ticket's runs as rows of its Activity (ticket #76): what each task read, edited and
// ran, the call that runs now, and what the employee wrote between two calls. The calls live in each
// task's own conversation; here they become rows that sit in time between the ticket's talk. Pure,
// so the rows can be tested without a page.

import type { ActivityView, MessageView } from "@aurelius/protocol";
import { answerParts } from "./answer-steps-logic";
import { isProgressTool } from "./chat-logic";
import type { ActivityFilter } from "./ticket-page-logic";

/** The kinds of work that get a row of their own. */
export type WorkKind = "read" | "edit" | "command";

const KINDS: Record<string, WorkKind> = {
  Read: "read",
  NotebookRead: "read",
  Edit: "edit",
  MultiEdit: "edit",
  Write: "edit",
  NotebookEdit: "edit",
  Bash: "command",
  BashOutput: "command",
  PowerShell: "command",
};

const bare = (tool: string) => tool.replace(/^claude\./, "");
/** Read, edit or command; other tools (searches, the web) stay in the task's own conversation. */
export const workKind = (tool: string): WorkKind | null => KINDS[bare(tool)] ?? null;
const running = (a: ActivityView) => a.ok === null || a.ok === undefined;

interface RowBase {
  id: string;
  taskId: string;
  /** The employee of the task, when it has one. */
  who: string | null;
  /** When it happened, as far as known: the time of the answer it is part of. */
  atMs: number;
  /** The answer in the task's conversation, for `activity.get`. */
  messageId: string;
}

/** One row of work in a ticket's Activity. */
export type WorkRow =
  | (RowBase & { kind: "steps"; work: WorkKind; steps: ActivityView[] })
  | (RowBase & { kind: "running"; step: ActivityView })
  | (RowBase & { kind: "finding"; text: string });

/** One task of the ticket with what its conversation holds. */
export interface TaskWork {
  taskId: string;
  who: string | null;
  messages: readonly Pick<MessageView, "id" | "role" | "run_id" | "text" | "activity" | "created_at_ms" | "update">[];
  /** What streams in the task's conversation now; it is newer than the message it belongs to. */
  live: readonly { message_id: string; run_id: string; text: string; activity: ActivityView[] }[];
}

/**
 * The rows of one answer: each run of calls of the same kind is one row ("Read 2 files"), a call
 * that still runs is a row of its own, and text the employee wrote before a call is a finding.
 * The text after the last call is the answer itself and stays in the task's conversation.
 */
export function answerRows(base: RowBase, text: string, activity: readonly ActivityView[]): WorkRow[] {
  const shown = activity.filter((a) => !isProgressTool(bare(a.tool)));
  const parts = answerParts(text, shown);
  const out: WorkRow[] = [];
  parts.forEach((part, i) => {
    if (part.kind === "text") {
      if (parts.slice(i + 1).some((p) => p.kind === "steps"))
        out.push({ ...base, id: `${base.id}:t${i}`, kind: "finding", text: part.text });
      return;
    }
    // Whether the next call of the same kind joins the last row: not across a call of another kind.
    let joins = false;
    for (const step of part.steps) {
      const work = workKind(step.tool);
      const last = out.at(-1);
      if (running(step)) {
        out.push({ ...base, id: `${base.id}:${step.call_id}`, kind: "running", step });
        joins = false;
      } else if (!work) {
        joins = false;
      } else if (joins && last?.kind === "steps" && last.work === work) {
        last.steps.push(step);
      } else {
        out.push({ ...base, id: `${base.id}:${step.call_id}`, kind: "steps", work, steps: [step] });
        joins = true;
      }
    }
  });
  return out;
}

/** Every task's work as rows, oldest first; an answer that only streams goes last. */
export function workRows(tasks: readonly TaskWork[]): WorkRow[] {
  const rows: WorkRow[] = [];
  for (const t of tasks) {
    const known = new Set<string>();
    const liveOf = new Map(t.live.map((u) => [u.message_id, u]));
    for (const m of t.messages) {
      if (m.role !== "assistant" || !m.run_id || m.update) continue;
      known.add(m.id);
      const u = liveOf.get(m.id);
      const base = { id: m.id, taskId: t.taskId, who: t.who, atMs: m.created_at_ms, messageId: m.id };
      rows.push(...answerRows(base, u?.text ?? m.text, u?.activity ?? m.activity));
    }
    for (const u of t.live) {
      if (known.has(u.message_id)) continue;
      const base = {
        id: u.message_id,
        taskId: t.taskId,
        who: t.who,
        atMs: Number.POSITIVE_INFINITY,
        messageId: u.message_id,
      };
      rows.push(...answerRows(base, u.text, u.activity));
    }
  }
  // A stable sort keeps the order of the rows of one answer.
  return rows.sort((a, b) => a.atMs - b.atMs);
}

/** Work and findings are changes; Messages shows only what people and employees said. */
export const workInFilter = (filter: ActivityFilter) => filter !== "messages";

/**
 * The ticket's talk and the work rows in one timeline. Each work row goes before the first message
 * that came after it; at the same moment the message comes first.
 */
export function mergeByTime<M extends { created_at_ms: number }, W extends { atMs: number }>(
  messages: readonly M[],
  work: readonly W[],
): ({ kind: "message"; m: M; index: number } | { kind: "work"; w: W })[] {
  const out: ({ kind: "message"; m: M; index: number } | { kind: "work"; w: W })[] = [];
  let w = 0;
  messages.forEach((m, index) => {
    while (w < work.length && (work[w]?.atMs ?? 0) < m.created_at_ms) out.push({ kind: "work", w: work[w++] as W });
    out.push({ kind: "message", m, index });
  });
  while (w < work.length) out.push({ kind: "work", w: work[w++] as W });
  return out;
}

const fileOf = (a: ActivityView): string | null => {
  const input = (a.input ?? {}) as Record<string, unknown>;
  const path = input.file_path ?? input.notebook_path;
  return typeof path === "string" && path !== "" ? path : null;
};

/** The short name of a file for its chip. */
export const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** The files of a row, each once, in the order they were first touched. */
export function rowFiles(steps: readonly ActivityView[]): string[] {
  return [...new Set(steps.map(fileOf).filter((f): f is string => f !== null))];
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** "Read 2 files", "Edited 1 file", "Ran 3 commands"; failed calls are counted after a dot. */
export function workTitle(work: WorkKind, steps: readonly ActivityView[]): string {
  const files = rowFiles(steps).length || steps.length;
  const base =
    work === "read"
      ? `Read ${plural(files, "file", "files")}`
      : work === "edit"
        ? `Edited ${plural(files, "file", "files")}`
        : steps.length === 1
          ? "Ran a command"
          : `Ran ${plural(steps.length, "command", "commands")}`;
  const failed = steps.filter((s) => s.ok === false).length;
  return failed > 0 ? `${base} · ${failed} failed` : base;
}

/** "Running a command", "Reading a file", "Running a tool": the head of the live block. */
export function runningTitle(step: ActivityView): string {
  switch (workKind(step.tool)) {
    case "read":
      return "Reading a file";
    case "edit":
      return "Editing a file";
    case "command":
      return "Running a command";
    default:
      return `Running ${bare(step.tool)}`;
  }
}

/** One line of a diff: what was there, what came in its place, or a file's head. */
export interface DiffLine {
  sign: "-" | "+" | "file";
  text: string;
}

const linesOf = (v: unknown): string[] => (typeof v === "string" && v !== "" ? v.split("\n") : []);

/**
 * What the edits of a row changed, from their inputs: per file its name, then the old lines and
 * the new ones. Edits that only say a file was written (a notebook) give the file and nothing more.
 */
export function rowDiff(steps: readonly ActivityView[]): DiffLine[] {
  const out: DiffLine[] = [];
  for (const s of steps) {
    if (s.ok === false) continue;
    const input = (s.input ?? {}) as Record<string, unknown>;
    const file = fileOf(s);
    if (file) out.push({ sign: "file", text: fileName(file) });
    const pairs: { old: unknown; next: unknown }[] =
      bare(s.tool) === "Write"
        ? [{ old: "", next: input.content }]
        : bare(s.tool) === "MultiEdit" && Array.isArray(input.edits)
          ? (input.edits as { old_string?: unknown; new_string?: unknown }[]).map((e) => ({
              old: e?.old_string,
              next: e?.new_string,
            }))
          : [{ old: input.old_string, next: input.new_string }];
    for (const p of pairs) {
      for (const text of linesOf(p.old)) out.push({ sign: "-", text });
      for (const text of linesOf(p.next)) out.push({ sign: "+", text });
    }
  }
  return out;
}

/** "+3 −1" over the lines of a diff. */
export function diffCounts(lines: readonly DiffLine[]): { added: number; removed: number } {
  return {
    added: lines.filter((l) => l.sign === "+").length,
    removed: lines.filter((l) => l.sign === "-").length,
  };
}
