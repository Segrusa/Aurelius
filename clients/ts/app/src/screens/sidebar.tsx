// The app's sidebar: a rail of workspaces on the left (All workspaces, one tile per project, the Inbox
// bell, the account and the fold button), and beside it the workspace's panel with New session, Board and History and its
// sessions under Running, Today and Earlier. Under All workspaces those come per workspace.

import type { IconType } from "@astryxdesign/core/Icon";
import { SideNav, SideNavItem } from "@astryxdesign/core/SideNav";
import { Tooltip } from "@astryxdesign/core/Tooltip";
import type { ProjectSummary, TicketView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useNavigate, useRouterState } from "@tanstack/react-router";
import { type ReactNode, useMemo, useState } from "react";
import { useLinkState } from "../connection";
import { useProjects, useTickets, useWorkSessions } from "../data";
import { type Feature, offered, shown } from "../hidden";
import { keysFor, SHORTCUTS, showChord } from "../keys/keymap";
import { isMac, useKeys } from "../keys/provider";
import { usePlatform } from "../platform";
import { Icon, NAV_ICONS, type NavIcon, NewIcon, PlusIcon, SearchIcon } from "../ui/icons";
import { KeyHint } from "../ui/key-hint";
import { initial, Mark, markColor } from "../ui/marks";
import { useWorkspace } from "../workspace";
import { pinnedFirst } from "./employee-logic";
import { InboxBell } from "./inbox-bell";
import { overlays } from "./overlays-store";
import { useProjectPins } from "./projects";
import { type SessionSignal, sessionSignal, startOfDay } from "./session-logic";
import {
  BUCKET_LABELS,
  type Bucket,
  bucketed,
  foldKey,
  isFolded,
  needsPerProject,
  type SidebarBucket,
  type SidebarItem,
  workspaceGroups,
} from "./sidebar-logic";
import { sidebarTickets, ticketSignal } from "./ticket-session-logic";
import { useTicketNeeds } from "./tickets-kit";
import { UserMenu } from "./user-menu";
import { useSwitchWorkspace, WorkspaceMark } from "./workspace-kit";
import { openProjects } from "./workspace-logic";

// A working session's dot breathes, so it reads as live next to the still ones.
const pulse = stylex.keyframes({
  "0%": { opacity: 1, transform: "scale(1)" },
  "50%": { opacity: 0.35, transform: "scale(0.75)" },
  "100%": { opacity: 1, transform: "scale(1)" },
});

const RAIL_WIDTH = 56;
const TILE = 36;

