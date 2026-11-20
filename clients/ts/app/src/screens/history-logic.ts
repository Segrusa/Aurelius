import type { HistoryEntry, SessionHistoryEntry } from "@aurelius/protocol";

/** A day as YYYY-MM-DD (the shape the date picker takes). */
export type IsoDay = `${number}${number}${number}${number}-${number}${number}-${number}${number}`;

/** What the History lists: tickets on Done, work sessions, or both (none in the URL). */
export type HistoryType = "tickets" | "sessions";

/** The period a History shows: a preset, or the days chosen (`from`..`to`, both included). */
export type HistoryPeriod = "7d" | "30d" | "all" | "custom";

/** What the History narrows on; each is in its URL, so a link keeps it (H-03 AC2). */
export interface HistoryFilters {
  project?: string;
  employee?: string;
  period?: HistoryPeriod;
  /** First day of a custom period. */
  from?: IsoDay;
  /** Last day of a custom period (included). */
  to?: IsoDay;
  /** Text in the title or the summary. */
  q?: string;
  /** Only tickets or only sessions; none: both. */
  type?: HistoryType;
  /** The ticket open next to the list. */
  ticket?: string;
}

export const HISTORY_PERIODS: { value: HistoryPeriod; label: string }[] = [
  { value: "7d", label: "Last 7 days" },
  { value: "30d", label: "Last 30 days" },
  { value: "all", label: "All time" },
  { value: "custom", label: "Custom" },
];

const DAY = /^\d{4}-\d{2}-\d{2}$/;

const text = (value: unknown): string | undefined =>
  typeof value === "string" && value.trim() !== "" ? value : undefined;
const isDay = (value: unknown): value is IsoDay =>
  typeof value === "string" && DAY.test(value) && !Number.isNaN(Date.parse(value));
const day = (value: unknown): IsoDay | undefined => (isDay(value) ? value : undefined);
const isPeriod = (value: unknown): value is HistoryPeriod => HISTORY_PERIODS.some((p) => p.value === value);
const isType = (value: unknown): value is HistoryType => value === "tickets" || value === "sessions";

/** The Type filter's choices; "all" is none in the URL. */
export const HISTORY_TYPES: { value: HistoryType | "all"; label: string }[] = [
  { value: "all", label: "All" },
  { value: "tickets", label: "Tickets" },
  { value: "sessions", label: "Sessions" },
];

/** The URL's search, whatever someone typed there. */
export function historyFilters(search: Record<string, unknown>): HistoryFilters {
  const project = text(search.project);
  const employee = text(search.employee);
  const q = text(search.q);
  const ticket = text(search.ticket);
  const period = isPeriod(search.period) ? search.period : undefined;
  const from = period === "custom" ? day(search.from) : undefined;
  const to = period === "custom" ? day(search.to) : undefined;
  const type = isType(search.type) ? search.type : undefined;
  return {
    ...(project ? { project } : {}),
    ...(employee ? { employee } : {}),
    ...(period && period !== "all" ? { period } : {}),
    ...(from ? { from } : {}),
    ...(to ? { to } : {}),
    ...(q ? { q } : {}),
    ...(ticket ? { ticket } : {}),
    ...(type ? { type } : {}),
  };
}

/** `project` in the URL for every project: it wins over the workspace's project. */
export const ALL_PROJECTS = "all";

/** The project the History asks for: the URL's, else the workspace's; none for all projects. */
export function historyProject(f: HistoryFilters, workspaceId: string | null): string | null {
  if (f.project === ALL_PROJECTS) return null;
  return f.project ?? workspaceId;
}

/**
 * What choosing `value` in the Project filter writes in the URL. All projects is
 * written only inside a workspace, where leaving it out would mean the workspace's project.
 */
export function projectInUrl(value: string, workspaceId: string | null): string | undefined {
  if (value !== ALL_PROJECTS) return value;
  return workspaceId ? ALL_PROJECTS : undefined;
}

/** Whether anything narrows the list (the open ticket does not). */
export const narrowed = (f: HistoryFilters): boolean =>
  Boolean((f.project && f.project !== ALL_PROJECTS) || f.employee || (f.period && f.period !== "all") || f.q || f.type);

/** Local midnight of `date`, `days` later. */
function midnight(date: Date, days = 0): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days).getTime();
}

/** A YYYY-MM-DD day as the local date it names. */
function localDay(iso: string): Date {
  const [y, m, d] = iso.split("-").map(Number) as [number, number, number];
  return new Date(y, m - 1, d);
}

/** The moments the period covers, as `ticket.history` and `work_session.history` take them (`to_ms` excluded). */
export function periodRange(f: HistoryFilters, now: Date): { from_ms?: number; to_ms?: number } {
  switch (f.period) {
    case "7d":
      return { from_ms: midnight(now, -6) };
    case "30d":
      return { from_ms: midnight(now, -29) };
    case "custom":
      return {
        ...(f.from ? { from_ms: midnight(localDay(f.from)) } : {}),
        ...(f.to ? { to_ms: midnight(localDay(f.to), 1) } : {}),
      };
    default:
      return {};
  }
}

/** "Today", "Yesterday", else the date. */
export function dayLabel(ms: number, now: Date, locale?: string): string {
  const start = midnight(new Date(ms));
  if (start === midnight(now)) return "Today";
  if (start === midnight(now, -1)) return "Yesterday";
  const date = new Date(ms);
  const sameYear = date.getFullYear() === now.getFullYear();
  return date.toLocaleDateString(locale, {
    weekday: "short",
    day: "numeric",
    month: "short",
    ...(sameYear ? {} : { year: "numeric" }),
  });
}

