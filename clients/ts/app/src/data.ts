import { ACTIVITY_REFRESH_MS, LOAD_REFRESH_MS } from "./screens/activity-bar-logic";
import { ACTIVITY_LIMIT } from "./screens/board-logic";
// Server data: a read per query key, refetched when an event says its area changed. Events come in
// bursts while agents work, so a refetch waits for the burst, as the Rust client did (500 ms).

import { expectBody } from "@aurelius/client";
import type {
  ApprovalView,
  EmployeeSummary,
  EventKind,
  GuidelineProposalView,
  GuidelineView,
  NotificationView,
  ProjectSummary,
  ProposalView,
  RailPinsView,
  SubscriptionUsage,
  TaskView,
  TicketView,
  WorkSessionView,
} from "@aurelius/protocol";
import { type QueryClient, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { useConnection, useLinkState } from "./connection";

const COALESCE_MS = 500;

export type Area =
  | "activity"
  | "approvals"
  | "notifications"
  | "proposals"
  | "guideline_proposals"
  | "projects"
  | "tasks"
  | "subscription"
  | "settings"
  | "employees"
  | "tickets"
  | "guidelines"
  | "messages"
  | "ideas"
  | "usage"
  | "memory"
  | "vault"
  | "scheduled"
  | "rail"
  | "code_review";

/** The areas an event may have changed. An event of a newer server changes nothing here. */
export function areasOf(kind: EventKind | string): Area[] {
  // One board member's own rail: nothing else changed.
  if (kind === "user.rail_pins_changed") return ["rail"];
  // A branch's review in the Code tab (ticket #84): only the review reads it again.
  if (kind === "code.review_changed") return ["code_review"];
  const family = kind.split(".")[0];
  switch (family) {
    case "approval":
      return ["approvals", "tasks", "notifications", "employees", "activity"];
    case "notification":
      return ["notifications"];
    case "proposal":
      return ["proposals", "notifications"];
    case "guideline":
      return ["guideline_proposals", "guidelines"];
    case "project":
      return ["projects"];
    case "task":
    case "run":
      return ["tasks", "notifications", "employees", "messages", "tickets", "ideas", "usage", "scheduled", "activity"];
    case "conversation":
    case "message":
    case "chat":
      return ["messages"];
    case "ticket":
      return ["tasks", "notifications", "tickets", "messages", "activity"];
    case "claude_login":
      return ["subscription", "settings"];
    case "company":
    case "user":
    case "connection":
      return ["settings", "employees"];
    // A proposed colleague or new instructions arrive as `employee.*` events.
    case "employee":
      return ["settings", "employees", "proposals", "activity"];
    case "idea":
      return ["ideas"];
    case "memory":
      return ["memory"];
    case "vault":
      return ["vault"];
    case "recurring_task":
    case "schedule":
      return ["settings", "scheduled"];
    default:
      return [];
  }
}

/** Keeps the queries current: refetches on events and after every reconnect. */
export function useLiveQueries(client: QueryClient = useQueryClient()): void {
  const connection = useConnection();
  useEffect(() => {
    const due = new Set<Area>();
    let timer: ReturnType<typeof setTimeout> | null = null;
    const stop = connection.events.on((event) => {
      for (const area of areasOf(event.kind)) due.add(area);
      if (timer !== null || due.size === 0) return;
      timer = setTimeout(() => {
        timer = null;
        for (const area of due) void client.invalidateQueries({ queryKey: [area] });
        due.clear();
      }, COALESCE_MS);
    });
    return () => {
      stop();
      if (timer !== null) clearTimeout(timer);
    };
  }, [connection, client]);

  const { status, user } = useLinkState();
  const signedIn = status === "online" && user !== null;
  useEffect(() => {
    if (signedIn) void client.invalidateQueries();
  }, [signedIn, client]);
}

function useRead<T>(area: Area, read: () => Promise<T>, staleTime = 30_000, wanted = true) {
  const { status, user } = useLinkState();
  return useQuery({
    queryKey: [area],
    queryFn: read,
    enabled: wanted && status === "online" && user !== null,
    staleTime,
  });
}

/** `project` narrows on the server (feature `workspace_filters`), the only place that knows. */
export function useApprovals(project: string | null = null) {
  const connection = useConnection();
  return useKeyed<ApprovalView[]>(
    ["approvals", project],
    async () =>
      expectBody(await connection.call("approval.list", project ? { project_id: project } : {}), "approvals").value
        .approvals,
  );
}

/** The board member's own rail (feature `rail_pins`); `wanted` false on a server that cannot keep it. */
export function useRailRead(wanted: boolean) {
  const connection = useConnection();
  return useRead<RailPinsView>(
    "rail",
    async () => expectBody(await connection.call("user.rail_pins"), "rail_pins").value,
    30_000,
    wanted,
  );
}

export function useNotifications(project: string | null = null) {
  const connection = useConnection();
  return useKeyed<NotificationView[]>(
    ["notifications", project],
    async () =>
      expectBody(await connection.call("notification.list", project ? { project_id: project } : {}), "notifications")
        .value.notifications,
  );
}

export function useProposals() {
  const connection = useConnection();
  return useRead<ProposalView[]>(
    "proposals",
    async () => expectBody(await connection.call("proposal.list"), "proposals").value.proposals,
  );
}

export function useGuidelineProposals() {
  const connection = useConnection();
  return useRead<GuidelineProposalView[]>(
    "guideline_proposals",
    async () =>
      expectBody(await connection.call("guideline.proposals", { status: "pending" }), "guideline_proposals").value
        .proposals,
  );
}

export function useProjects() {
  const connection = useConnection();
  return useRead<ProjectSummary[]>(
    "projects",
    async () => expectBody(await connection.call("project.list"), "projects").value.projects,
  );
}

export function useOpenTasks() {
  const connection = useConnection();
  return useRead<TaskView[]>(
    "tasks",
    async () => expectBody(await connection.call("task.list", { state: "open" }), "tasks").value.tasks,
  );
}

export function useSubscription() {
  const connection = useConnection();
  return useRead<SubscriptionUsage>(
    "subscription",
    async () => expectBody(await connection.call("subscription.get", { refresh: false }), "subscription").value,
    5 * 60_000,
  );
}

export function useEmployees(wanted = true) {
  const connection = useConnection();
  return useRead<EmployeeSummary[]>(
    "employees",
    async () => expectBody(await connection.call("employee.list"), "employees").value.employees,
    30_000,
    wanted,
  );
}

/** All tickets of all projects: only fetched while something needs them. */
export function useTickets(wanted = true) {
  const connection = useConnection();
  return useRead<TicketView[]>(
    "tickets",
    async () => expectBody(await connection.call("ticket.list", {}), "tickets").value.tickets,
    30_000,
    wanted,
  );
}

export function useGuidelines(wanted = true) {
  const connection = useConnection();
  return useRead<GuidelineView[]>(
    "guidelines",
    async () => expectBody(await connection.call("guideline.list", {}), "guidelines").value.guidelines,
    30_000,
    wanted,
  );
}

/** A read with its own key under an area: an event on the area refreshes it. */
export function useKeyed<T>(key: unknown[], read: () => Promise<T>, wanted = true) {
  const { status, user } = useLinkState();
  return useQuery({
    queryKey: key,
    queryFn: read,
    enabled: wanted && status === "online" && user !== null,
    staleTime: 30_000,
  });
}

/** Who is busy and what waits (ticket #50): events refetch it, and every 30 s besides. */
export function useActivityOverview() {
  const connection = useConnection();
  const { status, user, features } = useLinkState();
  return useQuery({
    queryKey: ["activity"],
    queryFn: async () => expectBody(await connection.call("activity.overview"), "activity_overview").value,
    enabled: status === "online" && user !== null && features.includes("activity_overview"),
    staleTime: 5_000,
    refetchInterval: ACTIVITY_REFRESH_MS,
  });
}

/**
 * The machine's CPU and memory for the status bar. The server pushes `load_changed` only while
 * runs wait, so it is also read on a timer; the key is the one the load settings use.
 */
export function useMachineLoad() {
  const connection = useConnection();
  const client = useQueryClient();
  const { status, user } = useLinkState();
  useEffect(
    () => connection.loads.on((snapshot) => client.setQueryData(["settings", "load"], snapshot)),
    [connection, client],
  );
  return useQuery({
    queryKey: ["settings", "load"],
    queryFn: async () => expectBody(await connection.call("load.get"), "load").value,
    enabled: status === "online" && user !== null,
    staleTime: 2_000,
    refetchInterval: LOAD_REFRESH_MS,
  });
}

export function useBoard(projectId: string | null) {
  const connection = useConnection();
  return useKeyed(
    ["tickets", "board", projectId],
    async () => expectBody(await connection.call("board.get", { project_id: projectId }), "board").value,
  );
}

export function useTicket(id: string | null) {
  const connection = useConnection();
  return useKeyed(
    ["tickets", "detail", id],
    async () => expectBody(await connection.call("ticket.get", { ticket_id: id as string }), "ticket").value,
    id !== null,
  );
}

export function useTicketActivity(id: string | null, limit = ACTIVITY_LIMIT) {
  const connection = useConnection();
  return useKeyed(
    ["tickets", "activity", id, limit],
    async () =>
      expectBody(await connection.call("timeline.list", { correlation_id: id, limit, activity: true }), "timeline")
        .value.cards,
    id !== null,
  );
}

/** The board members, for choosing who is responsible. */
export function useMembers() {
  const connection = useConnection();
  return useKeyed(["users"], async () => expectBody(await connection.call("user.list"), "users").value.users);
}

export function useMyTickets(projectId: string | null, includeDone: boolean, wanted = true) {
  const connection = useConnection();
  return useKeyed(
    ["tickets", "mine", projectId, includeDone],
    async () =>
      expectBody(
        await connection.call("ticket.my_tickets", { project_id: projectId, include_done: includeDone }),
        "my_tickets",
      ).value,
    wanted,
  );
}

export function useMessages(conversationId: string | null | undefined) {
  const connection = useConnection();
  return useKeyed(
    ["messages", conversationId],
    async () =>
      expectBody(await connection.call("conversation.get", { conversation_id: conversationId as string }), "messages")
        .value,
    Boolean(conversationId),
  );
}

/**
 * Several conversations at once, under the same keys and read as `useMessages`, so each shares its
 * cache: a ticket's discussion and its tasks' conversations, for what all its runs wrote.
 */
export function useMessagesOf(conversationIds: string[], wanted = true) {
  const connection = useConnection();
  const { status, user } = useLinkState();
  return useQueries({
    queries: conversationIds.map((id) => ({
      queryKey: ["messages", id],
      queryFn: async () =>
        expectBody(await connection.call("conversation.get", { conversation_id: id }), "messages").value,
      enabled: wanted && status === "online" && user !== null,
      staleTime: 30_000,
    })),
  });
}

/** Every task, ended too. */
export function useAllTasks() {
  const connection = useConnection();
  return useKeyed(
    ["tasks", "all"],
    async () => expectBody(await connection.call("task.list", {}), "tasks").value.tasks,
  );
}

export function useCompany() {
  const connection = useConnection();
  return useKeyed(
    ["settings", "company"],
    async () => expectBody(await connection.call("company.get"), "company").value,
  );
}

export function useRunGuidelines(runId: string | null | undefined) {
  const connection = useConnection();
  return useKeyed(
    ["messages", "guidelines", runId],
    async () =>
      expectBody(await connection.call("run.guidelines", { run_id: runId as string }), "run_guidelines").value,
    Boolean(runId),
  );
}

export function useChannels() {
  const connection = useConnection();
  return useKeyed(
    ["messages", "channels"],
    async () => expectBody(await connection.call("channel.list"), "channels").value.channels,
  );
}

/** The reader's work sessions (server feature `work_sessions`), the latest activity first; `projectId`: one project's. */
export function useWorkSessions(projectId: string | null = null) {
  const connection = useConnection();
  const { features } = useLinkState();
  return useKeyed<WorkSessionView[]>(
    ["messages", "work_sessions", projectId],
    async () =>
      expectBody(
        await connection.call("work_session.list", projectId ? { project_id: projectId } : {}),
        "work_sessions",
      ).value.sessions,
    features.includes("work_sessions"),
  );
}

/** A ticket channel's messages; reading marks it read on the server. Nothing is read without a channel (General is gone). */
export function useChannelFeed(channel: string | null, wanted = true) {
  const connection = useConnection();
  return useKeyed(
    ["messages", "feed", channel],
    async () => expectBody(await connection.call("group_chat.get", { channel }), "messages").value,
    wanted && channel !== null,
  );
}

export function useThread(messageId: string | null) {
  const connection = useConnection();
  return useKeyed(
    ["messages", "thread", messageId],
    async () => expectBody(await connection.call("thread.read", { message_id: messageId as string }), "thread").value,
    messageId !== null,
  );
}

export function useArchivedGuidelines(wanted: boolean) {
  const connection = useConnection();
  return useKeyed(
    ["guidelines", "archived"],
    async () => expectBody(await connection.call("guideline.list", { archived: true }), "guidelines").value.guidelines,
    wanted,
  );
}

export function useGuideline(id: string | null, version: number | null = null) {
  const connection = useConnection();
  return useKeyed(
    ["guidelines", "detail", id, version],
    async () =>
      expectBody(
        await connection.call("guideline.get", {
          guideline_id: id as string,
          ...(version === null ? {} : { version }),
        }),
        "guideline",
      ).value,
    id !== null,
  );
}

export function useGuidelineVersions(id: string | null, wanted: boolean) {
  const connection = useConnection();
  return useKeyed(
    ["guidelines", "versions", id],
    async () =>
      expectBody(await connection.call("guideline.versions", { guideline_id: id as string }), "guideline_versions")
        .value.versions,
    wanted && id !== null,
  );
}

export function useGuidelineAffected(id: string | null) {
  const connection = useConnection();
  return useKeyed(
    ["guidelines", "affected", id],
    async () =>
      expectBody(await connection.call("guideline.affected", { guideline_id: id as string }), "guideline_affected")
        .value,
    id !== null,
  );
}

/** What a run gets per employee and place, for saying a change would not fit. */
export function useEffectiveSets(pairs: [string, string | null][]) {
  const connection = useConnection();
  return useKeyed(
    ["guidelines", "effective", pairs],
    async () => {
      const answers = await Promise.all(
        pairs.map(async ([employee_id, project_id]) => {
          try {
            const set = expectBody(
              await connection.call("guideline.effective", { employee_id, ...(project_id ? { project_id } : {}) }),
              "effective_guidelines",
            ).value;
            return [`${employee_id}|${project_id ?? ""}`, set] as const;
          } catch {
            return null;
          }
        }),
      );
      return new Map(answers.flatMap((a) => (a ? [a] : [])));
    },
    pairs.length > 0,
  );
}

export function useIdeas(projectId: string | null) {
  const connection = useConnection();
  return useKeyed(["ideas", projectId], async () =>
    projectId
      ? expectBody(await connection.call("idea.list", { project_id: projectId }), "ideas").value.ideas
      : expectBody(await connection.call("idea.list_all", {}), "all_ideas").value.ideas,
  );
}

export function useMemory(projectId: string | null) {
  const connection = useConnection();
  return useKeyed(
    ["memory", "list", projectId],
    async () =>
      expectBody(await connection.call("memory.list", { project_id: projectId as string }), "memory").value.items,
    projectId !== null,
  );
}

export function useMemoryContext(projectId: string | null, employeeId: string | null) {
  const connection = useConnection();
  return useKeyed(
    ["memory", "context", projectId, employeeId],
    async () =>
      expectBody(
        await connection.call("memory.get_context", {
          ...(projectId ? { project_id: projectId } : {}),
          ...(employeeId ? { employee_id: employeeId } : {}),
        }),
        "memory_context",
      ).value,
    projectId !== null,
  );
}

/** Never a secret's value: the list holds those only for text and URLs. */
export function useVault(projectId: string | null) {
  const connection = useConnection();
  return useKeyed(
    ["vault", "list", projectId],
    async () =>
      expectBody(await connection.call("vault.list", projectId ? { project_id: projectId } : {}), "vault").value,
  );
}

export function useVaultAudit(projectId: string | null, all: boolean, wanted: boolean) {
  const connection = useConnection();
  return useKeyed(
    ["vault", "audit", projectId, all],
    async () =>
      expectBody(
        await connection.call("vault.audit", { ...(projectId ? { project_id: projectId } : {}), all, limit: 50 }),
        "vault_audit",
      ).value,
    wanted,
  );
}

export function useUsage() {
  const connection = useConnection();
  return useKeyed(["usage", "snapshot"], async () => expectBody(await connection.call("usage.get"), "usage").value);
}
