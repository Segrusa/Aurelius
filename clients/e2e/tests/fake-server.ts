// A page against a scripted server, for what a test daemon cannot produce on demand: pending
// approvals without a real agent run, and thousands of rows for measuring.

import type { Page } from "@playwright/test";

export interface Scripted {
  approvals?: unknown[];
  notifications?: unknown[];
  tasks?: unknown[];
  projects?: unknown[];
  limits?: unknown[];
  /** Answers for the writes; recorded in `writes`. */
  onWrite?: (request: Record<string, unknown>) => void;
}

export async function scriptServer(page: Page, script: Scripted): Promise<{ writes: Record<string, unknown>[] }> {
  const writes: Record<string, unknown>[] = [];
  await page.route("**/v1/session", (route) =>
    route.fulfill({ json: { expires_at_ms: Date.now() + 3_600_000, protocol: 5, environment: "dev", host_id: "h" } }),
  );
  await page.routeWebSocket("**/v1/ws", (ws) => {
    ws.onMessage((raw) => {
      const request = JSON.parse(String(raw)) as Record<string, unknown>;
      const answer = (ok: unknown) => ws.send(JSON.stringify({ type: "response", request_id: request.request_id, ok }));
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
          return answer({ type: "proposals", value: { proposals: [], as_of_seq: 1 } });
        case "guideline.proposals":
          return answer({ type: "guideline_proposals", value: { proposals: [] } });
        case "project.list":
          return answer({ type: "projects", value: { projects: script.projects ?? [], as_of_seq: 1 } });
        case "subscription.get":
          return answer({ type: "subscription", value: { fetched_at_ms: 0, limits: script.limits ?? [], raw: "" } });
        default:
          writes.push(request);
          script.onWrite?.(request);
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
  return { writes };
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
