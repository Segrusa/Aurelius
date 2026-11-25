// The Review card of a ticket as a finish line (ticket #89, design/review-finish.html): Review,
// Merge, Push (optional) and Done, with one state per moment: your decision, merging (after you
// asked the owner), merged, the merge stopped, and done. The server knows no merge state of its
// own: the state follows from the ticket's column, the board member's "Merge into main" post in
// the discussion, the main task's run, and the git view of the project (`code.get`).

import type { ActivityView, CodeBranch, CodeCommit, CodeView, MessageView, TicketView } from "@aurelius/protocol";
import { isProgressTool } from "./chat-logic";
import { intentOf, runningPhrase } from "./ticket-activity-plain-logic";

/** What the Merge into main button tells the owner (the reply the review card always offered). */
export const MERGE_ASK = "Merge this into main.";
/** What Explain what you did tells the owner. */
export const EXPLAIN_ASK = "Explain in a few lines what you changed and why.";
/** What Push to origin tells the owner. */
export const PUSH_ASK = "Push main to the remote.";
/** What Ask … to resolve and merge tells the owner. */
export const RESOLVE_ASK =
  "The merge into main stopped on conflicts. Merge main into your branch, resolve the conflicts, run the checks, then merge into main.";
/** What Undo the merge tells the owner. */
export const UNDO_ASK =
  "Undo your merge into main: revert the merge commit on main (do not rewrite main's history) and keep your branch.";

/** How many of the owner's latest steps the merging state lists. */
export const FINISH_STEPS_SHOWN = 6;
/** How long a command may be in a step line before it is cut. */
export const FINISH_COMMAND_CHARS = 80;

/** The moment of the finish card. */
export type FinishKind = "decide" | "merging" | "merged" | "conflict" | "done";

/** One step of the finish line: done, at work, waiting for you, stopped, or still to come. */
export type FinishStepState = "done" | "now" | "ask" | "stopped" | "open";

export interface FinishStep {
  label: "Review" | "Merge" | "Push" | "Done";
  state: FinishStepState;
  optional: boolean;
}

const MERGE_WORDS = /\bmerge\b[^.?!\n]*\b(in)?to\s+(main|master)\b/i;

/** Whether a board member's words ask the owner to merge into main. */
export function isMergeAsk(text: string): boolean {
  return MERGE_WORDS.test(text);
}

/**
 * The board member's request to merge, when it is still the latest thing they said: their newest
 * message in the discussion asks to merge. A later message of theirs (a question, feedback) means
 * the conversation moved on, and the card goes back to the decision.
 */
export function mergeAsk(
  messages: readonly Pick<
    MessageView,
    "role" | "text" | "system" | "deleted_at_ms" | "created_at_ms" | "id" | "author"
  >[],
): { id: string; atMs: number; author: string | null } | null {
  const last = [...messages].reverse().find((m) => m.role === "user" && !m.system && !m.deleted_at_ms);
  return last && isMergeAsk(last.text) ? { id: last.id, atMs: last.created_at_ms, author: last.author ?? null } : null;
}

/** The ticket's branch in the git view: a quick task's own, else the newest one that names the ticket. */
export function ticketBranch(
  code: Pick<CodeView, "branches"> | undefined,
  t: Pick<TicketView, "number" | "kind">,
): CodeBranch | null {
  const branches = (code?.branches ?? []).filter((b) => !b.is_main);
  const quick = branches.find((b) => b.name === `quick/ticket-${t.number}`);
  if (quick) return quick;
  const own = branches.filter((b) => b.ticket?.number === t.number);
  // The one with work main lacks goes first; then the newest.
  own.sort((a, b) => Number(b.ahead > 0) - Number(a.ahead > 0) || b.at_ms - a.at_ms);
  return own[0] ?? null;
}

/** The main branch in the git view. */
export function mainBranch(code: Pick<CodeView, "branches"> | undefined): CodeBranch | null {
  return code?.branches?.find((b) => b.is_main) ?? null;
}

/**
 * The merge commit that brought the ticket's work into main: a commit with two parents whose
 * subject names the branch (or, once the branch is gone, the ticket: `ticket-89`), newest first.
 */
export function mergeCommitOf(
  code: Pick<CodeView, "commits"> | undefined,
  t: Pick<TicketView, "number">,
  branch: Pick<CodeBranch, "name" | "head"> | null,
): CodeCommit | null {
  const byTicket = new RegExp(`\\bticket-${t.number}\\b`);
  return (
    (code?.commits ?? []).find(
      (c) =>
        (c.parents?.length ?? 0) >= 2 &&
        /^merge\b/i.test(c.subject) &&
        (branch
          ? c.subject.includes(branch.name) || (c.parents ?? []).slice(1).includes(branch.head)
          : byTicket.test(c.subject)),
    ) ?? null
  );
}

