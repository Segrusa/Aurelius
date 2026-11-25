// What the settings screens compute, without a screen.

import type {
  CompactionSetting,
  EmployeeRunLine,
  EmployeeRunsView,
  RemoteClientView,
  RunValueSource,
  Setting2,
  UserRunDefaults,
} from "@aurelius/protocol";
import { effortName } from "./employee-logic";

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

/** Pages under Settings › Company that keep their own route; a project itself stays in the app. */
const COMPANY_PAGES = ["/employees", "/knowledge"];

/** Whether `path` shows the Settings sidebar instead of the app's: Settings itself and the Company pages. */
export function inSettingsArea(path: string): boolean {
  const under = (root: string) => path === root || path.startsWith(`${root}/`);
  return under("/settings") || COMPANY_PAGES.some(under) || path === "/projects";
}

/** The feature that makes a paired device its pairer's own (Account → Your devices). */
export const OWN_DEVICES = "own_devices";

/** "ios · Connected now · Notifications on": where a paired device stands. `seen` formats a time. */
export function deviceHint(device: RemoteClientView, seen: (ms: number | null | undefined) => string): string {
  const state = device.connected ? "Connected now" : `Last seen ${seen(device.last_seen_ms)}`;
  return [device.platform, state, device.push ? "Notifications on" : null].filter(Boolean).join(" · ");
}

/**
 * The devices Settings → Remote access lists. With `own_devices` a board member's devices are
 * in Account, so only those of nobody; an older server keeps every device here.
 */
export function remoteAccessDevices(clients: RemoteClientView[], ownDevices: boolean): RemoteClientView[] {
  return ownDevices ? clients.filter((c) => !c.paired_by) : clients;
}

/** One field of a board member's own run defaults, as My defaults offers it. */
export type RunDefaultField = "model" | "effort" | "mode" | "subagent_model";

/** What a selector shows for a field: its value, or `default` when nothing is chosen. */
export function runDefaultValue(defaults: UserRunDefaults, field: RunDefaultField): string {
  return defaults[field] ?? "default";
}

/** The defaults with one field changed; `default` clears it. Fields left unset are left out. */
export function withRunDefault(defaults: UserRunDefaults, field: RunDefaultField, value: string): UserRunDefaults {
  const next: UserRunDefaults = { ...defaults, [field]: value === "default" ? null : value };
  return Object.fromEntries(Object.entries(next).filter(([, v]) => v != null)) as UserRunDefaults;
}

/** The Default choice of a model: what a run uses without one of its own. */
export function defaultModelLabel(model: string | null | undefined, name: (model: string) => string): string {
  return model ? `Default · ${name(model)} (connection's default)` : "Default · the connection's model";
}
