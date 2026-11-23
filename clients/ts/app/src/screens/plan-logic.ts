// What the plan panel decides without React: which plan opens by itself, what its status says,
// which decision belongs to it and where a comment's quote sits in the rendered text.

import type { MessageView } from "@aurelius/protocol";
import type { ArtifactTarget } from "./artifact-dock";
import { type Decision, decisionOf } from "./chat-logic";

/** A plan message in a feed, and whether its approval still waits. */
export interface PlanRef {
  id: string;
  pending: boolean;
}

export const isPlanMessage = (m: MessageView) => m.update === "plan" && !m.deleted_at_ms;

/** The newest plan of the feed; null when there is none. */
export function latestPlan(messages: readonly MessageView[], pending: ReadonlySet<string>): PlanRef | null {
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i];
    if (m && isPlanMessage(m)) return { id: m.id, pending: Boolean(m.approval_id && pending.has(m.approval_id)) };
  }
  return null;
}

/** Whether the dock shows this plan. */
export const showsPlan = (target: ArtifactTarget | null, planId: string) =>
  target?.kind === "plan" && target.messageId === planId;

/**
 * A plan that waits opens the dock by itself only when the dock is empty, and only once: a design
 * or another plan is never taken over, and a plan the board member closed stays closed.
 */
export function shouldAutoOpenPlan(
  target: ArtifactTarget | null,
  plan: PlanRef | null,
  handled: ReadonlySet<string>,
): boolean {
  return plan !== null && plan.pending && target === null && !handled.has(plan.id);
}

/** The "New plan waiting" pill: a waiting plan while the dock shows something else. */
export function newPlanWaiting(target: ArtifactTarget | null, plan: PlanRef | null): string | null {
  if (!plan?.pending || target === null || showsPlan(target, plan.id)) return null;
  return plan.id;
}

type PlanDecision = Extract<Decision, { kind: "plan" }>;

/**
 * The board member's decision on plan `planId`: the first plan decision after it in the same run,
 * before that run hands in its next plan. A plan message does not carry its decision's id.
 */
export function planDecision(
  messages: readonly MessageView[],
  planId: string,
): { decision: PlanDecision; at: number } | null {
  const from = messages.findIndex((m) => m.id === planId);
  const plan = messages[from];
  if (!plan) return null;
  const sameRun = (m: MessageView) => !plan.run_id || !m.run_id || m.run_id === plan.run_id;
  for (const m of messages.slice(from + 1)) {
    if (!sameRun(m)) continue;
    if (isPlanMessage(m)) return null;
    const d = decisionOf(m);
    if (d?.kind === "plan") return { decision: d, at: m.created_at_ms };
  }
  return null;
}

/** The status chip of a plan: words first, colour second. */
export type PlanStatus =
  | { label: "Waiting for you"; tone: "warning" }
  | { label: "Approved · auto" | "Approved · edits"; tone: "success" }
  | { label: "Changes requested"; tone: "info" }
  | { label: "Rejected"; tone: "error" }
  | { label: "Decided"; tone: "neutral" };

export function planStatus(pending: boolean, decision: PlanDecision | null): PlanStatus {
  if (pending) return { label: "Waiting for you", tone: "warning" };
  if (!decision) return { label: "Decided", tone: "neutral" };
  switch (decision.verdict) {
    case "approved":
      return { label: approvedWithEdits(decision) ? "Approved · edits" : "Approved · auto", tone: "success" };
    case "changes":
      return { label: "Changes requested", tone: "info" };
    case "rejected":
      return { label: "Rejected", tone: "error" };
  }
}

const approvedWithEdits = (d: PlanDecision) => d.title.includes("edits");

/** The feed card's label: the decision once there is one. */
export function planLabel(pending: boolean, decision: PlanDecision | null): string {
  if (pending) return "PLAN · WAITING FOR YOU";
  switch (decision?.verdict) {
    case "approved":
      return "PLAN · APPROVED";
    case "changes":
      return "PLAN · CHANGES REQUESTED";
    case "rejected":
      return "PLAN · REJECTED";
    default:
      return "PLAN";
  }
}

/** The footer's one line after a decision. */
export function decidedLine(decision: PlanDecision, time: string): string {
  switch (decision.verdict) {
    case "approved":
      return `You approved this plan at ${time} · ${approvedWithEdits(decision) ? "edits go through" : "auto mode"}`;
    case "changes":
      return `You asked for changes at ${time}`;
    case "rejected":
      return `You rejected this plan at ${time}`;
  }
}

/**
 * Where `quote` sits in `text` as [start, end), ignoring whitespace: a selection's text has line
 * breaks between blocks that the page's text nodes do not. Null when it is not there.
 */
export function findQuote(text: string, quote: string): [number, number] | null {
  const at: number[] = [];
  let bare = "";
  for (let i = 0; i < text.length; i++) {
    const ch = text[i] ?? "";
    if (/\s/.test(ch)) continue;
    bare += ch;
    at.push(i);
  }
  const wanted = quote.replace(/\s+/g, "");
  if (wanted === "") return null;
  const start = bare.indexOf(wanted);
  if (start < 0) return null;
  const first = at[start];
  const last = at[start + wanted.length - 1];
  if (first === undefined || last === undefined) return null;
  return [first, last + 1];
}

/** The plan to open from `?plan=<messageId>`, when the feed holds it. */
export function planFromSearch(search: string, messages: readonly MessageView[]): string | null {
  const id = new URLSearchParams(search).get("plan");
  return id && messages.some((m) => m.id === id && isPlanMessage(m)) ? id : null;
}