const styles = stylex.create({
  side: { display: "flex", height: "100%", minHeight: 0 },
  rail: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    gap: 10,
    width: RAIL_WIDTH,
    flexShrink: 0,
    paddingBlock: 12,
    boxSizing: "border-box",
    overflowY: "auto",
    scrollbarWidth: "none",
    backgroundColor: "var(--color-background-body)",
    borderInlineEnd: "1px solid var(--color-border)",
  },
  slot: { position: "relative", display: "flex", justifyContent: "center", width: RAIL_WIDTH, flexShrink: 0 },
  // The current workspace's pill at the rail's edge.
  pill: {
    position: "absolute",
    insetInlineStart: 0,
    top: 6,
    width: 3,
    height: 24,
    borderStartEndRadius: 3,
    borderEndEndRadius: 3,
    backgroundColor: "var(--color-text-primary)",
  },
  tile: {
    display: "grid",
    placeItems: "center",
    width: TILE,
    height: TILE,
    padding: 0,
    boxSizing: "border-box",
    borderWidth: 0,
    borderRadius: { default: TILE / 2, ":hover": 12 },
    cursor: "pointer",
    color: "#fff",
    font: "inherit",
    fontSize: 15,
    fontWeight: 600,
    opacity: { default: 0.75, ":hover": 1 },
    transition: "border-radius 120ms ease, opacity 120ms ease",
  },
  tileOn: { borderRadius: 12, opacity: 1 },
  allTile: {
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    backgroundColor: "var(--color-background-surface)",
    color: "var(--color-text-primary)",
  },
  divider: { width: 24, height: 1, flexShrink: 0, backgroundColor: "var(--color-border)" },
  badge: {
    position: "absolute",
    top: -4,
    insetInlineEnd: 6,
    display: "grid",
    placeItems: "center",
    minWidth: 16,
    height: 16,
    paddingInline: 4,
    boxSizing: "border-box",
    borderRadius: 8,
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: "var(--color-background-body)",
    backgroundColor: "var(--color-error)",
    color: "#fff",
    fontSize: 10,
    fontWeight: 600,
    fontVariantNumeric: "tabular-nums",
    pointerEvents: "none",
  },
  railButton: {
    display: "grid",
    placeItems: "center",
    width: TILE,
    height: TILE,
    flexShrink: 0,
    padding: 0,
    borderRadius: 10,
    cursor: "pointer",
    color: "var(--precision-text3)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    borderWidth: 0,
  },
  headButton: { width: 28, height: 28, borderRadius: 8 },
  newTile: { borderWidth: 1, borderStyle: "dashed", borderColor: "var(--color-border-emphasized)" },
  grow: { flexGrow: 1 },
  head: { display: "flex", alignItems: "center", gap: 4, height: 36 },
  title: {
    flex: 1,
    minWidth: 0,
    paddingInline: 8,
    fontSize: 17,
    fontWeight: 600,
    color: "var(--color-text-primary)",
    whiteSpace: "nowrap",
    overflow: "hidden",
    textOverflow: "ellipsis",
    userSelect: "none",
  },
  label: {
    marginBlockStart: 12,
    paddingBlockStart: 12,
    paddingInline: 10,
    borderBlockStartWidth: 1,
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--color-border)",
    fontSize: 11,
    fontWeight: 500,
    color: "var(--precision-text3)",
  },
  list: { display: "flex", flexDirection: "column", gap: 10, paddingBlock: 8 },
  group: { display: "flex", flexDirection: "column", gap: 1 },
  fold: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    height: 30,
    paddingInline: 8,
    borderWidth: 0,
    borderRadius: 8,
    font: "inherit",
    fontSize: 13,
    fontWeight: 600,
    textAlign: "start",
    cursor: "pointer",
    color: "var(--color-text-primary)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
  },
  subFold: {
    height: 24,
    gap: 6,
    paddingInline: 10,
    marginInlineStart: 8,
    fontSize: 11,
    fontWeight: 500,
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-secondary)" },
    backgroundColor: "transparent",
  },
  foldName: { flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  chevron: { display: "flex", color: "var(--precision-text3)", transition: "transform 120ms ease" },
  chevronFolded: { transform: "rotate(-90deg)" },
  count: { fontSize: 11.5, fontWeight: 500, color: "var(--precision-text3)", fontVariantNumeric: "tabular-nums" },
  runningCount: {
    display: "flex",
    alignItems: "center",
    gap: 5,
    fontSize: 11,
    fontWeight: 500,
    color: "var(--color-text-success)",
    fontVariantNumeric: "tabular-nums",
  },
  runningLabel: { color: "var(--color-text-success)" },
  rows: { display: "flex", flexDirection: "column", gap: 1, paddingInlineStart: 16 },
  subRows: { paddingInlineStart: 24 },
  dot: { display: "block", width: 6, height: 6, borderRadius: "50%", flexShrink: 0 },
  dotWorking: {
    backgroundColor: "var(--color-text-success)",
    animationName: pulse,
    animationDuration: "1.6s",
    animationIterationCount: "infinite",
    animationTimingFunction: "ease-in-out",
  },
  dotRunning: { backgroundColor: "var(--color-text-success)" },
  dotNeedsYou: { backgroundColor: "var(--color-text-error)" },
  empty: { margin: 0, paddingInline: 10, fontSize: 12, color: "var(--precision-text3)" },
  reconnect: { margin: 0, paddingInline: 8, fontSize: 12, color: "var(--color-text-warning)" },
});

