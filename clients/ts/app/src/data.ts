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
  | "guidelines";

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
      return ["tasks", "notifications", "employees"];
    case "ticket":
      return ["tasks", "notifications", "tickets"];
    case "claude_login":
      return ["subscription", "settings"];
    case "company":
    case "user":
    case "connection":
    case "employee":
      return ["settings", "employees"];
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
