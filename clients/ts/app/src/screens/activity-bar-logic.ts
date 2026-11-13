// What the status bar, the board cards and the ticket panel say about activity (ticket #50).

import type { ActivityOverview, ActivityWaiting, ActivityWork, TicketActivity } from "@aurelius/protocol";

/** How often the overview is read besides what events cause. */
export const ACTIVITY_REFRESH_MS = 30_000;

/** What stands for another board member's chat: the server sends nothing else of it. */
export const PRIVATE_CHAT = "Privégesprek";

/** Work held back only by a free slot, of its employee or of this machine. */
export const waitsForSlot = (w: ActivityWaiting): boolean => w.waits_for === "slot" || w.waits_for === "machine";

/** "2 busy" and "1 waiting for a slot", what the bar shows. */
export function activityCounts(o: ActivityOverview): { busy: number; waitingForSlot: number } {
  return { busy: o.busy.length, waitingForSlot: o.waiting.filter(waitsForSlot).length };
}

/** What a row is about. */
export const workLabel = (work: ActivityWork): string => (work.private ? PRIVATE_CHAT : (work.label ?? ""));

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
