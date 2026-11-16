// The rules of the New session view (ticket #60), pure: what it shows, what it picks unless told, and
// what Start sends. Every start is a `ticket.create` the dialogs already send, built by their own code.

import type { AgentMode, MethodParams, Receipt } from "@aurelius/protocol";
import { giveWorkParams, LAST_GIVEN, taskRun, ticketCreateParams, titleOfGoal } from "./dialog-logic";
import { LAST_QUICK, QUICK_NOTE, quickAssignee, quickCreateParams } from "./quick-logic";

/** `session`: a work session (server feature `work_sessions`), which replaces the quick task there. */
export type SessionKind = "ticket" | "quick" | "session";

/** The server feature of work sessions. */
export const SESSIONS_FEATURE = "work_sessions";
/** The server's dispatcher chooses who takes a session (`work_session.dispatch`). */
export const SESSION_DISPATCH_FEATURE = "session_dispatch";

/** What a session start says under the composer. */
export function sessionNote(whoName: string | null, worktree: boolean): string {
  const who = whoName ?? "The employee";
  return worktree
    ? `${who} works in a worktree of its own, on its own branch. No ticket: you talk with them in the session.`
    : `${who} works in the project's folder itself. No ticket: you talk with them in the session.`;
}

/** The dispatcher chooses who takes a session: its words after Start. */
export function dispatchedNote(worktree: boolean): string {
  return worktree
    ? "The dispatcher chooses who takes it, in a worktree of its own on its own branch. No ticket: you talk with them in the session."
    : "The dispatcher chooses who takes it, in the project's folder itself. No ticket: you talk with them in the session.";
}

/** The Who of a ticket nobody was chosen for: the dispatcher picks the owner. */
export const DISPATCHER = "dispatcher";

/** Projects listed in the centre card before "All projects…". */
export const PROJECT_ROWS = 6;

/** Tickets offered to continue. */
export const CONTINUE_MAX = 3;

/** The composer is never shorter than this, so it reads as a place to write, as a chat app's input does. */
export const COMPOSER_ROWS_MIN = 2;

/** The composer grows to this many lines, then scrolls. */
export const COMPOSER_ROWS_MAX = 10;

/** Projects remembered by last use; more than the card shows, so a project picked once keeps its place. */
const RECENT_MAX = 20;

/** Columns where a ticket is still being worked on. */
const CONTINUE_COLUMNS = new Set(["doing", "testing", "review"]);

/** What is remembered per server, like the workspace (`workspace:<host>`). */
export function sessionKey(what: "project" | "recent" | "kind" | "who", hostId: string | null): string {
  return `new_session.${what}:${hostId ?? ""}`;
}

interface Listed {
  id: string;
  status: string;
}

/** Open projects; only the workspace's when one is chosen and still there. */
export function shownProjects<T extends Listed>(projects: T[], workspaceId: string | null): T[] {
  const open = projects.filter((p) => p.status !== "archived");
  const here = open.filter((p) => p.id === workspaceId);
  return here.length > 0 ? here : open;
}

/** §4.1: the workspace's project, else the last used, else the only one, else none (the board member picks). */
export function defaultProject(shown: Listed[], workspaceId: string | null, last: string | null): string | null {
  const has = (id: string | null) => id !== null && shown.some((p) => p.id === id);
  if (has(workspaceId)) return workspaceId;
  if (has(last)) return last;
  return shown.length === 1 ? (shown[0]?.id ?? null) : null;
}

/** The card's rows: the first by last use, then in list order; the chosen one always in view. */
export function projectRows<T extends Listed>(
  shown: T[],
  recent: string[],
  chosen: string | null,
): { rows: T[]; more: boolean } {
  const rank = (id: string) => {
    const at = recent.indexOf(id);
    return at < 0 ? recent.length : at;
  };
  const ordered = shown.map((p, i) => ({ p, i })).sort((a, b) => rank(a.p.id) - rank(b.p.id) || a.i - b.i);
  const rows = ordered.slice(0, PROJECT_ROWS).map((o) => o.p);
  const picked = shown.find((p) => p.id === chosen);
  if (picked && !rows.includes(picked)) rows.splice(rows.length - 1, 1, picked);
  return { rows, more: shown.length > PROJECT_ROWS };
}

export function parseRecent(stored: string | null): string[] {
  if (!stored) return [];
  try {
    const value: unknown = JSON.parse(stored);
    return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
  } catch {
    return [];
  }
}

export function noteRecent(recent: string[], id: string): string[] {
  return [id, ...recent.filter((r) => r !== id)].slice(0, RECENT_MAX);
}

/**
 * §4.4: Ticket, or what was used last when the server has it: a Session on a server with
 * sessions (a quick task chosen before counts as one), else a Quick task.
 */
