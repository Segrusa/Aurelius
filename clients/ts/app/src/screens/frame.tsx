import type { TaskView } from "@aurelius/protocol";
import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import { type ReactNode, useEffect, useRef } from "react";
import { useLinkState } from "../connection";
import { useLiveQueries, useOpenTasks, useProjects, useSubscription } from "../data";
import { useAction } from "../keys/provider";
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
import { Icon, type NavIcon, NewIcon, SearchIcon } from "../ui/icons";
import { initial, Mark, markColor } from "../ui/kit";
import { Tooltip } from "../ui/overlay";
import { Toasts } from "../ui/toast";
import { GiveWorkDialog, NewTicketDialog } from "./dialogs";
import { useInbox } from "./inbox";
import { needsYouCount, ticketPlace } from "./inbox-logic";
import { overlays } from "./overlays-store";
import { Palette } from "./palette";
import { SettingsSidebar } from "./settings";

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

function NavRow({ to, label, icon, count }: { to: string; label: string; icon: NavIcon; count?: number }) {
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
      {count ? <span className="ml-auto text-[12px] text-text">{count}</span> : null}
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
  const { data } = useSubscription();
  const limit = data?.limits.find((l) => /week/i.test(l.label)) ?? data?.limits[0];
  if (!limit) return null;
  const percent = Math.max(0, Math.min(100, Math.round(limit.percent)));
  return (
    <div className="flex flex-col gap-2 px-2">
      <div className="flex justify-between text-[12px] text-text3">
        <span className="truncate">{limit.label}</span>
        <span className="text-text">{percent}%</span>
      </div>
      <div className="h-[3px] rounded-[2px] bg-track">
        <div className="h-[3px] rounded-[2px] bg-acc-text" style={{ width: `${percent}%` }} />
      </div>
      {limit.resets ? <div className="truncate text-[12px] text-text3">{limit.resets}</div> : null}
    </div>
  );
}

const GO_TO: Record<string, string> = {
  GoDashboard: "/inbox",
  GoMyTickets: "/my-tickets",
  GoBoard: "/board",
  GoTasks: "/tasks",
  GoTeamChat: "/chat",
  GoEmployees: "/employees",
  GoGuidelines: "/knowledge",
  OpenSettings: "/settings",
  OpenBackups: "/settings/backups",
  OpenRemote: "/settings/remote",
  OpenServers: "/settings/servers",
};

function GoAction({ action, to }: { action: string; to: string }) {
  const navigate = useNavigate();
  useAction(action, () => void navigate({ to }));
  return null;
}

function useGlobalActions(): void {
  const { open } = overlays;
  useAction("OpenPalette", () => open("palette"));
  useAction("MakeTicket", () => open("ticket"));
  useAction("NewItem", () => open("give"));
  useAction("NavBack", () => window.history.back());
  useAction("NavForward", () => window.history.forward());
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
  const { open } = overlays;
  const { groups } = useInbox();
  const projects = useProjects();
  const { hostName, status } = useLinkState();
  return (
    <aside className="flex w-[224px] flex-none flex-col gap-[18px] overflow-hidden pt-[6px] pr-[10px] pb-[10px] pl-[14px]">
      <div className="flex h-8 items-center gap-2 [-webkit-app-region:drag]">
        <div className="grid size-5 place-items-center rounded-[5px] bg-acc text-[11px] font-semibold text-white">
          A
        </div>
        <span className="truncate font-semibold" title={hostName ?? "Aurelius"}>
          {hostName ?? "Aurelius"}
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
            <NavRow key={n.to} {...n} />
          ))}
        </div>
      </nav>
      <div className="flex min-h-0 flex-col gap-px overflow-hidden">
        <SectionLabel>Projects</SectionLabel>
        {(projects.data ?? []).slice(0, 8).map((p) => (
          <div
            key={p.id}
            className="flex h-7 flex-none items-center gap-[9px] rounded-[6px] px-2 font-medium text-text2"
          >
            <Mark letter={initial(p.name)} color={markColor(p.id)} size={15} radius={4} />
            <span className="truncate">{p.name}</span>
          </div>
        ))}
      </div>
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
  return (
    <div className="flex h-full bg-bg py-2 pr-2 text-text">
      {Object.entries(GO_TO).map(([action, to]) => (
        <GoAction key={action} action={action} to={to} />
      ))}
      {inSettings ? <SettingsSidebar /> : <Sidebar />}
      <main className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-[8px] border border-line bg-panel shadow-panel">
        {children}
      </main>
      <Palette />
      <NewTicketDialog />
      <GiveWorkDialog />
      <Toasts />
    </div>
  );
}