export interface FinishInput {
  column: TicketView["column"];
  /** The board member asked to merge, and said nothing else since. */
  asked: boolean;
  /** The main task's run is at work or waits to start. */
  running: boolean;
  /** The ticket's branch; null when the git view does not know it (gone, or not read). */
  branch: Pick<CodeBranch, "merged" | "ahead"> | null;
  /** A merge commit of the ticket's work is on main. */
  mergedCommit: boolean;
}

/**
 * The moment of the card; null outside Review and Done. After the request: merging while the
 * owner's run goes, then merged when main holds the work, else stopped. When the git view cannot
 * tell (no branch, no merge commit), the card is back at the decision rather than guess.
 */
export function finishKind(i: FinishInput): FinishKind | null {
  if (i.column === "done") return "done";
  if (i.column !== "review") return null;
  if (!i.asked) return "decide";
  if (i.running) return "merging";
  if (i.mergedCommit || (i.branch?.merged && i.branch.ahead === 0)) return "merged";
  if ((i.branch?.ahead ?? 0) > 0) return "conflict";
  return "decide";
}

/** The four steps of the finish line in a moment. Push is done once main is not ahead of its remote. */
export function finishSteps(kind: FinishKind, opts: { merged: boolean; pushed: boolean }): FinishStep[] {
  const step = (label: FinishStep["label"], state: FinishStepState, optional = false): FinishStep => ({
    label,
    state,
    optional,
  });
  const push = step("Push", opts.merged && opts.pushed ? "done" : "open", true);
  switch (kind) {
    case "decide":
      return [step("Review", "ask"), step("Merge", "open"), push, step("Done", "open")];
    case "merging":
      return [step("Review", "done"), step("Merge", "now"), push, step("Done", "open")];
    case "merged":
      return [step("Review", "done"), step("Merge", "done"), push, step("Done", "ask")];
    case "conflict":
      return [step("Review", "done"), step("Merge", "stopped"), push, step("Done", "open")];
    case "done":
      return [step("Review", "done"), step("Merge", opts.merged ? "done" : "open"), push, step("Done", "done")];
  }
}

/** The border and dot colour of a moment. */
export function finishTone(kind: FinishKind): "amber" | "live" | "ok" | "bad" {
  return kind === "decide" ? "amber" : kind === "merging" ? "live" : kind === "conflict" ? "bad" : "ok";
}

/** "1:12", "12:05", "1:02:09": time since the request, as a clock. */
export function clock(ms: number): string {
  const s = Math.floor(Math.max(0, ms) / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** One line of what the owner does while merging. */
export interface FinishLine {
  id: string;
  state: "done" | "now" | "failed";
  /** Words before the command, or the whole line when there is no command. */
  text: string;
  command: string | null;
}

const bare = (tool: string) => tool.replace(/^claude\./, "");

/**
 * The owner's latest steps as lines: each command it ran (with the command), each other call in
 * plain words; lookups and the plan's own calls are left out. The newest `max`, oldest first.
 */
export function finishLines(activity: readonly ActivityView[], max = FINISH_STEPS_SHOWN): FinishLine[] {
  const out: FinishLine[] = [];
  for (const a of activity) {
    if (isProgressTool(bare(a.tool))) continue;
    const intent = intentOf(a);
    if (intent === "lookup" || intent === "read") continue;
    const state = a.ok === null || a.ok === undefined ? "now" : a.ok ? "done" : "failed";
    const command = (a.input as { command?: unknown } | null)?.command;
    if (typeof command === "string" && command.trim() !== "") {
      const one = command.trim().split("\n")[0] ?? "";
      const cut = one.length > FINISH_COMMAND_CHARS ? `${one.slice(0, FINISH_COMMAND_CHARS - 1)}…` : one;
      out.push({ id: a.call_id, state, text: state === "now" ? "Running" : "Ran", command: cut });
    } else {
      const phrase = runningPhrase(a).replace(/…$/, "");
      out.push({ id: a.call_id, state, text: phrase, command: null });
    }
  }
  return out.slice(-max);
}

/** Whether Mark done when merged should accept the ticket now. */
export function autoDoneDue(kind: FinishKind | null, wanted: boolean, ready: boolean): boolean {
  return kind === "merged" && wanted && ready;
}

/** "2 commits", "1 file": a count with its noun. */
export function count(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}