export function defaultKind(stored: string | null, quickOn: boolean, sessionsOn = false): SessionKind {
  if (sessionsOn) return stored === "session" || stored === "quick" ? "session" : "ticket";
  return quickOn && stored === "quick" ? "quick" : "ticket";
}

/** The kinds a server offers next to Ticket: Session where it has them, else Quick task, else none. */
export function offeredKind(features: readonly string[], quickOn: boolean): SessionKind | null {
  if (features.includes(SESSIONS_FEATURE)) return "session";
  return quickOn ? "quick" : null;
}

/**
 * §4.4: a ticket goes to the dispatcher, or to the one last chosen while still active; a quick task to the
 * employee last given one, else to no one (it needs an employee, so the board member chooses). A session
 * goes to the dispatcher where the server has one for sessions (`dispatch`), unless an employee was
 * chosen last and is still active.
 */
export function defaultWho(
  kind: SessionKind,
  stored: string | null,
  employees: Listed[],
  lastQuick: string | null,
  dispatch = false,
): string | null {
  if (kind === "session" && dispatch) return quickAssignee(employees, lastQuick) ?? DISPATCHER;
  if (kind !== "ticket") return quickAssignee(employees, lastQuick);
  const active = employees.some((e) => e.id === stored && e.status === "active");
  return active ? stored : DISPATCHER;
}

/** What a successful start remembers, as storage writes. The dialogs' own last choice goes along. */
export function remembered(
  hostId: string | null,
  start: { kind: SessionKind; projectId: string; who: string },
  recent: string[],
): [string, string][] {
  const writes: [string, string][] = [
    [sessionKey("project", hostId), start.projectId],
    [sessionKey("recent", hostId), JSON.stringify(noteRecent(recent, start.projectId))],
    [sessionKey("kind", hostId), start.kind],
  ];
  if (start.kind !== "ticket") return [...writes, [LAST_QUICK, start.who]];
  writes.push([sessionKey("who", hostId), start.who]);
  return start.who === DISPATCHER ? writes : [...writes, [LAST_GIVEN, start.who]];
}

/** Everything Start needs. */
export interface SessionStart {
  kind: SessionKind;
  projectId: string;
  text: string;
  /** DISPATCHER (tickets only) or an employee. */
  who: { id: string; name: string };
  model: string;
  effort: string;
  /** Tickets only: a quick task runs in Auto. */
  mode: string;
  files: string[];
  /** The board member, on a server with `ticket_responsible`; otherwise null. */
  responsibleUserId: string | null;
}

/**
 * §4.6: the dialogs' own request for each start (New ticket for the dispatcher, Give work for an employee,
 * the quick task of both), and the toast that confirms it.
 */
export function sessionRequest(
  start: SessionStart,
  confirmSecret: boolean,
): { params: MethodParams["ticket.create"]; done: string } {
  const pick = (value: string) => (value === "default" ? null : value);
  if (start.kind === "quick") {
    return {
      params: quickCreateParams(
        {
          projectId: start.projectId,
          title: "",
          description: start.text,
          employeeId: start.who.id,
          files: start.files,
          model: pick(start.model),
          effort: pick(start.effort),
        },
        confirmSecret,
      ),
      done: `Quick task started with ${start.who.name}.`,
    };
  }
  const run = taskRun(start.mode, start.model, start.effort);
  if (start.who.id === DISPATCHER) {
    return {
      params: ticketCreateParams(
        {
          projectId: start.projectId,
          title: titleOfGoal(start.text),
          description: start.text,
          priority: "normal",
          labels: [],
          deadlineMs: null,
          files: start.files,
          start: true,
          run,
          responsibleUserId: start.responsibleUserId,
          ownerEmployeeId: null,
        },
        confirmSecret,
      ),
      done: "Ticket created and started.",
    };
  }
  return {
    params: giveWorkParams(
      {
        projectId: start.projectId,
        goal: start.text,
        employeeId: start.who.id,
        run,
        files: start.files,
        responsibleUserId: start.responsibleUserId,
      },
      confirmSecret,
    ),
    done: `Created and started with ${start.who.name}.`,
  };
}

/** Why Start cannot go yet, or null. The server checks again and says why in its own words. */
export function startProblem(state: {
  projectId: string | null;
  text: string;
  kind: SessionKind;
  who: string | null;
  offline: boolean;
  /** Files (not photos) added to the composer. */
  files?: number;
  /** The server takes files with a session's first message (`chat_files`). */
  sessionFiles?: boolean;
  /** A session in a worktree of its own: that folder is made only before the first run. */
  worktree?: boolean;
  /** The server's dispatcher can choose who takes a session. */
  sessionDispatch?: boolean;
}): string | null {
  if (state.offline) return "Not connected to the server.";
  if (!state.projectId) return "Choose a project.";
  if (state.text.trim() === "") return "Describe what needs doing.";
  const dispatcherOk = state.kind === "ticket" || (state.kind === "session" && (state.sessionDispatch ?? false));
  if (state.kind !== "ticket" && (!state.who || (state.who === DISPATCHER && !dispatcherOk)))
    return "Choose who does it.";
  if (state.kind === "session" && (state.files ?? 0) > 0) {
    if (!state.sessionFiles) return "Add files in the session once it has started.";
    if (state.worktree)
      return "Add files once the session works in its own worktree, or start it in the project folder.";
  }
  return null;
}

