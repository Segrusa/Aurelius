// The Board's rules, pure. (ui-ontwerp §3, §5, §6): a
// drag, "Move to…" and the buttons follow one table. The server decides again; this keeps the client honest.

import type {
  ColumnKind,
  EmployeeSummary,
  TaskView,
  TicketDetail,
  TicketPriority,
  TicketView,
  TimelineCard,
} from "@aurelius/protocol";
import { type KindFilter, kindMatches } from "./quick-logic";

export const DONE_SHOWN = 20;
export const GAVE_UP_LIMIT = 20;
export const ACTIVITY_LIMIT = 100;

export type DropEffect =
  | { kind: "reorder" }
  | { kind: "start" }
  | { kind: "back_to_backlog"; confirm: boolean }
  | { kind: "reject" }
  | { kind: "plan" }
  | { kind: "send_back" }
  | { kind: "mark_done" }
  | { kind: "refused"; why: string };

const refused = (why: string): DropEffect => ({ kind: "refused", why });

/** §6, the table "Van \ Naar". `choosing`: the dispatcher is choosing the owner of this Todo ticket. */
export function dropEffect(from: ColumnKind, to: ColumnKind, choosing: boolean): DropEffect {
  if (from === "done") return refused("Done tickets are closed.");
  if (from === to) return { kind: "reorder" };
  if (from === "backlog" && to === "todo") return { kind: "start" };
  if (from === "todo" && to === "backlog") return { kind: "back_to_backlog", confirm: choosing };
  if ((from === "doing" || from === "testing" || from === "review") && to === "backlog")
    return { kind: "back_to_backlog", confirm: true };
  if (from === "plan_review" && to === "backlog") return { kind: "reject" };
  if (from === "plan_review" && to === "doing") return { kind: "plan" };
  if (from === "review" && to === "doing") return { kind: "send_back" };
  if (from === "review" && to === "done") return { kind: "mark_done" };
  if (from === "testing" || to === "testing") return refused("Testing moves on by itself when the tests have run.");
  if (from === "backlog") return refused("Start first.");
  if (to === "done") return refused("Only from Review.");
  if (to === "review") return refused("The owner hands it in.");
  if (to === "plan_review") return refused("The engine moves tickets here.");
  if (from === "todo" && to === "doing") return refused("Doing starts when an owner is chosen.");
  if (from === "doing" && to === "todo") return refused("Not from Doing.");
  if (to === "todo") return refused("Only from Backlog.");
  return refused("The engine moves tickets here.");
}

export type Needs = "review" | "plan_review" | "choose_owner" | "failed" | "input" | "approval" | "connect_login";

const NEEDS_MINE: Record<Needs, string> = {
  review: "Needs you · review",
  plan_review: "Needs you · plan",
  choose_owner: "Choose an employee",
  failed: "✕ Failed · Needs you",
  input: "Needs you · stopped",
  approval: "Waiting for your approval",
  connect_login: "Needs you · Claude login",
};

const NEEDS_OTHERS: Record<Needs, string> = {
  review: "Waiting for review",
  plan_review: "Waiting for plan review",
  choose_owner: "Choose an employee",
  failed: "✕ Failed",
  input: "Stopped, waits for an answer",
  approval: "Waiting for approval",
  connect_login: "Waiting for a Claude login",
};

export function needsLine(needs: Needs, mine: boolean): string {
  return mine ? NEEDS_MINE[needs] : NEEDS_OTHERS[needs];
}

/** What a card waits for from a board member, without a login in the way. */
export function needsYou(
  t: TicketView,
  choosing: boolean,
  rootApprovals: number,
  rootWaiting: string | null = null,
): Needs | null {
  if (t.column === "review") return "review";
  if (t.column === "plan_review") return "plan_review";
  if (t.column === "backlog" || t.column === "done") return null;
  if ((t.column === "todo" || t.column === "doing") && t.root_task_status === "failed") return "failed";
  if (rootApprovals > 0) return "approval";
  if (t.column === "doing" && t.root_task_status === "waiting" && rootWaiting === "input") return "input";
  if (t.column === "todo" && !t.owner_employee_id && !choosing) return "choose_owner";
  return null;
}

/** A missing Claude login counts only for its own board member. */
export function ticketNeeds(
  t: TicketView,
  me: string | null,
  choosing: boolean,
  rootApprovals: number,
  rootWaiting: string | null = null,
): Needs | null {
  if (t.needs === "login" && (t.column === "todo" || t.column === "doing"))
    return me === t.login_user ? "connect_login" : null;
  return needsYou(t, choosing, rootApprovals, rootWaiting);
}

