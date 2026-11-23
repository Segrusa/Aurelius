// The app's sidebar: a rail of workspaces on the left (All workspaces, a tile per workspace this board member
// pinned, plus the current one while it is not pinned, the Inbox
// bell, the account and the fold button), and beside it the workspace's panel with Overview, New session, Board and History, and
// a Sessions and a Tickets section, each under Running, Today and Earlier. Under All workspaces those come per
// workspace. Both sections always show and fold on their own; a closed fold still shows the row you are in.

import type { IconType } from "@astryxdesign/core/Icon";
import { SideNav, SideNavItem } from "@astryxdesign/core/SideNav";
import { Tooltip } from "@astryxdesign/core/Tooltip";
import type { ProjectSummary, TicketView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
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
import { useCodeWorktreeCount } from "./code";
import { CODE_VIEW } from "./code-logic";
import { pinnedFirst } from "./employee-logic";
import { InboxBell } from "./inbox-bell";
import { overlays } from "./overlays-store";
import { useRailPins } from "./projects";
import { type SessionSignal, sessionSignal, startOfDay } from "./session-logic";
import {
  BUCKET_LABELS,
  type Bucket,
  bucketed,
  foldKey,
  isFolded,
  isOpenAt,
  needsPerProject,
  openRows,
  type SidebarBucket,
  type SidebarItem,
  type SidebarSection,
  sectionKey,
  workspaceGroups,
} from "./sidebar-logic";
import { inTicketsSection, ticketSignal } from "./ticket-session-logic";
import { useTicketNeeds } from "./tickets-kit";
import { UserMenu } from "./user-menu";
import { WorkspaceMark } from "./workspace-kit";
import { openProjects, railProjects } from "./workspace-logic";

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
    position: "relative",
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
    // The browser hit-tests the rounded shape, and the radius only shrinks once hovered: a click in a
    // corner of the circle (a quick move, or a click while the radius still animates) hit nothing. This
    // square overlay keeps the whole tile clickable, whatever its radius.
    "::before": { content: '""', position: "absolute", inset: 0 },
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
  // New session is the panel's main action: a filled green button above the nav rows.
  newSession: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    height: 32,
    marginBottom: 6,
    paddingInline: 10,
    borderRadius: 8,
    fontSize: 13,
    fontWeight: 600,
    textDecoration: "none",
    color: "var(--color-on-success)",
    backgroundColor: {
      default: "var(--color-success)",
      ":hover": "color-mix(in srgb, var(--color-success) 88%, black)",
    },
    outline: { default: "none", ":focus-visible": "2px solid var(--color-border-success)" },
    outlineOffset: 2,
  },
  newSessionOn: { boxShadow: "inset 0 0 0 1px rgb(255 255 255 / 0.35)" },
  newSessionLabel: { flex: 1, minWidth: 0 },
  headButtonOn: { color: "var(--color-text-primary)", backgroundColor: "var(--color-overlay-hover)" },
  newTile: { borderWidth: 1, borderStyle: "dashed", borderColor: "var(--color-border-emphasized)" },
  grow: { flexGrow: 1 },
  head: { display: "flex", alignItems: "center", gap: 4, height: 36 },
  titleMark: { display: "flex", flexShrink: 0, paddingInlineStart: 8 },
  titleBeside: { paddingInlineStart: 4 },
  title: {
    flex: 1,
    minWidth: 0,
    paddingInline: 8,
    fontSize: 15,
    fontWeight: 600,
    color: "var(--color-text-primary)",
    whiteSpace: "nowrap",
    overflow: "hidden",
    textOverflow: "ellipsis",
    userSelect: "none",
  },
  // Sessions and Tickets share the panel's height: each open one scrolls on its own, so a long list never
  // pushes the other out of sight. Open sections grow from nothing to at most their content, so a short one
  // keeps all it needs and the long one takes the rest; two long ones split the room.
  sections: { display: "flex", flexDirection: "column", height: "100%", minHeight: 0 },
  section: {
    display: "flex",
    flexDirection: "column",
    flex: "1 1 0",
    minHeight: 0,
    maxHeight: "max-content",
    marginBlockStart: 12,
    paddingBlockStart: 8,
    borderBlockStartWidth: 1,
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--color-border)",
  },
  firstSection: { marginBlockStart: 4 },
  foldedSection: { flex: "0 0 auto" },
  // The list scrolls under its section's head, which stays put.
  sectionScroll: { flex: "1 1 auto", minHeight: 0, overflowY: "auto", overscrollBehavior: "contain" },
  // A section's head reads as a quiet label, but folds the section.
  sectionFold: {
    display: "flex",
    alignItems: "center",
    gap: 6,
    height: 26,
    paddingInline: 10,
    borderWidth: 0,
    borderRadius: 6,
    font: "inherit",
    fontSize: 11,
    fontWeight: 500,
    textAlign: "start",
    cursor: "pointer",
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-secondary)" },
    backgroundColor: "transparent",
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
  doneMark: { display: "flex", flexShrink: 0, color: "var(--precision-text3)" },
  dotWorking: {
    backgroundColor: "var(--color-text-success)",
    animationName: pulse,
    animationDuration: "1.6s",
    animationIterationCount: "infinite",
    animationTimingFunction: "ease-in-out",
  },
  dotRunning: { backgroundColor: "var(--color-text-success)" },
  dotNeedsYou: { backgroundColor: "var(--color-text-error)" },
  dotDone: { backgroundColor: "var(--precision-dot-orange)" },
  empty: { margin: 0, paddingInline: 10, fontSize: 12, color: "var(--precision-text3)" },
  reconnect: { margin: 0, paddingInline: 8, fontSize: 12, color: "var(--color-text-warning)" },
  railReconnect: { margin: 0, display: "flex", justifyContent: "center", width: RAIL_WIDTH, flexShrink: 0 },
  railReconnectDot: {
    width: 8,
    height: 8,
    borderRadius: "50%",
    backgroundColor: "var(--color-text-warning)",
  },
  srOnly: {
    position: "absolute",
    width: 1,
    height: 1,
    overflow: "hidden",
    clipPath: "inset(50%)",
    whiteSpace: "nowrap",
  },
});

