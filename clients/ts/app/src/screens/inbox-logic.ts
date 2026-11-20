// What the Inbox lists and in which group. The rules of the Rust client's screens/updates.rs,
// screens/approval.rs and ui.rs::is_waiting; pure, so they are tested without a screen.

import type {
  ApprovalView,
  GuidelineProposalView,
  MessageView,
  NotificationView,
  ProposalView,
  TaskView,
} from "@aurelius/protocol";

export type InboxTone = "amber" | "violet" | "blue" | "green" | "muted" | "red";
export type InboxGlyph = "warn" | "ask" | "half" | "dot";
export type InboxFilter = "all" | "decisions" | "reviews" | "mentions";

export type InboxSource =
  | { kind: "approval"; approval: ApprovalView }
  | { kind: "proposal"; proposal: ProposalView }
  | { kind: "guideline"; proposal: GuidelineProposalView }
  | { kind: "task"; task: TaskView }
  | { kind: "notification"; notification: NotificationView };

export interface InboxRow {
  /** Unique over all kinds, and stable between refetches. */
  id: string;
  title: string;
  meta: string;
  atMs: number;
  tone: InboxTone;
  glyph: InboxGlyph;
  source: InboxSource;
}

export interface InboxGroup {
  key: "decide" | "review" | "updates";
  label: string;
  tone: InboxTone;
  rows: InboxRow[];
}

/** The tool without Claude Code's `claude.` prefix. */
export function toolName(tool: string): string {
  return tool.startsWith("claude.") ? tool.slice("claude.".length) : tool;
}

function text(args: unknown, key: string): string {
  const value = (args as Record<string, unknown> | null)?.[key];
  return typeof value === "string" ? value : "";
}

function firstLine(value: string): string {
  return value.split("\n", 1)[0] ?? "";
}

/** "Flux wants to run a command": the headline of an approval. */
export function approvalSentence(a: ApprovalView): string {
  const who = a.requester;
  const tool = toolName(a.tool);
  switch (tool) {
    case "Write":
      return `${who} wants to write a file`;
    case "Edit":
    case "MultiEdit":
    case "NotebookEdit":
      return `${who} wants to edit a file`;
    case "Bash":
    case "PowerShell":
      return `${who} wants to run a command`;
    case "WebFetch":
      return `${who} wants to open a web page`;
    case "WebSearch":
      return `${who} wants to search the web`;
    case "mcp__aurelius__screenshot":
      return `${who} wants to see your screen`;
    case "mcp__aurelius__list_windows":
      return `${who} wants to see which windows are open`;
    default:
      return `${who} wants to use ${tool}`;
  }
}

export interface ApprovalAction {
  /** "bash", "write": what the code box is titled. */
  label: string;
  /** The command, path, URL or query; empty when the arguments say nothing readable. */
  body: string;
  /** A second explanation of the agent's, if any. */
  description: string;
}

/** What an approval would do, as text. Everything comes from the agent, so React escapes it as text. */
export function approvalAction(a: ApprovalView): ApprovalAction {
  const args = a.arguments;
  const tool = toolName(a.tool);
  switch (tool) {
    case "Bash":
    case "PowerShell":
      return { label: tool.toLowerCase(), body: text(args, "command"), description: text(args, "description") };
    case "Write":
    case "Edit":
    case "MultiEdit":
      return { label: tool.toLowerCase(), body: text(args, "file_path"), description: "" };
    case "NotebookEdit":
      return { label: "notebook", body: text(args, "notebook_path"), description: "" };
    case "WebFetch":
      return { label: "web page", body: text(args, "url"), description: text(args, "prompt") };
    case "WebSearch":
      return { label: "web search", body: text(args, "query"), description: "" };
    default:
      return { label: tool, body: JSON.stringify(args, null, 2), description: "" };
  }
}

function approvalGist(a: ApprovalView): string {
  const { body } = approvalAction(a);
  return firstLine(body);
}

/** `status == waiting`, or queued with a reason. */
export function isWaiting(t: TaskView): boolean {
  return t.status === "waiting" || (t.status === "queued" && Boolean(t.waiting_reason));
}

/** The row's heading: the sentences that depend on who reads follow `needs_you`. */
export function notificationHeading(n: NotificationView): string {
  switch (n.reason) {
    case "plan_review":
      return n.needs_you ? "Plan ready for your review" : "Plan ready for review";
    case "review":
      return n.needs_you ? "Ready for your review" : "Ready for review";
    case "failed":
      return "The main task failed";
    case "login":
      return "Waiting for your Claude login";
    default:
      return n.title;
  }
}

