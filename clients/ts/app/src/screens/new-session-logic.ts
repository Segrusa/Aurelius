// The rules of the New session view (ticket #60), pure: what it shows, what it picks unless told, and
// what Start sends. Every start is a `ticket.create` the dialogs already send, built by their own code.

import type { AgentMode, MethodParams, Receipt } from "@aurelius/protocol";
import { giveWorkParams, LAST_GIVEN, taskRun, ticketCreateParams, titleOfGoal } from "./dialog-logic";
import { LAST_QUICK, quickAssignee, quickCreateParams } from "./quick-logic";

/** `session`: a work session (server feature `work_sessions`), which replaces the quick task there. */
export type SessionKind = "ticket" | "quick" | "session";

/** The server feature of work sessions. */
export const SESSIONS_FEATURE = "work_sessions";
/** The server's dispatcher chooses who takes a session (`work_session.dispatch`). */
export const SESSION_DISPATCH_FEATURE = "session_dispatch";
/** The server has design sessions: a session whose employee makes HTML pages (`design`). */
export const DESIGN_SESSIONS_FEATURE = "design_sessions";
/** The server lets a session work on a branch of its own in the project's folder. */
export const OWN_BRANCH_FEATURE = "session_own_branch";
/**
 * Every session works in a worktree of its own (ticket #86): there is no Where to choose, and files go
 * with the first message (the server keeps them in the worktree's folder until it is made).
 */
export const WORKTREES_ONLY_FEATURE = "worktrees_only";

/** Files may go with a session's first message: the server takes them (`chat_files`) and has a folder for them. */
export function sessionFilesAllowed(sessionFiles: boolean, worktree: boolean, worktreesOnly: boolean): boolean {
  return sessionFiles && (worktreesOnly || !worktree);
}

/**
 * Where a session works: its own branch in the project's folder, or a worktree of its own. Never on
 * whatever the project's folder has checked out, so a session's work always stays on a branch of its own.
 */
export type SessionPlace = "branch" | "worktree";

const PLACES: { key: SessionPlace; label: string; description: string }[] = [
  {
    key: "branch",
    label: "Own branch",
    description: "A branch of its own, checked out in the project's folder",
  },
  { key: "worktree", label: "Own worktree", description: "A worktree and branch of its own" },
];

/** The Where menu's choices; Own branch only where the server has `session_own_branch`. */
export function placeOptions(ownBranch: boolean): { key: SessionPlace; label: string; description: string }[] {
  return PLACES.filter((p) => p.key !== "branch" || ownBranch);
}

/** The place a session starts in unless chosen: its own branch, or a worktree where the server lacks that. */
export function shownPlace(chosen: SessionPlace | null, ownBranch: boolean): SessionPlace {
  if (!ownBranch) return "worktree";
  return chosen ?? "branch";
}

export function placeLabel(place: SessionPlace): string {
  return PLACES.find((p) => p.key === place)?.label ?? "Own worktree";
}
/** The server makes a ticket's title from its description when the title is empty. */
export const MADE_TITLE_FEATURE = "ticket_made_title";

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
export function sessionKey(what: "project" | "recent" | "kind" | "who" | RunChoice, hostId: string | null): string {
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
  if (sessionsOn) return stored === "session" || stored === "quick" || stored === "design" ? "session" : "ticket";
  return quickOn && stored === "quick" ? "quick" : "ticket";
}

/** Whether the last start was a design, so New session chooses Design again (where the server has them). */
export function defaultDesign(stored: string | null, designsOn: boolean): boolean {
  return designsOn && stored === "design";
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

/** The run choices of the composer that the next start opens on again. */
export type RunChoice = "model" | "effort" | "mode";

/** The composer's run choices; `default` leaves it to the employee or owner. */
export type RunChoices = Record<RunChoice, string>;

/**
 * A remembered model, effort or mode while the composer still offers it, else `default`. Before the
 * offered list has loaded ([loaded] false) the remembered one is kept, so it is not lost to a slow start.
 */
export function keptChoice(stored: string | null, offered: readonly string[], loaded = true): string {
  if (!stored) return "default";
  return !loaded || offered.includes(stored) ? stored : "default";
}

/** What a successful start remembers, as storage writes. The dialogs' own last choice goes along. */
export function remembered(
  hostId: string | null,
  start: { kind: SessionKind; design?: boolean; projectId: string; who: string; run?: RunChoices },
  recent: string[],
): [string, string][] {
  const writes: [string, string][] = [
    [sessionKey("project", hostId), start.projectId],
    [sessionKey("recent", hostId), JSON.stringify(noteRecent(recent, start.projectId))],
    [sessionKey("kind", hostId), start.design ? "design" : start.kind],
  ];
  if (start.run) {
    for (const what of ["model", "effort", "mode"] as const) writes.push([sessionKey(what, hostId), start.run[what]]);
  }
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
  /** On a server with `ticket_made_title`: the title is left empty, so the server makes one. */
  madeTitle?: boolean;
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
          title: start.madeTitle ? "" : titleOfGoal(start.text),
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
  const given = giveWorkParams(
    {
      projectId: start.projectId,
      goal: start.text,
      employeeId: start.who.id,
      run,
      files: start.files,
      responsibleUserId: start.responsibleUserId,
    },
    confirmSecret,
  );
  return {
    params: start.madeTitle ? { ...given, title: "" } : given,
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
  /** The server offers Own branch (`session_own_branch`), where files can go with the first message. */
  ownBranch?: boolean;
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
      return state.ownBranch
        ? "Add files once the session works in its own worktree, or start it on its own branch."
        : "Add files once the session works in its own worktree.";
  }
  return null;
}

export function placeholderOf(kind: SessionKind, design = false): string {
  if (kind === "session" && design)
    return "What should it look like? The pages show beside the conversation as they are made";
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
    /** A design session (`design_sessions`). */
    design?: boolean;
  },
  place: SessionPlace,
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
    worktree: place === "worktree",
    ...(place === "branch" ? { own_branch: true } : {}),
    run: {
      ...(mode ? { mode: mode as AgentMode } : {}),
      model: pick(start.model),
      effort: pick(start.effort),
    },
    confirm_secret: confirmSecret,
    ...(start.design ? { design: true } : {}),
  };
}

/** The request of a session the dispatcher gives to someone: a session start without the employee. */
export function sessionDispatchParams(
  start: Omit<Parameters<typeof sessionStartParams>[0], "employeeId">,
  place: SessionPlace,
  confirmSecret: boolean,
): MethodParams["work_session.dispatch"] {
  const { employee_id: _, ...params } = sessionStartParams({ ...start, employeeId: "" }, place, confirmSecret);
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
