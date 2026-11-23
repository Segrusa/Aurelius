// The ticket page (tickets #74 and #83): which part of the ticket's talk the Timeline's filter
// shows, how far a plan is, and whether a task is at work. Pure, so it can be tested on its own.

import type { ActivityView, MessageView, TaskView } from "@aurelius/protocol";
import type { Todo } from "./chat-logic";

/** What the Activity list of a ticket shows. */
export type ActivityFilter = "all" | "changes" | "messages";

export const ACTIVITY_FILTERS: { key: ActivityFilter; label: string }[] = [
  { key: "all", label: "All" },
  { key: "changes", label: "Changes" },
  { key: "messages", label: "Messages" },
];

const tool = (a: { tool: string }) => a.tool.replace(/^claude\./, "");
const WRITING = new Set(["Write", "Edit", "MultiEdit", "NotebookEdit"]);

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

/** A task that has something to say right now: at work, or waiting on someone. */
export function busy(t: TaskView): boolean {
  if (t.live_run) return true;
  if (t.pending_approvals > 0) return true;
  return t.status === "running" || t.status === "waiting" || (t.status === "queued" && !t.waiting_reason);
}