/** B6: the ◆, the filter and the badge follow this; anyone can still decide. */
export function needsMe(needs: Needs | null, t: TicketView, me: string | null): boolean {
  if (needs === null) return false;
  if (needs === "connect_login") return me === t.login_user;
  return !t.responsible_user_id || Boolean(t.responsible_disabled) || t.responsible_user_id === me;
}

/** Freeze §5: a ticket's main task counts only when its ticket needs this member. */
export function taskCounts(
  task: TaskView,
  responsibleSupported: boolean,
  ticket: TicketView | undefined,
  mine: (t: TicketView) => boolean,
): boolean {
  if (!responsibleSupported || !task.ticket_id || task.parent_task_id) return true;
  return ticket === undefined || mine(ticket);
}

/** What the review and plan bars say to someone else than the Responsible. */
export function decideToo(t: TicketView): string | null {
  return t.responsible_username && t.responsible_user_id && !t.responsible_disabled
    ? `${t.responsible_username} is responsible. You can decide too.`
    : null;
}

/** Whose Claude login a ticket runs on. */
export function loginWait(t: TicketView, me: string | null, disabled: boolean): string | null {
  if (t.needs !== "login") return null;
  if (me === t.login_user) return "◆ Waiting for your Claude login";
  return disabled ? `Waiting: ${t.login_username} is disabled` : `Waiting for ${t.login_username}'s Claude login`;
}

export function takeOverOffered(t: TicketView, me: string | null): boolean {
  return t.needs === "login" && me !== null && me !== t.login_user && t.column !== "done" && t.column !== "backlog";
}

export const PRIORITIES: TicketPriority[] = ["urgent", "high", "normal", "low"];

/** As text and shape, never only colour. */
export function priorityLabel(p: TicketPriority): string {
  return { urgent: "▲▲ Urgent", high: "▲ High", normal: "Normal", low: "▽ Low" }[p];
}

export function priorityColor(p: TicketPriority): string {
  return {
    urgent: "var(--color-text-error)",
    high: "var(--color-text-warning)",
    normal: "var(--color-text-secondary)",
    low: "var(--color-text-secondary)",
  }[p];
}

export function emptyText(kind: ColumnKind): string {
  return {
    backlog: "New tickets land here.",
    todo: "Started tickets wait here for an owner.",
    doing: "Nothing in progress.",
    plan_review: "Only tickets in Plan mode stop here, with their plan.",
    testing: "Nothing being tested.",
    review: "Nothing to review.",
    done: "Closed tickets appear here.",
  }[kind];
}

/** Testing only on a server with `board_testing`. */
export function columnShown(kind: ColumnKind, testing: boolean): boolean {
  return kind !== "testing" || testing;
}

export function testingNote(tester: string | null): string {
  return `◆ Testing · ${tester ?? "The tester"} runs the full test suite. Green goes on to Review; red back to Doing.`;
}

const DAY_MS = 86_400_000;
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const pad = (n: number) => String(n).padStart(2, "0");

/** The form `parseDeadline` reads. */
export function deadlineInput(ms: number, offsetMinutes: number): string {
  const local = new Date(ms + offsetMinutes * 60_000);
  const date = `${local.getUTCFullYear()}-${pad(local.getUTCMonth() + 1)}-${pad(local.getUTCDate())}`;
  const minute = local.getUTCHours() * 60 + local.getUTCMinutes();
  return minute === 23 * 60 + 59 ? date : `${date} ${pad(local.getUTCHours())}:${pad(local.getUTCMinutes())}`;
}

export type DeadlineTone = "muted" | "soon" | "over";

/** "Due Nov 3", within a day "Due today 17:00", past "Overdue · Oct 29" (not in Done). */
export function deadlineText(
  deadlineMs: number,
  nowMs: number,
  done: boolean,
  offsetMinutes: number,
): { text: string; tone: DeadlineTone } {
  const local = new Date(deadlineMs + offsetMinutes * 60_000);
  const day = `${MONTHS[local.getUTCMonth()]} ${local.getUTCDate()}`;
  if (done) return { text: `Due ${day}`, tone: "muted" };
  if (deadlineMs < nowMs) return { text: `Overdue · ${day}`, tone: "over" };
  if (deadlineMs - nowMs <= DAY_MS) {
    const dayNumber = (ms: number) => Math.floor((ms + offsetMinutes * 60_000) / DAY_MS);
    const when = dayNumber(deadlineMs) === dayNumber(nowMs) ? "today" : "tomorrow";
    return { text: `Due ${when} ${pad(local.getUTCHours())}:${pad(local.getUTCMinutes())}`, tone: "soon" };
  }
  return { text: `Due ${day}`, tone: "muted" };
}

/**
 * The neighbour a card goes right above when dropped on `onto` (null: the bottom of the column).
 * `undefined` when the drop changes nothing.
 */
