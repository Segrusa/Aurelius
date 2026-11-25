// What a ticket's chat shows beside the talk: the owner's own todo list, what runs right now,
// and the ticket's tasks. Pure, so the card and the panel say the same.

import type { ActivityView, MessageView, TaskView, TicketView } from "@aurelius/protocol";
import { runLabel, type Todo, type TodoState } from "./chat-logic";
import { isActive, treeOrder } from "./tasks-logic";

const TODO_TOOL = "TodoWrite";
const CREATE_TOOL = "TaskCreate";
const UPDATE_TOOL = "TaskUpdate";

function todoState(status: unknown): TodoState {
  if (status === "completed") return "done";
  if (status === "in_progress") return "doing";
  return "open";
}

const str = (v: unknown): string | null => (typeof v === "string" && v.trim() ? v.trim() : null);

/** One item as the list shows it: what is going on reads as "Running the tests", the rest as the plan says it. */
function todoOf(content: unknown, activeForm: unknown, status: unknown): Todo | null {
  const state = todoState(status);
  const words = (state === "doing" ? str(activeForm) : null) ?? str(content);
  return words ? { text: words, state } : null;
}

/** The rows of one TodoWrite call (Claude Code before 2.1.318): always the whole list. */
function fromTodoWrite(call: ActivityView): Todo[] | null {
  const list = (call.input as { todos?: unknown } | undefined)?.todos;
  if (!Array.isArray(list)) return null;
  const todos: Todo[] = [];
  for (const item of list as unknown[]) {
    if (typeof item !== "object" || item === null) continue;
    const { content, activeForm, status } = item as { content?: unknown; activeForm?: unknown; status?: unknown };
    const todo = todoOf(content, activeForm, status);
    if (todo) todos.push(todo);
  }
  return todos;
}

type TaskItem = { subject: string | null; activeForm: string | null; status: string };

/** Where the fold stands: a TodoWrite snapshot, or the items TaskCreate and TaskUpdate built. */
type Fold =
  | { kind: "none" }
  | { kind: "snapshot"; todos: Todo[] }
  | { kind: "tasks"; items: Map<string, TaskItem>; top: number };

/**
 * The todo list over a conversation's answers, oldest first. Since Claude Code 2.1.318 a run
 * makes it one call at a time: TaskCreate numbers its items 1, 2, 3 (the result says the
 * number) and TaskUpdate changes one later, often in a later answer, so the list carries over
 * from answer to answer. A TodoWrite (older CLIs) replaces the whole list. Offsets count per
 * answer, so each answer is sorted on its own.
 */
export function todosAcross(answers: readonly (readonly ActivityView[])[]): Todo[] | null {
  let fold: Fold = { kind: "none" };
  for (const activity of answers) {
    for (const a of [...activity].sort((x, y) => x.offset - y.offset)) {
      if (a.ok === false) continue;
      const input = (a.input ?? {}) as Record<string, unknown>;
      if (a.tool === TODO_TOOL) {
        const todos = fromTodoWrite(a);
        if (todos) fold = { kind: "snapshot", todos };
      } else if (a.tool === CREATE_TOOL) {
        if (fold.kind !== "tasks") fold = { kind: "tasks", items: new Map(), top: 0 };
        const id = /#(\d+)/.exec(a.output ?? "")?.[1] ?? String(fold.top + 1);
        // Numbering that starts over is a new session's list.
        if (fold.items.has(id) || (id === "1" && fold.top > 0)) fold = { kind: "tasks", items: new Map(), top: 0 };
        fold.items.set(id, { subject: str(input.subject), activeForm: str(input.activeForm), status: "pending" });
        fold.top = Math.max(fold.top, Number(id) || 0);
      } else if (a.tool === UPDATE_TOOL && fold.kind === "tasks") {
        const id = String(input.taskId ?? "");
        const item = fold.items.get(id);
        if (!item) continue;
        if (input.status === "deleted") {
          fold.items.delete(id);
          continue;
        }
        if (typeof input.status === "string") item.status = input.status;
        item.subject = str(input.subject) ?? item.subject;
        item.activeForm = str(input.activeForm) ?? item.activeForm;
      }
    }
  }
  let todos: Todo[] = [];
  if (fold.kind === "snapshot") todos = fold.todos;
  else if (fold.kind === "tasks") {
    for (const item of fold.items.values()) {
      const todo = todoOf(item.subject, item.activeForm, item.status);
      if (todo) todos.push(todo);
    }
  }
  return todos.length > 0 ? todos : null;
}

