import type { HistoryEntry } from "@aurelius/protocol";

/** A day as YYYY-MM-DD (the shape the date picker takes). */
export type IsoDay = `${number}${number}${number}${number}-${number}${number}-${number}${number}`;

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

/** The URL's search, whatever someone typed there. */
export function historyFilters(search: Record<string, unknown>): HistoryFilters {
  const project = text(search.project);
  const employee = text(search.employee);
  const q = text(search.q);
  const ticket = text(search.ticket);
  const period = isPeriod(search.period) ? search.period : undefined;
  const from = period === "custom" ? day(search.from) : undefined;
  const to = period === "custom" ? day(search.to) : undefined;
  return {
    ...(project ? { project } : {}),
    ...(employee ? { employee } : {}),
    ...(period && period !== "all" ? { period } : {}),
    ...(from ? { from } : {}),
    ...(to ? { to } : {}),
    ...(q ? { q } : {}),
    ...(ticket ? { ticket } : {}),
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
  Boolean((f.project && f.project !== ALL_PROJECTS) || f.employee || (f.period && f.period !== "all") || f.q);

/** Local midnight of `date`, `days` later. */
function midnight(date: Date, days = 0): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days).getTime();
}

/** A YYYY-MM-DD day as the local date it names. */
function localDay(iso: string): Date {
  const [y, m, d] = iso.split("-").map(Number) as [number, number, number];
  return new Date(y, m - 1, d);
}

/** The Done moments the period covers, as `ticket.history` takes them (`to_ms` excluded). */
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
  const groups: { key: number; label: string; entries: T[] }[] = [];
  for (const entry of entries) {
    const key = midnight(new Date(entry.done_at_ms));
    const last = groups.at(-1);
    if (last && last.key === key) last.entries.push(entry);
    else groups.push({ key, label: dayLabel(entry.done_at_ms, now, locale), entries: [entry] });
  }
  return groups;
}

/** What a row says where its summary goes, and how. */
export function summaryShown(entry: Pick<HistoryEntry, "summary" | "summary_state">): {
  text: string;
  kind: "summary" | "quiet" | "busy" | "failed";
} {
  switch (entry.summary_state) {
    case "ready":
      return entry.summary
        ? { text: entry.summary, kind: "summary" }
        : { text: "Summary not available", kind: "failed" };
    case "summarizing":
      return { text: "Summarizing…", kind: "busy" };
    case "before_history":
      return { text: "No summary (finished before History)", kind: "quiet" };
    default:
      return { text: "Summary not available", kind: "failed" };
  }
}

/** The owner as the ticket names it; none: a dash. */
export const ownerName = (entry: Pick<HistoryEntry, "owner_name">): string => entry.owner_name ?? "—";

/** The next page's keyset: the last row's Done moment and number. */
export function nextPage(page: {
  has_more?: boolean;
  entries: Pick<HistoryEntry, "done_at_ms" | "number">[];
}): { before_done_at_ms: number; before_number: number } | undefined {
  const last = page.entries.at(-1);
  return page.has_more && last ? { before_done_at_ms: last.done_at_ms, before_number: last.number } : undefined;
}
