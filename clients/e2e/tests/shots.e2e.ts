// Pictures of the screens, dark and light, to lay beside the design's boards.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { approval, notification, scriptServer } from "./fake-server";

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
  "board",
  "my-tickets",
  "tasks",
  "chat",
  "employees",
  "employees/org",
  "knowledge/guidelines",
  "projects",
  "usage",
  "settings/app",
];

for (const theme of ["dark", "light"] as const) {
  test(`every screen, ${theme}`, async ({ page }) => {
    const site = await serveStatic(WEB_DIST);
    try {
      await page.addInitScript((t) => localStorage.setItem("aurelius.appearance", t), theme);
      await scriptServer(page, {
        employees: [
          { id: "e1", name: "Flux", role: "engineer", title: "Engineer", status: "active" },
          { id: "e2", name: "Atlas", role: "ceo", title: "CEO", status: "active" },
        ],
        limits: [{ label: "Weekly limit", percent: 58, resets: "Resets Mon 09:00" }],
      });
      const dir = join(REPO, "clients/e2e/test-results/shots");
      mkdirSync(dir, { recursive: true });
      for (const route of ROUTES) {
        await page.goto(`${site.origin}/${route}`);
        await page.getByRole("navigation").first().waitFor();
        await page.evaluate(() => document.fonts.ready);
        await page.waitForTimeout(300);
        const name = route.replaceAll("/", "-");
        await page.screenshot({ path: join(dir, `${name}-${theme}.png`), animations: "disabled" });
      }
    } finally {
      site.close();
    }
  });
}