const ALL_NAME = "All workspaces";
const NO_PROJECT = "No project";

// `server`: the row shows only when the server has that feature.
const NAV: { to: string; label: string; icon: NavIcon; feature?: Feature; server?: string }[] = [
  { to: "/board", label: "Board", icon: "board" },
  { to: "/history", label: "History", icon: "history", server: "ticket_history" },
  { to: "/tasks", label: "Tasks", icon: "tasks", feature: "TasksInSidebar" },
  { to: "/scheduled", label: "Scheduled", icon: "clock", feature: "ScheduledInSidebar" },
  { to: "/activity", label: "Activity log", icon: "note", feature: "ActivityInSidebar" },
];

const NEW_SESSION_KEYS = SHORTCUTS.find((s) => s.action === "NewItem")?.keys ?? [];

// SideNavItem takes a component: one per mark, kept, or every render would remount the rows' icons.
const MARK_ICONS = new Map<string, IconType>();

function markIcon(letter: string, color: string): IconType {
  const key = `${letter}|${color}`;
  let icon = MARK_ICONS.get(key);
  if (!icon) {
    icon = () => <Mark letter={letter} color={color} size={18} radius={4} />;
    MARK_ICONS.set(key, icon);
  }
  return icon;
}

function NavRow({ to, label, icon, end }: { to: string; label: string; icon: IconType; end?: ReactNode }) {
  const path = useRouterState({ select: (s) => s.location.pathname });
  // A ticket's session counts as the Board, where it came from (ticket #70).
  const on = path === to || path.startsWith(`${to}/`) || (to === "/board" && path.startsWith("/ticket/"));
  return <SideNavItem label={label} href={to} icon={icon} size="sm" isSelected={on} endContent={end} />;
}

/** A row's dot: green and breathing while it works, red while it waits for you. */
function SignalDot({ signal }: { signal: SessionSignal | null }) {
  if (signal === null) return null;
  const working = signal === "working";
  const text = working ? "Working" : "Waiting for you";
  return (
    <Tooltip content={text} delay={400} hasHoverIndication={false}>
      <span
        role="img"
        aria-label={text}
        {...stylex.props(styles.dot, working ? styles.dotWorking : styles.dotNeedsYou)}
      />
    </Tooltip>
  );
}

/**
 * Every work session and open ticket session as one list of sidebar rows, of all workspaces: the rail
 * counts per workspace and All workspaces lists them all.
 */
function useSidebarItems(): SidebarItem[] {
  const sessions = useWorkSessions().data;
  const tickets = useTickets().data;
  const need = useTicketNeeds(tickets ?? []);
  return useMemo(() => {
    const signal = (t: TicketView) => ticketSignal(t, need.mine(t));
    const fromSessions: SidebarItem[] = (sessions ?? []).map((s) => ({
      key: `s:${s.conversation_id}`,
      to: `/session/${s.conversation_id}`,
      label: s.title || "Session",
      who: s.employee_name,
      whoId: s.employee_id,
      projectId: s.project_id ?? null,
      signal: sessionSignal(s),
      at: s.last_activity_ms,
    }));
    // Since 0: every open ticket, not only the latest few.
    const fromTickets: SidebarItem[] = sidebarTickets(tickets ?? [], null, signal, 0).map((t) => ({
      key: `t:${t.id}`,
      to: `/ticket/${t.number}`,
      label: `#${t.number} ${t.title}`,
      who: t.owner_name ?? "?",
      whoId: t.owner_employee_id ?? t.id,
      projectId: t.project_id ?? null,
      signal: signal(t),
      at: t.updated_at_ms,
    }));
    return [...fromSessions, ...fromTickets];
  }, [sessions, tickets, need]);
}

