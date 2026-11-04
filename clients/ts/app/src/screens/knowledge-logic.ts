// What Ideas and Memory compute, without a screen. Port of screens/projects.rs and app.rs.

import type { IdeaView, MemoryScope, MemoryView } from "@aurelius/protocol";

export type Author = "anyone" | "you" | "employees";
export const nextAuthor = (a: Author): Author => (a === "anyone" ? "you" : a === "you" ? "employees" : "anyone");

export const byEmployee = (i: Pick<IdeaView, "employee_id" | "author">) => i.employee_id != null || i.author !== "You";

export function admits(author: Author, i: Pick<IdeaView, "employee_id" | "author">): boolean {
  if (author === "anyone") return true;
  return author === "employees" ? byEmployee(i) : !byEmployee(i);
}

/** Yours first, then the employees', each as the server sent them. */
export function splitIdeas<T extends Pick<IdeaView, "employee_id" | "author">>(
  list: T[],
): { yours: T[]; employees: T[] } {
  return { yours: list.filter((i) => !byEmployee(i)), employees: list.filter(byEmployee) };
}

export const canBecomeTicket = (i: Pick<IdeaView, "task_id" | "ticket_id">) => !i.task_id && !i.ticket_id;

export function ideaPrefill(i: Pick<IdeaView, "title" | "note" | "kind">): {
  title: string;
  description: string;
  labels: string;
} {
  return { title: i.title, description: i.note ?? "", labels: i.kind === "bug" ? "bug" : "" };
}

export function ideasEmpty(status: string, author: Author): string {
  if (status === "done") return "Nothing done yet.";
  if (status === "dropped") return "Nothing dropped.";
  if (author === "you") return "You have no open ideas. Note one with Ctrl+N.";
  return author === "employees" ? "No open ideas from employees yet." : "No open ideas. Note one with Ctrl+N.";
}

const MEMORY_KIND: Record<string, string> = {
  identity: "Identity",
  project_knowledge: "Knowledge",
  decision: "Decision",
  episode: "Episode",
  summary: "Summary",
};

export const memoryKind = (m: Pick<MemoryView, "kind" | "lesson">) =>
  m.lesson ? "Lesson" : (MEMORY_KIND[m.kind] ?? m.kind);

/** Newest proposals first. */
export const memoryProposals = (items: MemoryView[]) =>
  items.filter((m) => m.status === "proposed").sort((a, b) => b.created_at_ms - a.created_at_ms);

export const memoryHistory = (items: MemoryView[]) =>
  items.filter((m) => m.status === "superseded" || m.status === "rejected" || m.status === "forgotten");

/** The project, and each employee with memory in it, once. */
export function consolidationScopes(
  project: string,
  items: Pick<MemoryView, "scope" | "employee_id">[],
): MemoryScope[] {
  const out: MemoryScope[] = [{ kind: "project", id: project }];
  for (const m of items) {
    if (m.scope === "employee" && m.employee_id && !out.some((s) => s.kind === "employee" && s.id === m.employee_id)) {
      out.push({ kind: "employee", id: m.employee_id });
    }
  }
  return out;
}

export const thousands = (n: number) => String(n).replace(/\B(?=(\d{3})+$)/g, ",");
