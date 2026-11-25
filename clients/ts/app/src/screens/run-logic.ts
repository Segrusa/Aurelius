// Texts and parsing around a run, as the desktop app writes them (markdown.rs, views.rs).

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

export function elapsedText(ms: number): string {
  const s = Math.floor(Math.max(0, ms) / 1000);
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;
  return `${Math.floor(s / 3600)}h ${String(Math.floor(s / 60) % 60).padStart(2, "0")}m`;
}

/** A subagent's latest step and how many it took ("Grep · x · 3 steps"), as on mobile. */
export function subagentDoingText(a: { doing?: string | null; steps: number }): string {
  if (!a.doing) return "Starting…";
  return `${a.doing} · ${a.steps} ${a.steps === 1 ? "step" : "steps"}`;
}

/** The calls one subagent made, in the order of the run (`subagent_steps`); none without its call. */
export function subagentSteps<T extends { parent_call_id?: string | null }>(
  calls: readonly T[],
  callId: string | null | undefined,
): T[] {
  if (!callId) return [];
  return calls.filter((c) => c.parent_call_id === callId);
}

/** A subagent's steps in batches of `size`, in order: each batch is one folded line in its log. */
export function stepBatches<T>(steps: readonly T[], size: number): T[][] {
  const batches: T[][] = [];
  for (let i = 0; i < steps.length; i += size) batches.push(steps.slice(i, i + size));
  return batches;
}

/** What a subagent reported: the output of the call that started it, once that call ended. */
export function subagentReport(
  calls: readonly { call_id: string; ok?: boolean | null; output?: string | null }[],
  callId: string | null | undefined,
): string | null {
  if (!callId) return null;
  const call = calls.find((c) => c.call_id === callId);
  if (!call || call.ok == null) return null;
  return call.output?.trim() || null;
}

/** Which subagent a row is; the server gives no id, its name and start time tell them apart. */
export const subagentKey = (a: { name: string; since_ms: number }) => `${a.name}|${a.since_ms}`;

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

/** Why the last answer did not come: what a stopped task says it waits for. */
export function lastFailure(
  messages: { role: string; status: string; text: string; deleted_at_ms?: number | null }[],
): string | null {
  const last = [...messages].reverse().find((m) => m.role === "assistant" && !m.deleted_at_ms);
  return last?.status === "failed" ? last.text.replace(/\s*Send the message again to retry\.\s*$/, "") : null;
}

/** What a conversation's answers used, added up (the side panel of a task and of a session). */
export interface UsageTotals {
  input: number;
  output: number;
  cached: number;
  written: number;
  micro: number;
}

export function usageTotals(messages: { usage?: TokenUsage | null }[]): UsageTotals {
  const totals = { input: 0, output: 0, cached: 0, written: 0, micro: 0 };
  for (const m of messages) {
    totals.input += m.usage?.input_tokens ?? 0;
    totals.output += m.usage?.output_tokens ?? 0;
    totals.cached += m.usage?.cached_tokens ?? 0;
    totals.written += m.usage?.cache_creation_tokens ?? 0;
    totals.micro += m.usage?.cost_micro_usd ?? 0;
  }
  return totals;
}

/** The run whose guidelines and context the side panel shows: the latest one that answered. */
export function lastRunOf(messages: { run_id?: string | null }[]): string | null {
  return [...messages].reverse().find((m) => m.run_id)?.run_id ?? null;
}

/** The last part of a path, on any platform's separator. */
export function fileName(path: string): string {
  const parts = path.split(/[\\/]/).filter((p) => p !== "");
  return parts.at(-1) ?? path;
}

/** Whether a path is absolute on Unix (`/…`) or Windows (`C:\…`, `\\server\…`): only those can be opened. */
export function isAbsolutePath(path: string): boolean {
  return path.startsWith("/") || /^[A-Za-z]:[\\/]/.test(path) || path.startsWith("\\\\");
}

/** A written file as the list shows it: its name, and its folder only where another file has the same name. */
export interface FileRow {
  path: string;
  name: string;
  folder: string | null;
}

export function fileRows(paths: readonly string[]): FileRow[] {
  const names = paths.map(fileName);
  const seen = new Map<string, number>();
  for (const n of names) seen.set(n, (seen.get(n) ?? 0) + 1);
  return paths.map((path, i) => {
    const name = names[i] ?? path;
    if ((seen.get(name) ?? 0) < 2) return { path, name, folder: null };
    const parts = path.split(/[\\/]/).filter((p) => p !== "");
    return { path, name, folder: parts.at(-2) ?? null };
  });
}