/** What a waiting task waits for, in words. */
export function waitingLabel(reason: string | null | undefined): string | null {
  if (!reason) return null;
  const known: Record<string, string> = {
    plan_review: "plan to review",
    input: "waits for your answer",
    login: "waits for a Claude login",
    dependencies: "waits for other tasks",
    quota: "waits for its usage limit",
  };
  return known[reason] ?? reason.replaceAll("_", " ");
}

/** Where the row is: `#42 Title`, or `# general` outside a ticket. */
export function notificationPlace(n: NotificationView): string {
  const subject = n.subject === "" ? null : n.subject;
  if (n.ticket_number != null) {
    const number = `#${n.ticket_number}`;
    if (subject === null) return number;
    // Some subjects already start with their number.
    return subject === number || subject.startsWith(`${number} `) ? subject : `${number} ${subject}`;
  }
  if (subject === null) return "Update";
  if (subject.startsWith("#") || n.ticket_id != null) return subject;
  return `# ${subject}`;
}

/** The post in a discussion that still waits for the board member: the latest one. */
export function openQuestion(messages: MessageView[]): MessageView | null {
  return [...messages].reverse().find((m) => m.awaiting_user && !m.deleted_at_ms) ?? null;
}

export function ticketPlace(t: TaskView): string {
  return t.ticket_number == null ? t.title : `#${t.ticket_number}`;
}

const REVIEW_REASONS = new Set(["review", "plan_review"]);

export function buildInbox(input: {
  approvals: ApprovalView[];
  proposals: ProposalView[];
  guidelineProposals: GuidelineProposalView[];
  tasks: TaskView[];
  notifications: NotificationView[];
}): InboxGroup[] {
  const planHasNote = (t: TaskView) =>
    t.waiting_reason === "plan_review" &&
    input.notifications.some((n) => n.reason === "plan_review" && (n.task_id === t.id || n.ticket_id === t.ticket_id));
  const decide: InboxRow[] = [
    ...input.approvals
      .filter((a) => a.status === "pending")
      .map(
        (a): InboxRow => ({
          id: `approval:${a.id}`,
          title: approvalSentence(a),
          meta: [a.requester, approvalGist(a)].filter(Boolean).join(" · "),
          atMs: a.requested_at_ms,
          tone: "amber",
          glyph: "warn",
          source: { kind: "approval", approval: a },
        }),
      ),
    ...input.proposals
      .filter((p) => p.status === "pending")
      .map(
        (p): InboxRow => ({
          id: `proposal:${p.id}`,
          title: `Proposes ${p.name}`,
          meta: p.proposed_by_name,
          atMs: p.created_at_ms,
          tone: "violet",
          glyph: "ask",
          source: { kind: "proposal", proposal: p },
        }),
      ),
    ...input.guidelineProposals
      .filter((p) => p.status === "pending")
      .map(
        (p): InboxRow => ({
          id: `guideline:${p.id}`,
          title: `Guideline: ${p.title}`,
          meta: "Proposed guideline",
          atMs: p.created_at_ms,
          tone: "violet",
          glyph: "ask",
          source: { kind: "guideline", proposal: p },
        }),
      ),
    // A plan that also has its own row in Ready for review is one decision, not two.
    ...input.tasks
      .filter((t) => isWaiting(t) && !planHasNote(t))
      .map(
        (t): InboxRow => ({
          id: `task:${t.id}`,
          title: t.title,
          meta: [
            t.employee_name,
            t.ticket_number == null ? null : `#${t.ticket_number}`,
            waitingLabel(t.waiting_reason),
          ]
            .filter(Boolean)
            .join(" · "),
          atMs: t.updated_at_ms,
          tone: "violet",
          glyph: "ask",
          source: { kind: "task", task: t },
        }),
      ),
  ].sort((a, b) => b.atMs - a.atMs);

  const toRow = (n: NotificationView): InboxRow => {
    const review = REVIEW_REASONS.has(n.reason ?? "");
    const failed = n.reason === "failed";
    return {
      id: `notification:${n.id}`,
      title: notificationHeading(n),
      meta: notificationPlace(n) + ((n.merged_count ?? 0) > 0 ? ` · +${n.merged_count}` : ""),
      atMs: n.created_at_ms,
      tone: review ? "blue" : failed ? "red" : n.read ? "muted" : "green",
      glyph: review ? "half" : failed ? "warn" : "dot",
      source: { kind: "notification", notification: n },
    };
  };

  // "X mentioned you" on a ticket that already waits for your answer is the same question: one row.
  const asked = new Set(input.tasks.filter((t) => t.waiting_reason === "input" && t.ticket_id).map((t) => t.ticket_id));
  const sameQuestion = (n: NotificationView) => n.reason === "mention" && asked.has(n.ticket_id ?? null);
  const review = input.notifications.filter((n) => REVIEW_REASONS.has(n.reason ?? "")).map(toRow);
  const updates = input.notifications.filter((n) => !REVIEW_REASONS.has(n.reason ?? "") && !sameQuestion(n)).map(toRow);

  const groups: InboxGroup[] = [
    { key: "decide", label: "Needs decision", tone: "amber", rows: decide },
    { key: "review", label: "Ready for review", tone: "blue", rows: review },
    { key: "updates", label: "Updates", tone: "muted", rows: updates },
  ];
  return groups.filter((g) => g.rows.length > 0);
}

