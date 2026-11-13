// What the Employees screens compute, without a screen. Port of screens/employees.rs, org.rs, dashboard.rs.

import type {
  AccessLevel,
  AgentView,
  ConnectionsSnapshot,
  ConnectionView,
  EmployeeSummary,
  Setting2,
} from "@aurelius/protocol";

export const MAX_PINS = 5;

/** The access levels a board member chooses from; "unknown" is only ever read from a newer server. */
export const ACCESS_LEVELS: readonly { value: AccessLevel; label: string; hint: string }[] = [
  {
    value: "inherit",
    label: "Inherit",
    hint: "All work tools: files, commands and the web. The mode decides what is asked.",
  },
  {
    value: "read_only",
    label: "Read only",
    hint: "Reads and searches files. No edits, commands, web or MCP servers.",
  },
  { value: "read_edit", label: "Read and edit", hint: "Also changes files. No commands, web or MCP servers." },
];

export const accessOf = (e: Pick<EmployeeSummary, "access_level"> | null): AccessLevel => e?.access_level ?? "inherit";

export const accessLabel = (level: AccessLevel): string =>
  ACCESS_LEVELS.find((l) => l.value === level)?.label ?? "Unknown";

export const asChoice = (s: Setting2 | undefined): string => (s?.mode === "value" ? s.value : "inherit");

const EFFORT_NAMES: Record<string, string> = {
  low: "Low",
  medium: "Medium",
  high: "High",
  xhigh: "Extra high",
  max: "Max",
};

export const effortName = (effort: string): string => EFFORT_NAMES[effort] ?? effort;

/** What an employee runs on without a choice of its own: the company's default. */
export interface InheritedRun {
  provider: string | null;
  model: string | null;
  effort: string | null;
}

export function inheritedRun(list: Partial<ConnectionsSnapshot> | undefined): InheritedRun {
  const settings = list?.defaults?.find((d) => d.purpose === "employee" && d.scope.kind === "global")?.settings ?? {};
  const id = asChoice(settings.connection);
  const connection = (list?.connections ?? []).find((c) => c.id === id);
  const model = asChoice(settings.model);
  const effort = asChoice(settings.effort);
  return {
    provider: connection?.display_name ?? null,
    model: model !== "inherit" ? model : (connection?.default_model ?? null),
    effort: effort !== "inherit" ? effort : null,
  };
}

/** A connection a run can be put on: the server refuses a disconnected one. */
export const usableConnections = (all: ConnectionView[]): ConnectionView[] =>
  all.filter((c) => c.status !== "disconnected");

/** The models to offer: the chosen connection's, or those of every connection on Inherit. */
export function modelsFor(all: ConnectionView[], provider: string): string[] {
  const from = provider === "inherit" ? all : all.filter((c) => c.id === provider);
  return [...new Set(from.flatMap((c) => c.models))];
}

/** A model the newly chosen connection does not offer goes back to Inherit. */
export function modelAfterProvider(all: ConnectionView[], provider: string, model: string): string {
  return model === "inherit" || modelsFor(all, provider).includes(model) ? model : "inherit";
}

/** "Inherit · Sonnet 4.5" when there is something to inherit, else "Inherit". */
export const inheritLabel = (value: string | null): string => (value ? `Inherit · ${value}` : "Inherit");

/** A stored choice for reading: its value, or Inherit with what that comes to. */
export const choiceLine = (
  choice: string,
  inherited: string | null,
  name: (value: string) => string = (v) => v,
): string => (choice === "inherit" ? inheritLabel(inherited) : name(choice));

export type EmployeeTab = "inbox" | "memory" | "settings";

/** The employee page's tabs. An older server has no mailbox to show. */
export function employeeTabs(queue: boolean): { key: EmployeeTab; label: string }[] {
  return [
    ...(queue ? [{ key: "inbox" as const, label: "Inbox" }] : []),
    { key: "memory", label: "Memory" },
    { key: "settings", label: "Settings" },
  ];
}