/** The todo list among one answer's calls, or null when there is none. */
export function todosOf(activity: readonly ActivityView[]): Todo[] | null {
  return todosAcross([activity]);
}

/** What streams in a conversation now: the newest activity of one answer. */
export interface LiveActivity {
  message_id: string;
  activity: readonly ActivityView[];
}

/**
 * The todo list of a conversation as it stands now: every answer from old to new, where what
 * streams (oldest first) is newer than the stored message it belongs to.
 */
export function latestTodos(live: readonly LiveActivity[], messages: readonly MessageView[]): Todo[] | null {
  const liveOf = new Map(live.map((u) => [u.message_id, u.activity]));
  const answers = messages
    .filter((m) => m.role === "assistant" && !m.update)
    .map((m) => liveOf.get(m.id) ?? m.activity ?? []);
  const known = new Set(messages.map((m) => m.id));
  for (const u of live) if (!known.has(u.message_id)) answers.push(u.activity);
  return todosAcross(answers);
}

/** "45s", "1m 36s", "2h 05m". */
export function elapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m`;
}

/** What the running answer does now, with how long the run has gone: "Running a command · 1m 36s". */
export function nowLine(activity: readonly ActivityView[], startedAtMs: number | null, nowMs: number): string {
  const open = activity.some((a) => a.ok === null || a.ok === undefined);
  const what = open ? runLabel([...activity]) : "Thinking";
  return startedAtMs === null ? what : `${what} · ${elapsed(nowMs - startedAtMs)}`;
}

/** The ticket's tasks still going, as a tree; the main task stays out (the ticket is that task). */
export function openTasks(
  tasks: readonly TaskView[],
  rootId: string | null | undefined,
): { task: TaskView; depth: number }[] {
  const rows = treeOrder(tasks.filter((t) => t.id !== rootId && isActive(t)));
  return rows;
}

/** "2 of 5 done" over the delegated tasks, or null without any. */
export function taskTally(tasks: readonly TaskView[], rootId: string | null | undefined): string | null {
  const below = tasks.filter((t) => t.id !== rootId);
  if (below.length === 0) return null;
  const done = below.filter((t) => !isActive(t)).length;
  return `${done} of ${below.length} done`;
}

/** Who is at work on the ticket's tasks, "Working: Flux, Fiona"; null when no task runs. */
export function workingLine(tasks: readonly TaskView[]): string | null {
  const busy = tasks.filter((t) => t.status === "running" || t.live_run === "running");
  if (busy.length === 0) return null;
  const names = [...new Set(busy.map((t) => t.employee_name).filter((n): n is string => Boolean(n)))];
  return names.length > 0 ? `Working: ${names.join(", ")}` : "Working";
}

/** "3 of 7" over a todo list. */
export function todoTally(todos: readonly Todo[]): string {
  return `${todos.filter((t) => t.state === "done").length} of ${todos.length}`;
}

/** The branch the ticket's work goes on, when the server says which (a quick ticket's). */
export function branchOf(t: Pick<TicketView, "kind" | "number">): string | null {
  return t.kind === "quick" ? `quick/ticket-${t.number}` : null;
}

/** Whether the card has anything to say: something runs, or there is a list or a task to show. */
export function cardShown(running: boolean, todos: readonly Todo[] | null, open: number): boolean {
  return running || (todos?.some((t) => t.state !== "done") ?? false) || open > 0;
}