/** Whether a fold of the session list is closed, kept per fold across starts. */
function useFold(key: string, bucket?: Bucket): [boolean, () => void] {
  const { storage } = usePlatform();
  const [folded, setFolded] = useState(() => isFolded(storage.get(key), bucket));
  const toggle = () =>
    setFolded((was) => {
      storage.set(key, was ? "open" : "folded");
      return !was;
    });
  return [folded, toggle];
}

function Chevron({ folded, size }: { folded: boolean; size: number }) {
  return (
    <span {...stylex.props(styles.chevron, folded && styles.chevronFolded)}>
      <Icon name="chevronDown" size={size} weight={2} />
    </span>
  );
}

function Rows({ items, sub }: { items: SidebarItem[]; sub?: boolean }) {
  return (
    <div {...stylex.props(styles.rows, sub && styles.subRows)}>
      {items.map((i) => (
        <NavRow
          key={i.key}
          to={i.to}
          label={i.label}
          icon={markIcon(initial(i.who), markColor(i.whoId))}
          end={<SignalDot signal={i.signal} />}
        />
      ))}
    </div>
  );
}

/** One of Running, Today and Earlier in a single workspace. */
function BucketFold({ entry }: { entry: SidebarBucket }) {
  const [folded, toggle] = useFold(foldKey(undefined, entry.bucket), entry.bucket);
  const running = entry.bucket === "running";
  return (
    <div {...stylex.props(styles.group)}>
      <button type="button" aria-expanded={!folded} onClick={toggle} {...stylex.props(styles.fold)}>
        <Chevron folded={folded} size={12} />
        {running ? <span {...stylex.props(styles.dot, styles.dotRunning)} /> : null}
        <span {...stylex.props(styles.foldName)}>{BUCKET_LABELS[entry.bucket]}</span>
        <span {...stylex.props(styles.count)}>{entry.items.length}</span>
      </button>
      {folded ? null : <Rows items={entry.items} />}
    </div>
  );
}

/** Running, Today or Earlier inside a workspace under All workspaces. */
function SubFold({ workspace, entry }: { workspace: string | null; entry: SidebarBucket }) {
  const [folded, toggle] = useFold(foldKey(workspace, entry.bucket), entry.bucket);
  return (
    <div {...stylex.props(styles.group)}>
      <button type="button" aria-expanded={!folded} onClick={toggle} {...stylex.props(styles.fold, styles.subFold)}>
        <Chevron folded={folded} size={10} />
        <span {...stylex.props(entry.bucket === "running" && styles.runningLabel)}>{BUCKET_LABELS[entry.bucket]}</span>
        <span {...stylex.props(styles.count)}>{entry.items.length}</span>
      </button>
      {folded ? null : <Rows items={entry.items} sub />}
    </div>
  );
}

function WorkspaceFold({
  project,
  id,
  buckets,
  count,
  running,
}: {
  project: ProjectSummary | null;
  id: string | null;
  buckets: SidebarBucket[];
  count: number;
  running: number;
}) {
  const [folded, toggle] = useFold(foldKey(id));
  const name = project?.name ?? NO_PROJECT;
  return (
    <div {...stylex.props(styles.group)}>
      <button type="button" aria-expanded={!folded} onClick={toggle} {...stylex.props(styles.fold)}>
        <Chevron folded={folded} size={12} />
        {project ? <WorkspaceMark project={project} size={18} /> : null}
        <span {...stylex.props(styles.foldName)}>{name}</span>
        {running > 0 ? (
          <span {...stylex.props(styles.runningCount)} title={`${running} running`}>
            <span {...stylex.props(styles.dot, styles.dotRunning)} />
            {running}
          </span>
        ) : null}
        <span {...stylex.props(styles.count)}>{count}</span>
      </button>
      {folded ? null : buckets.map((b) => <SubFold key={b.bucket} workspace={id} entry={b} />)}
    </div>
  );
}

