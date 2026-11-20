import { AppShell } from "@astryxdesign/core/AppShell";
import type { IconType } from "@astryxdesign/core/Icon";
import { IconButton } from "@astryxdesign/core/IconButton";
import { SideNav, SideNavItem, SideNavSection } from "@astryxdesign/core/SideNav";
import { Tooltip } from "@astryxdesign/core/Tooltip";
import { expectBody } from "@aurelius/client";
import type { NotificationView, TaskView, TicketView, WorkSessionView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import {
  useAllTasks,
  useEmployees,
  useLiveQueries,
  useNotifications,
  useOpenTasks,
  useSubscription,
  useTickets,
  useWorkSessions,
} from "../data";
import { type Feature, offered, shown } from "../hidden";
import { useAction, useFallbackAction, useKeys } from "../keys/provider";
import { usePlatform } from "../platform";
import {
  noteScreen,
  parseScreenUse,
  SAVE_DELAY_MS,
  SCREEN_USE_KEY,
  type ScreenUse,
  screenKey,
  screenUseJson,
} from "../screen-use";
import { Icon, Logo, NAV_ICONS, type NavIcon, NewIcon, SearchIcon } from "../ui/icons";
import { initial, Mark, markColor } from "../ui/marks";
import { ToastBridge, toast } from "../ui/toast";
import { useWorkspace } from "../workspace";
import { ActivityBar } from "./activity-bar";
import { GiveWorkDialog, NewTicketDialog } from "./dialogs";
import { pinKey, usePins } from "./employees";
import { openInboxBell } from "./inbox-bell";
import { newNotes, notificationHeading, ticketPlace } from "./inbox-logic";
import { useOpenNewSession } from "./new-session";
import { overlays } from "./overlays-store";
import { Palette } from "./palette";
import {
  activeSessions,
  type SessionSignal,
  sessionSignal,
  sidebarSessions,
  startOfDay,
  withEarlier,
} from "./session-logic";
import { SettingsSidebar } from "./settings";
import { inSettingsArea } from "./settings-logic";
import { sidebarTickets, ticketSignal } from "./ticket-session-logic";
import { useTicketNeeds } from "./tickets-kit";
import { subscriptionFor } from "./usage-logic";
import { useOpenedWorkspace, WorkspaceSwitcher } from "./workspace-kit";
import { narrow } from "./workspace-logic";

// A working session's dot breathes, so it reads as live next to the still unread dots.
const pulse = stylex.keyframes({
  "0%": { opacity: 1, transform: "scale(1)" },
  "50%": { opacity: 0.35, transform: "scale(0.75)" },
  "100%": { opacity: 1, transform: "scale(1)" },
});

const styles = stylex.create({
  // What the screens were written against: a column that fills the shell and scrolls inside.
  screen: {
    display: "flex",
    flexDirection: "column",
    height: "100%",
    minWidth: 0,
    minHeight: 0,
    overflow: "hidden",
  },
  page: {
    display: "flex",
    flexDirection: "column",
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: "0%",
    minWidth: 0,
    minHeight: 0,
    overflow: "hidden",
  },
  settingsSidebar: { display: "flex", height: "100%" },
  head: { display: "flex", height: 32, alignItems: "center", gap: 8, paddingInline: 8 },
  logo: { display: "flex" },
  brand: {
    fontSize: 14,
    fontWeight: 600,
    color: "var(--color-text-primary)",
    whiteSpace: "nowrap",
    overflow: "hidden",
    textOverflow: "ellipsis",
    userSelect: "none",
  },
  headActions: { marginInlineStart: "auto", display: "flex", gap: 2 },
  section: {
    display: "flex",
    alignItems: "center",
    gap: 6,
    margin: 0,
    marginBlockEnd: 4,
    paddingInline: 8,
    fontSize: 12,
    fontWeight: 500,
    color: "var(--precision-text3)",
  },
  sectionLink: { textDecoration: "none", color: "var(--precision-text3)" },
  sectionButton: {
    border: "none",
    background: "none",
    paddingBlock: 0,
    paddingInlineStart: 4,
    font: "inherit",
    fontSize: 12,
    textAlign: "start",
    cursor: "pointer",
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-primary)" },
  },
  chevron: { display: "flex", transition: "transform 120ms ease" },
  chevronFolded: { transform: "rotate(-90deg)" },
  // The "1 working" / "2 need you" count sits at the right end of the section header.
  summary: { display: "flex", alignItems: "center", gap: 6, marginInlineStart: "auto" },
  more: { marginBlockStart: 2, paddingInlineStart: 22, fontWeight: 400 },
  group: { display: "flex", flexDirection: "column", gap: 1, marginBlockStart: 14 },
  dot: { display: "block", width: 6, height: 6, borderRadius: "50%", flexShrink: 0 },
  dotLive: { backgroundColor: "var(--color-text-accent)" },
  dotWorking: {
    width: 8,
    height: 8,
    backgroundColor: "var(--color-text-accent)",
    animationName: pulse,
    animationDuration: "1.6s",
    animationIterationCount: "infinite",
    animationTimingFunction: "ease-in-out",
  },
  dotNeedsYou: { width: 8, height: 8, backgroundColor: "var(--color-text-warning)" },
  place: {
    maxWidth: 90,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: "var(--precision-text3)",
  },
  liveEnd: { display: "flex", alignItems: "center", gap: 8 },
  foot: { display: "flex", flexDirection: "column", gap: 8, paddingInline: 8 },
  reconnect: { margin: 0, paddingInline: 8, fontSize: 12, color: "var(--color-text-warning)" },
  usage: {
    display: "flex",
    flexDirection: "column",
    gap: 8,
    borderRadius: 6,
    paddingInline: 8,
    paddingBlock: 4,
    textDecoration: "none",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
  },
  usageOn: { backgroundColor: "var(--color-overlay-hover)" },
  usageTop: { display: "flex", justifyContent: "space-between", fontSize: 12, color: "var(--precision-text3)" },
  usagePercent: { color: "var(--color-text-primary)" },
  truncate: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  usageResets: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  bar: { height: 3, borderRadius: 2, backgroundColor: "var(--color-track)" },
  barFill: { height: 3, borderRadius: 2, backgroundColor: "var(--color-text-accent)" },
});

