// What Memory computes, without a screen. Port of screens/projects.rs and app.rs.

import type { MemoryScope, MemoryView } from "@aurelius/protocol";

const MEMORY_KIND: Record<string, string> = {
  identity: "Identity",
  project_knowledge: "Knowledge",
  decision: "Decision",
  episode: "Episode",
  summary: "Summary",
};

export const memoryKind = (m: Pick<MemoryView, "kind" | "lesson">) =>
  m.lesson ? "Lesson" : (MEMORY_KIND[m.kind] ?? m.kind);

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