export function beforeFor(cards: { id: string }[], dragged: string, onto: string | null): string | null | undefined {
  const at = cards.findIndex((t) => t.id === dragged);
  if (onto === dragged) return undefined;
  if (onto !== null) return at >= 0 && cards[at + 1]?.id === onto ? undefined : onto;
  return at >= 0 && at + 1 === cards.length ? undefined : null;
}

export type OwnerFilter = { kind: "anyone" } | { kind: "none" } | { kind: "employee"; id: string };

/** Who is responsible: anyone, nobody, or this user's id. */
export type ResponsibleFilter = { kind: "anyone" } | { kind: "none" } | { kind: "user"; id: string };

export interface CardFilter {
  owner: OwnerFilter;
  responsible?: ResponsibleFilter;
  query: string;
  needsOnly: boolean;
  needs: (t: TicketView) => boolean;
  /** Only quick tasks, on a server with them. */
  kind?: KindFilter;
}

/** The cards a column shows. Done: newest first, `DONE_SHOWN` unless all. */
export function shownCards(
  kind: ColumnKind,
  tickets: TicketView[],
  filter: CardFilter,
  doneAll: boolean,
): { cards: TicketView[]; older: number } {
  const query = filter.query.trim().toLowerCase();
  const number = query.replace(/^#+/, "");
  let cards = tickets.filter(
    (t) =>
      (filter.owner.kind === "anyone" ||
        (filter.owner.kind === "none" ? !t.owner_employee_id : t.owner_employee_id === filter.owner.id)) &&
      (!filter.responsible ||
        filter.responsible.kind === "anyone" ||
        (filter.responsible.kind === "none"
          ? !t.responsible_user_id
          : t.responsible_user_id === filter.responsible.id)) &&
      (query === "" ||
        t.title.toLowerCase().includes(query) ||
        (number !== "" && String(t.number) === number) ||
        t.labels.some((l) => l.toLowerCase().includes(query))) &&
      (!filter.needsOnly || filter.needs(t)) &&
      kindMatches(filter.kind ?? "all", t),
  );
  let older = 0;
  if (kind === "done") {
    cards = [...cards].sort((a, b) => (b.done_at_ms ?? b.updated_at_ms) - (a.done_at_ms ?? a.updated_at_ms));
    if (!doneAll && cards.length > DONE_SHOWN) {
      older = cards.length - DONE_SHOWN;
      cards = cards.slice(0, DONE_SHOWN);
    }
  }
  return { cards, older };
}

/** Moves a card locally for an optimistic drop; the next read of the board settles it. */
export function movedCard<T extends { id: string }>(
  columns: { key: string; tickets: T[] }[],
  id: string,
  to: string,
  before: string | null,
): { key: string; tickets: T[] }[] {
  const card = columns.flatMap((c) => c.tickets).find((t) => t.id === id);
  if (!card) return columns;
  return columns.map((c) => {
    const rest = c.tickets.filter((t) => t.id !== id);
    if (c.key !== to) return { ...c, tickets: rest };
    const at = before === null ? rest.length : rest.findIndex((t) => t.id === before);
    return {
      ...c,
      tickets: [...rest.slice(0, at < 0 ? rest.length : at), card, ...rest.slice(at < 0 ? rest.length : at)],
    };
  });
}

export type Dispatcher =
  | { kind: "project" | "company"; id: string; name: string }
  | { kind: "inactive"; name: string; state: string }
  | { kind: "none" };

/** As the server's `dispatcher_for`: the project's own, else the company's, only while active. */
export function dispatcherOf(
  project: string | null | undefined,
  company: string | null | undefined,
  employees: EmployeeSummary[],
): Dispatcher {
  const id = project ?? company;
  if (!id) return { kind: "none" };
  const own = Boolean(project);
  const e = employees.find((x) => x.id === id);
  if (!e)
    return {
      kind: "inactive",
      name: own ? "The project's dispatcher" : "The company's dispatcher",
      state: "not active",
    };
  if (e.status === "active") return { kind: own ? "project" : "company", id, name: e.name };
  return { kind: "inactive", name: e.name, state: e.status === "paused" ? "paused" : "archived" };
}

/** The server says so with a `ticket.updated` the system made, newer than the last start, choice or cancel. Newest first. */
export function dispatcherGaveUp(activity: TimelineCard[]): boolean {
  for (const card of activity) {
    if (card.title === "ticket.started" || card.title === "ticket.dispatched" || card.title === "ticket.cancelled")
      return false;
    if (card.title === "ticket.updated" && card.actor === "system") return true;
  }
  return false;
}

export function isChoosing(t: TicketView, hasDispatcher: boolean, gaveUp: boolean): boolean {
  return t.column === "todo" && !t.owner_employee_id && hasDispatcher && !gaveUp;
}

/** Which feedback the Doing bar shows: "Sent back" only after Review. Newest first. */
export function feedbackLabel(activity: TimelineCard[] | undefined): string {
  const last = activity?.find(
    (c) => c.title === "ticket.sent_back" || c.title === "ticket.plan_approved" || c.title === "ticket.plan_revised",
  )?.title;
  if (last === "ticket.sent_back") return "Sent back";
  if (last === "ticket.plan_approved") return "With the plan's approval";
  return last === "ticket.plan_revised" ? "New plan asked" : "Feedback";
}

export const runLocked = (t: TicketView) => t.root_task_status === "running";
export const editable = (t: TicketView) => t.column !== "done";
/** In Backlog, and in Todo while the main task is a draft (`check_assign`). */
export const ownerChoosable = (t: TicketView) =>
  t.column === "backlog" || (t.column === "todo" && (!t.root_task_status || t.root_task_status === "draft"));

export type DoneCheck =
  | { kind: "loading" }
  | { kind: "working"; tasks: TaskView[]; conversation: boolean }
  | { kind: "ready" };

/** Whether ✓ Done can close a ticket in Review now, as `ticket.accept` checks it. */
export function doneCheck(detail: TicketDetail | undefined): DoneCheck {
  if (!detail) return { kind: "loading" };
  const root = detail.ticket.root_task_id;
  const tasks = detail.tasks.filter(
    (t) => t.id !== root && (t.status === "queued" || t.status === "running" || t.status === "waiting"),
  );
  const conversation = Boolean(detail.root_task?.live_run);
  return tasks.length === 0 && !conversation ? { kind: "ready" } : { kind: "working", tasks, conversation };
}

/** A plan is the newest message marked as one. */
export function latestPlan(messages: { update?: string | null; text: string }[]): string | null {
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i];
    if (m?.update === "plan") return m.text;
  }
  return null;
}

