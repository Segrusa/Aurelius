// What the waiting line under a thread says.

import type { ScheduledMessageView, ThreadWaiter } from "@aurelius/protocol";

/** Who waits to pick up a thread, and why, on one line ("Flux is waiting to pick up this thread (behind 2 items)"). */
export function waiterLine(w: ThreadWaiter): string {
  if (w.reason === "question") return `${w.employee_name} is waiting for an answer on this thread`;
  if (w.reason === "limit") return `${w.employee_name} is waiting for a usage limit to pick up this thread`;
  const behind = w.behind > 0 ? ` (behind ${w.behind} ${w.behind === 1 ? "item" : "items"})` : "";
  return `${w.employee_name} is waiting to pick up this thread${behind}`;
}

/**
 * The waiters worth a line. `behind > 0` without a reason means the employee's own run in this thread
 * goes first: it is at work here and the next message only waits its turn, which the thread shows itself
 * (the run's live answer and the message marked Queued). Paused, a limit or an open question stay.
 */
export function shownWaiters(waiters: readonly ThreadWaiter[]): ThreadWaiter[] {
  return waiters.filter((w) => w.reason || w.behind === 0);
}

/** What is scheduled in a thread: "Ann has a reminder for 14:30" or "Ann → Bo at 14:30". */
export function scheduledLine(m: ScheduledMessageView, at: string): string {
  return m.from_employee === m.to_employee
    ? `${m.from_name} has a reminder for ${at}`
    : `${m.from_name} → ${m.to_name} at ${at}`;
}
