// A page against a scripted server, for what a test daemon cannot produce on demand: pending
// approvals without a real agent run, and thousands of rows for measuring.

import type { Page } from "@playwright/test";

export interface Scripted {
  approvals?: unknown[];
  notifications?: unknown[];
  tasks?: unknown[];
  projects?: unknown[];
  limits?: unknown[];
  employees?: unknown[];
  tickets?: unknown[];
  guidelines?: unknown[];
  /** A `board.get` answer. */
  board?: unknown;
  details?: Record<string, unknown>;
  conversations?: Record<string, unknown>;
  timeline?: unknown[];
  myTickets?: unknown;
  company?: unknown;
  proposals?: unknown[];
  features?: string[];
  channels?: unknown[];
  /** `group_chat.get` answers by channel; "general" for none. */
  feeds?: Record<string, unknown>;
  threads?: Record<string, unknown>;
  /** Any other answer, by method: the whole response body. */
  reads?: Record<string, (request: Record<string, unknown>) => unknown>;
  /** A write answered with this instead of a receipt, or nothing. */
  refuse?: (request: Record<string, unknown>) => { code: string; message: string; matched_items?: string[] } | null;
  /** Answers for the writes; recorded in `writes`. */
  onWrite?: (request: Record<string, unknown>) => void;
}

export async function scriptServer(
  page: Page,
  script: Scripted,
): Promise<{ writes: Record<string, unknown>[]; push: (frame: unknown) => void }> {
  const writes: Record<string, unknown>[] = [];
  await page.route("**/v1/session", (route) =>
    route.fulfill({ json: { expires_at_ms: Date.now() + 3_600_000, protocol: 5, environment: "dev", host_id: "h" } }),
  );
  let socket: { send: (text: string) => void } | null = null;
  await page.routeWebSocket("**/v1/ws", (ws) => {
    socket = ws;
    ws.onMessage((raw) => {
      const request = JSON.parse(String(raw)) as Record<string, unknown>;
      const answer = (ok: unknown) => ws.send(JSON.stringify({ type: "response", request_id: request.request_id, ok }));
      const read = script.reads?.[request.method as string];
      if (read) return answer(read(request) as never);
      switch (request.method) {
        case "session.hello":
          return answer({
            type: "hello",
            value: {
              host_id: "h",
              daemon_version: "1",
              protocol: 5,
              environment: "dev",
              host_name: "Northwind",
              os: "linux",
              remote: true,
              capabilities: [],
              features: script.features ?? [],
              board_members_exist: true,
              user: { id: "u", username: "Sam", created_at_ms: 0 },
            },
          });
        case "server.get_health":
          return answer({ type: "health", value: { last_seq: 1 } });
        case "event.subscribe":
          return answer({ type: "subscribed" });
        case "approval.list":
          return answer({ type: "approvals", value: { approvals: script.approvals ?? [], as_of_seq: 1 } });
        case "notification.list":
          return answer({ type: "notifications", value: { notifications: script.notifications ?? [], unread: 0 } });
        case "task.list":
          return answer({ type: "tasks", value: { tasks: script.tasks ?? [], as_of_seq: 1 } });
        case "proposal.list":
          return answer({ type: "proposals", value: { proposals: script.proposals ?? [], as_of_seq: 1 } });
        case "guideline.proposals":
          return answer({ type: "guideline_proposals", value: { proposals: [] } });
        case "project.list":
          return answer({ type: "projects", value: { projects: script.projects ?? [], as_of_seq: 1 } });
        case "employee.list":
          return answer({ type: "employees", value: { employees: script.employees ?? [], as_of_seq: 1 } });
        case "ticket.list":
          return answer({ type: "tickets", value: { tickets: script.tickets ?? [], as_of_seq: 1 } });
        case "guideline.list":
          return answer({ type: "guidelines", value: { guidelines: script.guidelines ?? [] } });
        case "board.get":
          return answer({ type: "board", value: script.board ?? { columns: [], as_of_seq: 1 } });
        case "ticket.get":
          return answer({
            type: "ticket",
            value: script.details?.[(request.params as { ticket_id: string }).ticket_id],
          });
        case "conversation.get":
          return answer({
            type: "messages",
            value: script.conversations?.[(request.params as { conversation_id: string }).conversation_id],
          });
        case "timeline.list":
          return answer({ type: "timeline", value: { cards: script.timeline ?? [], next_before_seq: null } });
        case "ticket.my_tickets":
          return answer({ type: "my_tickets", value: script.myTickets ?? { tickets: [], following: [] } });
        case "company.get":
          return answer({ type: "company", value: script.company ?? { company: {}, members: [] } });
        case "run.guidelines":
          return answer({ type: "run_guidelines", value: { guidelines: [], run_id: "r" } });
        case "channel.list":
          return answer({ type: "channels", value: { channels: script.channels ?? [] } });
        case "group_chat.get":
          return answer({
            type: "messages",
            value: script.feeds?.[(request.params as { channel: string | null }).channel ?? "general"],
          });
        case "thread.read":
          return answer({
            type: "thread",
            value: script.threads?.[(request.params as { message_id: string }).message_id],
          });
        case "connection.list":
          return answer({ type: "connections", value: { connections: [] } });
        case "subscription.get":
          return answer({ type: "subscription", value: { fetched_at_ms: 0, limits: script.limits ?? [], raw: "" } });
        default:
          writes.push(request);
          script.onWrite?.(request);
          {
            const refused = script.refuse?.(request);
            if (refused) {
              return ws.send(JSON.stringify({ type: "response", request_id: request.request_id, error: refused }));
            }
          }
          return answer({
            type: "receipt",
            value: {
              idempotency_key: request.idempotency_key,
              committed_seq: 2,
              replayed: false,
              result: { kind: "command", value: {} },
            },
          });
      }
    });
  });
  return {
    writes,
    push: (frame) => socket?.send(JSON.stringify(frame)),
  };
}

