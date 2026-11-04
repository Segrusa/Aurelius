// Texts and parsing around a run, as the desktop app writes them (trace.rs, markdown.rs, views.rs).

import type { LoadSnapshot, TokenUsage } from "@aurelius/protocol";
import { costText, tokensShort } from "./usage-logic";

export function usageLine(u: TokenUsage): string {
  return [
    `${tokensShort(Math.max(0, u.input_tokens - u.cache_creation_tokens))} in`,
    `${tokensShort(u.output_tokens)} out`,
    `${tokensShort(u.cached_tokens)} cache read`,
    `${tokensShort(u.cache_creation_tokens)} cache write`,
    costText(u.cost_micro_usd, u.estimate),
  ].join(" · ");
}

export function traceSessionText(session: string): string {
  if (session === "continue" || session === "resume") return "resumed session: only the new messages";
  return session === "new" ? "new session: the whole conversation sent again" : "no session";
}

export function elapsedText(ms: number): string {
  const s = Math.floor(Math.max(0, ms) / 1000);
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;
  return `${Math.floor(s / 3600)}h ${String(Math.floor(s / 60) % 60).padStart(2, "0")}m`;
}

const MARKDOWN_MAX = 4;

/** The first few markdown files a text names; a link to the web is not a file here. */
export function markdownPaths(text: string): string[] {
  const found: string[] = [];
  for (const raw of text.split(/[\s()[\]<>"'`,;*]+/)) {
    const token = (raw.replace(/[.:!?]+$/, "").split("#")[0] ?? "").trim();
    if (token.length < 4 || token.includes("://") || !/\.(md|markdown)$/i.test(token)) continue;
    if (!found.includes(token)) found.push(token);
    if (found.length === MARKDOWN_MAX) break;
  }
  return found;
}

/** The id in a screenshot step's output; an id is a UUID, never a path. */
export function screenshotId(output: string): string | null {
  const match = /Saved as screenshot ([0-9a-fA-F-]{36})(?![0-9a-fA-F-])/.exec(output);
  return match?.[1] ?? null;
}

function gigabytes(mb: number): string {
  const gb = mb / 1024;
  return gb < 10 ? `${gb.toFixed(1)} GB` : `${Math.round(gb)} GB`;
}

export function waitingText(s: LoadSnapshot): string | null {
  switch (s.hold?.kind) {
    case "cpu":
      return `Server busy: CPU ${Math.round(s.cpu_pct)}%, waiting`;
    case "memory":
      return `Server busy: ${gigabytes(s.mem_free_mb)} free, waiting`;
    case "max_runs":
      return `Server busy: ${s.running === 1 ? "1 run" : `${s.running} runs`} active, waiting`;
    case "disabled_all":
      return "Runs paused on the server";
    default:
      return null;
  }
}

export interface PlanComment {
  quote: string;
  text: string;
}

/** Each comment under its quoted fragment, then the free text. */
export function composePlanFeedback(comments: PlanComment[], free: string): string {
  const parts = comments.map((c) => {
    const quote = c.quote
      .trim()
      .split("\n")
      .map((line) => (line.trim() === "" ? ">" : `> ${line}`))
      .join("\n");
    return `${quote}\n\n${c.text.trim()}`;
  });
  if (free.trim() !== "") parts.push(free.trim());
  return parts.join("\n\n");
}

/** Why a task cannot be given to someone else or cancelled here; a ticket's main task goes through the board. */
export function boardOnlyReason(t: {
  ticket_id?: string | null;
  parent_task_id?: string | null;
  delegated_by?: unknown;
  ticket_number?: number | null;
}): string | null {
  if (!t.ticket_id || t.parent_task_id || t.delegated_by) return null;
  const which = t.ticket_number ? `ticket #${t.ticket_number}` : "a ticket";
  return `This is the main task of ${which}: assign or cancel it on the Board.`;
}

/** Whether the ticket's last summary attempt failed; one that came after it counts instead. */
export function lastSummaryFailed(messages: { system?: { kind: string } | null | undefined }[]): boolean {
  for (let i = messages.length - 1; i >= 0; i--) {
    const kind = messages[i]?.system?.kind;
    if (kind === "summary") return false;
    if (kind === "summary_failed") return true;
  }
  return false;
}
