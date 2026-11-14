import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { LayerProvider } from "@astryxdesign/core/Layer";
import { LinkProvider } from "@astryxdesign/core/Link";
import { Theme } from "@astryxdesign/core/theme";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createRootRoute, createRoute, createRouter, Outlet, RouterProvider, redirect } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import {
  type Appearance,
  AppearanceContext,
  applyAppearance,
  readAppearance,
  saveAppearance,
  useResolvedMode,
} from "./appearance";
import { ConnectionProvider } from "./connection";
import { hidden } from "./hidden";
import { KeysProvider } from "./keys/provider";
import { type Platform, PlatformProvider } from "./platform";
import { activityFilters } from "./screens/activity-logic";
import { BoardScreen } from "./screens/board";
import { ChatScreen } from "./screens/chat";
import { EmployeeChatScreen } from "./screens/employee-chat";
import { type EmployeeTab, isEmployeeTab } from "./screens/employee-logic";
import { EmployeePage } from "./screens/employee-page";
import { EmployeesScreen, OrgChartScreen } from "./screens/employees";
import { Frame } from "./screens/frame";
import { Gate } from "./screens/gate";
import { GuidelineEditor, GuidelinesScreen } from "./screens/guidelines";
import { HistoryScreen } from "./screens/history";
import { historyFilters } from "./screens/history-logic";
import { IdeasBody } from "./screens/ideas";
import { InboxScreen } from "./screens/inbox";
import { LinkFollower } from "./screens/links";
import { MemoryBody } from "./screens/memory";
import { MyTicketsScreen } from "./screens/my-tickets";
import { ProjectPage, ProjectsScreen, projectTabShown } from "./screens/projects";
import { ScheduledScreen } from "./screens/scheduled";
import { SettingsScreen } from "./screens/settings";
import { TaskRunScreen } from "./screens/task-run";
import { TasksScreen } from "./screens/tasks";
import { UsageScreen } from "./screens/usage";
import { VaultScreen } from "./screens/vault";
import { precisionTheme } from "./theme/precision/precision";
import { RouterLink } from "./ui/router-link";
import { useWorkspace, WorkspaceProvider } from "./workspace";

/** A screen of the design that is not built yet: says so, and the phase, instead of looking finished. */

const root = createRootRoute({
  component: () => (
    <WorkspaceProvider>
      <LinkFollower>
        <Frame>
          <Outlet />
        </Frame>
      </LinkFollower>
    </WorkspaceProvider>
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
const ticketSearch = (search: Record<string, unknown>): { ticket?: string; message?: string } => ({
  ...(typeof search.ticket === "string" ? { ticket: search.ticket } : {}),
  ...(typeof search.message === "string" ? { message: search.message } : {}),
});
const board = createRoute({
  getParentRoute: () => root,
  path: "/board",
  validateSearch: ticketSearch,
  component: function BoardRoute() {
    const { ticket, message } = board.useSearch();
    return <BoardScreen ticket={ticket} {...(message ? { message } : {})} />;
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
  validateSearch: (search: Record<string, unknown>): { tab?: EmployeeTab } =>
    isEmployeeTab(search.tab) ? { tab: search.tab } : {},
  component: function EmployeeRoute() {
    const { id } = employee.useParams();
    const { tab } = employee.useSearch();
    return <EmployeePage key={id} id={id} {...(tab ? { tab } : {})} />;
  },
});
const employeeChat = createRoute({
  getParentRoute: () => root,
  path: "/talk/$conversationId",
  component: function EmployeeChatRoute() {
    const { conversationId } = employeeChat.useParams();
    return <EmployeeChatScreen key={conversationId} conversationId={conversationId} />;
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
    const workspace = useWorkspace();
    if (tab === "memory") {
      return <MemoryBody key={workspace.id ?? start ?? ""} projectId={workspace.id} start={start ?? null} />;
    }
    if (tab === "ideas" && !hidden("Ideas")) return <IdeasBody projectId={null} />;
    if (tab === "vault" && !hidden("Vault")) return <VaultScreen />;
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
    PROJECT_TABS.includes(search.tab as (typeof PROJECT_TABS)[number]) && projectTabShown(search.tab as string)
      ? { tab: search.tab as (typeof PROJECT_TABS)[number] }
      : {},
  component: function ProjectRoute() {
    const { id } = project.useParams();
    return <ProjectPage key={id} id={id} tab={project.useSearch().tab ?? "overview"} />;
  },
});

const history = createRoute({
  getParentRoute: () => root,
  path: "/history",
  validateSearch: historyFilters,
  component: function HistoryRoute() {
    return <HistoryScreen filters={history.useSearch()} />;
  },
});
const usage = createRoute({ getParentRoute: () => root, path: "/usage", component: UsageScreen });
const scheduled = createRoute({ getParentRoute: () => root, path: "/scheduled", component: ScheduledScreen });
// The log lives in Settings now; an old link keeps what it narrowed on.
const activity = createRoute({
  getParentRoute: () => root,
  path: "/activity",
  validateSearch: activityFilters,
  beforeLoad: ({ search }) => {
    throw redirect({ to: "/settings/$section", params: { section: "activity" }, search });
  },
});
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
  validateSearch: activityFilters,
  component: function SettingsRoute() {
    return <SettingsScreen section={settings.useParams().section} filters={settings.useSearch()} />;
  },
});

function ScreenFailed({ reset }: { reset: () => void }) {
  return (
    <EmptyState
      title="This screen ran into a problem."
      description="The rest of the app still works. Try it again, or go to another screen."
      actions={<Button label="Try again" onClick={() => reset()} />}
    />
  );
}

const router = createRouter({
  routeTree: root.addChildren([
    index,
    inbox,
    board,
    myTickets,
    history,
    tasks,
    taskRun,
    chat,
    employeeChat,
    settingsIndex,
    settings,
    usage,
    scheduled,
    activity,
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
  const mode = useResolvedMode(appearance);
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
        <Theme theme={precisionTheme} mode={mode}>
          <LayerProvider toast={{ maxVisible: 3, position: "bottomEnd" }}>
            <LinkProvider component={RouterLink}>
              <QueryClientProvider client={client}>
                <ConnectionProvider platform={platform} generation={generation}>
                  <Gate onPaired={() => setGeneration((g) => g + 1)}>
                    <KeysProvider>
                      <RouterProvider router={router} />
                    </KeysProvider>
                  </Gate>
                </ConnectionProvider>
              </QueryClientProvider>
            </LinkProvider>
          </LayerProvider>
        </Theme>
      </AppearanceContext.Provider>
    </PlatformProvider>
  );
}
