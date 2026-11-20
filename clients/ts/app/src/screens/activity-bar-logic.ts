// What the status bar, the board cards and the ticket panel say about activity (ticket #50).

import type { ActivityOverview, ActivityWaiting, ActivityWork, LoadSnapshot, TicketActivity } from "@aurelius/protocol";

/** How often the overview is read besides what events cause. */
export const ACTIVITY_REFRESH_MS = 30_000;

/** What stands for another board member's chat: the server sends nothing else of it. */
export const PRIVATE_CHAT = "Privégesprek";

/** How often the machine's load is read; the server averages CPU over ~10 s. */
export const LOAD_REFRESH_MS = 10_000;

/** Work held back only by a free slot, of its employee or of this machine. */
export const waitsForSlot = (w: ActivityWaiting): boolean => w.waits_for === "slot" || w.waits_for === "machine";

/** "2 busy" and "1 waiting for a slot", what the bar shows. */
export function activityCounts(o: ActivityOverview): { busy: number; waitingForSlot: number } {
  return { busy: o.busy.length, waitingForSlot: o.waiting.filter(waitsForSlot).length };
}

/** What a row is about. */
export const workLabel = (work: ActivityWork): string => (work.private ? PRIVATE_CHAT : (work.label ?? ""));

/**
 * Where a row opens: its ticket (as a session), else its task, channel or chat; a chat that is
 * a work session opens as that session. Null for another board member's chat or no place.
 */
export function workHref(work: ActivityWork, sessionIds: ReadonlySet<string>): string | null {
  if (work.private) return null;
  if (work.ticket_id) return `/board?ticket=${work.ticket_id}`;
  if (work.task_id) return `/tasks/${work.task_id}`;
  if (work.channel_id) return `/chat?channel=${work.channel_id}`;
  if (work.chat_id) return sessionIds.has(work.chat_id) ? `/session/${work.chat_id}` : `/talk/${work.chat_id}`;
  return null;
}

/** Why something waits, in words. `who`: the employee it waits with. */
export function waitLabel(waitsFor: string | null | undefined, who: string | null | undefined): string {
  switch (waitsFor) {
    case "slot":
      return who ? `Waiting for a free slot: ${who} is busy` : "Waiting for a free slot";
    case "machine":
      return "Waiting for a free slot";
    case "paused":
      return who ? `Paused: ${who}` : "Paused";
    case "question":
    case "board_member":
      return "Waiting for a board member";
    case "approval":
      return "Waiting for an approval";
    case "colleague":
      return "Waiting for a colleague";
    case "limit":
      return "Waiting for the usage limit";
    case "login":
      return "Waiting for a Claude login";
    case "later":
      return "Scheduled for later";
    case "handoff":
      return "Waiting for a handoff";
    default:
      return "Waiting";
  }
}

/** One line on a card or in the panel. */
export function ticketActivityLine(a: TicketActivity): string {
  if (a.state === "running") return a.employee_name ? `Running: ${a.employee_name}` : "Running";
  if (a.state === "paused") return waitLabel("paused", a.employee_name);
  return waitLabel(a.waits_for, a.employee_name);
}

/** The server's name and version at the left of the machine figures. */
export function serverLine(name: string | null | undefined, version: string | null | undefined): string {
  const shown = name || "Aurelius";
  return version ? `${shown} · v${version}` : shown;
}

/** CPU and memory in use, what the bar shows next to their icons. */
export interface MachineFigures {
  cpu: string;
  memory: string;
  cpuHint: string;
  memoryHint: string;
}

const gb = (mb: number): string => (mb / 1024).toFixed(1);

/** Null where the server does not measure CPU and memory (another data folder). */
export function machineFigures(load: LoadSnapshot): MachineFigures | null {
  if (!load.checks_active || load.mem_total_mb <= 0) return null;
  const cpu = `${Math.round(load.cpu_pct)}%`;
  const usedMb = Math.max(0, load.mem_total_mb - load.mem_free_mb);
  const memory = `${Math.round((usedMb / load.mem_total_mb) * 100)}%`;
  return {
    cpu,
    memory,
    cpuHint: `CPU ${cpu} (all cores, last ~10 s)`,
    memoryHint: `Memory ${memory} in use: ${gb(usedMb)} of ${gb(load.mem_total_mb)} GB`,
  };
}
