// A ticket as a session (`/ticket/$number`, ticket #70): which ticket a number is, what the bottom of
// the screen shows for its column, who answers a message without a name, and where old links go.
// The server decides all of it as before; these only read what it sends.

import type { ChannelView, ColumnKind, TicketView, TokenUsage } from "@aurelius/protocol";
import { ownerAnswers as ownerAnswersText } from "./chat-logic";
import type { UsageTotals } from "./run-logic";
import type { SessionSignal } from "./session-logic";
import { filesWritten } from "./tasks-logic";

/** The ticket of a number, the search still to do, or none. */
export type Resolved = { ticketId: string; conversationId: string | null } | "search" | "missing";

/**
 * A number to a ticket: the cached channel list first (it has archived tickets too), then the
 * `ticket.list { query: "#N" }` answer once it is read (`listed`); `undefined` means not read yet.
 */
export function resolveNumber(
  n: number,
  channels: ChannelView[] | undefined,
  listed: TicketView[] | undefined,
): Resolved {
  const c = channels?.find((x) => x.ticket_number === n && x.ticket_id);
  if (c?.ticket_id) return { ticketId: c.ticket_id, conversationId: c.conversation_id };
  if (listed === undefined) return "search";
  const t = listed.find((x) => x.number === n);
  return t ? { ticketId: t.id, conversationId: t.conversation_id ?? null } : "missing";
}

/** A closed ticket: Done or archived. */
const closed = (t: Pick<TicketView, "column" | "archived_at_ms">) => t.column === "done" || Boolean(t.archived_at_ms);

/** What sits right above the composer: the decision of the moment. */
export function gateOf(t: Pick<TicketView, "column" | "archived_at_ms">): "plan" | "review" | "status" | "none" {
  if (closed(t)) return "none";
  if (t.column === "plan_review") return "plan";
  return t.column === "review" ? "review" : "status";
}

/**
 * Whether the session only reads. The ticket decides once it is known, so leaving Done makes it
 * writable before the channel's read catches up; before that, the feed's own flag.
 */
export function readOnly(
  t: Pick<TicketView, "column" | "archived_at_ms"> | null | undefined,
  feed: { read_only?: boolean } | null | undefined,
): boolean {
  if (t) return closed(t);
  return feed?.read_only === true;
}

const OWNER_COLUMNS: ColumnKind[] = ["todo", "doing", "plan_review", "testing", "review"];

/** Who answers a message without a name (BR1, `owner_answers` on the server), or nobody. */
export function ownerAnswers(
  t: Pick<TicketView, "column" | "archived_at_ms" | "owner_name" | "owner_employee_id">,
): string | null {
  if (!t.owner_name || t.archived_at_ms || !OWNER_COLUMNS.includes(t.column)) return null;
  return t.owner_name;
}

type Talked = Pick<TicketView, "column" | "archived_at_ms" | "owner_name" | "owner_employee_id" | "number">;

export function composerPlaceholder(t: Talked): string {
  const owner = ownerAnswers(t);
  return owner ? `Message ${owner} (owner)… · @ to mention` : `Message #${t.number} · @ to mention`;
}

export function composerHint(t: Talked): string {
  return `@Name asks that employee${ownerAnswersText(ownerAnswers(t))}.`;
}

/** Where an old link goes: an id still to resolve into a ticket's session. */
export type Redirect = { ticketId: string };

/** BR6. `/board?ticket=` and `/history?ticket=` carry an id, which `ticket.get` turns into a number. */
export function sessionRedirect(path: string, search: { ticket?: string }): Redirect | null {
  if ((path === "/board" || path === "/history") && search.ticket) return { ticketId: search.ticket };
  return null;
}

/** The head's second line: who answers now, else owner, responsible and project. */
export function headLine(
  t: Pick<TicketView, "owner_name" | "responsible_user_id" | "responsible_username" | "project_name">,
  me: string | null | undefined,
  answering: string[],
): string {
  if (answering.length > 0) return `${answering.join(", ")} answering`;
  const responsible =
    t.responsible_user_id && t.responsible_user_id === me
      ? "responsible You"
      : t.responsible_username
        ? `responsible ${t.responsible_username}`
        : null;
  return [t.owner_name ? `owner ${t.owner_name}` : "No owner", responsible, t.project_name || null]
    .filter(Boolean)
    .join(" · ");
}