/** What keeps the column names from being saved, or null. */
export function columnsProblem(columns: { name: string; title: string }[]): string | null {
  const blank = columns.find((c) => c.name === "");
  if (blank) return `Give ${blank.title} a name.`;
  const names = columns.map((c) => c.name.toLowerCase());
  return new Set(names).size === names.length ? null : "Each column needs its own name.";
}

export interface DiscussionView<M> {
  shown: M[];
  /** The message after which earlier updates are folded away, and how many. */
  earlier: Map<string, number>;
}

/** Runs of the system's own lines fold to their latest: people's words stay readable. */
export function foldUpdates<M extends { id: string; system?: unknown }>(
  list: M[],
  unfolded: boolean,
): DiscussionView<M> {
  const earlier = new Map<string, number>();
  if (unfolded) return { shown: list, earlier };
  const shown: M[] = [];
  let run = 0;
  list.forEach((m, i) => {
    const next = list[i + 1];
    if (m.system && next?.system) {
      run += 1;
      return;
    }
    if (m.system && run > 0) earlier.set(m.id, run);
    run = 0;
    shown.push(m);
  });
  return { shown, earlier };
}

/** A ready-made reply to a report: what the button says and what the owner is told. */
export interface SuggestedReply {
  label: string;
  text: string;
}

/**
 * One-click answers to a report in review: the usual next steps, plus the ones the report itself points at.
 * A quick task is merged by Approve and merge, so it is not asked to merge.
 */
export function suggestedReplies(report: string | null, quick = false): SuggestedReply[] {
  const text = report ?? "";
  const replies: SuggestedReply[] = quick ? [] : [{ label: "Merge into main", text: "Merge this into main." }];
  if (/\bpush/i.test(text) || /\bmerge[dn]?\b/i.test(text)) {
    replies.push({ label: "Push", text: "Push main to the remote." });
  }
  if (/not (tested|built)|niet (getest|gebouwd)|untested|could not (test|build)/i.test(text)) {
    replies.push({ label: "Build and test it", text: "Build it and run the tests, then report the result." });
  }
  replies.push({ label: "Explain what you did", text: "Explain in a few lines what you changed and why." });
  return replies;
}

/** A span on the board's scrolling row, in pixels from its start. */
export type BoardSpan = { start: number; end: number };

/** Where the row scrolls to show a card. `stops` are the scroll positions that start the row at a column, so a move
 * never leaves a column cut in half at the edge; a card that is already in view moves nothing. */
export function scrollToShow(card: BoardSpan, stops: number[], scroll: number, width: number): number {
  if (card.start >= scroll && card.end <= scroll + width) return scroll;
  const fitting = stops.filter((stop) => stop <= card.start && stop + width >= card.end);
  if (fitting.length === 0) return card.start;
  return card.end > scroll + width ? (fitting[0] as number) : (fitting[fitting.length - 1] as number);
}