export const isEmployeeTab = (value: unknown): value is EmployeeTab =>
  value === "inbox" || value === "memory" || value === "settings";

/** What the dot and word beside an employee say (U6). Colors are CSS values. */
export interface EmployeeState {
  label: string;
  color: string;
  variant: "accent" | "warning" | "neutral";
  isPulsing: boolean;
}

const STATE_TEXT = "var(--color-text-secondary)";
const STATE_DIM = "var(--precision-text3)";
const STATE_ON = "var(--color-text-accent)";
const STATE_WARN = "var(--color-text-warning)";

export function employeeState(e: Pick<EmployeeSummary, "status" | "working" | "activity_state">): EmployeeState {
  if (e.status === "paused") return { label: "Paused", color: STATE_DIM, variant: "neutral", isPulsing: false };
  // An older server has no state, and a state of a newer server says no more than `working`.
  const known = e.activity_state && e.activity_state !== "unknown" ? e.activity_state : null;
  switch (known ?? (e.working ? "working" : "idle")) {
    case "working":
      return { label: "Working", color: STATE_ON, variant: "accent", isPulsing: true };
    case "waiting":
      return { label: "Waiting for you", color: STATE_WARN, variant: "warning", isPulsing: false };
    case "queued":
      return { label: "Queued", color: STATE_TEXT, variant: "accent", isPulsing: false };
    default:
      return { label: "Available", color: STATE_TEXT, variant: "neutral", isPulsing: false };
  }
}

export function matchesSearch(e: EmployeeSummary, query: string): boolean {
  const q = query.trim().toLowerCase();
  return (
    q === "" ||
    e.name.toLowerCase().includes(q) ||
    e.role.toLowerCase().includes(q) ||
    (e.title ?? "").toLowerCase().includes(q)
  );
}

export const position = (e: EmployeeSummary) => (e.title ? `${e.title} · ${e.role}` : e.role);

/** Active, and without a boss who still works here. */
export function roots(all: EmployeeSummary[]): EmployeeSummary[] {
  const here = new Set(all.filter((e) => e.status !== "archived").map((e) => e.id));
  return all.filter((e) => e.status === "active" && (!e.reports_to || !here.has(e.reports_to)));
}

/** How many chats the sidebar shows. */
const RECENT_CHATS = 4;

/** The sidebar's chats: the server gives them latest first. */
export const recentChats = <T>(chats: T[]): T[] => chats.slice(0, RECENT_CHATS);

export function pinnedFirst<T extends { id: string }>(list: T[], pins: string[]): T[] {
  const pinned = pins.flatMap((id) => list.find((x) => x.id === id) ?? []);
  return [...pinned, ...list.filter((x) => !pins.includes(x.id))];
}

export function togglePin(pins: string[], id: string): string[] {
  return pins.includes(id) ? pins.filter((x) => x !== id) : [...pins, id];
}

export const canPin = (pins: string[], id: string) => pins.includes(id) || pins.length < MAX_PINS;

export function employeeGroups(
  all: EmployeeSummary[],
  shown: EmployeeSummary[],
  pins: string[],
): { label: string; list: EmployeeSummary[] }[] {
  const team = new Set(roots(all).map((e) => e.id));
  const pinned = pins.flatMap((id) => shown.find((e) => e.id === id) ?? []);
  const rest = shown.filter((e) => !pins.includes(e.id));
  const groups = [
    { label: "PINNED", list: pinned },
    { label: "MANAGEMENT TEAM", list: rest.filter((e) => team.has(e.id)) },
  ];
  let left = rest.filter((e) => !team.has(e.id));
  const bosses = [...new Set(left.flatMap((e) => e.reports_to ?? []))];
  for (const boss of bosses) {
    const name = all.find((e) => e.id === boss)?.name.toUpperCase() ?? "(archived)";
    groups.push({ label: `REPORTS TO ${name}`, list: left.filter((e) => e.reports_to === boss) });
    left = left.filter((e) => e.reports_to !== boss);
  }
  groups.push({ label: "OTHERS", list: left });
  return groups.filter((g) => g.list.length > 0);
}

