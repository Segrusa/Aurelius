// The rules of the New session view (ticket #60), pure: what it shows, what it picks unless told, and
// what Start sends. Every start is a `ticket.create` the dialogs already send, built by their own code.

import type { MethodParams } from "@aurelius/protocol";
import { giveWorkParams, LAST_GIVEN, taskRun, ticketCreateParams, titleOfGoal } from "./dialog-logic";
import { LAST_QUICK, QUICK_NOTE, quickAssignee, quickCreateParams } from "./quick-logic";

export type SessionKind = "ticket" | "quick";

/** The Who of a ticket nobody was chosen for: the dispatcher picks the owner. */
export const DISPATCHER = "dispatcher";

/** Projects listed in the centre card before "All projects…". */
export const PROJECT_ROWS = 6;

/** Tickets offered to continue. */
export const CONTINUE_MAX = 3;

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

/** §4.4: Ticket, or Quick task when that was used last and the server has it. */
export function defaultKind(stored: string | null, quickOn: boolean): SessionKind {
  return quickOn && stored === "quick" ? "quick" : "ticket";
}

/**
 * §4.4: a ticket goes to the dispatcher, or to the one last chosen while still active; a quick task to the
 * employee last given one, else to no one (it needs an employee, so the board member chooses).
 */
export function defaultWho(
  kind: SessionKind,
  stored: string | null,
  employees: Listed[],
  lastQuick: string | null,
): string | null {
  if (kind === "quick") return quickAssignee(employees, lastQuick);
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
  if (start.kind === "quick") return [...writes, [LAST_QUICK, start.who]];
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
}): string | null {
  if (state.offline) return "Not connected to the server.";
  if (!state.projectId) return "Choose a project.";
  if (state.text.trim() === "") return "Describe what needs doing.";
  if (state.kind === "quick" && (!state.who || state.who === DISPATCHER)) return "Choose who does it.";
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
}): FooterLine {
  if (state.noConnection) return { type: "no_connection" };
  if (state.kind === "quick") return { type: "text", text: QUICK_NOTE };
  if (state.whoName) return { type: "text", text: `${state.whoName} reports back to you when it is done.` };
  const text = state.responsible
    ? "The dispatcher picks the owner. You are responsible."
    : "The dispatcher picks the owner.";
  return { type: "text", text };
}

export function placeholderOf(kind: SessionKind): string {
  return kind === "quick" ? "What needs doing? Starts right away on its own branch" : "Describe a ticket or give work…";
}

export function composerRows(text: string): number {
  return Math.min(COMPOSER_ROWS_MAX, Math.max(1, text.split("\n").length));
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
