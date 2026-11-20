// Switching workspace (the sidebar's rail and the palette do it), the Switch workspace key, and following a
// link into its project. The state itself is in workspace.tsx; the rules are in workspace-logic.ts.

import type { ProjectSummary } from "@aurelius/protocol";
import { useNavigate, useRouter, useRouterState } from "@tanstack/react-router";
import { useEffect, useMemo, useRef } from "react";
import { useAllTasks, useProjects, useTicket } from "../data";
import { useAction } from "../keys/provider";
import { initial, Mark, markColor } from "../ui/marks";
import { useWorkspace } from "../workspace";
import { overlays } from "./overlays-store";
import { pathAfterSwitch, targetOf, type Workspace, workspaceAfterOpening } from "./workspace-logic";

const ALL_NAME = "All workspaces";

export function WorkspaceMark({ project, size = 18 }: { project: ProjectSummary | null; size?: number }) {
  return project ? (
    <Mark letter={initial(project.name)} color={markColor(project.id)} size={size} radius={5} />
  ) : (
    <Mark letter={initial(ALL_NAME)} color="var(--color-accent)" size={size} radius={5} />
  );
}

/** Changes the workspace, and leaves what only showed one thing of the old one. */
export function useSwitchWorkspace(): (next: Workspace) => void {
  const { choose } = useWorkspace();
  const navigate = useNavigate();
  const router = useRouter();
  return (next) => {
    choose(next);
    const to = pathAfterSwitch(router.state.location.pathname, next);
    if (to) void navigate({ to, search: {} });
  };
}

/** The Switch workspace key opens the palette, whose first group is the workspaces: the rail shows them all. */
export function useSwitchWorkspaceKey(): void {
  const { available, locked } = useWorkspace();
  useAction("SwitchWorkspace", () => overlays.open("palette"), available && !locked);
}

/** Opening a ticket or task of another project moves the workspace there, so nothing shows empty. */
export function useOpenedWorkspace(): void {
  const { id, available, locked, choose } = useWorkspace();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const search = useRouterState({ select: (s) => s.location.search as Record<string, unknown> });
  const target = useMemo(() => targetOf(pathname, search), [pathname, search]);
  const key = target ? `${target.kind}:${target.id}` : null;
  const ticket = useTicket(target?.kind === "ticket" ? target.id : null);
  const tasks = useAllTasks();
  const projects = useProjects();
  // Known: a project, or null for what has none. Undefined: not known (yet), so nothing moves.
  let owner: string | null | undefined;
  if (target?.kind === "ticket") owner = ticket.data?.ticket.project_id;
  if (target?.kind === "project") owner = target.id;
  if (target?.kind === "task") {
    const task = tasks.data?.find((t) => t.id === target.id);
    owner = task ? task.project_id : undefined;
  }
  const handled = useRef<string | null>(null);
  useEffect(() => {
    if (key === null) {
      handled.current = null;
      return;
    }
    if (handled.current === key || owner === undefined || !projects.data) return;
    // Once per opening: switching away afterwards is the reader's choice.
    handled.current = key;
    if (locked || !available) return;
    const next = workspaceAfterOpening(id, owner, projects.data);
    if (next !== id) choose(next);
  }, [key, owner, projects.data, id, locked, available, choose]);
}