/** `usage.get_totals { ticket_id }`'s total as the side panel's usage; none without one. */
export function ticketUsage(total: TokenUsage | null | undefined): UsageTotals | null {
  if (!total) return null;
  return {
    input: total.input_tokens ?? 0,
    output: total.output_tokens ?? 0,
    cached: total.cached_tokens ?? 0,
    written: total.cache_creation_tokens ?? 0,
    micro: total.cost_micro_usd ?? 0,
  };
}

/** The conversations a ticket's runs answer in: its discussion and each of its tasks', once each. */
export function ticketConversations(
  discussion: string | null | undefined,
  tasks: { conversation_id?: string | null }[],
): string[] {
  const out = new Set<string>();
  if (discussion) out.add(discussion);
  for (const t of tasks) if (t.conversation_id) out.add(t.conversation_id);
  return [...out];
}

type Activity = { activity: { tool: string; input: unknown; ok?: boolean | null }[] };

/** The files written in all of them, in order, without doubles; a list not read yet adds nothing. */
export function filesWrittenAcross(lists: (Activity[] | undefined)[]): string[] {
  const out = new Set<string>();
  for (const list of lists) for (const f of filesWritten(list ?? [])) out.add(f);
  return [...out];
}

/** The latest run that answered in any of them: its guidelines show in the side panel. */
export function latestRunAcross(
  lists: ({ run_id?: string | null; created_at_ms: number }[] | undefined)[],
): string | null {
  let best: { run: string; at: number } | null = null;
  for (const list of lists) {
    for (const m of list ?? []) {
      if (m.run_id && (best === null || m.created_at_ms >= best.at)) best = { run: m.run_id, at: m.created_at_ms };
    }
  }
  return best?.run ?? null;
}

export type TicketPlace =
  | { to: "/ticket/$number"; params: { number: string }; search?: { message?: string; thread?: string } }
  | { to: "/board"; search: { ticket: string } };

/** Where "Open ticket" goes: the session by number; with only an id, the Board's redirect resolves it. */
export function ticketPlace(
  t: { id: string; number?: number | null },
  search?: { message?: string; thread?: string },
): TicketPlace {
  if (t.number === null || t.number === undefined) return { to: "/board", search: { ticket: t.id } };
  return {
    to: "/ticket/$number",
    params: { number: String(t.number) },
    ...(search && Object.keys(search).length > 0 ? { search } : {}),
  };
}

/** Ctrl+Enter decides in the gate, except while it would send the words in a filled field. */
export function gateKeyAllowed(focused: { tagName: string; value?: string } | null): boolean {
  if (!focused || (focused.tagName !== "TEXTAREA" && focused.tagName !== "INPUT")) return true;
  return (focused.value ?? "").trim() === "";
}

/** How many tickets the sidebar lists at most, besides the ones that work or need you. */
export const SIDEBAR_TICKETS = 8;

/** The columns of a ticket that is under way: not waiting in the Backlog and not done yet. */
const OPEN_COLUMNS: readonly ColumnKind[] = ["todo", "doing", "plan_review", "testing", "review"];

/** A ticket's dot in the sidebar: it needs you first, else it works while its main task runs. */
export function ticketSignal(t: Pick<TicketView, "root_task_status">, needsYou: boolean): SessionSignal | null {
  if (needsYou) return "needs_you";
  return t.root_task_status === "running" || t.root_task_status === "queued" ? "working" : null;
}

/**
 * The sidebar's tickets: the open ones of the workspace's project (all when none is chosen), the last
 * changed first, at most `SIDEBAR_TICKETS` plus any older one with a signal, so none that needs you hides.
 * With `since`, only the ones with a signal and those changed from then on: the default view, today's work.
 */
export function sidebarTickets<
  T extends Pick<TicketView, "project_id" | "column" | "archived_at_ms" | "updated_at_ms">,
>(tickets: T[], workspaceId: string | null, signal: (t: T) => SessionSignal | null, since: number | null = null): T[] {
  const open = tickets
    .filter(
      (t) =>
        (workspaceId === null || t.project_id === workspaceId) && !t.archived_at_ms && OPEN_COLUMNS.includes(t.column),
    )
    .sort((a, b) => b.updated_at_ms - a.updated_at_ms);
  if (since !== null) return open.filter((t) => signal(t) !== null || t.updated_at_ms >= since);
  return open.filter((t, i) => i < SIDEBAR_TICKETS || signal(t) !== null);
}

/** The first `#N` a line names (a system line, the "→ #N" chip), so its link opens the session directly. */
export function ticketNumberIn(text: string): number | null {
  const n = /#(\d+)/.exec(text)?.[1];
  return n ? Number(n) : null;
}