/** Rows grouped per local day, in the order they came (newest first). */
export function groupByDay<T extends Pick<HistoryEntry, "done_at_ms">>(
  entries: readonly T[],
  now: Date,
  locale?: string,
): { key: number; label: string; entries: T[] }[] {
  return groupAt(entries, (e) => e.done_at_ms, now, locale);
}

/** `groupByDay` for any row, by the moment `at` gives it. */
export function groupAt<T>(
  entries: readonly T[],
  at: (entry: T) => number,
  now: Date,
  locale?: string,
): { key: number; label: string; entries: T[] }[] {
  const groups: { key: number; label: string; entries: T[] }[] = [];
  for (const entry of entries) {
    const ms = at(entry);
    const key = midnight(new Date(ms));
    const last = groups.at(-1);
    if (last && last.key === key) last.entries.push(entry);
    else groups.push({ key, label: dayLabel(ms, now, locale), entries: [entry] });
  }
  return groups;
}

/** One row of the History: a ticket on its Done, or a session on its last activity. */
export type HistoryRow =
  | { kind: "ticket"; key: string; at_ms: number; ticket: HistoryEntry }
  | { kind: "session"; key: string; at_ms: number; session: SessionHistoryEntry };

/** The pages one list has loaded (none: it is not asked for). */
interface Loaded<T> {
  pages: readonly { entries: readonly T[]; has_more?: boolean }[];
}

/** The oldest moment a list has loaded while it has more; none when it has all. */
function floorOf<T>(loaded: Loaded<T> | undefined, at: (entry: T) => number): number | null {
  const last = loaded?.pages.at(-1);
  const entry = last?.entries.at(-1);
  return last?.has_more && entry ? at(entry) : null;
}

/**
 * Tickets and sessions as one list, newest first. Each list comes in pages of its own, so the rows
 * stop at the newest moment down to which a list that has more is loaded: a later page never puts a
 * row above rows already shown. `more`: the lists whose next page moves that line (Load more).
 */
export function mergeHistory(
  tickets: Loaded<HistoryEntry> | undefined,
  sessions: Loaded<SessionHistoryEntry> | undefined,
): { rows: HistoryRow[]; more: HistoryType[] } {
  const rows: HistoryRow[] = [
    ...(tickets?.pages ?? []).flatMap((p) =>
      p.entries.map((t): HistoryRow => ({ kind: "ticket", key: `t:${t.ticket_id}`, at_ms: t.done_at_ms, ticket: t })),
    ),
    ...(sessions?.pages ?? []).flatMap((p) =>
      p.entries.map(
        (s): HistoryRow => ({ kind: "session", key: `s:${s.conversation_id}`, at_ms: s.last_activity_ms, session: s }),
      ),
    ),
  ];
  rows.sort((a, b) => b.at_ms - a.at_ms || (a.key < b.key ? 1 : a.key > b.key ? -1 : 0));
  const floors: [HistoryType, number | null][] = [
    ["tickets", floorOf(tickets, (t) => t.done_at_ms)],
    ["sessions", floorOf(sessions, (s) => s.last_activity_ms)],
  ];
  const cut = Math.max(...floors.map(([, f]) => f ?? Number.NEGATIVE_INFINITY));
  const more = floors.filter(([, f]) => f !== null && f === cut).map(([type]) => type);
  return { rows: rows.filter((r) => r.at_ms >= cut), more };
}

/** What a row says where its summary goes, and how. */
export function summaryShown(
  entry: Pick<HistoryEntry, "summary" | "summary_state">,
  of: "ticket" | "session" = "ticket",
): {
  text: string;
  kind: "summary" | "quiet" | "busy" | "failed" | "missing";
} {
  switch (entry.summary_state) {
    case "ready":
      return entry.summary
        ? { text: entry.summary, kind: "summary" }
        : { text: "Summary not available", kind: "failed" };
    case "summarizing":
      return { text: "Summarizing…", kind: "busy" };
    case "before_history":
      // A session has no Done: no summary was asked for yet, and one can be.
      return of === "session"
        ? { text: "No summary yet", kind: "missing" }
        : { text: "No summary (finished before History)", kind: "quiet" };
    default:
      return { text: "Summary not available", kind: "failed" };
  }
}

/** The owner as the ticket names it; none: a dash. */
export const ownerName = (entry: Pick<HistoryEntry, "owner_name">): string => entry.owner_name ?? "—";

/** Who started a session, as the History names them; gone: a dash. */
export const starterName = (entry: Pick<SessionHistoryEntry, "started_by_name">): string =>
  entry.started_by_name ?? "—";

/** The next page of `work_session.history`: the last row's last activity and id. */
export function nextSessionPage(page: {
  has_more?: boolean;
  entries: Pick<SessionHistoryEntry, "last_activity_ms" | "conversation_id">[];
}): { before_last_activity_ms: number; before_conversation_id: string } | undefined {
  const last = page.entries.at(-1);
  return page.has_more && last
    ? { before_last_activity_ms: last.last_activity_ms, before_conversation_id: last.conversation_id }
    : undefined;
}

/** What the History says when nothing is in it, for what it lists. */
export function emptyText(type: HistoryType | undefined): string {
  if (type === "tickets") return "A ticket shows up here once it is on Done.";
  if (type === "sessions") return "A session shows up here on the day it was last active.";
  return "A ticket shows up here once it is on Done, a session on the day it was last active.";
}

/** The next page's keyset: the last row's Done moment and number. */
export function nextPage(page: {
  has_more?: boolean;
  entries: Pick<HistoryEntry, "done_at_ms" | "number">[];
}): { before_done_at_ms: number; before_number: number } | undefined {
  const last = page.entries.at(-1);
  return page.has_more && last ? { before_done_at_ms: last.done_at_ms, before_number: last.number } : undefined;
}
