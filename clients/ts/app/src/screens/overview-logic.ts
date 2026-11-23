// The Overview of a workspace (ticket #82): what needs the board member, what runs, the board in five lanes,
// what got done, and under All workspaces a card per workspace. Everything is derived from the tickets and
// work sessions the client already reads, so the page needs no read of its own on the server.

import type { ColumnKind, GuidelineView, TicketView, WorkSessionView } from "@aurelius/protocol";
import type { Needs } from "./board-logic";
import { sessionSignal } from "./session-logic";

/** How many tickets a lane of the Board strip names under its count. */
export const LANE_TOP = 2;
/** How many finished tickets Recently done lists. */
export const RECENT_DONE = 5;
/** How many designs the Designs section shows; All designs has the rest. */
export const DESIGNS_SHOWN = 4;

export type NeedKind = "question" | "plan" | "review";

/** One card under Needs you: a ticket or a session that waits for the reader. */
export interface NeedCard {
  key: string;
  kind: NeedKind;
  title: string;
  who: string;
  whoId: string;
  projectId: string;
  atMs: number;
  /** Where the card opens: the ticket's or the session's screen. */
  to: string;
  /**
   * What it waits for, in one line. Not the post itself: reading a conversation marks it seen, which an
   * overview must not do.
   */
  text: string;
}

/** A ticket's need as one of the three cards of the design: a plan, a review, else a question. */
export function needKind(needs: Needs | null): NeedKind {
  if (needs === "review") return "review";
  if (needs === "plan_review") return "plan";
  return "question";
}

export const NEED_LABELS: Record<NeedKind, string> = {
  question: "Question for you",
  plan: "Plan to approve",
  review: "Ready for review",
};

export const NEED_ACTIONS: Record<NeedKind, string> = {
  question: "Answer",
  plan: "Open plan",
  review: "Review",
};

const NEED_TEXTS: Record<Needs, string> = {
  review: "Ready for your review.",
  plan_review: "A plan waits for your approval.",
  choose_owner: "Choose who takes this ticket.",
  failed: "The run failed. Open it to see why.",
  input: "Stopped, waits for your answer.",
  approval: "Waits for your approval.",
  connect_login: "Waits for your Claude login.",
};

/**
 * What needs the reader, newest first: the tickets `mine` says need them and the sessions whose dispatcher
 * chose nobody. The same rule as the rail's red badge, so the two counts agree.
 */
export function needCards(
  tickets: TicketView[],
  sessions: WorkSessionView[],
  needs: (t: TicketView) => Needs | null,
  mine: (t: TicketView) => boolean,
): NeedCard[] {
  const fromTickets = tickets
    .filter((t) => !t.archived_at_ms && mine(t))
    .map((t): NeedCard => {
      const need = needs(t);
      return {
        key: `t:${t.id}`,
        kind: needKind(need),
        title: `#${t.number} ${t.title}`,
        who: t.owner_name ?? "Dispatcher",
        whoId: t.owner_employee_id ?? t.id,
        projectId: t.project_id,
        atMs: t.updated_at_ms,
        to: `/ticket/${t.number}`,
        text: need ? NEED_TEXTS[need] : "Waits for you.",
      };
    });
  const fromSessions = sessions
    .filter((s) => sessionSignal(s) === "needs_you")
    .map(
      (s): NeedCard => ({
        key: `s:${s.conversation_id}`,
        kind: "question",
        title: s.title || "Session",
        who: s.employee_name,
        whoId: s.employee_id,
        projectId: s.project_id,
        atMs: s.last_activity_ms,
        to: `/session/${s.conversation_id}`,
        text: "Nobody took this session. Choose who does it.",
      }),
    );
  return [...fromTickets, ...fromSessions].sort((a, b) => b.atMs - a.atMs);
}

/** One row under Running now. */
export interface RunningRow {
  key: string;
  number: number | null;
  title: string;
  who: string;
  whoId: string;
  projectId: string;
  /** When it started working, when known; else its last change. */
  sinceMs: number;
  to: string;
  /** Queued: waits for a slot. */
  queued: boolean;
}

/**
 * What works now: tickets whose main task runs or is queued and sessions with a run, the longest running
 * first. `since` gives a ticket's start from the activity overview, when the server has it.
 */