const ALL_NAME = "All workspaces";
const NO_PROJECT = "No project";

// `server`: the row shows only when the server has that feature.
const NAV: { to: string; label: string; icon: NavIcon; feature?: Feature; server?: string }[] = [
  { to: "/board", label: "Board", icon: "board" },
  { to: "/code", label: "Code", icon: "code", server: CODE_VIEW },
  { to: "/history", label: "History", icon: "history", server: "ticket_history" },
  { to: "/designs", label: "Designs", icon: "design", server: "designs" },
  { to: "/tasks", label: "Tasks", icon: "tasks", feature: "TasksInSidebar" },
  { to: "/scheduled", label: "Scheduled", icon: "clock", feature: "ScheduledInSidebar" },
  { to: "/activity", label: "Activity log", icon: "note", feature: "ActivityInSidebar" },
];

/** Beside Code: how many worktrees the workspace's repository has besides its folder (none: nothing). */
function CodeCount() {
  const n = useCodeWorktreeCount();
  if (!n) return null;
  return (
    <span title={`${n} ${n === 1 ? "worktree" : "worktrees"}`} {...stylex.props(styles.count)}>
      {n}
    </span>
  );
}

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
  const on = isOpenAt(to, path) || (to === "/board" && path.startsWith("/ticket/"));
  return <SideNavItem label={label} href={to} icon={icon} size="sm" isSelected={on} endContent={end} />;
}

/** The panel's main action, green and on top so a new session is always one obvious click away. */
function NewSessionButton({ hint }: { hint?: string }) {
  const path = useRouterState({ select: (s) => s.location.pathname });
  const on = isOpenAt("/new", path);
  return (
    <Link
      to="/new"
      aria-current={on ? "page" : undefined}
      {...stylex.props(styles.newSession, on && styles.newSessionOn)}
    >
      <NewIcon />
      <span {...stylex.props(styles.newSessionLabel)}>New session</span>
      {hint ? <KeyHint onAccent>{hint}</KeyHint> : null}
    </Link>
  );
}