export type FooterLine = { type: "no_connection" } | { type: "text"; text: string };

/** §4.5: the dialogs' words for what happens after Start. */
export function footerLine(state: {
  kind: SessionKind;
  /** Null for the dispatcher. */
  whoName: string | null;
  /** The server knows a responsible board member (`ticket_responsible`). */
  responsible: boolean;
  noConnection: boolean;
  /** A session in a worktree of its own. */
  worktree?: boolean;
  /** The dispatcher chooses who takes the session. */
  dispatcher?: boolean;
}): FooterLine {
  if (state.noConnection) return { type: "no_connection" };
  if (state.kind === "quick") return { type: "text", text: QUICK_NOTE };
  if (state.kind === "session") {
    const worktree = state.worktree ?? false;
    const text = state.dispatcher ? dispatchedNote(worktree) : sessionNote(state.whoName, worktree);
    return { type: "text", text };
  }
  if (state.whoName) return { type: "text", text: `${state.whoName} reports back to you when it is done.` };
  const text = state.responsible
    ? "The dispatcher picks the owner. You are responsible."
    : "The dispatcher picks the owner.";
  return { type: "text", text };
}

export function placeholderOf(kind: SessionKind): string {
  if (kind === "session") return "What do you want to work on? Starts right away, without a ticket";
  return kind === "quick" ? "What needs doing? Starts right away on its own branch" : "Describe a ticket or give work…";
}

export function composerRows(text: string): number {
  return Math.min(COMPOSER_ROWS_MAX, Math.max(COMPOSER_ROWS_MIN, text.split("\n").length));
}

/** §4.2: the board member's tickets still in the works, newest activity first. */
export function continueTickets<T extends { column: string; updated_at_ms: number }>(tickets: T[]): T[] {
  return tickets
    .filter((t) => CONTINUE_COLUMNS.has(t.column))
    .sort((a, b) => b.updated_at_ms - a.updated_at_ms)
    .slice(0, CONTINUE_MAX);
}

/** The pill on a ticket to continue. */
export function columnLabel(column: string): string {
  return column === "doing" ? "Doing" : column === "testing" ? "Testing" : column === "review" ? "Review" : column;
}

/** The composer's text per server, kept while the app runs and cleared by a successful start. */
export function createDrafts() {
  const texts = new Map<string, string>();
  const key = (hostId: string | null) => hostId ?? "";
  return {
    get: (hostId: string | null) => texts.get(key(hostId)) ?? "",
    set: (hostId: string | null, text: string) => void texts.set(key(hostId), text),
    clear: (hostId: string | null) => void texts.delete(key(hostId)),
  };
}

/** The request of a session start: the employee, the project and the first message. */
export function sessionStartParams(
  start: {
    projectId: string;
    employeeId: string;
    text: string;
    model: string;
    effort: string;
    mode: string;
    images?: string[];
    /** Only on a server with `chat_files`, and not with a worktree of its own. */
    files?: string[];
  },
  worktree: boolean,
  confirmSecret: boolean,
): MethodParams["work_session.start"] {
  const pick = (value: string) => (value === "default" ? null : value);
  const mode = pick(start.mode);
  return {
    project_id: start.projectId,
    employee_id: start.employeeId,
    text: start.text.trim(),
    ...(start.images && start.images.length > 0 ? { images: start.images } : {}),
    ...(start.files && start.files.length > 0 ? { files: start.files } : {}),
    worktree,
    run: {
      ...(mode ? { mode: mode as AgentMode } : {}),
      model: pick(start.model),
      effort: pick(start.effort),
    },
    confirm_secret: confirmSecret,
  };
}

/** The request of a session the dispatcher gives to someone: a session start without the employee. */
export function sessionDispatchParams(
  start: Omit<Parameters<typeof sessionStartParams>[0], "employeeId">,
  worktree: boolean,
  confirmSecret: boolean,
): MethodParams["work_session.dispatch"] {
  const { employee_id: _, ...params } = sessionStartParams({ ...start, employeeId: "" }, worktree, confirmSecret);
  return params;
}

/** The session a start made, from its receipt (also one the dispatcher still gives to someone). */
export function startedSession(receipt: Receipt): string | null {
  const { result } = receipt;
  if (result.kind !== "command") return null;
  const { value } = result;
  return value.type === "work_session_started" || value.type === "work_session_dispatching"
    ? value.conversation_id
    : null;
}
