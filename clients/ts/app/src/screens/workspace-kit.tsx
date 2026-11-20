// The switcher at the top of the sidebar, switching from anywhere, and following a link into its
// project. The state itself is in workspace.tsx; the rules are in workspace-logic.ts.

import { Icon } from "@astryxdesign/core/Icon";
import type { ProjectSummary } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useNavigate, useRouter, useRouterState } from "@tanstack/react-router";
import { useEffect, useMemo, useRef, useState } from "react";
import { useAllTasks, useProjects, useTicket } from "../data";
import { useAction } from "../keys/provider";
import { initial, Mark, markColor } from "../ui/marks";
import { Menu } from "../ui/menu";
import { useWorkspace } from "../workspace";
import { pinnedFirst } from "./employee-logic";
import { useProjectPins } from "./projects";
import { openProjects, pathAfterSwitch, targetOf, type Workspace, workspaceAfterOpening } from "./workspace-logic";

const ALL_KEY = "all";
const SETTINGS_KEY = "workspace-settings";
const LIST_KEY = "project-list";

const styles = stylex.create({
  slot: { display: "flex", flexDirection: "column", paddingBlockEnd: 4 },
  trigger: {
    display: "flex",
    minWidth: 0,
    width: "100%",
    height: 30,
    cursor: "pointer",
    alignItems: "center",
    gap: 8,
    borderWidth: 0,
    borderRadius: 6,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    paddingInline: 8,
    fontSize: 13,
    fontWeight: 500,
    color: "var(--color-text-primary)",
  },
  rail: { width: 32, height: 32, justifyContent: "center", marginInline: "auto", paddingInline: 0 },
  name: {
    minWidth: 0,
    flex: 1,
    overflow: "hidden",
    textAlign: "start",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
  },
});

const ALL_NAME = "All projects";

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

export function WorkspaceSwitcher({ collapsed }: { collapsed: boolean }) {
  const { id, project, available, locked } = useWorkspace();
  const projects = useProjects();
  const [pins] = useProjectPins();
  const switchTo = useSwitchWorkspace();
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  useAction("SwitchWorkspace", () => setOpen(true), available && !locked);
  if (!available || locked) return null;
  const list = pinnedFirst(openProjects(projects.data ?? []), pins);
  const name = project?.name ?? ALL_NAME;
  const pick = (key: string) => {
    if (key === SETTINGS_KEY && id) void navigate({ to: "/projects/$id", params: { id } });
    else if (key === LIST_KEY) void navigate({ to: "/projects" });
    else switchTo(key === ALL_KEY ? null : key);
  };
  return (
    <div {...stylex.props(styles.slot)} data-workspace-switcher>
      <Menu
        label={`Workspace: ${name}`}
        open={open}
        onOpenChange={setOpen}
        value={id ?? ALL_KEY}
        options={[
          { key: ALL_KEY, label: ALL_NAME, icon: <WorkspaceMark project={null} size={16} /> },
          ...list.map((p) => ({
            key: p.id,
            label: p.status === "paused" ? `${p.name} · paused` : p.name,
            icon: <WorkspaceMark project={p} size={16} />,
          })),
          ...(id ? [{ key: SETTINGS_KEY, label: "Workspace settings", dividerBefore: true }] : []),
          { key: LIST_KEY, label: "All workspaces…", ...(id ? {} : { dividerBefore: true }) },
        ]}
        onPick={pick}
        trigger={
          <button
            type="button"
            title={collapsed ? name : undefined}
            {...stylex.props(styles.trigger, collapsed && styles.rail)}
          >
            <WorkspaceMark project={project} size={collapsed ? 22 : 18} />
            {collapsed ? null : (
              <>
                <span {...stylex.props(styles.name)}>{name}</span>
                <Icon icon="chevronDown" size="sm" color="secondary" />
              </>
            )}
          </button>
        }
      />
    </div>
  );
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
