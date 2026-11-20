import { AppShell } from "@astryxdesign/core/AppShell";
import { expectBody } from "@aurelius/client";
import type { NotificationView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useNavigate, useRouterState } from "@tanstack/react-router";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useAllTasks, useLiveQueries, useNotifications, useTickets } from "../data";
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
import { ToastBridge, toast } from "../ui/toast";
import { useWorkspace } from "../workspace";
import { ActivityBar } from "./activity-bar";
import { GiveWorkDialog, NewTicketDialog } from "./dialogs";
import { openInboxBell } from "./inbox-bell";
import { newNotes, notificationHeading } from "./inbox-logic";
import { useOpenNewSession } from "./new-session";
import { overlays } from "./overlays-store";
import { Palette } from "./palette";
import { SettingsSidebar } from "./settings";
import { inSettingsArea } from "./settings-logic";
import { Sidebar } from "./sidebar";
import { useTicketNeeds } from "./tickets-kit";
import { useOpenedWorkspace, useSwitchWorkspaceKey } from "./workspace-kit";

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
});

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

export function Frame({ children }: { children: ReactNode }) {
  useLiveQueries();
  // Employees and Knowledge sit under Settings › Company, so they show its sidebar.
  const inSettings = useRouterState({ select: (s) => inSettingsArea(s.location.pathname) });
  useGlobalActions();
  useOpenedWorkspace();
  useSwitchWorkspaceKey();
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
