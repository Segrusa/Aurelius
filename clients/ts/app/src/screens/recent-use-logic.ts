// When you last opened a session or a ticket on this device, so the sidebar can list them most recently
// used first: the server only knows their activity, not that you looked at one.

/** Where the open times are kept in the platform's storage. */
export const RECENT_USE_KEY = "sidebar.used";

/** How many open times are kept; the oldest go first. */
export const RECENT_USE_MAX = 100;

/** Open time in ms per use key (`session:<conversation id>`, `ticket:<number>`). */
export type RecentUses = Record<string, number>;

export const sessionUseKey = (conversationId: string) => `session:${conversationId}`;
export const ticketUseKey = (number: number) => `ticket:${number}`;

/** The use key of the screen at `pathname`: a session or a ticket, else none. */
export function useKeyOf(pathname: string): string | null {
  const session = /^\/session\/([^/]+)/.exec(pathname)?.[1];
  if (session) return sessionUseKey(decodeURIComponent(session));
  const ticket = /^\/ticket\/(\d+)/.exec(pathname)?.[1];
  return ticket ? ticketUseKey(Number(ticket)) : null;
}

/** The stored open times; anything unreadable counts as none, so a bad value never breaks the sidebar. */
export function parseUses(raw: string | null): RecentUses {
  if (!raw) return {};
  try {
    const value: unknown = JSON.parse(raw);
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    return Object.fromEntries(
      Object.entries(value).filter((e): e is [string, number] => typeof e[1] === "number" && Number.isFinite(e[1])),
    );
  } catch {
    return {};
  }
}

/** `uses` with `key` opened at `now`, keeping only the latest `max`. */
export function touchUse(uses: RecentUses, key: string, now: number, max = RECENT_USE_MAX): RecentUses {
  const entries = Object.entries({ ...uses, [key]: now }).sort((a, b) => b[1] - a[1]);
  return Object.fromEntries(entries.slice(0, max));
}

/** When an item was last used: its own activity, or when you last opened it here, whichever is later. */
export function lastUsed(uses: RecentUses, key: string, activityMs: number): number {
  return Math.max(activityMs, uses[key] ?? 0);
}