const SIGNAL_DOTS = {
  working: { text: "Working", style: styles.dotWorking },
  needs_you: { text: "Waiting for you", style: styles.dotNeedsYou },
  done: { text: "Finished, not opened yet", style: styles.dotDone },
} as const;

/**
 * A row's dot: green and breathing while it works, red while it waits for you, orange when it finished an
 * answer you have not opened yet.
 */
function SignalDot({ signal }: { signal: SessionSignal | null }) {
  if (signal === null) return null;
  const { text, style } = SIGNAL_DOTS[signal];
  return (
    <Tooltip content={text} delay={400} hasHoverIndication={false}>
      <span role="img" aria-label={text} {...stylex.props(styles.dot, style)} />
    </Tooltip>
  );
}

/** A ticket on Done: a quiet check where an open row has its dot. */
function DoneMark() {
  return (
    <Tooltip content="Done" delay={400} hasHoverIndication={false}>
      <span role="img" aria-label="Done" {...stylex.props(styles.doneMark)}>
        <Icon name="check" size={12} weight={2} />
      </span>
    </Tooltip>
  );
}

/**
 * Every work session and open or Done ticket session as one list of sidebar rows, of all workspaces: the rail
 * counts per workspace and All workspaces lists them all.
 */
function useSidebarItems(): SidebarItem[] {
  const sessions = useWorkSessions().data;
  const tickets = useTickets().data;
  const need = useTicketNeeds(tickets ?? []);
  return useMemo(() => {
    const signal = (t: TicketView) => ticketSignal(t, need.mine(t), need.rootWaiting(t));
    const fromSessions: SidebarItem[] = (sessions ?? []).map((s) => ({
      key: `s:${s.conversation_id}`,
      section: "sessions",
      to: `/session/${s.conversation_id}`,
      label: s.title || "Session",
      who: s.employee_name,
      whoId: s.employee_id,
      projectId: s.project_id ?? null,
      signal: sessionSignal(s),
      at: s.last_activity_ms,
      ...(s.design ? { design: true } : {}),
    }));
    // Open and Done tickets alike (ticket #75); Earlier caps how many old ones show.
    const fromTickets: SidebarItem[] = (tickets ?? []).filter(inTicketsSection).map((t) => ({
      key: `t:${t.id}`,
      section: "tickets",
      to: `/ticket/${t.number}`,
      label: `#${t.number} ${t.title}`,
      who: t.owner_name ?? "?",
      whoId: t.owner_employee_id ?? t.id,
      projectId: t.project_id ?? null,
      signal: signal(t),
      at: t.updated_at_ms,
      done: t.column === "done",
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

/** The path of the page that is open, for the rows a closed fold still shows. */
function usePath(): string {
  return useRouterState({ select: (s) => s.location.pathname });
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
          icon={i.design ? NAV_ICONS.design : markIcon(initial(i.who), markColor(i.whoId))}
          end={i.done && i.signal === null ? <DoneMark /> : <SignalDot signal={i.signal} />}
        />
      ))}
    </div>
  );
}

/** One of Running, Today and Earlier in a single workspace. */
function BucketFold({ section, entry }: { section: SidebarSection; entry: SidebarBucket }) {
  const [folded, toggle] = useFold(foldKey(undefined, entry.bucket, section), entry.bucket);
  const path = usePath();
  const running = entry.bucket === "running";
  return (
    <div {...stylex.props(styles.group)}>
      <button type="button" aria-expanded={!folded} onClick={toggle} {...stylex.props(styles.fold)}>
        <Chevron folded={folded} size={12} />
        {running ? <span {...stylex.props(styles.dot, styles.dotRunning)} /> : null}
        <span {...stylex.props(styles.foldName)}>{BUCKET_LABELS[entry.bucket]}</span>
        <span {...stylex.props(styles.count)}>{entry.items.length}</span>
      </button>
      <Rows items={folded ? openRows(entry.items, path) : entry.items} />
    </div>
  );
}

/** Running, Today or Earlier inside a workspace under All workspaces. */
function SubFold({
  section,
  workspace,
  entry,
}: {
  section: SidebarSection;
  workspace: string | null;
  entry: SidebarBucket;
}) {
  const [folded, toggle] = useFold(foldKey(workspace, entry.bucket, section), entry.bucket);
  const path = usePath();
  return (
    <div {...stylex.props(styles.group)}>
      <button type="button" aria-expanded={!folded} onClick={toggle} {...stylex.props(styles.fold, styles.subFold)}>
        <Chevron folded={folded} size={10} />
        <span {...stylex.props(entry.bucket === "running" && styles.runningLabel)}>{BUCKET_LABELS[entry.bucket]}</span>
        <span {...stylex.props(styles.count)}>{entry.items.length}</span>
      </button>
      <Rows items={folded ? openRows(entry.items, path) : entry.items} sub />
    </div>
  );
}

function WorkspaceFold({
  section,
  project,
  id,
  buckets,
  count,
  running,
}: {
  section: SidebarSection;
  project: ProjectSummary | null;
  id: string | null;
  buckets: SidebarBucket[];
  count: number;
  running: number;
}) {
  const [folded, toggle] = useFold(foldKey(id, undefined, section));
  const path = usePath();
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
      {folded ? (
        <Rows
          items={openRows(
            buckets.flatMap((b) => b.items),
            path,
          )}
          sub
        />
      ) : (
        buckets.map((b) => <SubFold key={b.bucket} section={section} workspace={id} entry={b} />)
      )}
    </div>
  );
}

const SECTION_TITLES: Record<SidebarSection, string> = { sessions: "Sessions", tickets: "Tickets" };
const SECTION_EMPTY: Record<SidebarSection, string> = { sessions: "No sessions yet", tickets: "No tickets yet" };

/** One section's rows, cut into Running, Today and Earlier, per workspace under All workspaces. */
function SectionList({
  section,
  items,
  projects,
}: {
  section: SidebarSection;
  items: SidebarItem[];
  projects: ProjectSummary[];
}) {
  const workspace = useWorkspace().id;
  const since = startOfDay(Date.now());
  if (workspace !== null) {
    const buckets = bucketed(
      items.filter((i) => i.projectId === workspace),
      since,
    );
    if (buckets.length === 0) return <p {...stylex.props(styles.empty)}>{SECTION_EMPTY[section]}</p>;
    return (
      <div {...stylex.props(styles.list)}>
        {buckets.map((b) => (
          <BucketFold key={b.bucket} section={section} entry={b} />
        ))}
      </div>
    );
  }
  const groups = workspaceGroups(
    items,
    projects.map((p) => p.id),
    since,
  );
  if (groups.length === 0) return <p {...stylex.props(styles.empty)}>{SECTION_EMPTY[section]}</p>;
  return (
    <div {...stylex.props(styles.list)}>
      {groups.map((g) => (
        <WorkspaceFold
          key={g.id ?? ""}
          section={section}
          project={projects.find((p) => p.id === g.id) ?? null}
          {...g}
        />
      ))}
    </div>
  );
}

/** Sessions or Tickets: always shown, even empty, and folded by its head (kept across starts). */
function Section({
  section,
  items,
  projects,
  first,
}: {
  section: SidebarSection;
  items: SidebarItem[];
  projects: ProjectSummary[];
  first?: boolean;
}) {
  const [folded, toggle] = useFold(sectionKey(section));
  const path = usePath();
  const workspace = useWorkspace().id;
  const mine = items.filter((i) => i.section === section);
  // Folded, the section still shows the row you are in, if this workspace lists it.
  const open = folded ? openRows(mine, path).filter((i) => workspace === null || i.projectId === workspace) : [];
  return (
    <section
      aria-label={SECTION_TITLES[section]}
      {...stylex.props(styles.section, first && styles.firstSection, folded && styles.foldedSection)}
    >
      <button type="button" aria-expanded={!folded} onClick={toggle} {...stylex.props(styles.sectionFold)}>
        <Chevron folded={folded} size={10} />
        <span {...stylex.props(styles.foldName)}>{SECTION_TITLES[section]}</span>
      </button>
      {folded ? (
        open.length > 0 ? (
          <Rows items={open} />
        ) : null
      ) : (
        <div {...stylex.props(styles.sectionScroll)}>
          <SectionList section={section} items={mine} projects={projects} />
        </div>
      )}
    </section>
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
  pins,
  needs,
  reconnecting,
}: {
  projects: ProjectSummary[];
  pins: string[];
  needs: Map<string, number>;
  reconnecting: boolean;
}) {
  const { id, available, locked, choose } = useWorkspace();
  const { user } = useLinkState();
  const keys = useKeys();
  const navigate = useNavigate();
  const workspaces = available && !locked;
  // All workspaces counts what waits everywhere, also in workspaces that are not on the rail.
  const allNeeds = [...needs.values()].reduce((n, v) => n + v, 0);
  // A tile opens that workspace's Overview, not the screen you were on (ticket #82).
  const switchTo = (next: string | null) => {
    choose(next);
    void navigate({ to: "/overview", search: {} });
  };
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
              label={[p.name, p.status === "paused" ? "paused" : null, pins.includes(p.id) ? null : "not pinned"]
                .filter(Boolean)
                .join(" · ")}
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
      {reconnecting ? (
        // Without a panel (All workspaces) its footer is not there to say the link dropped.
        <Tooltip content="Reconnecting…" placement="end" delay={300} hasHoverIndication={false}>
          <p role="status" {...stylex.props(styles.railReconnect)}>
            <span aria-hidden {...stylex.props(styles.railReconnectDot)} />
            <span {...stylex.props(styles.srOnly)}>Reconnecting…</span>
          </p>
        </Tooltip>
      ) : null}
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
  const path = usePath();
  const settingsOpen = id ? path === `/projects/${id}` : path === "/projects";
  return (
    <SideNav
      aria-label="Main"
      resizable={{ defaultWidth: 264, minWidth: 200, maxWidth: 440, autoSaveId: "sidebar-panel" }}
      header={
        <div data-sidebar-drag {...stylex.props(styles.head)}>
          {available ? (
            <span {...stylex.props(styles.titleMark)}>
              <WorkspaceMark project={project} size={20} />
            </span>
          ) : null}
          <span title={title} {...stylex.props(styles.title, available && styles.titleBeside)}>
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
                aria-current={settingsOpen ? "page" : undefined}
                onClick={() =>
                  void (id ? navigate({ to: "/projects/$id", params: { id } }) : navigate({ to: "/projects" }))
                }
                {...stylex.props(styles.railButton, styles.headButton, settingsOpen && styles.headButtonOn)}
              >
                <Icon name="settings" size={15} />
              </button>
            </Tooltip>
          ) : null}
        </div>
      }
      topContent={
        <>
          <NewSessionButton hint={newKey ? showChord(newKey, isMac) : undefined} />
          <NavRow to="/overview" label="Overview" icon={NAV_ICONS.home} />
          {offered(shown(NAV), features).map((n) => (
            <NavRow
              key={n.to}
              to={n.to}
              label={n.label}
              icon={NAV_ICONS[n.icon]}
              end={n.to === "/code" ? <CodeCount /> : undefined}
            />
          ))}
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
      <div {...stylex.props(styles.sections)}>
        <Section section="sessions" items={items} projects={projects} first />
        <Section section="tickets" items={items} projects={projects} />
      </div>
    </SideNav>
  );
}

/** The rail, and beside it the panel of the chosen workspace (none under All workspaces). */
export function Sidebar() {
  const projects = useProjects();
  const { pins } = useRailPins();
  const { id, available } = useWorkspace();
  const items = useSidebarItems();
  const open = useMemo(() => openProjects(projects.data ?? []), [projects.data]);
  // The panel groups every workspace, the rail shows only what this board member pinned.
  const list = useMemo(() => pinnedFirst(open, pins), [open, pins]);
  const tiles = useMemo(() => railProjects(pins, open, id), [pins, open, id]);
  const needs = useMemo(() => needsPerProject(items), [items]);
  const { status } = useLinkState();
  // The panel belongs to one workspace; All workspaces is only the list page beside the rail.
  const panel = !(available && id === null);
  return (
    <div data-sidebar {...stylex.props(styles.side)}>
      <Rail projects={tiles} pins={pins} needs={needs} reconnecting={!panel && status !== "online"} />
      {panel ? <Panel items={items} projects={list} /> : null}
    </div>
  );
}
