import { expectBody } from "@aurelius/client";
import type { NotificationView, TaskView } from "@aurelius/protocol";
import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import {
  useAllTasks,
  useChannels,
  useEmployees,
  useLiveQueries,
  useNotifications,
  useOpenTasks,
  useProjects,
  useSubscription,
  useTickets,
} from "../data";
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
import { Icon, Logo, type NavIcon, NewIcon, SearchIcon } from "../ui/icons";
import { initial, Mark, markColor } from "../ui/kit";
import { Tooltip } from "../ui/overlay";
import { ToastBridge, toast } from "../ui/toast";
import { chatBadge } from "./chat-logic";
import { GiveWorkDialog, NewTicketDialog } from "./dialogs";
import { pinnedFirst } from "./employee-logic";
import { pinKey, usePins } from "./employees";
import { useInbox } from "./inbox";
import { needsYouCount, newNotes, notificationHeading, ticketPlace } from "./inbox-logic";
import { overlays } from "./overlays-store";
import { Palette } from "./palette";
import { useProjectPins } from "./projects";
import { SettingsSidebar } from "./settings";
import { useTicketNeeds } from "./tickets-kit";
import { subscriptionFor } from "./usage-logic";

const NAV_MAIN: { to: string; label: string; icon: NavIcon }[] = [
  { to: "/inbox", label: "Inbox", icon: "inbox" },
  { to: "/my-tickets", label: "My tickets", icon: "mine" },
];
const NAV_WORKSPACE: { to: string; label: string; icon: NavIcon }[] = [
  { to: "/board", label: "Board", icon: "board" },
  { to: "/tasks", label: "Tasks", icon: "tasks" },
  { to: "/chat", label: "Team chat", icon: "chat" },
  { to: "/employees", label: "Employees", icon: "team" },
  { to: "/knowledge", label: "Knowledge", icon: "know" },
];

function NavRow({
  to,
  label,
  icon,
  count,
  dot,
}: {
  to: string;
  label: string;
  icon: NavIcon;
  count?: number;
  dot?: boolean;
}) {
  const path = useRouterState({ select: (s) => s.location.pathname });
  const on = path === to || path.startsWith(`${to}/`);
  return (
    <Link
      to={to}
      aria-current={on ? "page" : undefined}
      className={`flex h-7 items-center gap-[9px] rounded-[6px] px-2 font-medium no-underline ${on ? "bg-sel text-text" : "text-text2 hover:bg-sel hover:text-text"}`}
    >
      <Icon name={icon} />
      {label}
      {count ? (
        <span className="ml-auto text-[12px] text-text">{count}</span>
      ) : dot ? (
        <span role="img" aria-label="Unread" className="ml-auto size-[6px] rounded-full bg-text2" />
      ) : null}
    </Link>
  );
}

function SectionLabel({ children }: { children: ReactNode }) {
  return <p className="m-0 mb-1 flex items-center gap-[6px] px-2 text-[12px] font-medium text-text3">{children}</p>;
}

function liveColor(t: TaskView): string {
  if (t.pending_approvals > 0 || t.status === "waiting") return "var(--amber)";
  return t.status === "running" ? "var(--acc-text)" : "var(--text3)";
}

function Live() {
  const tasks = useOpenTasks();
  const live = (tasks.data ?? []).filter((t) => t.live_run != null);
  if (live.length === 0) return null;
  return (
    <div className="flex min-h-0 flex-col gap-px">
      <SectionLabel>
        Live <span className="size-[6px] rounded-full bg-acc-text" /> {live.length}
      </SectionLabel>
      {live.slice(0, 6).map((t) => {
        const who = t.employee_name ?? "Task";
        return (
          <Link
            key={t.id}
            to="/tasks/$taskId"
            params={{ taskId: t.id }}
            className="flex h-7 items-center gap-[9px] rounded-[6px] px-2 text-text no-underline hover:bg-sel"
          >
            <Mark letter={initial(who)} color={markColor(t.employee_id ?? t.id)} />
            <span className="font-medium whitespace-nowrap">{who}</span>
            <span className="min-w-0 truncate text-text3">{ticketPlace(t)}</span>
            <span className="ml-auto size-[6px] flex-none rounded-full" style={{ background: liveColor(t) }} />
          </Link>
        );
      })}
    </div>
  );
}

