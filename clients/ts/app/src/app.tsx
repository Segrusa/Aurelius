import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createRootRoute, createRoute, createRouter, Outlet, RouterProvider, redirect } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { type Appearance, AppearanceContext, applyAppearance, readAppearance, saveAppearance } from "./appearance";
import { ConnectionProvider } from "./connection";
import { KeysProvider } from "./keys/provider";
import { type Platform, PlatformProvider } from "./platform";
import { BoardScreen } from "./screens/board";
import { ChatScreen } from "./screens/chat";
import { EmployeePage } from "./screens/employee-page";
import { EmployeesScreen, OrgChartScreen } from "./screens/employees";
import { Frame } from "./screens/frame";
import { Gate } from "./screens/gate";
import { GuidelineEditor, GuidelinesScreen } from "./screens/guidelines";
import { IdeasBody } from "./screens/ideas";
import { InboxScreen } from "./screens/inbox";
import { LinkFollower } from "./screens/links";
import { MemoryBody } from "./screens/memory";
import { MyTicketsScreen } from "./screens/my-tickets";
import { ProjectPage, ProjectsScreen } from "./screens/projects";
import { SettingsScreen } from "./screens/settings";
import { TaskRunScreen } from "./screens/task-run";
import { TasksScreen } from "./screens/tasks";
import { UsageScreen } from "./screens/usage";
import { VaultScreen } from "./screens/vault";
import { Button } from "./ui/kit";
import { TooltipProvider } from "./ui/overlay";

/** A screen of the design that is not built yet: says so, and the phase, instead of looking finished. */

const root = createRootRoute({
  component: () => (
    <LinkFollower>
      <Frame>
        <Outlet />
      </Frame>
    </LinkFollower>
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
const tasks = createRoute({ getParentRoute: () => root, path: "/tasks", component: () => <TasksScreen /> });
const taskRun = createRoute({
  getParentRoute: () => root,
  path: "/tasks/$taskId",
  component: function TaskRunRoute() {
    const { taskId } = taskRun.useParams();
    return <TaskRunScreen key={taskId} taskId={taskId} />;
  },
});
const chat = createRoute({
  getParentRoute: () => root,
  path: "/chat",
  validateSearch: (search: Record<string, unknown>): { channel?: string; thread?: string; message?: string } => ({
    ...(typeof search.channel === "string" ? { channel: search.channel } : {}),
    ...(typeof search.thread === "string" ? { thread: search.thread } : {}),
    ...(typeof search.message === "string" ? { message: search.message } : {}),
  }),
  component: function ChatRoute() {
    const { channel, thread, message } = chat.useSearch();
    return <ChatScreen channel={channel ?? null} thread={thread ?? null} message={message ?? null} />;
  },
});
const employees = createRoute({ getParentRoute: () => root, path: "/employees", component: EmployeesScreen });
const orgChart = createRoute({ getParentRoute: () => root, path: "/employees/org", component: OrgChartScreen });
const employee = createRoute({
  getParentRoute: () => root,
  path: "/employees/$id",
  component: function EmployeeRoute() {
    const { id } = employee.useParams();
    return <EmployeePage key={id} id={id} />;
  },
});
const knowledgeIndex = createRoute({
  getParentRoute: () => root,
  path: "/knowledge",
  beforeLoad: () => {
    throw redirect({ to: "/knowledge/$tab", params: { tab: "guidelines" } });
  },
});
const knowledge = createRoute({
  getParentRoute: () => root,
  path: "/knowledge/$tab",
  validateSearch: (search: Record<string, unknown>): { project?: string } =>
    typeof search.project === "string" ? { project: search.project } : {},
  component: function KnowledgeRoute() {
    const { tab } = knowledge.useParams();
    const { project: start } = knowledge.useSearch();
    if (tab === "memory") return <MemoryBody key={start ?? ""} projectId={null} start={start ?? null} />;
    if (tab === "ideas") return <IdeasBody projectId={null} />;
    if (tab === "vault") return <VaultScreen />;
    return <GuidelinesScreen />;
  },
});
const guideline = createRoute({
  getParentRoute: () => root,
  path: "/knowledge/guidelines/$id",
  validateSearch: (search: Record<string, unknown>): { proposal?: string } =>
    typeof search.proposal === "string" ? { proposal: search.proposal } : {},
  component: function GuidelineRoute() {
    const { id } = guideline.useParams();
    const { proposal } = guideline.useSearch();
    return <GuidelineEditor key={`${id}|${proposal ?? ""}`} id={id} proposalId={proposal ?? null} />;
  },
});
const projectsList = createRoute({ getParentRoute: () => root, path: "/projects", component: ProjectsScreen });
const PROJECT_TABS = ["overview", "board", "tasks", "ideas", "vault"] as const;
const project = createRoute({
  getParentRoute: () => root,
  path: "/projects/$id",
  validateSearch: (search: Record<string, unknown>): { tab?: (typeof PROJECT_TABS)[number] } =>
    PROJECT_TABS.includes(search.tab as (typeof PROJECT_TABS)[number])
      ? { tab: search.tab as (typeof PROJECT_TABS)[number] }
      : {},
  component: function ProjectRoute() {
    const { id } = project.useParams();
    return <ProjectPage key={id} id={id} tab={project.useSearch().tab ?? "overview"} />;
  },
});

const usage = createRoute({ getParentRoute: () => root, path: "/usage", component: UsageScreen });
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

function ScreenFailed({ reset }: { reset: () => void }) {
  return (
    <div className="m-auto flex max-w-[420px] flex-col items-center gap-3 p-10 text-center">
      <p className="m-0 text-[15px] font-semibold">This screen ran into a problem.</p>
      <p className="m-0 text-text2">The rest of the app still works. Try it again, or go to another screen.</p>
      <Button onClick={reset}>Try again</Button>
    </div>
  );
}

const router = createRouter({
  routeTree: root.addChildren([
    index,
    inbox,
    board,
    myTickets,
    tasks,
    taskRun,
    chat,
    settingsIndex,
    settings,
    usage,
    orgChart,
    employee,
    employees,
    knowledgeIndex,
    guideline,
    knowledge,
    projectsList,
    project,
  ]),
  defaultPreload: "intent",
  defaultErrorComponent: ScreenFailed,
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
