// Work sessions: the words the session's head and the sidebar use.

import type { WorkSessionView } from "@aurelius/protocol";

/** Sessions the sidebar lists. */
export const SIDEBAR_SESSIONS = 8;

/** The head's second line: who works where. */
export function sessionPlace(session: Pick<WorkSessionView, "employee_name" | "project_name" | "branch">): string {
  const where = session.branch ? `own worktree on ${session.branch}` : "project folder";
  return `${session.employee_name} · ${session.project_name} · ${where}`;
}

/** The sidebar's sessions: the workspace's project only when one is chosen, at most `SIDEBAR_SESSIONS`. */
export function sidebarSessions<T extends Pick<WorkSessionView, "project_id">>(
  sessions: T[],
  workspaceId: string | null,
): T[] {
  const here = workspaceId ? sessions.filter((s) => s.project_id === workspaceId) : sessions;
  return here.slice(0, SIDEBAR_SESSIONS);
}