export const approval = (id: string, over: Record<string, unknown> = {}) => ({
  id,
  account: "Northwind",
  action_digest: `digest-${id}`,
  arguments: { command: "kubectl -n prod rollout restart deploy/api", description: "Restart the API." },
  effect: "Writes to production",
  expires_at_ms: Date.now() + 3_600_000,
  reasons: [],
  requested_at_ms: Date.now() - 120_000,
  requester: "Flux",
  resources: ["prod/api"],
  run_id: "r",
  status: "pending",
  tool: "claude.Bash",
  always_allow_eligible: true,
  ...over,
});

export const notification = (i: number) => ({
  id: `n${i}`,
  created_at_ms: Date.now() - i * 60_000,
  read: i % 3 === 0,
  reason: i % 4 === 0 ? "review" : "mention",
  needs_you: i % 4 === 0,
  severity: "info",
  subject: `Ticket title number ${i}`,
  target_type: "ticket",
  ticket_number: i,
  ticket_id: `t${i}`,
  title: `Update ${i} on this ticket`,
});

export const COLUMNS = ["backlog", "todo", "doing", "plan_review", "testing", "review", "done"] as const;
const COLUMN_NAMES: Record<string, string> = {
  backlog: "Backlog",
  todo: "Todo",
  doing: "Doing",
  plan_review: "Plan review",
  testing: "Testing",
  review: "In review",
  done: "Done",
};

export const ticket = (id: string, number: number, column: string, over: Record<string, unknown> = {}) => ({
  id,
  number,
  column,
  column_id: `c-${column}`,
  title: `Ticket ${number}`,
  attachments: [],
  created_at_ms: Date.now() - 3_600_000,
  created_by_user: "u",
  created_by_username: "Sam",
  labels: [],
  login_user: "u",
  login_username: "Sam",
  position: number,
  priority: "normal",
  project_id: "p",
  project_name: "Northwind API",
  run: {},
  updated_at_ms: Date.now() - 60_000,
  version: 1,
  ...over,
});

export const boardOf = (tickets: { column: string }[]) => ({
  project_id: null,
  as_of_seq: 1,
  columns: COLUMNS.map((kind, position) => ({
    column: { id: `c-${kind}`, kind, name: COLUMN_NAMES[kind], position, version: 1 },
    tickets: tickets.filter((t) => t.column === kind),
  })),
});

export const task = (id: string, over: Record<string, unknown> = {}) => ({
  id,
  title: `Task ${id}`,
  goal: `The goal of ${id}.`,
  attachments: [],
  children: [],
  created_at_ms: Date.now() - 3_600_000,
  criteria: [],
  depends_on: [],
  employee_id: "e1",
  employee_name: "Flux",
  handoffs: [],
  pending_approvals: 0,
  project_id: "p",
  run: {},
  status: "running",
  updated_at_ms: Date.now() - 60_000,
  ...over,
});

export const messages = (conversationId: string, list: unknown[]) => ({
  agent_mode: "ask",
  conversation_id: conversationId,
  employee_id: "e1",
  messages: list,
  read_only: false,
  run: {},
});

export const message = (id: string, role: string, text: string, over: Record<string, unknown> = {}) => ({
  id,
  role,
  text,
  activity: [],
  created_at_ms: Date.now() - 300_000,
  status: "complete",
  uploads: [],
  ...over,
});

export const channel = (id: string, name: string, over: Record<string, unknown> = {}) => ({
  archived: false,
  conversation_id: id,
  kind: "project",
  last_activity_ms: 0,
  member: true,
  member_count: 3,
  mentions: 0,
  name,
  read_through: null,
  unread: 0,
  ...over,
});
