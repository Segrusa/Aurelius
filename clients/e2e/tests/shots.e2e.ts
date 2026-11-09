// Pictures of the screens, dark and light, to lay beside the design's boards.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { approval, boardOf, channel, message, messages, notification, scriptServer, task, ticket } from "./fake-server";

for (const theme of ["dark", "light"] as const) {
  test(`the Inbox, ${theme}`, async ({ page }) => {
    const site = await serveStatic(WEB_DIST);
    try {
      await page.addInitScript((t) => localStorage.setItem("aurelius.appearance", t), theme);
      await scriptServer(page, {
        approvals: [
          approval("a"),
          approval("b", {
            requester: "Atlas",
            tool: "Write",
            arguments: { file_path: "src/api/retry.ts" },
            requested_at_ms: Date.now() - 480_000,
          }),
        ],
        notifications: [notification(4), notification(8), notification(5), notification(7), notification(9)],
        tasks: [
          {
            id: "t1",
            title: "Fix token race",
            status: "running",
            live_run: "running",
            employee_name: "Atlas",
            employee_id: "e1",
            ticket_number: 142,
            pending_approvals: 0,
            updated_at_ms: 1,
          },
          {
            id: "t2",
            title: "Rollout",
            status: "running",
            live_run: "running",
            employee_name: "Flux",
            employee_id: "e2",
            ticket_number: 147,
            pending_approvals: 1,
            updated_at_ms: 1,
          },
        ],
        projects: ["Northwind API", "Halcyon iOS", "Website", "Data pipeline"].map((name, i) => ({
          id: `p${i}`,
          name,
          status: "active",
          created_at_ms: 0,
          employees_active: 0,
          version: 1,
        })),
        limits: [{ label: "Weekly limit", percent: 58, resets: "Resets Mon 09:00" }],
      });
      await page.goto(`${site.origin}/inbox`);
      await page.getByRole("heading", { name: "Flux wants to run a command" }).waitFor();
      await page.evaluate(() => document.fonts.ready);
      const dir = join(REPO, "clients/e2e/test-results/shots");
      mkdirSync(dir, { recursive: true });
      await page.screenshot({ path: join(dir, `inbox-${theme}.png`), animations: "disabled" });
    } finally {
      site.close();
    }
  });
}

const ROUTES = [
  "inbox",
  "my-tickets",
  "board",
  "tasks",
  "scheduled",
  "activity",
  "chat",
  "employees",
  "employees/org",
  "employees/e1",
  "knowledge",
  "knowledge/guidelines",
  "projects",
  "projects/p0",
  "usage",
  "settings/account",
  "settings/app",
  "settings/company",
  "settings/servers",
  "settings/remote",
  "settings/connections",
  "settings/mcp",
  "settings/backups",
  "settings/load",
];

const t1 = ticket("t1", 21, "review", {
  title: "Screenshots in review",
  owner_employee_id: "e1",
  owner_name: "Flux",
  root_task_id: "k1",
  root_task_status: "review",
  responsible_user_id: "u",
  responsible_username: "Sam",
  priority: "high",
  labels: ["ui"],
  conversation_id: "c0",
  project_id: "p0",
  project_name: "Northwind API",
});
const t2 = ticket("t2", 22, "doing", {
  title: "Fix token refresh race on every request that fails with a 401 at the same moment",
  owner_employee_id: "e1",
  owner_name: "Flux",
  root_task_id: "k3",
  root_task_status: "running",
  responsible_user_id: "u",
  project_id: "p0",
  project_name: "Northwind API",
});
const t3 = ticket("t3", 23, "backlog", { title: "Rate limit the export", deadline_ms: Date.now() + 86_400_000 });
const t4 = ticket("t4", 24, "done", { title: "Retry the delivery", done_at_ms: Date.now() - 3_600_000 });
const tickets = [t1, t2, t3, t4];
const k1 = task("k1", {
  status: "review",
  ticket_id: "t1",
  ticket_number: 21,
  result: "All screens have a **screenshot** now.\n\n- Board\n- Task run",
  conversation_id: "c1",
  title: "Screenshots in review",
  employee_id: "e1",
  employee_name: "Flux",
});
const k3 = task("k3", {
  status: "running",
  ticket_id: "t2",
  ticket_number: 22,
  conversation_id: "c3",
  live_run: "running",
  title: "Fix token refresh race",
  employee_id: "e1",
  employee_name: "Flux",
});
const flux = {
  id: "e1",
  name: "Flux",
  role: "engineer",
  title: "Engineer",
  status: "active",
  working: true,
  activity_state: "working",
  agent_mode: "ask",
  agents: [],
  conversations: [],
  projects_active: ["p0"],
  version: 1,
};
const atlas = {
  ...flux,
  id: "e2",
  name: "Atlas",
  role: "ceo",
  title: "CEO",
  working: false,
  activity_state: undefined,
};
const projects = ["Northwind API", "Halcyon iOS", "Website", "Data pipeline"].map((name, i) => ({
  id: `p${i}`,
  name,
  status: "active",
  folder: `/srv/${i}`,
  created_at_ms: 0,
  employees_active: 1,
  version: 1,
}));