/** The limit that resets furthest away: the week, when the plan has one. */
function Usage() {
  const { user } = useLinkState();
  const data = subscriptionFor(useSubscription().data, user?.id ?? null);
  const limit = data?.limits.find((l) => /week/i.test(l.label)) ?? data?.limits[0];
  if (!limit) return null;
  const percent = Math.max(0, Math.min(100, Math.round(limit.percent)));
  return (
    <Link
      to="/usage"
      activeProps={{ className: "bg-sel" }}
      className="flex flex-col gap-2 rounded-[6px] px-2 py-1 no-underline hover:bg-sel"
    >
      <div className="flex justify-between text-[12px] text-text3">
        <span className="truncate">{limit.label}</span>
        <span className="text-text">{percent}%</span>
      </div>
      <div className="h-[3px] rounded-[2px] bg-track">
        <div className="h-[3px] rounded-[2px] bg-acc-text" style={{ width: `${percent}%` }} />
      </div>
      {limit.resets ? <div className="truncate text-[12px] text-text3">{limit.resets}</div> : null}
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
  const notes = useNotifications();
  const before = useRef<NotificationView[] | null>(null);
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
  for (const el of document.querySelectorAll("main *")) {
    if (el.scrollHeight <= el.clientHeight + 1 || getComputedStyle(el).overflowY === "visible") continue;
    if (getComputedStyle(el).overflowY === "hidden") continue;
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

function Sidebar() {
  const channels = useChannels();
  const chat = useMemo(() => chatBadge(channels.data ?? []), [channels.data]);
  const { open } = overlays;
  const { groups } = useInbox();
  const projects = useProjects();
  const [projectPins] = useProjectPins();
  const { hostName, status } = useLinkState();
  const employees = useEmployees();
  const [employeePins] = usePins(pinKey(hostName, "pinned_employees"));
  const pinnedEmployees = employeePins.flatMap(
    (id) => employees.data?.filter((e) => e.id === id && e.status !== "archived") ?? [],
  );
  return (
    <aside className="flex w-[224px] flex-none flex-col gap-[18px] overflow-hidden pt-[6px] pr-[10px] pb-[10px] pl-[14px]">
      <div className="flex h-8 items-center gap-2 [-webkit-app-region:drag]">
        <span className="flex" title={hostName ?? "Aurelius"}>
          <Logo />
        </span>
        <div className="ml-auto flex gap-[2px] [-webkit-app-region:no-drag]">
          <Tooltip text="Search and commands">
            <button
              type="button"
              aria-label="Search"
              onClick={() => open("palette")}
              className="grid size-7 cursor-pointer place-items-center rounded-[6px] border-0 bg-transparent text-text2 hover:bg-sel hover:text-text"
            >
              <SearchIcon />
            </button>
          </Tooltip>
          <Tooltip text="New ticket">
            <button
              type="button"
              aria-label="New ticket"
              onClick={() => open("ticket")}
              className="grid size-[26px] cursor-pointer place-items-center rounded-[6px] border border-btn-line bg-btn text-text hover:bg-sel"
            >
              <NewIcon />
            </button>
          </Tooltip>
        </div>
      </div>
      <nav aria-label="Main" className="flex flex-col gap-[14px]">
        <div className="flex flex-col gap-px">
          {NAV_MAIN.map((n) => (
            <NavRow key={n.to} {...n} {...(n.to === "/inbox" ? { count: needsYouCount(groups) } : {})} />
          ))}
        </div>
        <div className="flex flex-col gap-px">
          <SectionLabel>Workspace</SectionLabel>
          {NAV_WORKSPACE.map((n) => (
            <NavRow key={n.to} {...n} {...(n.to === "/chat" ? { count: chat.mentions, dot: chat.unread } : {})} />
          ))}
        </div>
      </nav>
      <div className="flex min-h-0 flex-col gap-px overflow-hidden">
        <Link to="/projects" className="text-text3 no-underline hover:text-text">
          <SectionLabel>Projects</SectionLabel>
        </Link>
        {pinnedFirst(
          (projects.data ?? []).filter((p) => p.status !== "archived"),
          projectPins,
        )
          .slice(0, 8)
          .map((p) => (
            <Link
              key={p.id}
              to="/projects/$id"
              params={{ id: p.id }}
              className="flex h-7 flex-none items-center gap-[9px] rounded-[6px] px-2 font-medium text-text2 no-underline hover:bg-sel hover:text-text"
              activeProps={{ className: "bg-sel text-text" }}
            >
              <Mark letter={initial(p.name)} color={markColor(p.id)} size={15} radius={4} />
              <span className="truncate">{p.name}</span>
            </Link>
          ))}
      </div>
      {pinnedEmployees.length > 0 ? (
        <div className="flex min-h-0 flex-col gap-px overflow-hidden">
          <Link to="/employees" className="text-text3 no-underline hover:text-text">
            <SectionLabel>Employees</SectionLabel>
          </Link>
          {pinnedEmployees.map((e) => (
            <Link
              key={e.id}
              to="/employees/$id"
              params={{ id: e.id }}
              className="flex h-7 flex-none items-center gap-[9px] rounded-[6px] px-2 font-medium text-text2 no-underline hover:text-text"
              activeProps={{ className: "bg-sel text-text" }}
            >
              <Mark letter={initial(e.name)} color={markColor(e.id)} size={15} radius={4} />
              <span className="truncate">{e.name}</span>
            </Link>
          ))}
        </div>
      ) : null}
      <Live />
      <div className="mt-auto flex flex-col gap-2">
        {status !== "online" ? (
          <p role="status" className="m-0 px-2 text-[12px] text-amber">
            Reconnecting…
          </p>
        ) : null}
        <Usage />
        <NavRow to="/settings" label="Settings" icon="settings" />
      </div>
    </aside>
  );
}

export function Frame({ children }: { children: ReactNode }) {
  useLiveQueries();
  const inSettings = useRouterState({ select: (s) => s.location.pathname.startsWith("/settings") });
  useGlobalActions();
  useScreenCounter();
  useUpdateNotice();
  useNoteToasts();
  const { storage, windowProject } = usePlatform();
  const [sidebarHidden, setSidebarHidden] = useState(() => storage.get(SIDEBAR_KEY) === "hidden");
  useAction("ToggleSidebar", () => {
    storage.set(SIDEBAR_KEY, sidebarHidden ? "shown" : "hidden");
    setSidebarHidden(!sidebarHidden);
  });
  return (
    <div className="flex h-full bg-bg py-2 pr-2 text-text">
      {Object.entries(GO_TO).map(([action, to]) => (
        <GoAction key={action} action={action} to={to} />
      ))}
      {sidebarHidden || windowProject !== null ? (
        <div className="w-2 flex-none" />
      ) : inSettings ? (
        <SettingsSidebar />
      ) : (
        <Sidebar />
      )}
      <main className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-[8px] border border-line bg-panel shadow-panel">
        {children}
      </main>
      <Palette />
      <NewTicketDialog />
      <GiveWorkDialog />
      <ToastBridge />
    </div>
  );
}
