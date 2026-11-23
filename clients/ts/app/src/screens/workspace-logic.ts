// What a workspace is, without a screen: which project is current, what belongs to it, and where a
// link takes it. A workspace is a project; null is All projects, where everything shows as it always did.

import type {
  EmployeeSummary,
  GuidelineProposalView,
  GuidelineView,
  ProjectSummary,
  RailPinsView,
} from "@aurelius/protocol";

export type Workspace = string | null;

/** The server takes a project on the reads that have none on their rows. Without it: no workspaces. */
export const WORKSPACE_FEATURE = "workspace_filters";

export const workspaceKey = (hostId: string | null) => `workspace:${hostId ?? ""}`;

export function storedWorkspace(value: string | null): Workspace {
  return value ? value : null;
}

export const openProjects = (projects: ProjectSummary[]) => projects.filter((p) => p.status !== "archived");

/** Where this device keeps the workspaces pinned to its rail, per server, when the server cannot. */
export const RAIL_PINS = "rail_projects";

/** The server keeps each board member's rail, so it follows them to every device. */
export const RAIL_PINS_FEATURE = "rail_pins";

/**
 * What the board member chose for their rail: the server's choice when it keeps rails, else this
 * device's. null: nothing chosen yet (every open workspace counts as pinned). undefined: the server's
 * rail has not loaded yet, so a pin now would overwrite a choice it does not know.
 */
export function chosenRail(
  serverKeeps: boolean,
  server: RailPinsView | undefined,
  device: string[] | null,
): string[] | null | undefined {
  if (!serverKeeps) return device;
  if (server === undefined) return undefined;
  return server.chosen ? server.project_ids : null;
}

/**
 * The workspaces pinned to the rail, in rail order. Until this device chose (`stored` null) every open
 * workspace counts as pinned, in the order the old "pin to the top" stars gave, so nobody's rail empties.
 */
export function railPins(stored: string[] | null, open: ProjectSummary[], legacy: string[]): string[] {
  if (stored !== null) return stored;
  const first = legacy.filter((id) => open.some((p) => p.id === id));
  return [...first, ...open.filter((p) => !first.includes(p.id)).map((p) => p.id)];
}

/** The rail's tiles: the pinned open workspaces, then the current one while it is not pinned. */
export function railProjects(pins: string[], open: ProjectSummary[], current: Workspace): ProjectSummary[] {
  const pinned = pins.flatMap((id) => open.find((p) => p.id === id) ?? []);
  const here = current !== null && !pins.includes(current) ? open.filter((p) => p.id === current) : [];
  return [...pinned, ...here];
}

/** Every open workspace for All workspaces, by name. */
export const byName = (projects: ProjectSummary[]) =>
  [...projects].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));

/** The remembered workspace, unless its project is gone or archived. While projects are unknown: as remembered. */
export function resolveWorkspace(stored: Workspace, projects: ProjectSummary[] | undefined): Workspace {
  if (stored === null || projects === undefined) return stored;
  return projects.some((p) => p.id === stored && p.status !== "archived") ? stored : null;
}

/** Everything in All projects; otherwise only what is this project's. No project: only under All projects. */
export function inWorkspace(workspace: Workspace, projectId: string | null | undefined): boolean {
  return workspace === null || projectId === workspace;
}

export function narrow<T extends { project_id?: string | null }>(items: T[], workspace: Workspace): T[] {
  return workspace === null ? items : items.filter((item) => item.project_id === workspace);
}

/** A proposed guideline counts for the workspace it suggests a link to. */
export function guidelineProposalsIn(list: GuidelineProposalView[], workspace: Workspace): GuidelineProposalView[] {
  if (workspace === null) return list;
  return list.filter((p) => (p.suggested_links ?? []).some((t) => t.kind === "project" && t.id === workspace));
}

export type GuidelineReach = "project" | "employee" | "company";

/**
 * Why a guideline shows in a workspace: linked to its project, reaching it through an employee who
 * works there, or company-level (bound to no project). Null: it is another project's alone.
 */
export function guidelineReach(
  guideline: Pick<GuidelineView, "links">,
  workspace: string,
  employees: Pick<EmployeeSummary, "id" | "projects_active">[],
): GuidelineReach | null {
  const { links } = guideline;
  if (links.some((t) => t.kind === "project" && t.id === workspace)) return "project";
  const worksHere = (id: string) => employees.some((e) => e.id === id && e.projects_active.includes(workspace));
  if (links.some((t) => t.kind === "employee" && worksHere(t.id))) return "employee";
  return links.some((t) => t.kind === "project") ? null : "company";
}

export function guidelinesIn<T extends Pick<GuidelineView, "links">>(
  list: T[],
  workspace: Workspace,
  employees: Pick<EmployeeSummary, "id" | "projects_active">[],
): T[] {
  if (workspace === null) return list;
  return list.filter((g) => guidelineReach(g, workspace, employees) !== null);
}

/** What a location points at, when it can only show in one project. */
export type OpenTarget = { kind: "ticket" | "task"; id: string } | { kind: "project"; id: string };

export function targetOf(pathname: string, search: Record<string, unknown>): OpenTarget | null {
  const text = (key: string) =>
    typeof search[key] === "string" && search[key] !== "" ? (search[key] as string) : null;
  if (pathname === "/board") {
    const id = text("ticket");
    return id ? { kind: "ticket", id } : null;
  }
  const task = /^\/tasks\/([^/]+)$/.exec(pathname)?.[1];
  if (task) return { kind: "task", id: task };
  if (pathname === "/knowledge/memory") {
    const id = text("project");
    return id ? { kind: "project", id } : null;
  }
  return null;
}

/**
 * The workspace after opening something of project `owner` (null: of no project), so it never
 * shows empty. All projects has everything, and only an open project can be a workspace.
 */
export function workspaceAfterOpening(current: Workspace, owner: string | null, projects: ProjectSummary[]): Workspace {
  if (current === null || owner === current) return current;
  return owner !== null && projects.some((p) => p.id === owner && p.status !== "archived") ? owner : null;
}

/** Where to go when the workspace changes under a screen that shows one thing of the old one. */
export function pathAfterSwitch(pathname: string, next: Workspace): string | null {
  if (pathname === "/board") return pathname;
  if (/^\/tasks\/[^/]+$/.test(pathname)) return "/tasks";
  if (/^\/ticket\/[^/]+$/.test(pathname)) return "/board";
  if (/^\/projects\/[^/]+$/.test(pathname)) return next === null ? "/projects" : `/projects/${next}`;
  return null;
}
