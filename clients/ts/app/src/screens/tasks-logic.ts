// The Tasks page's filter and tree, pure. Port of clients/desktop/src/screens/tasks.rs.

import type { TaskStatus, TaskView } from "@aurelius/protocol";
import { isWaiting } from "./inbox-logic";

export type TaskState = "open" | "needs_you" | "ended" | "all";

export const TASK_STATES: { key: TaskState; label: string }[] = [
  { key: "open", label: "Open" },
  { key: "needs_you", label: "Needs you" },
  { key: "ended", label: "Ended" },
  { key: "all", label: "All" },
];

const TERMINAL: TaskStatus[] = ["succeeded", "failed", "cancelled"];

export const isActive = (t: TaskView) => !TERMINAL.includes(t.status) || Boolean(t.live_run);

export function admits(state: TaskState, t: TaskView): boolean {
  if (state === "open") return isActive(t);
  if (state === "needs_you") return isWaiting(t) || t.pending_approvals > 0;
  return state === "ended" ? !isActive(t) : true;
}

export interface TaskFilter {
  state: TaskState;
  employee: string | null;
  project: string | null;
}

/** Newest change first. */
export function filterTasks(tasks: TaskView[], f: TaskFilter): TaskView[] {
  return tasks
    .filter(
      (t) =>
        admits(f.state, t) &&
        (f.employee === null || t.employee_id === f.employee) &&
        (f.project === null || t.project_id === f.project),
    )
    .sort((a, b) => b.updated_at_ms - a.updated_at_ms);
}

export interface TreeRow {
  task: TaskView;
  depth: number;
  /** All tasks below this one, shown or not. */
  below: number;
  open: boolean;
}

/** A delegated task sits right under the one that delegated it, oldest first; a task whose parent is not here stays on top. */
export function treeOrder(rows: TaskView[]): { task: TaskView; depth: number }[] {
  const present = new Set(rows.map((t) => t.id));
  const kids = new Map<string, TaskView[]>();
  const roots: TaskView[] = [];
  for (const t of rows) {
    const parent = t.parent_task_id;
    if (parent && parent !== t.id && present.has(parent)) kids.set(parent, [...(kids.get(parent) ?? []), t]);
    else roots.push(t);
  }
  for (const list of kids.values()) list.sort((a, b) => a.created_at_ms - b.created_at_ms);
  const out: { task: TaskView; depth: number }[] = [];
  const seen = new Set<string>();
  const walk = (t: TaskView, depth: number) => {
    if (seen.has(t.id)) return;
    seen.add(t.id);
    out.push({ task: t, depth });
    for (const k of kids.get(t.id) ?? []) walk(k, depth + 1);
  };
  for (const t of roots) walk(t, 0);
  // A parent cycle has no root; keep those tasks visible anyway.
  for (const t of rows) walk(t, 0);
  return out;
}

/** Hides what sits below a folded task. Open when `folds` says so, else while it is not finished. */
export function foldTree(tree: { task: TaskView; depth: number }[], folds: Record<string, boolean>): TreeRow[] {
  const out: TreeRow[] = [];
  let hiddenFrom: number | null = null;
  tree.forEach(({ task, depth }, i) => {
    if (hiddenFrom !== null && depth > hiddenFrom) return;
    hiddenFrom = null;
    let below = 0;
    for (let j = i + 1; j < tree.length && (tree[j]?.depth ?? 0) > depth; j++) below++;
    const open = folds[task.id] ?? isActive(task);
    out.push({ task, depth, below, open });
    if (below > 0 && !open) hiddenFrom = depth;
  });
  return out;
}

/** What a task's state says, and in which colour. */
export function taskStateLine(t: TaskView): { text: string; color: string } {
  if (t.pending_approvals > 0) return { text: "Paused · needs approval", color: "var(--amber)" };
  if (t.live_run === "running" || t.status === "running") return { text: "Running", color: "var(--acc-text)" };
  switch (t.status) {
    case "draft":
      return { text: "Draft", color: "var(--text3)" };
    case "queued":
      return { text: t.waiting_reason ? "Waiting" : "Queued", color: "var(--text2)" };
    case "waiting":
      return { text: "Waiting for you", color: "var(--amber)" };
    case "review":
      return { text: "In review", color: "var(--violet)" };
    case "succeeded":
      return { text: "Done", color: "var(--text3)" };
    case "failed":
      return { text: "Failed", color: "var(--red)" };
    default:
      return { text: "Cancelled", color: "var(--text3)" };
  }
}

export interface Steps {
  count: number;
  failed: number;
  last: string;
}

/** A message's tool calls as one line: "9 steps, 2 failed", and the last one. */
export function stepsOf(activity: { tool: string; input: unknown; ok?: boolean | null }[]): Steps | null {
  if (activity.length === 0) return null;
  const last = activity[activity.length - 1];
  return {
    count: activity.length,
    failed: activity.filter((a) => a.ok === false).length,
    last: last ? stepLine(last) : "",
  };
}

export function stepLine(step: { tool: string; input: unknown }): string {
  const input = (step.input ?? {}) as Record<string, unknown>;
  for (const key of ["command", "file_path", "path", "pattern", "url", "query", "description"]) {
    const value = input[key];
    if (typeof value === "string" && value !== "") return value.split("\n", 1)[0] ?? "";
  }
  return step.tool.replace(/^claude\./, "");
}

const WRITING = new Set(["Write", "Edit", "MultiEdit", "NotebookEdit"]);

/** The files a run wrote, in order, without doubles. */
export function filesWritten(
  messages: { activity: { tool: string; input: unknown; ok?: boolean | null }[] }[],
): string[] {
  const out = new Set<string>();
  for (const m of messages) {
    for (const a of m.activity) {
      const path = (a.input as { file_path?: unknown } | null)?.file_path;
      if (WRITING.has(a.tool.replace(/^claude\./, "")) && a.ok !== false && typeof path === "string") out.add(path);
    }
  }
  return [...out];
}