export function runningRows(
  tickets: TicketView[],
  sessions: WorkSessionView[],
  since: (ticketId: string) => number | null = () => null,
): RunningRow[] {
  const fromTickets = tickets
    .filter((t) => !t.archived_at_ms && (t.root_task_status === "running" || t.root_task_status === "queued"))
    .map(
      (t): RunningRow => ({
        key: `t:${t.id}`,
        number: t.number,
        title: t.title,
        who: t.owner_name ?? "Dispatcher",
        whoId: t.owner_employee_id ?? t.id,
        projectId: t.project_id,
        sinceMs: since(t.id) ?? t.updated_at_ms,
        to: `/ticket/${t.number}`,
        queued: t.root_task_status === "queued",
      }),
    );
  const fromSessions = sessions
    .filter((s) => s.running)
    .map(
      (s): RunningRow => ({
        key: `s:${s.conversation_id}`,
        number: null,
        title: s.title || "Session",
        who: s.employee_name,
        whoId: s.employee_id,
        projectId: s.project_id,
        sinceMs: s.last_activity_ms,
        to: `/session/${s.conversation_id}`,
        queued: false,
      }),
    );
  return [...fromTickets, ...fromSessions].sort((a, b) => a.sinceMs - b.sinceMs);
}

export type LaneName = "Backlog" | "Todo" | "Doing" | "Review" | "Done";

export interface Lane {
  name: LaneName;
  count: number;
  top: Pick<TicketView, "id" | "number" | "title">[];
}

/** The five lanes of the strip; plan review and testing count as Review, where the board member acts. */
const LANE_OF: Record<ColumnKind, LaneName> = {
  backlog: "Backlog",
  todo: "Todo",
  doing: "Doing",
  plan_review: "Review",
  testing: "Review",
  review: "Review",
  done: "Done",
};

export const LANES: readonly LaneName[] = ["Backlog", "Todo", "Doing", "Review", "Done"];

/**
 * The Board strip: per lane its count and the first `LANE_TOP` tickets, in the board's order (Done: the
 * last finished first). Archived tickets are not on the board.
 */
export function boardLanes(tickets: TicketView[]): Lane[] {
  const live = tickets.filter((t) => !t.archived_at_ms);
  return LANES.map((name) => {
    const mine = live.filter((t) => LANE_OF[t.column] === name);
    const ordered =
      name === "Done"
        ? [...mine].sort((a, b) => (b.done_at_ms ?? b.updated_at_ms) - (a.done_at_ms ?? a.updated_at_ms))
        : [...mine].sort((a, b) => a.position - b.position);
    return {
      name,
      count: mine.length,
      top: ordered.slice(0, LANE_TOP).map((t) => ({ id: t.id, number: t.number, title: t.title })),
    };
  });
}

const doneAt = (t: TicketView) => t.done_at_ms ?? t.updated_at_ms;

/** The last finished tickets, newest first. */
export function recentlyDone(tickets: TicketView[], max = RECENT_DONE): TicketView[] {
  return tickets
    .filter((t) => t.column === "done" && !t.archived_at_ms)
    .sort((a, b) => doneAt(b) - doneAt(a))
    .slice(0, max);
}

/** The guidelines linked to this workspace's project, and how much text they add to every run. */
export function linkedGuidelines(
  guidelines: Pick<GuidelineView, "links" | "chars" | "archived">[],
  projectId: string,
): { count: number; chars: number } {
  const linked = guidelines.filter(
    (g) => !g.archived && g.links.some((l) => l.kind === "project" && l.id === projectId),
  );
  return { count: linked.length, chars: linked.reduce((n, g) => n + g.chars, 0) };
}

/** "11 240": thousands apart with a space, as the design writes them. */
export function groupDigits(n: number): string {
  return String(Math.round(n)).replace(/\B(?=(\d{3})+(?!\d))/g, " ");
}

/**
 * A readable name for a styleguide link: the last part of its path without dashes ("Aurelius Precision"
 * from ".../aurelius-precision"), else its host.
 */
export function styleguideName(url: string): string {
  try {
    const u = new URL(url);
    const last = u.pathname.split("/").filter(Boolean).at(-1);
    if (!last) return u.host;
    const words = decodeURIComponent(last)
      .replace(/\.[a-z0-9]+$/i, "")
      .split(/[-_\s]+/)
      .filter(Boolean);
    if (words.length === 0 || /^[0-9a-f-]{16,}$/i.test(last)) return u.host;
    return words.map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");
  } catch {
    return url;
  }
}

/** "now", "4 min", "1 h", "3 d": how long ago, or how long something has run. */
export function elapsed(fromMs: number, nowMs: number): string {
  const minutes = Math.max(0, Math.floor((nowMs - fromMs) / 60_000));
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h`;
  return `${Math.floor(hours / 24)} d`;
}