export function filterInbox(groups: InboxGroup[], filter: InboxFilter): InboxGroup[] {
  switch (filter) {
    case "all":
      return groups;
    case "decisions":
      return groups.filter((g) => g.key === "decide");
    case "reviews":
      return groups.filter((g) => g.key === "review");
    case "mentions":
      return groups
        .map((g) => ({
          ...g,
          rows: g.rows.filter((r) => r.source.kind === "notification" && r.source.notification.reason === "mention"),
        }))
        .filter((g) => g.rows.length > 0);
  }
}

/** What the sidebar's Inbox counts: what needs a decision or the reader's review. */
export function needsYouCount(groups: InboxGroup[]): number {
  return groups.reduce((sum, g) => {
    if (g.key === "decide") return sum + g.rows.length;
    return (
      sum +
      g.rows.filter(
        (r) => r.source.kind === "notification" && r.source.notification.needs_you && !r.source.notification.read,
      ).length
    );
  }, 0);
}

/** "2m", "8h", "1d". */
export function age(atMs: number, nowMs: number): string {
  const minutes = Math.max(0, Math.floor((nowMs - atMs) / 60_000));
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

/** What is new since the last list: nothing the first time, read rows and old rows never. */
export function newNotes(before: NotificationView[] | null, after: NotificationView[]): NotificationView[] {
  if (before === null) return [];
  return after.filter((n) => {
    if (n.read) return false;
    const old = before.find((b) => b.id === n.id);
    return !old || n.created_at_ms > old.created_at_ms;
  });
}

/** Where a row of the bell leads: the place where it is answered or decided. */
export type RowPlace =
  | { kind: "ticket"; id: string; number: number | null; message?: string }
  | { kind: "task"; id: string }
  | { kind: "conversation"; id: string }
  | { kind: "message"; id: string }
  | { kind: "employees" }
  | { kind: "guideline"; id: string; proposal: string }
  /** Nowhere else: an approval is decided in the bell itself, a plain update is only read. */
  | { kind: "here" };

/**
 * The place of a row. `canLocate`: the server finds a message by its id (feature `message_locate`), so a
 * mention opens at that message wherever it was written.
 */
export function rowPlace(row: InboxRow, canLocate: boolean): RowPlace {
  const { source } = row;
  switch (source.kind) {
    case "approval": {
      const a = source.approval;
      // Its run shows the card in line; a ticket or conversation shows it where the run asked.
      if (a.task_id) return { kind: "task", id: a.task_id };
      if (a.ticket_id) return { kind: "ticket", id: a.ticket_id, number: null };
      if (a.conversation_id) return { kind: "conversation", id: a.conversation_id };
      return { kind: "here" };
    }
    case "proposal":
      return { kind: "employees" };
    case "guideline":
      return { kind: "guideline", id: source.proposal.guideline_id ?? "new", proposal: source.proposal.id };
    case "task": {
      const t = source.task;
      if (t.ticket_id) return { kind: "ticket", id: t.ticket_id, number: t.ticket_number ?? null };
      return { kind: "task", id: t.id };
    }
    case "notification": {
      const n = source.notification;
      // A mention or a message outside a ticket: right to that message.
      const inDiscussion = n.reason === "mention" || n.reason === "sent_back";
      if (n.message_id && canLocate && (!n.ticket_id || inDiscussion)) return { kind: "message", id: n.message_id };
      if (n.ticket_id) {
        return {
          kind: "ticket",
          id: n.ticket_id,
          number: n.ticket_number ?? null,
          ...(n.message_id ? { message: n.message_id } : {}),
        };
      }
      if (n.task_id) return { kind: "task", id: n.task_id };
      if (n.conversation_id) return { kind: "conversation", id: n.conversation_id };
      return { kind: "here" };
    }
  }
}
