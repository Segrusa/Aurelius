// A recurrence as the schedule dialog types it, and as the wire holds it.

import type {
  CompactionSetting,
  EmployeeRunLine,
  EmployeeRunsView,
  Recurrence,
  RunValueSource,
  Setting2,
} from "@aurelius/protocol";
import { effortName } from "./employee-logic";

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

// ── Model and effort per employee ───────────────────────────────────────

type RunsView = Pick<EmployeeRunsView, "company" | "company_model_default_effort">;

const levelOf = (s: Setting2 | undefined): string | null => (s?.mode === "value" ? s.value : null);

/** What "Default" comes to for an employee: the company's level when its model has it, else the model's. */
export function defaultEffortLabel(
  view: Pick<RunsView, "company">,
  supported: readonly string[],
  modelDefault: string | null | undefined,
): string {
  const company = levelOf(view.company.effort);
  if (company && supported.includes(company)) return `${effortName(company)} (company default)`;
  return modelDefault ? `${effortName(modelDefault)} (model's default)` : "Model's default";
}

/**
 * The choices for an effort: Default, then what the model has. A stored level
 * the model lacks stays listed, marked, so the selector shows what is stored.
 */
export function effortOptions(
  supported: readonly string[],
  defaultLabel: string,
  stored: string | null,
): { value: string; label: string }[] {
  const levels = supported.map((value) => ({ value, label: effortName(value) }));
  const lost =
    stored && !supported.includes(stored) ? [{ value: stored, label: `${effortName(stored)} (not supported)` }] : [];
  return [{ value: "inherit", label: `Default · ${defaultLabel}` }, ...levels, ...lost];
}

const MODEL_SOURCE: Record<RunValueSource, string> = {
  employee: "chosen for this employee",
  company: "company default",
  connection: "connection's default",
  runtime: "Claude Code's default",
};

/** Under an employee's name: the model and where it comes from, then what a chat or the model changes. */
export function employeeRunHint(run: EmployeeRunLine, name: (model: string) => string): string {
  const model = run.model ? name(run.model) : "Model not known yet";
  const parts = [`${model} · ${MODEL_SOURCE[run.model_source]}`];
  if (run.supported_efforts.length === 0) parts.push("This model has no effort setting");
  if (run.unsupported_effort)
    parts.push(`${effortName(run.unsupported_effort)} is not supported by this model, so it is not used`);
  if (run.chat_model || run.chat_effort) {
    const chat = [run.chat_model && name(run.chat_model), run.chat_effort && effortName(run.chat_effort)].filter(
      Boolean,
    );
    parts.push(`Its chat is set to ${chat.join(", ")}`);
  }
  return parts.join(". ");
}

/** The compaction threshold as the field shows it: thousands of tokens, "0" for never. */
export function compactFieldOf(tokens: number): string {
  return String(Math.round(tokens / 1000));
}

/** What the field holds (thousands of tokens) as tokens to send, or why it cannot be sent. */
export function compactThresholdOf(field: string, limits: CompactionSetting): number | string {
  const k = Number(field.trim());
  if (field.trim() === "" || !Number.isInteger(k) || k < 0) return "Type a whole number of thousands of tokens.";
  const tokens = k * 1000;
  if (tokens !== 0 && (tokens < limits.min_tokens || tokens > limits.max_tokens))
    return `Choose 0 (never) or ${compactFieldOf(limits.min_tokens)} to ${compactFieldOf(limits.max_tokens)}.`;
  return tokens;
}
