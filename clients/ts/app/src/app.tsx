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
import { CodeScreen } from "./screens/code";
import { DesignsScreen } from "./screens/designs";
import { type EmployeeTab, isEmployeeTab } from "./screens/employee-logic";
import { EmployeePage } from "./screens/employee-page";
import { EmployeesScreen, OrgChartScreen } from "./screens/employees";
import { Frame } from "./screens/frame";
import { Gate } from "./screens/gate";
import { GuidelineEditor, GuidelinesScreen } from "./screens/guidelines";
import { HistoryScreen } from "./screens/history";
import { historyFilters } from "./screens/history-logic";
import { IdeasBody } from "./screens/ideas";
import { LinkFollower } from "./screens/links";
import { MemoryBody } from "./screens/memory";
import { NewSessionScreen } from "./screens/new-session";
import { ProjectPage, ProjectsScreen, projectTabShown } from "./screens/projects";
import { ScheduledScreen } from "./screens/scheduled";
import { SessionScreen } from "./screens/session";
import { SettingsScreen } from "./screens/settings";
import { TaskRunScreen } from "./screens/task-run";
import { TasksScreen } from "./screens/tasks";
import { TicketRedirect, TicketSessionScreen } from "./screens/ticket-session";
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
  // The app starts on New session; the Inbox is the bell in every screen's head.
  beforeLoad: () => {
    throw redirect({ to: "/new" });
  },
});
// The Inbox page is gone: an old link to it starts on New session too.
const inbox = createRoute({
  getParentRoute: () => root,
  path: "/inbox",
  beforeLoad: () => {
    throw redirect({ to: "/new" });
  },
});
// The Team chat and the chat with an employee are gone: their old links start on New session.
const formerChat = createRoute({
  getParentRoute: () => root,
  path: "/chat",
  beforeLoad: () => {
    throw redirect({ to: "/new" });
  },
});
const formerTalk = createRoute({
  getParentRoute: () => root,
  path: "/talk/$conversationId",
  beforeLoad: () => {
    throw redirect({ to: "/new" });
  },
});
const newSession = createRoute({ getParentRoute: () => root, path: "/new", component: NewSessionScreen });
const ticketSearch = (search: Record<string, unknown>): { ticket?: string } => ({
  ...(typeof search.ticket === "string" ? { ticket: search.ticket } : {}),
});
const board = createRoute({
  getParentRoute: () => root,
  path: "/board",
  validateSearch: ticketSearch,
  component: function BoardRoute() {
    const { ticket } = board.useSearch();
    // An old link to a ticket on the Board opens its session (BR6); the Board shows while it resolves.
    return (
      <>
        {ticket ? <TicketRedirect ticketId={ticket} /> : null}
        <BoardScreen />
      </>
    );
  },
});
// My tickets is gone (ticket #65); an old link opens its ticket on the Board.
const myTickets = createRoute({
  getParentRoute: () => root,
  path: "/my-tickets",
  validateSearch: ticketSearch,
  beforeLoad: ({ search }) => {
    throw redirect({ to: "/board", search });
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
// A ticket opens as its own session (ticket #70); the number is the one people see on the card.
const ticketSession = createRoute({
  getParentRoute: () => root,
  path: "/ticket/$number",
  validateSearch: (search: Record<string, unknown>): { message?: string; thread?: string } => ({
    ...(typeof search.message === "string" ? { message: search.message } : {}),
    ...(typeof search.thread === "string" ? { thread: search.thread } : {}),
  }),
  component: function TicketSessionRoute() {
    const { number } = ticketSession.useParams();
    const { message, thread } = ticketSession.useSearch();
    const n = Number(number);
    if (!Number.isInteger(n) || n <= 0) {
      return (
        <EmptyState title={`Ticket #${number} not found.`} actions={<Button label="Open the Board" href="/board" />} />
      );
    }
    return <TicketSessionScreen key={n} number={n} message={message ?? null} thread={thread ?? null} />;
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
const workSession = createRoute({
  getParentRoute: () => root,
  path: "/session/$conversationId",
  component: function SessionRoute() {
    const { conversationId } = workSession.useParams();
    return <SessionScreen key={conversationId} conversationId={conversationId} />;
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
    const { ticket, ...filters } = history.useSearch();
    // An old link to a ticket in History opens its session (BR6).
    return (
      <>
        {ticket ? <TicketRedirect ticketId={ticket} /> : null}
        <HistoryScreen filters={filters} />
      </>
    );
  },
});
// The Code tab of the current workspace (ticket #77); the screen says so when the server lacks it.
const code = createRoute({ getParentRoute: () => root, path: "/code", component: CodeScreen });
const usage = createRoute({ getParentRoute: () => root, path: "/usage", component: UsageScreen });
const scheduled = createRoute({ getParentRoute: () => root, path: "/scheduled", component: ScheduledScreen });
// The workspace's designs; the screen says so when the server lacks them.
const designs = createRoute({ getParentRoute: () => root, path: "/designs", component: DesignsScreen });
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
    formerChat,
    formerTalk,
    newSession,
    inbox,
    board,
    myTickets,
    history,
    code,
    tasks,
    taskRun,
    ticketSession,
    workSession,
    settingsIndex,
    settings,
    usage,
    scheduled,
    designs,
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
