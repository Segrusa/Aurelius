// What the Employees screens compute, without a screen. Port of screens/employees.rs, org.rs, dashboard.rs.

import type { AgentView, EmployeeSummary } from "@aurelius/protocol";

export const MAX_PINS = 5;

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
