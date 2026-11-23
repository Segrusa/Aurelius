// What the ticket's Timeline merges (tickets #76 and #83): the ticket's talk and its tasks' work in
// one order, which filter shows the work, and the files a piece of work touched. Pure, so it can be
// tested without a page.

import type { ActivityView } from "@aurelius/protocol";
import type { ActivityFilter } from "./ticket-page-logic";

/** Work is a change; Messages shows only what people and employees said. */
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
