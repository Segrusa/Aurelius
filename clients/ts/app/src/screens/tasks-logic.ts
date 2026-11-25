// A task's tree, state and steps, pure.

import type { TaskStatus, TaskView } from "@aurelius/protocol";

const TERMINAL: TaskStatus[] = ["succeeded", "failed", "cancelled"];

export const isActive = (t: TaskView) => !TERMINAL.includes(t.status) || Boolean(t.live_run);

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

/** Every task below `rootId` (children, their children, …), in tree order, with depth 1 for a direct child. */
export function subtasksOf(rows: TaskView[], rootId: string): { task: TaskView; depth: number }[] {
  const below = new Set<string>([rootId]);
  for (let grew = true; grew; ) {
    grew = false;
    for (const t of rows) {
      if (t.parent_task_id && below.has(t.parent_task_id) && !below.has(t.id)) {
        below.add(t.id);
        grew = true;
      }
    }
  }
  return treeOrder(rows.filter((t) => below.has(t.id)))
    .filter(({ task }) => task.id !== rootId)
    .map(({ task, depth }) => ({ task, depth }));
}

/** Where Back leads from a task's run: the ticket it works for, else its employee, else New session. */
export function taskBackPath(t: Pick<TaskView, "ticket_id" | "ticket_number" | "employee_id"> | undefined): string {
  if (t?.ticket_number) return `/ticket/${t.ticket_number}`;
  if (t?.ticket_id) return `/board?ticket=${t.ticket_id}`;
  return t?.employee_id ? `/employees/${t.employee_id}` : "/new";
}

/** What a task's state says, and in which colour. */
export function taskStateLine(t: TaskView): { text: string; color: string } {
  if (t.pending_approvals > 0) return { text: "Paused · needs approval", color: "var(--color-text-warning)" };
  if (t.live_run === "running" || t.status === "running") return { text: "Running", color: "var(--color-text-accent)" };
  switch (t.status) {
    case "draft":
      return { text: "Draft", color: "var(--precision-text3)" };
    case "queued":
      return { text: t.waiting_reason ? "Waiting" : "Queued", color: "var(--color-text-secondary)" };
    case "waiting":
      return { text: "Waiting for you", color: "var(--color-text-warning)" };
    case "review":
      return { text: "In review", color: "var(--precision-violet)" };
    case "succeeded":
      return { text: "Done", color: "var(--precision-text3)" };
    case "failed":
      return { text: "Failed", color: "var(--color-text-error)" };
    default:
      return { text: "Cancelled", color: "var(--precision-text3)" };
  }
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
