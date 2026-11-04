// A recurrence as the schedule dialog types it, and as the wire holds it.

import type { Recurrence } from "@aurelius/protocol";

export const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
export const EVERY_HOURS_MAX = 168;

export interface RecurrenceForm {
  kind: Recurrence["kind"];
  hours: string;
  time: string;
  weekdays: number[];
}

const clock = (minute: number) =>
  `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;

export function parseClock(text: string): number | null {
  const match = /^(\d{1,2})[:.](\d{2})$/.exec(text.trim());
  if (!match) return null;
  const hour = Number(match[1]);
  const minute = Number(match[2]);
  return hour < 24 && minute < 60 ? hour * 60 + minute : null;
}

export function recurrenceForm(r: Recurrence | null): RecurrenceForm {
  const base = { hours: "24", time: "09:00", weekdays: [0] };
  if (!r) return { kind: "daily", ...base };
  if (r.kind === "every_hours") return { ...base, kind: r.kind, hours: String(r.hours) };
  if (r.kind === "daily") return { ...base, kind: r.kind, time: clock(r.at_minute) };
  return { ...base, kind: r.kind, time: clock(r.at_minute), weekdays: r.weekdays };
}

/** The recurrence, or what is wrong with the form. */
export function recurrenceOf(form: RecurrenceForm): Recurrence | string {
  if (form.kind === "every_hours") {
    const hours = Number(form.hours);
    if (!Number.isInteger(hours)) return "Enter the number of hours.";
    if (hours < 1 || hours > EVERY_HOURS_MAX) return `Hours must be between 1 and ${EVERY_HOURS_MAX}.`;
    return { kind: "every_hours", hours };
  }
  const at = parseClock(form.time);
  if (at === null) return "Enter the time as HH:MM.";
  if (form.kind === "daily") return { kind: "daily", at_minute: at };
  if (form.weekdays.length === 0) return "Pick at least one weekday.";
  return { kind: "weekly", at_minute: at, weekdays: [...form.weekdays].sort((a, b) => a - b) };
}

export function recurrenceText(r: Recurrence): string {
  if (r.kind === "every_hours") return r.hours === 1 ? "Every hour" : `Every ${r.hours} hours`;
  if (r.kind === "daily") return `Daily at ${clock(r.at_minute)}`;
  return `${r.weekdays.map((d) => WEEKDAYS[d]).join(", ")} at ${clock(r.at_minute)}`;
}

const ONE_PAGE = /^(.+)\.[0-9a-f]{8}-[0-9a-f-]{27}$/;

/** A screen's counts, most opened first: every employee's or project's own page counts as one. */
export function screenUseRows<U extends { opens: number; last_ms: number }>(use: Record<string, U>): [string, U][] {
  const rows = new Map<string, U>();
  for (const [key, u] of Object.entries(use)) {
    const place = ONE_PAGE.exec(key);
    const name = place ? `${place[1]} (their own pages)` : key;
    const so_far = rows.get(name);
    rows.set(
      name,
      so_far ? { ...so_far, opens: so_far.opens + u.opens, last_ms: Math.max(so_far.last_ms, u.last_ms) } : u,
    );
  }
  return [...rows.entries()].sort((a, b) => b[1].opens - a[1].opens);
}
