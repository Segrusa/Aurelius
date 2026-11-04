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
  SubscriptionUsage,
  TaskView,
  TicketView,
} from "@aurelius/protocol";
import { type QueryClient, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { useConnection, useLinkState } from "./connection";

const COALESCE_MS = 500;

export type Area =
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
  | "memory"
  | "vault";

/** The areas an event may have changed. An event of a newer server changes nothing here. */
export function areasOf(kind: EventKind | string): Area[] {
  const family = kind.split(".")[0];
  switch (family) {
    case "approval":
      return ["approvals", "tasks", "notifications"];
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
      return ["tasks", "notifications", "employees", "messages", "tickets", "ideas"];
    case "conversation":
    case "message":
    case "chat":
      return ["messages"];
    case "ticket":
      return ["tasks", "notifications", "tickets", "messages"];
    case "claude_login":
      return ["subscription", "settings"];
    case "company":
    case "user":
    case "connection":
      return ["settings", "employees"];
    // A proposed colleague or new instructions arrive as `employee.*` events.
    case "employee":
      return ["settings", "employees", "proposals"];
    case "idea":
      return ["ideas"];
    case "memory":
    case "share":
      return ["memory"];
    case "vault":
      return ["vault"];
    case "recurring_task":
    case "schedule":
      return ["settings"];
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

export function useApprovals() {
  const connection = useConnection();
  return useRead<ApprovalView[]>(
    "approvals",
    async () => expectBody(await connection.call("approval.list"), "approvals").value.approvals,
  );
}

export function useNotifications() {
  const connection = useConnection();
  return useRead<NotificationView[]>(
    "notifications",
    async () => expectBody(await connection.call("notification.list"), "notifications").value.notifications,
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

/** A channel's messages; reading marks it read on the server. `channel` null: general. */
export function useChannelFeed(channel: string | null) {
  const connection = useConnection();
  return useKeyed(
    ["messages", "feed", channel],
    async () => expectBody(await connection.call("group_chat.get", { channel }), "messages").value,
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
