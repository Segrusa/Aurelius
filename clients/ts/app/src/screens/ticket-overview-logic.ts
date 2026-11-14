// What a ticket's chat shows beside the talk: the owner's own todo list, what runs right now,
// and the ticket's tasks. Pure, so the card and the panel say the same.

import type { ActivityView, MessageView, TaskView, TicketView } from "@aurelius/protocol";
import { runLabel, type Todo, type TodoState } from "./chat-logic";
import { isActive, treeOrder } from "./tasks-logic";

const TODO_TOOL = "TodoWrite";

function todoState(status: unknown): TodoState {
  if (status === "completed") return "done";
  if (status === "in_progress") return "doing";
  return "open";
}

/** The latest todo list among these calls (Claude's TodoWrite), or null when there is none. */
export function todosOf(activity: readonly ActivityView[]): Todo[] | null {
  let best: ActivityView | null = null;
  for (const a of activity) if (a.tool === TODO_TOOL && (!best || a.offset >= best.offset)) best = a;
  const list = (best?.input as { todos?: unknown } | undefined)?.todos;
  if (!Array.isArray(list)) return null;
  const todos: Todo[] = [];
  for (const item of list as unknown[]) {
    if (typeof item !== "object" || item === null) continue;
    const { content, activeForm, status } = item as { content?: unknown; activeForm?: unknown; status?: unknown };
    const state = todoState(status);
    // What is going on reads as "Running the tests"; the rest as the plan says it.
    const words = state === "doing" && typeof activeForm === "string" && activeForm.trim() ? activeForm : content;
    if (typeof words !== "string" || !words.trim()) continue;
    todos.push({ text: words.trim(), state });
  }
  return todos.length > 0 ? todos : null;
}

/** The newest todo list of a conversation: what is streaming first, then the answers from new to old. */
export function latestTodos(live: readonly ActivityView[][], messages: readonly MessageView[]): Todo[] | null {
  for (const activity of live) {
    const found = todosOf(activity);
    if (found) return found;
  }
  for (let i = messages.length - 1; i >= 0; i--) {
    const found = todosOf(messages[i]?.activity ?? []);
    if (found) return found;
  }
  return null;
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