function SessionList({ items, projects }: { items: SidebarItem[]; projects: ProjectSummary[] }) {
  const workspace = useWorkspace().id;
  const since = startOfDay(Date.now());
  if (workspace !== null) {
    const buckets = bucketed(
      items.filter((i) => i.projectId === workspace),
      since,
    );
    if (buckets.length === 0) return <p {...stylex.props(styles.empty)}>No sessions yet</p>;
    return (
      <div {...stylex.props(styles.list)}>
        {buckets.map((b) => (
          <BucketFold key={b.bucket} entry={b} />
        ))}
      </div>
    );
  }
  const groups = workspaceGroups(
    items,
    projects.map((p) => p.id),
    since,
  );
  if (groups.length === 0) return <p {...stylex.props(styles.empty)}>No sessions yet</p>;
  return (
    <div {...stylex.props(styles.list)}>
      {groups.map((g) => (
        <WorkspaceFold key={g.id ?? ""} project={projects.find((p) => p.id === g.id) ?? null} {...g} />
      ))}
    </div>
  );
}

function RailTile({
  label,
  on,
  needs,
  onClick,
  children,
  all,
  color,
}: {
  label: string;
  on: boolean;
  needs: number;
  onClick: () => void;
  children: ReactNode;
  all?: boolean;
  color?: string;
}) {
  return (
    <div {...stylex.props(styles.slot)}>
      {on ? <span {...stylex.props(styles.pill)} /> : null}
      <Tooltip content={label} placement="end" delay={300} hasHoverIndication={false}>
        <button
          type="button"
          aria-label={needs > 0 ? `${label}, ${needs} waiting for you` : label}
          aria-current={on ? "true" : undefined}
          onClick={onClick}
          {...stylex.props(styles.tile, all && styles.allTile, on && styles.tileOn)}
          style={color ? { background: color } : undefined}
        >
          {children}
        </button>
      </Tooltip>
      {needs > 0 && !on ? (
        <span aria-hidden="true" {...stylex.props(styles.badge)}>
          {needs > 99 ? "99+" : needs}
        </span>
      ) : null}
    </div>
  );
}

function Rail({
  projects,
  needs,
  collapsed,
  onCollapse,
}: {
  projects: ProjectSummary[];
  needs: Map<string, number>;
  collapsed: boolean;
  onCollapse: (collapsed: boolean) => void;
}) {
  const { id, available, locked } = useWorkspace();
  const { user } = useLinkState();
  const switchTo = useSwitchWorkspace();
  const keys = useKeys();
  const navigate = useNavigate();
  const workspaces = available && !locked;
  const allNeeds = [...needs.values()].reduce((n, v) => n + v, 0);
  const fold = collapsed ? "Show sidebar" : "Hide sidebar";
  return (
    <nav aria-label="Workspaces" data-sidebar-drag {...stylex.props(styles.rail)}>
      {workspaces ? (
        <>
          <RailTile label={ALL_NAME} on={id === null} needs={allNeeds} onClick={() => switchTo(null)} all>
            <Icon name="grid" size={16} />
          </RailTile>
          <span {...stylex.props(styles.divider)} />
          {projects.map((p) => (
            <RailTile
              key={p.id}
              label={p.status === "paused" ? `${p.name} · paused` : p.name}
              on={id === p.id}
              needs={needs.get(p.id) ?? 0}
              onClick={() => switchTo(p.id)}
              color={markColor(p.id)}
            >
              {initial(p.name)}
            </RailTile>
          ))}
          <Tooltip content="New workspace" placement="end" delay={300} hasHoverIndication={false}>
            <button
              type="button"
              aria-label="New workspace"
              onClick={() => keys.run("NewProject")}
              {...stylex.props(styles.railButton, styles.newTile)}
            >
              <PlusIcon size={16} />
            </button>
          </Tooltip>
        </>
      ) : null}
      <span {...stylex.props(styles.grow)} />
      <InboxBell />
      {user ? (
        <UserMenu collapsed />
      ) : (
        <Tooltip content="Settings" placement="end" delay={300} hasHoverIndication={false}>
          <button
            type="button"
            aria-label="Settings"
            onClick={() => void navigate({ to: "/settings" })}
            {...stylex.props(styles.railButton)}
          >
            <Icon name="settings" size={18} />
          </button>
        </Tooltip>
      )}
      <Tooltip content={fold} placement="end" delay={400} hasHoverIndication={false}>
        <button
          type="button"
          aria-label={fold}
          onClick={() => onCollapse(!collapsed)}
          {...stylex.props(styles.railButton)}
        >
          <Icon name="sidebar" size={18} />
        </button>
      </Tooltip>
    </nav>
  );
}

