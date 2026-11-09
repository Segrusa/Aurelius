import { AppShell } from "@astryxdesign/core/AppShell";
import type { IconType } from "@astryxdesign/core/Icon";
import { IconButton } from "@astryxdesign/core/IconButton";
import { SideNav, SideNavItem, SideNavSection } from "@astryxdesign/core/SideNav";
import { Tooltip } from "@astryxdesign/core/Tooltip";
import { expectBody } from "@aurelius/client";
import type { NotificationView, TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import {
  useAllTasks,
  useChannels,
  useEmployeeChats,
  useEmployees,
  useLiveQueries,
  useNotifications,
  useOpenTasks,
  useSubscription,
  useTickets,
} from "../data";
import { type Feature, shown } from "../hidden";
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
import { Logo, NAV_ICONS, type NavIcon, NewIcon, SearchIcon } from "../ui/icons";
import { initial, Mark, markColor } from "../ui/marks";
import { ToastBridge, toast } from "../ui/toast";
import { useWorkspace } from "../workspace";
import { ChatDock } from "./chat-dock";
import { chatBadge } from "./chat-logic";
import { GiveWorkDialog, NewTicketDialog } from "./dialogs";
import { recentChats } from "./employee-logic";
import { pinKey, usePins } from "./employees";
import { useInbox } from "./inbox";
import { needsYouCount, newNotes, notificationHeading, ticketPlace } from "./inbox-logic";
import { overlays } from "./overlays-store";
import { Palette } from "./palette";
import { SettingsSidebar } from "./settings";
import { useTicketNeeds } from "./tickets-kit";
import { subscriptionFor } from "./usage-logic";
import { useOpenedWorkspace, WorkspaceSwitcher } from "./workspace-kit";
import { narrow } from "./workspace-logic";

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
  settingsSidebar: { display: "flex", height: "100%" },
  head: { display: "flex", height: 32, alignItems: "center", gap: 8, paddingInline: 8 },
  logo: { display: "flex" },
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
  group: { display: "flex", flexDirection: "column", gap: 1, marginBlockStart: 14 },
  count: { fontSize: 12, color: "var(--color-text-primary)" },
  dot: { display: "block", width: 6, height: 6, borderRadius: "50%", flexShrink: 0 },
  dotUnread: { backgroundColor: "var(--color-text-secondary)" },
  dotLive: { backgroundColor: "var(--color-text-accent)" },
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

const NAV_MAIN: { to: string; label: string; icon: NavIcon }[] = [
  { to: "/inbox", label: "Inbox", icon: "inbox" },
  { to: "/my-tickets", label: "My tickets", icon: "mine" },
];
const NAV_WORKSPACE: { to: string; label: string; icon: NavIcon; feature?: Feature }[] = [
  { to: "/board", label: "Board", icon: "board" },
  { to: "/tasks", label: "Tasks", icon: "tasks", feature: "TasksInSidebar" },
  { to: "/scheduled", label: "Scheduled", icon: "clock", feature: "ScheduledInSidebar" },
  { to: "/activity", label: "Activity log", icon: "note", feature: "ActivityInSidebar" },
  { to: "/chat", label: "Team chat", icon: "chat" },
  { to: "/employees", label: "Employees", icon: "team" },
  { to: "/knowledge", label: "Knowledge", icon: "know" },
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
  // The list of projects is a tab of Knowledge.
  const on = path === to || path.startsWith(`${to}/`) || (to === "/knowledge" && path.startsWith("/projects"));
  return <SideNavItem label={label} href={to} icon={icon} size="sm" isSelected={on} endContent={end} />;
}

function Counted({ count, dot }: { count?: number; dot?: boolean }) {
  if (count) return <span {...stylex.props(styles.count)}>{count}</span>;
  return dot ? <span role="img" aria-label="Unread" {...stylex.props(styles.dot, styles.dotUnread)} /> : null;
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

/** The board member's latest chats with employees. */
function RecentChats() {
  const path = useRouterState({ select: (s) => s.location.pathname });
  const chats = recentChats(useEmployeeChats().data ?? []);
  if (chats.length === 0) return null;
  return (
    <div {...stylex.props(styles.group)}>
      <SectionLabel>Chats</SectionLabel>
      {chats.map((c) => {
        const to = `/talk/${c.conversation_id}`;
        return (
          <NavRow
            key={c.conversation_id}
            to={to}
            label={c.employee_name}
            icon={markIcon(initial(c.employee_name), markColor(c.employee_id), 15, 4)}
            end={<Counted dot={c.unread > 0 && path !== to} />}
          />
        );
      })}
    </div>
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
  GoDashboard: "/inbox",
  GoMyTickets: "/my-tickets",
  GoBoard: "/board",
  GoTasks: "/tasks",
  GoTeamChat: "/chat",
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
  OpenNotifications: "/inbox",
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
  useAction("NewItem", () => open("give"));
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
  const channels = useChannels();
  const workspace = useWorkspace().id;
  const chat = useMemo(() => chatBadge(narrow(channels.data ?? [], workspace)), [channels.data, workspace]);
  const { open } = overlays;
  const { groups } = useInbox();
  const { hostName, status } = useLinkState();
  const employees = useEmployees();
  const [employeePins] = usePins(pinKey(hostName, "pinned_employees"));
  const pinnedEmployees = employeePins.flatMap(
    (id) => employees.data?.filter((e) => e.id === id && e.status !== "archived") ?? [],
  );
  const inbox = needsYouCount(groups);
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
              <Tooltip content="New ticket" delay={400} hasHoverIndication={false}>
                <IconButton label="New ticket" icon={<NewIcon />} size="sm" onClick={() => open("ticket")} />
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
        <NavRow
          key={n.to}
          to={n.to}
          label={n.label}
          icon={NAV_ICONS[n.icon]}
          end={n.to === "/inbox" && inbox ? <Counted count={inbox} /> : undefined}
        />
      ))}
      <SideNavSection title="Workspace">
        {shown(NAV_WORKSPACE).map((n) => (
          <NavRow
            key={n.to}
            to={n.to}
            label={n.label}
            icon={NAV_ICONS[n.icon]}
            end={n.to === "/chat" ? <Counted count={chat.mentions} dot={chat.unread} /> : undefined}
          />
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
          <RecentChats />
          <Live />
        </>
      )}
    </SideNav>
  );
}

export function Frame({ children }: { children: ReactNode }) {
  useLiveQueries();
  const inSettings = useRouterState({ select: (s) => s.location.pathname.startsWith("/settings") });
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
        <div {...stylex.props(styles.screen)}>{children}</div>
      </AppShell>
      <Palette />
      <NewTicketDialog />
      <GiveWorkDialog />
      <ToastBridge />
      {windowProject === null ? <ChatDock /> : null}
    </>
  );
}
