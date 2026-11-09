// What the Scheduled page and the waiting line under a thread say.

import type { EmployeeSummary, HeartbeatView, ScheduledMessageView, ThreadWaiter } from "@aurelius/protocol";

/** Who waits to pick up a thread, and why, on one line ("Flux — waiting to pick up this thread (behind 2 items)"). */
export function waiterLine(w: ThreadWaiter): string {
  if (w.reason === "question") return `${w.employee_name} — waiting for an answer on this thread`;
  if (w.reason === "limit") return `${w.employee_name} — waiting for a usage limit to pick up this thread`;
  const behind = w.behind > 0 ? ` (behind ${w.behind} ${w.behind === 1 ? "item" : "items"})` : "";
  return `${w.employee_name} — waiting to pick up this thread${behind}`;
}

/** What is scheduled in a thread: "Ann has a reminder for 14:30" or "Ann → Bo at 14:30". */
export function scheduledLine(m: ScheduledMessageView, at: string): string {
  return m.from_employee === m.to_employee
    ? `${m.from_name} has a reminder for ${at}`
    : `${m.from_name} → ${m.to_name} at ${at}`;
}

/** What waits, soonest first, apart from what already came of it. */
export function splitScheduled(all: readonly ScheduledMessageView[]): {
  waiting: ScheduledMessageView[];
  past: ScheduledMessageView[];
} {
  const waiting: ScheduledMessageView[] = [];
  const past: ScheduledMessageView[] = [];
  for (const m of all) (m.state === "scheduled" ? waiting : past).push(m);
  return { waiting, past };
}

/** The outcome column: what happened, with the reason when it did not go out. A newer state shows as it is. */
export function outcomeText(m: ScheduledMessageView): string {
  switch (m.state) {
    case "delivered":
      return "Delivered";
    case "cancelled":
      return "Taken back";
    case "not_delivered":
      return m.outcome ? `Not delivered: ${m.outcome}` : "Not delivered";
    default:
      return m.state;
  }
}

function clock(minute: number): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(Math.floor(minute / 60))}:${pad(minute % 60)}`;
}

/** How often and when a heartbeat is quiet: "Every 30 min · quiet 22:00–07:00". */
export function heartbeatRule(h: HeartbeatView): string {
  const every =
    h.every_minutes >= 60 && h.every_minutes % 60 === 0
      ? `Every ${h.every_minutes / 60} h`
      : `Every ${h.every_minutes} min`;
  return h.quiet_start_min === h.quiet_end_min
    ? every
    : `${every} · quiet ${clock(h.quiet_start_min)}–${clock(h.quiet_end_min)}`;
}

/** The employees with a heartbeat on, as the Scheduled page lists them. */
export function withHeartbeat(all: readonly EmployeeSummary[]): (EmployeeSummary & { heartbeat: HeartbeatView })[] {
  return all.filter((e): e is EmployeeSummary & { heartbeat: HeartbeatView } => Boolean(e.heartbeat));
}
