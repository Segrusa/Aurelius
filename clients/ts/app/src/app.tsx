import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createRootRoute, createRoute, createRouter, Outlet, RouterProvider, redirect } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { type Appearance, AppearanceContext, applyAppearance, readAppearance, saveAppearance } from "./appearance";
import { ConnectionProvider } from "./connection";
import { KeysProvider } from "./keys/provider";
import { type Platform, PlatformProvider } from "./platform";
import { BoardScreen } from "./screens/board";
import { Frame } from "./screens/frame";
import { Gate } from "./screens/gate";
import { InboxScreen } from "./screens/inbox";
import { MyTicketsScreen } from "./screens/my-tickets";
import { SettingsScreen } from "./screens/settings";
import { TaskRunScreen } from "./screens/task-run";
import { TasksScreen } from "./screens/tasks";
import { TooltipProvider } from "./ui/overlay";

/** A screen of the design that is not built yet: says so, and the phase, instead of looking finished. */
function NotYet({ title }: { title: string }) {
  return (
    <>
      <header className="flex h-11 flex-none items-center border-b border-line pl-[18px]">
        <h1 className="m-0 text-[13px] font-medium">{title}</h1>
      </header>
      <p className="m-0 p-[18px] text-text3">
        This screen is not in the new client yet. Use the desktop app for it for now.
      </p>
    </>
  );
}

const root = createRootRoute({
  component: () => (
    <Frame>
      <Outlet />
    </Frame>
  ),
});
const index = createRoute({
  getParentRoute: () => root,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/inbox" });
  },
});
const inbox = createRoute({ getParentRoute: () => root, path: "/inbox", component: InboxScreen });
const ticketSearch = (search: Record<string, unknown>): { ticket?: string } =>
  typeof search.ticket === "string" ? { ticket: search.ticket } : {};
const board = createRoute({
  getParentRoute: () => root,
  path: "/board",
  validateSearch: ticketSearch,
  component: function BoardRoute() {
    return <BoardScreen ticket={board.useSearch().ticket} />;
  },
});
const myTickets = createRoute({
  getParentRoute: () => root,
  path: "/my-tickets",
  validateSearch: ticketSearch,
  component: function MyTicketsRoute() {
    return <MyTicketsScreen ticket={myTickets.useSearch().ticket} />;
  },
});
const tasks = createRoute({ getParentRoute: () => root, path: "/tasks", component: TasksScreen });
const taskRun = createRoute({
  getParentRoute: () => root,
  path: "/tasks/$taskId",
  component: function TaskRunRoute() {
    const { taskId } = taskRun.useParams();
    return <TaskRunScreen key={taskId} taskId={taskId} />;
  },
});
const others = (
  [
    ["/chat", "Team chat"],
    ["/employees", "Employees"],
    ["/knowledge", "Knowledge"],
  ] as const
).map(([path, title]) => createRoute({ getParentRoute: () => root, path, component: () => <NotYet title={title} /> }));

const settingsIndex = createRoute({
  getParentRoute: () => root,
  path: "/settings",
  beforeLoad: () => {
    throw redirect({ to: "/settings/$section", params: { section: "account" } });
  },
});
const settings = createRoute({
  getParentRoute: () => root,
  path: "/settings/$section",
  component: function SettingsRoute() {
    return <SettingsScreen section={settings.useParams().section} />;
  },
});

const router = createRouter({
  routeTree: root.addChildren([index, inbox, board, myTickets, tasks, taskRun, settingsIndex, settings, ...others]),
  defaultPreload: "intent",
});

export function App({ platform }: { platform: Platform }) {
  const client = useMemo(
    () => new QueryClient({ defaultOptions: { queries: { retry: 1, refetchOnWindowFocus: false } } }),
    [],
  );
  const [generation, setGeneration] = useState(0);
  const [appearance, setAppearance] = useState<Appearance>(() => readAppearance(platform.storage));
  useEffect(() => applyAppearance(appearance), [appearance]);
  const look = useMemo(
    () => ({
      appearance,
      choose: (next: Appearance) => {
        saveAppearance(platform.storage, next);
        setAppearance(next);
      },
    }),
    [appearance, platform],
  );
  return (
    <PlatformProvider value={platform}>
      <AppearanceContext.Provider value={look}>
        <QueryClientProvider client={client}>
          <ConnectionProvider platform={platform} generation={generation}>
            <Gate onPaired={() => setGeneration((g) => g + 1)}>
              <KeysProvider>
                <TooltipProvider>
                  <RouterProvider router={router} />
                </TooltipProvider>
              </KeysProvider>
            </Gate>
          </ConnectionProvider>
        </QueryClientProvider>
      </AppearanceContext.Provider>
    </PlatformProvider>
  );
}
