// What a run card, an interrupt card and the odometer say. Pure, so they are tested without a screen.

import type { ApprovalView, MessageView, RunBrief, TokenUsage } from "@aurelius/protocol";
import { costText, tokensShort } from "./usage-logic";

export interface RunState {
  label: string;
  variant: "success" | "warning" | "error" | "accent" | "neutral";
  /** Still under way: the dot pulses and the run can be stopped. */
  running: boolean;
}

/** The badge of a run; a state a newer server invents reads as itself and neutral. */
export function runState(status: string, waiting: boolean): RunState {
  if (waiting) return { label: "Waiting for you", variant: "warning", running: status === "running" };
  switch (status) {
    case "created":
    case "starting":
      return { label: "Starting", variant: "accent", running: true };
    case "running":
      return { label: "Running", variant: "accent", running: true };
    case "waiting":
      return { label: "Waiting", variant: "warning", running: true };
    case "cancelling":
      return { label: "Stopping", variant: "neutral", running: false };
    case "succeeded":
      return { label: "Done", variant: "success", running: false };
    case "failed":
      return { label: "Failed", variant: "error", running: false };
    case "cancelled":
      return { label: "Stopped", variant: "neutral", running: false };
    default:
      return { label: status, variant: "neutral", running: false };
  }
}

/** "3 tool calls · 1 failed · 14.2k tokens" */
export function runLine(brief: RunBrief, usage: TokenUsage | null | undefined): string {
  return [
    brief.tool_calls === 1 ? "1 tool call" : `${brief.tool_calls} tool calls`,
    brief.failed_calls > 0 ? `${brief.failed_calls} failed` : null,
    usage ? `${tokensShort(usage.input_tokens + usage.output_tokens)} tokens` : null,
  ]
    .filter(Boolean)
    .join(" · ");
}

/** Whether a message asks the reader something that no question or approval card already asks. */
export function waitsOnReply(
  m: Pick<MessageView, "awaiting_user" | "run_id">,
  approvals: Pick<ApprovalView, "run_id">[],
): boolean {
  return Boolean(m.awaiting_user) && !approvals.some((a) => a.run_id === m.run_id);
}

/** The open approvals of a discussion: asked in it, by a run it woke, or on its ticket. */
export function approvalsFor<T extends ApprovalView>(
  approvals: T[],
  place: { conversationId?: string | null | undefined; ticketId?: string | null | undefined },
): T[] {
  const { conversationId, ticketId } = place;
  return approvals.filter(
    (a) =>
      a.status === "pending" &&
      ((conversationId && (a.conversation_id === conversationId || a.discussion_id === conversationId)) ||
        (ticketId && a.ticket_id === ticketId)),
  );
}

export interface OdometerItem {
  label: string;
  value: string;
}

/** Calls, tokens in and out, and the cost; no calls item for usage from before they were counted. */
export function odometerItems(u: TokenUsage): OdometerItem[] {
  return [
    ...(u.model_calls ? [{ label: "Calls", value: String(u.model_calls) }] : []),
    { label: "Tokens in", value: tokensShort(u.input_tokens) },
    { label: "Tokens out", value: tokensShort(u.output_tokens) },
    { label: "Cost", value: costText(u.cost_micro_usd, u.estimate) },
  ];
}

export function odometerLine(u: TokenUsage): string {
  return odometerItems(u)
    .map((i) => `${i.value} ${i.label.toLowerCase()}`)
    .join(" · ");
}

/** Nothing to show while nothing was used. */
export const odometerEmpty = (u: TokenUsage): boolean => u.input_tokens + u.output_tokens + u.cached_tokens === 0;
