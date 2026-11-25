// Work sessions: the words the session's head and the sidebar use.

import type { WorkSessionView } from "@aurelius/protocol";

/** Sessions the sidebar lists. */
export const SIDEBAR_SESSIONS = 8;

/** What stands where the composer is while nobody works in the session yet; none once somebody does. */
export function chooserText(session: Pick<WorkSessionView, "employee_name" | "dispatch">): string | null {
  if (session.dispatch === "choosing")
    return `${session.employee_name} is choosing who takes this session. You can also choose yourself.`;
  if (session.dispatch === "nobody") return "The dispatcher chose nobody. Choose who takes this session.";
  return null;
}

/**
 * What a session's sidebar row signals: it works now, it waits for your choice of who takes it, or it
 * finished an answer you have not opened yet (`done`, until you open the session).
 */
export type SessionSignal = "working" | "needs_you" | "done";

type SignalFields = Pick<WorkSessionView, "running"> & Partial<Pick<WorkSessionView, "dispatch" | "unseen">>;

/**
 * A session works while a run of it is queued or under way, or while the dispatcher chooses who takes it.
 * Done only says something when neither of the others does.
 */
export function sessionSignal(session: SignalFields): SessionSignal | null {
  if (session.running || session.dispatch === "choosing") return "working";
  if (session.dispatch === "nobody") return "needs_you";
  if (session.unseen) return "done";
  return null;
}

/** How many of these sessions work now. */
export function activeSessions(sessions: SignalFields[]): number {
  return sessions.filter((s) => sessionSignal(s) === "working").length;
}

/** Midnight of the day `now` falls in, local time: where the sidebar's "today" starts. */
export function startOfDay(now: number): number {
  const d = new Date(now);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/** `items` most recently used first; equal ones keep their order. */
export function byLastUse<T>(items: T[], usedAt: (item: T) => number): T[] {
  return [...items].sort((a, b) => usedAt(b) - usedAt(a));
}

/**
 * The sidebar's sessions: the workspace's project only when one is chosen, most recently used first
 * (`usedAt`, by default the last activity), the latest `SIDEBAR_SESSIONS` plus any older one that still has
 * a signal, so an active session is never out of sight. With `since`, only the active ones and those used
 * from then on: the default view, today's work.
 */
export function sidebarSessions<
  T extends Pick<WorkSessionView, "project_id"> & Partial<SignalFields> & { last_activity_ms?: number },
>(
  sessions: T[],
  workspaceId: string | null,
  since: number | null = null,
  usedAt: (s: T) => number = (s) => s.last_activity_ms ?? 0,
): T[] {
  const here = byLastUse(workspaceId ? sessions.filter((s) => s.project_id === workspaceId) : sessions, usedAt);
  const active = (s: T) => sessionSignal({ running: s.running ?? false, dispatch: s.dispatch }) !== null;
  if (since !== null) return here.filter((s) => active(s) || usedAt(s) >= since);
  return here.filter((s, i) => i < SIDEBAR_SESSIONS || active(s));
}

/**
 * A sidebar list by default (`today`) or with "Show earlier" (`today` plus the latest from before, in the
 * order of `ordered`), and whether there is anything earlier to show at all.
 */
export function withEarlier<T>(
  ordered: T[],
  today: T[],
  latest: T[],
  id: (item: T) => string,
  earlier: boolean,
): { list: T[]; hasEarlier: boolean } {
  const inToday = new Set(today.map(id));
  const before = new Set(latest.map(id).filter((i) => !inToday.has(i)));
  if (!earlier) return { list: today, hasEarlier: before.size > 0 };
  return { list: ordered.filter((item) => inToday.has(id(item)) || before.has(id(item))), hasEarlier: before.size > 0 };
}

/**
 * What the Model and Effort chips of a session say: the chosen one, else `fallback` ("Your default ·
 * Opus 5.5"; on an old server the employee's own).
 */
export function runChipLabel(
  value: string | null | undefined,
  labelOf: (v: string) => string = (v) => v,
  fallback = "Employee's default",
): string {
  return value ? labelOf(value) : fallback;
}

/**
 * `conversation.set_run` after a pick: the server keeps model and effort together, so the one not
 * picked is sent as it is. `"default"` clears the pick, and the employee's own setting applies again.
 */
export function runChange(
  conversationId: string,
  current: { model?: string | null; effort?: string | null },
  pick: { model: string } | { effort: string },
): { conversation_id: string; model: string | null; effort: string | null } {
  const of = (v: string) => (v === "default" ? null : v);
  return {
    conversation_id: conversationId,
    model: "model" in pick ? of(pick.model) : (current.model ?? null),
    effort: "effort" in pick ? of(pick.effort) : (current.effort ?? null),
  };
}

/**
 * The Status row of a session's side panel: a run queued or under way works; otherwise it is your turn.
 * While the dispatcher chooses, or after it chose nobody, it says so.
 */
export function sessionStatus(running: boolean, dispatch?: WorkSessionView["dispatch"]): string {
  if (dispatch === "choosing") return "Dispatcher is choosing";
  if (dispatch === "nobody") return "Waiting for your choice";
  return running ? "Working" : "Waiting for you";
}

/** The Folder row: the session's worktree when it has one, else the project's own folder. */
export function sessionFolder(session: Pick<WorkSessionView, "worktree">): string {
  return session.worktree || "Project folder";
}
