// What a workspace is, without a screen: which project is current, what belongs to it, and where a
// link takes it. A workspace is a project; null is All projects, where everything shows as it always did.

import type {
  ChannelView,
  EmployeeSummary,
  GuidelineProposalView,
  GuidelineView,
  ProjectSummary,
} from "@aurelius/protocol";

export type Workspace = string | null;

/** The server takes a project on the reads that have none on their rows. Without it: no workspaces. */
export const WORKSPACE_FEATURE = "workspace_filters";

export const workspaceKey = (hostId: string | null) => `workspace:${hostId ?? ""}`;

export function storedWorkspace(value: string | null): Workspace {
  return value ? value : null;
}

export const openProjects = (projects: ProjectSummary[]) => projects.filter((p) => p.status !== "archived");

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

/** The channel a workspace opens on: the project's own. The general channel only under All projects. */
export function homeChannel(channels: ChannelView[], workspace: Workspace): ChannelView | null {
  if (workspace === null) return channels.find((c) => (c.kind ?? "general") === "general") ?? null;
  return channels.find((c) => c.project_id === workspace && !c.ticket_id && !c.archived) ?? null;
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
export type OpenTarget = { kind: "ticket" | "task" | "channel"; id: string } | { kind: "project"; id: string };

export function targetOf(pathname: string, search: Record<string, unknown>): OpenTarget | null {
  const text = (key: string) =>
    typeof search[key] === "string" && search[key] !== "" ? (search[key] as string) : null;
  if (pathname === "/board") {
    const id = text("ticket");
    return id ? { kind: "ticket", id } : null;
  }
  const task = /^\/tasks\/([^/]+)$/.exec(pathname)?.[1];
  if (task) return { kind: "task", id: task };
  if (pathname === "/chat") {
    const id = text("channel");
    return id ? { kind: "channel", id } : null;
  }
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
  if (pathname === "/board" || pathname === "/chat") return pathname;
  if (/^\/tasks\/[^/]+$/.test(pathname)) return "/tasks";
  if (/^\/projects\/[^/]+$/.test(pathname)) return next === null ? "/projects" : `/projects/${next}`;
  return null;
}