const NAV_MAIN: { to: string; label: string; icon: IconType }[] = [{ to: "/new", label: "New session", icon: NewIcon }];
// `server`: the row shows only when the server has that feature.
const NAV_WORKSPACE: { to: string; label: string; icon: NavIcon; feature?: Feature; server?: string }[] = [
  { to: "/board", label: "Board", icon: "board" },
  { to: "/history", label: "History", icon: "history", server: "ticket_history" },
  { to: "/tasks", label: "Tasks", icon: "tasks", feature: "TasksInSidebar" },
  { to: "/scheduled", label: "Scheduled", icon: "clock", feature: "ScheduledInSidebar" },
  { to: "/activity", label: "Activity log", icon: "note", feature: "ActivityInSidebar" },
];

// SideNavItem takes a component: one per mark, kept, or every render would remount the rows' icons.
const MARK_ICONS = new Map<string, IconType>();

function markIcon(letter: string, color: string, size: number, radius: number): IconType {
  const key = `${letter}|${color}|${size}|${radius}`;
  let icon = MARK_ICONS.get(key);
  if (!icon) {
    icon = () => <Mark letter={letter} color={color} size={size} radius={radius} />;
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

function SectionLabel({ children }: { children: ReactNode }) {
  return <p {...stylex.props(styles.section)}>{children}</p>;
}

function liveColor(t: TaskView): string {
  if (t.pending_approvals > 0 || t.status === "waiting") return "var(--color-text-warning)";
  return t.status === "running" ? "var(--color-text-accent)" : "var(--precision-text3)";
}

function Live() {
  const tasks = useOpenTasks();
  const workspace = useWorkspace().id;
  const live = narrow(tasks.data ?? [], workspace).filter((t) => t.live_run != null);
  if (live.length === 0) return null;
  return (
    <div {...stylex.props(styles.group)}>
      <SectionLabel>
        Live <span {...stylex.props(styles.dot, styles.dotLive)} /> {live.length}
      </SectionLabel>
      {live.slice(0, 6).map((t) => {
        const who = t.employee_name ?? "Task";
        return (
          <NavRow
            key={t.id}
            to={`/tasks/${t.id}`}
            label={who}
            icon={markIcon(initial(who), markColor(t.employee_id ?? t.id), 18, 5)}
            end={
              <span {...stylex.props(styles.liveEnd)}>
                <span {...stylex.props(styles.place)}>{ticketPlace(t)}</span>
                <span {...stylex.props(styles.dot)} style={{ background: liveColor(t) }} />
              </span>
            }
          />
        );
      })}
    </div>
  );
}

/** What the end of a session's row shows: a pulsing dot while it works, a warning dot while it needs you. */
function SessionMark({ signal }: { signal: SessionSignal | null }) {
  if (signal === null) return null;
  const working = signal === "working";
  return (
    <Tooltip content={working ? "Working" : "Waiting for your choice"} delay={400} hasHoverIndication={false}>
      <span
        role="img"
        aria-label={working ? "Working" : "Waiting for your choice"}
        {...stylex.props(styles.dot, working ? styles.dotWorking : styles.dotNeedsYou)}
      />
    </Tooltip>
  );
}

/** Whether a sidebar section is folded, kept per section across starts. */
function useFolded(section: string): [boolean, () => void] {
  const { storage } = usePlatform();
  const key = `${SIDEBAR_KEY}.${section}`;
  const [folded, setFolded] = useState(() => storage.get(key) === "folded");
  const toggle = () =>
    setFolded((was) => {
      storage.set(key, was ? "open" : "folded");
      return !was;
    });
  return [folded, toggle];
}

/**
 * A sidebar list that folds under its header. By default it shows today's work and what is active;
 * "Show earlier" brings back the latest ones from before today, for this start only.
 */
function FoldingSection({
  id,
  title,
  summary,
  earlier,
  onEarlier,
  hasEarlier,
  children,
}: {
  id: string;
  title: string;
  summary: ReactNode;
  earlier: boolean;
  onEarlier: () => void;
  hasEarlier: boolean;
  children: ReactNode;
}) {
  const [folded, toggle] = useFolded(id);
  return (
    <div {...stylex.props(styles.group)}>
      <button
        type="button"
        aria-expanded={!folded}
        onClick={toggle}
        {...stylex.props(styles.section, styles.sectionButton)}
      >
        <span {...stylex.props(styles.chevron, folded && styles.chevronFolded)}>
          <Icon name="chevronDown" size={12} />
        </span>
        {title}
        {summary ? <span {...stylex.props(styles.summary)}>{summary}</span> : null}
      </button>
      {folded ? null : (
        <>
          {children}
          {hasEarlier || earlier ? (
            <button
              type="button"
              onClick={onEarlier}
              {...stylex.props(styles.section, styles.sectionButton, styles.more)}
            >
              {earlier ? "Show today only" : "Show earlier"}
            </button>
          ) : null}
        </>
      )}
    </div>
  );
}

/**
 * The board member's work sessions (server feature `work_sessions`), the workspace's when one is chosen.
 * Ordered by their own activity only: opening one must not move it to the top.
 */
function RecentSessions() {
  const workspace = useWorkspace().id;
  const [earlier, setEarlier] = useState(false);
  const all = useWorkSessions().data ?? [];
  const usedAt = (s: WorkSessionView) => s.last_activity_ms;
  const today = sidebarSessions(all, workspace, startOfDay(Date.now()), usedAt);
  const { list: sessions, hasEarlier } = withEarlier(
    sidebarSessions(all, workspace, 0, usedAt),
    today,
    sidebarSessions(all, workspace, null, usedAt),
    (s) => s.conversation_id,
    earlier,
  );
  if (sessions.length === 0 && !hasEarlier) return null;
  const active = activeSessions(sessions);
  return (
    <FoldingSection
      id="sessions"
      title="Sessions"
      earlier={earlier}
      onEarlier={() => setEarlier((e) => !e)}
      hasEarlier={hasEarlier}
      summary={
        active > 0 ? (
          <>
            <span {...stylex.props(styles.dot, styles.dotLive)} /> {active} working
          </>
        ) : null
      }
    >
      {sessions.map((s) => (
        <NavRow
          key={s.conversation_id}
          to={`/session/${s.conversation_id}`}
          label={s.title || "Session"}
          icon={markIcon(initial(s.employee_name), markColor(s.employee_id), 15, 4)}
          end={<SessionMark signal={sessionSignal(s)} />}
        />
      ))}
    </FoldingSection>
  );
}

/**
 * The open tickets (ticket #70: each opens as its own session), the workspace's when one is chosen.
 * Ordered by their own changes only: opening one must not move it to the top.
 */
function OpenTickets() {
  const workspace = useWorkspace().id;
  const [earlier, setEarlier] = useState(false);
  const all = useTickets().data ?? [];
  const need = useTicketNeeds(all);
  const signal = (t: TicketView) => ticketSignal(t, need.mine(t));
  const usedAt = (t: TicketView) => t.updated_at_ms;
  const today = sidebarTickets(all, workspace, signal, startOfDay(Date.now()), usedAt);
  const { list: tickets, hasEarlier } = withEarlier(
    sidebarTickets(all, workspace, signal, 0, usedAt),
    today,
    sidebarTickets(all, workspace, signal, null, usedAt),
    (t) => t.id,
    earlier,
  );
  if (tickets.length === 0 && !hasEarlier) return null;
  const waiting = tickets.filter((t) => signal(t) === "needs_you").length;
  return (
    <FoldingSection
      id="tickets"
      title="Tickets"
      earlier={earlier}
      onEarlier={() => setEarlier((e) => !e)}
      hasEarlier={hasEarlier}
      summary={
        waiting > 0 ? (
          <>
            <span {...stylex.props(styles.dot, styles.dotNeedsYou)} /> {waiting} need you
          </>
        ) : null
      }
    >
      {tickets.map((t) => (
        <NavRow
          key={t.id}
          to={`/ticket/${t.number}`}
          label={`#${t.number} ${t.title}`}
          icon={markIcon(initial(t.owner_name ?? "?"), markColor(t.owner_employee_id ?? t.id), 15, 4)}
          end={<SessionMark signal={signal(t)} />}
        />
      ))}
    </FoldingSection>
  );
}

/** The limit that resets furthest away: the week, when the plan has one. */
function Usage() {
  const { user } = useLinkState();
  const on = useRouterState({ select: (s) => s.location.pathname === "/usage" });
  const data = subscriptionFor(useSubscription().data, user?.id ?? null);
  const limit = data?.limits.find((l) => /week/i.test(l.label)) ?? data?.limits[0];
  if (!limit) return null;
  const percent = Math.max(0, Math.min(100, Math.round(limit.percent)));
  return (
    <Link to="/usage" {...stylex.props(styles.usage, on && styles.usageOn)}>
      <div {...stylex.props(styles.usageTop)}>
        <span {...stylex.props(styles.truncate)}>{limit.label}</span>
        <span {...stylex.props(styles.usagePercent)}>{percent}%</span>
      </div>
      <div {...stylex.props(styles.bar)}>
        <div {...stylex.props(styles.barFill)} style={{ width: `${percent}%` }} />
      </div>
      {limit.resets ? <div {...stylex.props(styles.usageResets)}>{limit.resets}</div> : null}
    </Link>
  );
}

const SIDEBAR_KEY = "sidebar";

const GO_TO: Record<string, string> = {
  GoDashboard: "/new",
  GoBoard: "/board",
  GoTasks: "/tasks",
  GoEmployees: "/employees",
  OpenUsage: "/usage",
  GoOrgChart: "/employees/org",
  GoGuidelines: "/knowledge/guidelines",
  GoIdeas: "/knowledge/ideas",
  GoVault: "/knowledge/vault",
  OpenSettings: "/settings",
  OpenBackups: "/settings/backups",
  OpenRemoteAccess: "/settings/remote",
  GoProjects: "/projects",
  OpenServers: "/settings/servers",
};

function GoAction({ action, to }: { action: string; to: string }) {
  const navigate = useNavigate();
  useAction(action, () => void navigate({ to }));
  return null;
}

/** Says once per start that the app is behind; the service looks every six hours. */
function useUpdateNotice(): void {
  const connection = useConnection();
  const { quit } = usePlatform();
  const { remote } = useLinkState();
  useEffect(() => {
    if (quit === null || remote) return;
    connection
      .call("update.get", { check: false })
      .then((body) => {
        const app = expectBody(body, "update").value.app;
        if (app?.available && app.latest) toast(`App ${app.latest} is available · Ctrl Shift U`);
      })
      .catch(() => undefined);
  }, [connection, quit, remote]);
}

/** Actions that belong to one screen: from elsewhere they go there first, then do it. */
const LIVES_ON: Record<string, string> = {
  NewProject: "/projects",
  AddEmployee: "/employees",
  NewGuideline: "/knowledge/guidelines",
  FindGuideline: "/knowledge/guidelines",
};
const SCREEN_ACTIONS = Object.keys(LIVES_ON);

function useScreenAction(action: string): void {
  const keys = useKeys();
  const navigate = useNavigate();
  useFallbackAction(action, () => {
    void navigate({ to: LIVES_ON[action] as string });
    const started = performance.now();
    const when = () => {
      if (keys.count(action) > 1) keys.run(action);
      else if (performance.now() - started < 2000) requestAnimationFrame(when);
    };
    requestAnimationFrame(when);
  });
}

/** Something new shows for a moment, wherever you are. */
function useNoteToasts(): void {
  // What is in the workspace, as the Inbox shows it; another workspace's news waits there.
  const workspace = useWorkspace().id;
  const notes = useNotifications(workspace);
  const before = useRef<NotificationView[] | null>(null);
  const beforeOf = useRef(workspace);
  const tasks = useAllTasks();
  const tickets = useTickets();
  const need = useTicketNeeds(tickets.data ?? []);
  // A step of someone else's ticket is theirs to hear about; what names you always is yours.
  const noteIsMine = (note: NotificationView) => {
    if (note.reason) return true;
    const task = tasks.data?.find((t) => t.id === note.task_id);
    if (!task?.ticket_id || task.parent_task_id) return true;
    const ticket = tickets.data?.find((t) => t.id === task.ticket_id);
    return !ticket || need.mine(ticket);
  };
  // biome-ignore lint/correctness/useExhaustiveDependencies: only a new list says something new.
  useEffect(() => {
    if (!notes.data) return;
    // Another workspace's list is not news.
    if (beforeOf.current !== workspace) {
      beforeOf.current = workspace;
      before.current = null;
    }
    for (const note of newNotes(before.current, notes.data).slice(0, 3)) {
      // Its conversation is in view: you see it happen.
      if (note.conversation_id && window.location.href.includes(note.conversation_id)) continue;
      if (!noteIsMine(note)) continue;
      toast(notificationHeading(note));
    }
    before.current = notes.data;
  }, [notes.data]);
}

function useGlobalActions(): void {
  for (const action of SCREEN_ACTIONS) {
    // biome-ignore lint/correctness/useHookAtTopLevel: a fixed list: the same hooks in the same order.
    useScreenAction(action);
  }
  const { open } = overlays;
  const platform = usePlatform();
  const connection = useConnection();
  const navigate = useNavigate();
  const openNewSession = useOpenNewSession();
  // The Inbox is the bell in the head: the key opens its dropdown.
  useAction("OpenNotifications", openInboxBell);
  useAction("Quit", () => platform.quit?.(), platform.quit !== null);
  useAction("OpenUpdates", () => {
    connection
      .call("update.get", { check: true })
      .then((body) => {
        const status = expectBody(body, "update").value;
        // Only the server is behind: its page. Otherwise the app's, where its own update is.
        void navigate({
          to: "/settings/$section",
          params: { section: status.available && !status.app?.available ? "servers" : "app" },
        });
      })
      .catch(() => void navigate({ to: "/settings/$section", params: { section: "app" } }));
  });
  useFallbackAction("OpenMemory", () => void navigate({ to: "/knowledge/$tab", params: { tab: "memory" } }));
  useAction("OpenPalette", () => open("palette"));
  useAction("MakeTicket", () => open("ticket"));
  // Pages with their own New (projects, memory, ideas…) answer it first; elsewhere it is a new session.
  useFallbackAction("NewItem", openNewSession);
  useAction("NavBack", () => window.history.back());
  useAction("NavForward", () => window.history.forward());
  useAction("ScrollPageUp", () => scrollMain((el) => el.scrollBy({ top: -el.clientHeight * 0.9 })));
  useAction("ScrollPageDown", () => scrollMain((el) => el.scrollBy({ top: el.clientHeight * 0.9 })));
  useAction("ScrollTop", () => scrollMain((el) => el.scrollTo({ top: 0 })));
  useAction("ScrollBottom", () => scrollMain((el) => el.scrollTo({ top: el.scrollHeight })));
}

/** The biggest list or page that scrolls in the screen now shown. */
function scrollMain(move: (el: Element) => void): void {
  let best: Element | null = null;
  for (const el of document.querySelectorAll('[role="main"], [role="main"] *')) {
    if (el.scrollHeight <= el.clientHeight + 1) continue;
    const { overflowY } = getComputedStyle(el);
    if (overflowY === "visible" || overflowY === "hidden" || overflowY === "clip") continue;
    if (!best || el.clientHeight * el.clientWidth > best.clientHeight * best.clientWidth) best = el;
  }
  if (best) move(best);
}

function useScreenCounter(): void {
  const { storage } = usePlatform();
  const path = useRouterState({ select: (s) => s.location.pathname });
  const state = useRef<{ use: ScreenUse; previous: string | null; timer: ReturnType<typeof setTimeout> | null } | null>(
    null,
  );
  useEffect(() => {
    state.current ??= { use: parseScreenUse(storage.get(SCREEN_USE_KEY)), previous: null, timer: null };
    const counter = state.current;
    const key = screenKey(path);
    if (noteScreen(counter.use, counter.previous, key, Date.now())) {
      counter.timer ??= setTimeout(() => {
        counter.timer = null;
        storage.set(SCREEN_USE_KEY, screenUseJson(counter.use));
      }, SAVE_DELAY_MS);
    }
    counter.previous = key;
  }, [path, storage]);
}

function Sidebar({ collapsed, onCollapse }: { collapsed: boolean; onCollapse: (collapsed: boolean) => void }) {
  const { open } = overlays;
  const { hostName, status, features } = useLinkState();
  const employees = useEmployees();
  const [employeePins] = usePins(pinKey(hostName, "pinned_employees"));
  const pinnedEmployees = employeePins.flatMap(
    (id) => employees.data?.filter((e) => e.id === id && e.status !== "archived") ?? [],
  );
  const openNewSession = useOpenNewSession();
  return (
    <SideNav
      aria-label="Main"
      data-sidebar
      collapsible
      resizable={{ defaultWidth: 224, autoSaveId: "sidebar", isCollapsed: collapsed, onCollapseChange: onCollapse }}
      header={
        <div data-sidebar-drag {...stylex.props(styles.head)}>
          <span title={hostName ?? "Aurelius"} {...stylex.props(styles.logo)}>
            <Logo />
          </span>
          {collapsed ? null : <span {...stylex.props(styles.brand)}>Aurelius</span>}
          {collapsed ? null : (
            <div {...stylex.props(styles.headActions)}>
              <Tooltip content="Search and commands" delay={400} hasHoverIndication={false}>
                <IconButton
                  label="Search"
                  icon={<SearchIcon />}
                  variant="ghost"
                  size="sm"
                  onClick={() => open("palette")}
                />
              </Tooltip>
              <Tooltip content="New session" delay={400} hasHoverIndication={false}>
                <IconButton label="New session" icon={<NewIcon />} size="sm" onClick={openNewSession} />
              </Tooltip>
            </div>
          )}
        </div>
      }
      topContent={<WorkspaceSwitcher collapsed={collapsed} />}
      footer={
        <div {...stylex.props(styles.foot)}>
          {status !== "online" && !collapsed ? (
            <p role="status" {...stylex.props(styles.reconnect)}>
              Reconnecting…
            </p>
          ) : null}
          {collapsed ? null : <Usage />}
          <NavRow to="/settings" label="Settings" icon={NAV_ICONS.settings} />
        </div>
      }
    >
      {NAV_MAIN.map((n) => (
        <NavRow key={n.to} to={n.to} label={n.label} icon={n.icon} />
      ))}
      <SideNavSection title="Workspace">
        {offered(shown(NAV_WORKSPACE), features).map((n) => (
          <NavRow key={n.to} to={n.to} label={n.label} icon={NAV_ICONS[n.icon]} />
        ))}
      </SideNavSection>
      {/* What only fits written out: the rail keeps the rows above. */}
      {collapsed ? null : (
        <>
          {pinnedEmployees.length > 0 ? (
            <div {...stylex.props(styles.group)}>
              <Link to="/employees" {...stylex.props(styles.sectionLink)}>
                <SectionLabel>Employees</SectionLabel>
              </Link>
              {pinnedEmployees.map((e) => (
                <NavRow
                  key={e.id}
                  to={`/employees/${e.id}`}
                  label={e.name}
                  icon={markIcon(initial(e.name), markColor(e.id), 15, 4)}
                />
              ))}
            </div>
          ) : null}
          <RecentSessions />
          <OpenTickets />
          <Live />
        </>
      )}
    </SideNav>
  );
}

export function Frame({ children }: { children: ReactNode }) {
  useLiveQueries();
  // Employees and Knowledge sit under Settings › Company, so they show its sidebar.
  const inSettings = useRouterState({ select: (s) => inSettingsArea(s.location.pathname) });
  useGlobalActions();
  useOpenedWorkspace();
  useScreenCounter();
  useUpdateNotice();
  useNoteToasts();
  const { storage, windowProject } = usePlatform();
  const [sidebarHidden, setSidebarHidden] = useState(() => storage.get(SIDEBAR_KEY) === "hidden");
  const setHidden = (hidden: boolean) => {
    storage.set(SIDEBAR_KEY, hidden ? "hidden" : "shown");
    setSidebarHidden(hidden);
  };
  useAction("ToggleSidebar", () => setHidden(!sidebarHidden));

  // A project window has no sidebar. The app's sidebar folds to a rail; Settings' has no rail to fold to.
  let side: ReactNode = null;
  if (windowProject === null) {
    if (!inSettings) side = <Sidebar collapsed={sidebarHidden} onCollapse={setHidden} />;
    else if (!sidebarHidden) {
      side = (
        <div data-sidebar {...stylex.props(styles.settingsSidebar)}>
          <SettingsSidebar />
        </div>
      );
    }
  }

  return (
    <>
      {Object.entries(GO_TO).map(([action, to]) => (
        <GoAction key={action} action={action} to={to} />
      ))}
      <AppShell variant="section" height="fill" mobileNav={false} {...(side ? { sideNav: side } : {})}>
        <div {...stylex.props(styles.screen)}>
          <div {...stylex.props(styles.page)}>{children}</div>
          <ActivityBar />
        </div>
      </AppShell>
      <Palette />
      <NewTicketDialog />
      <GiveWorkDialog />
      <ToastBridge />
    </>
  );
}