function Panel({ items, projects }: { items: SidebarItem[]; projects: ProjectSummary[] }) {
  const { status, features } = useLinkState();
  const { kind } = usePlatform();
  const { id, project, available } = useWorkspace();
  const navigate = useNavigate();
  const newKey = keysFor(NEW_SESSION_KEYS, kind)[0];
  const title = !available ? "Aurelius" : (project?.name ?? ALL_NAME);
  const settingsLabel = id ? "Workspace settings" : "All workspaces settings";
  return (
    <SideNav
      aria-label="Main"
      resizable={{ defaultWidth: 264, minWidth: 200, maxWidth: 440, autoSaveId: "sidebar-panel" }}
      header={
        <div data-sidebar-drag {...stylex.props(styles.head)}>
          <span title={title} {...stylex.props(styles.title)}>
            {title}
          </span>
          <Tooltip content="Search and commands" delay={400} hasHoverIndication={false}>
            <button
              type="button"
              aria-label="Search"
              onClick={() => overlays.open("palette")}
              {...stylex.props(styles.railButton, styles.headButton)}
            >
              <SearchIcon />
            </button>
          </Tooltip>
          {available ? (
            <Tooltip content={settingsLabel} delay={400} hasHoverIndication={false}>
              <button
                type="button"
                aria-label={settingsLabel}
                onClick={() =>
                  void (id ? navigate({ to: "/projects/$id", params: { id } }) : navigate({ to: "/projects" }))
                }
                {...stylex.props(styles.railButton)}
              >
                <Icon name="settings" size={17} />
              </button>
            </Tooltip>
          ) : null}
        </div>
      }
      topContent={
        <>
          <NavRow
            to="/new"
            label="New session"
            icon={NewIcon}
            end={newKey ? <KeyHint>{showChord(newKey, isMac)}</KeyHint> : undefined}
          />
          {offered(shown(NAV), features).map((n) => (
            <NavRow key={n.to} to={n.to} label={n.label} icon={NAV_ICONS[n.icon]} />
          ))}
          <div {...stylex.props(styles.label)}>Sessions</div>
        </>
      }
      footer={
        status !== "online" ? (
          <p role="status" {...stylex.props(styles.reconnect)}>
            Reconnecting…
          </p>
        ) : undefined
      }
    >
      <SessionList items={items} projects={projects} />
    </SideNav>
  );
}

/** The rail, and beside it the panel unless the sidebar is folded to the rail. */
export function Sidebar({ collapsed, onCollapse }: { collapsed: boolean; onCollapse: (collapsed: boolean) => void }) {
  const projects = useProjects();
  const [pins] = useProjectPins();
  const items = useSidebarItems();
  const list = useMemo(() => pinnedFirst(openProjects(projects.data ?? []), pins), [projects.data, pins]);
  const needs = useMemo(() => needsPerProject(items), [items]);
  return (
    <div data-sidebar {...stylex.props(styles.side)}>
      <Rail projects={list} needs={needs} collapsed={collapsed} onCollapse={onCollapse} />
      {collapsed ? null : <Panel items={items} projects={list} />}
    </div>
  );
}
