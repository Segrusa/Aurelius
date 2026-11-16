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

/** What the Model and Effort chips of a session say: the chosen one, or the employee's own. */
export function runChipLabel(value: string | null | undefined, labelOf: (v: string) => string = (v) => v): string {
  return value ? labelOf(value) : "Employee's default";
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

/** The Status row of a session's side panel: a run queued or under way works; otherwise it is your turn. */
export function sessionStatus(running: boolean): string {
  return running ? "Working" : "Waiting for you";
}

/** The Folder row: the session's worktree when it has one, else the project's own folder. */
export function sessionFolder(session: Pick<WorkSessionView, "worktree">): string {
  return session.worktree || "Project folder";
}