export interface OrgNode {
  employee: EmployeeSummary;
  children: OrgNode[];
}

/** The tree of who reports to whom. A boss who is gone makes a root; a loop is shown once. */
export function orgTree(all: EmployeeSummary[]): OrgNode[] {
  const here = all.filter((e) => e.status !== "archived");
  const ids = new Set(here.map((e) => e.id));
  const seen = new Set<string>();
  const node = (employee: EmployeeSummary): OrgNode => {
    seen.add(employee.id);
    const children = here.filter((e) => e.reports_to === employee.id && !seen.has(e.id));
    for (const c of children) seen.add(c.id);
    return { employee, children: children.map(node) };
  };
  return here.filter((e) => !e.reports_to || !ids.has(e.reports_to)).map(node);
}

/** Who may be the boss: anyone but itself and those who report to it, directly or not. */
export function reportsCandidates(all: EmployeeSummary[], me: string | null): EmployeeSummary[] {
  const here = all.filter((e) => e.status !== "archived");
  if (me === null) return here;
  const below = new Set([me]);
  for (let changed = true; changed; ) {
    changed = false;
    for (const e of here) {
      if (e.reports_to && below.has(e.reports_to) && !below.has(e.id)) {
        below.add(e.id);
        changed = true;
      }
    }
  }
  return here.filter((e) => !below.has(e.id));
}

export function workOf(agents: AgentView[]): { title: string; since: number } | null {
  const rank = (kind: string) => {
    if (kind === "planning" || kind === "follow_up") return 0;
    if (kind === "conversation") return 1;
    return kind === "delegated" ? 2 : 3;
  };
  const best = agents
    .filter((a) => rank(a.kind) < 3)
    .sort((a, b) => rank(a.kind) - rank(b.kind) || a.since_ms - b.since_ms)[0];
  if (!best) return null;
  const title =
    best.kind === "conversation"
      ? best.name
      : best.kind === "delegated"
        ? `Waiting on ${best.name}: ${best.detail}`
        : best.detail.trim() === ""
          ? best.name
          : best.detail;
  return { title, since: best.since_ms };
}

export type ThreadGroup = "active" | "past" | "earlier";

const DAY = 86_400_000;
const days = (ms: number) => Math.floor((ms + 2 * 3_600_000) / DAY);

export function threadGroup(ongoing: boolean, open: boolean, updatedMs: number, nowMs: number): ThreadGroup {
  if (ongoing) return "active";
  return open || days(nowMs) - days(updatedMs) < 1 ? "past" : "earlier";
}

export type AgentMode = "ask" | "accept_edits" | "auto" | "plan";
export const MODES: AgentMode[] = ["ask", "accept_edits", "auto", "plan"];
export const MODE_LABELS: Record<AgentMode, string> = {
  ask: "Ask",
  accept_edits: "Accept edits",
  auto: "Auto",
  plan: "Plan",
};
export const nextMode = (mode: AgentMode) => MODES[(MODES.indexOf(mode) + 1) % MODES.length] as AgentMode;

/** Lines of the instructions the employee page shows before "Show all". */
export const INSTRUCTIONS_PREVIEW_LINES = 12;
/** About the characters that fill those lines when the text wraps. */
const INSTRUCTIONS_PREVIEW_CHARS = 1200;

/**
 * Whether an employee's instructions are longer than the folded preview, so
 * the page offers "Show all": its persona and rules are in that text, and the
 * rules usually come after the first lines.
 */
export const instructionsFold = (text: string) =>
  text.split("\n").length > INSTRUCTIONS_PREVIEW_LINES || text.length > INSTRUCTIONS_PREVIEW_CHARS;