for (const theme of ["dark", "light"] as const) {
  test(`every screen, ${theme}`, async ({ page }) => {
    test.setTimeout(180_000);
    const site = await serveStatic(WEB_DIST);
    try {
      await page.addInitScript((t) => localStorage.setItem("aurelius.appearance", t), theme);
      await scriptServer(page, {
        features: [
          "my_tickets",
          "ticket_responsible",
          "ticket_follow",
          "ticket_discussion",
          "board_testing",
          "employee_queue",
          "activity_log",
          "workspace_filters",
          "team_channels",
          "ticket_channels",
          "chat_threads",
          "guidelines",
          "vault",
        ],
        employees: [flux, atlas],
        projects,
        tickets,
        board: boardOf(tickets),
        tasks: [k1, k3],
        myTickets: { tickets: [t1, t2], following: [] },
        details: {
          t1: { ticket: t1, root_task: k1, tasks: [k1], as_of_seq: 1 },
          t2: { ticket: t2, root_task: k3, tasks: [k3], as_of_seq: 1 },
        },
        conversations: {
          c0: messages("c0", [
            message("d1", "user", "Is the dark theme included?", { author_user: { username: "Sam" } }),
            message("d2", "assistant", "Yes, both themes.", { author: "Flux" }),
          ]),
          c1: messages("c1", []),
          c3: messages("c3", []),
        },
        channels: [channel("g", "general", { kind: "general" })],
        feeds: { general: messages("g", [message("m1", "assistant", "All **green** on the rollout.")]) },
        limits: [{ label: "Weekly limit", percent: 58, resets: "Mon 09:00" }],
        company: {
          company: { name: "Northwind", tester_employee_id: null },
          employees: [],
          roots: [],
          members: [],
        },
        guidelines: [
          {
            id: "g1",
            title: "Write Rust the way the crates do",
            chars: 18,
            archived: false,
            links: [],
            open_proposals: 0,
            version: 3,
            updated_at_ms: 1_700_000_000_000,
            updated_by: { id: "u1", username: "sam" },
          },
        ],
        reads: {
          "employee.queue": () => ({
            type: "employee_queue",
            value: {
              as_of_seq: 1,
              employee_id: "e1",
              items: [
                {
                  id: "i1",
                  kind: "task",
                  source: "board",
                  state: "claimed",
                  thread_key: "ticket:t2",
                  place: { label: "#22 Fix token refresh race", ticket_id: "t2" },
                  run_id: "r1",
                  hop: 0,
                  created_at_ms: 1,
                  not_before_ms: 1,
                },
              ],
            },
          }),
          "memory.list": () => ({ type: "memory", value: { items: [] } }),
          "connection.list": () => ({ type: "connections", value: { connections: [], defaults: [] } }),
        },
      });
      const dir = join(REPO, "clients/e2e/test-results/shots");
      mkdirSync(dir, { recursive: true });
      const shoot = async (name: string) => {
        await page.evaluate(() => document.fonts.ready);
        await page.waitForTimeout(300);
        await page.screenshot({ path: join(dir, `${name}-${theme}.png`), animations: "disabled" });
      };
      for (const route of ROUTES) {
        await page.goto(`${site.origin}/${route}`);
        await page.getByRole("navigation").first().waitFor();
        await shoot(route.replaceAll("/", "-"));
      }
      await page.goto(`${site.origin}/board?ticket=t1`);
      await page.getByText("In review · your decision").waitFor();
      await shoot("ticket-panel");
      await page.goto(`${site.origin}/board`);
      await page.getByRole("button", { name: /^Workspace: / }).click();
      await page.getByRole("menuitem", { name: "Northwind API" }).waitFor();
      await shoot("workspace-switcher");
    } finally {
      site.close();
    }
  });
}
