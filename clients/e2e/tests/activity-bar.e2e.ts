// The status bar's Busy list (OPR-1): a row opens its ticket, task run or work session; a row whose
// screen is gone (a chat that is no work session) is plain text, not a button that leads nowhere.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { scriptServer } from "./fake-server";

const SHOTS = join(REPO, "target/OPR-1");

const busy = (employee_name: string, work: Record<string, unknown>) => ({
  employee_name,
  since_ms: Date.now() - 60_000,
  work,
});

const session = {
  conversation_id: "s1",
  created_at_ms: Date.now() - 3_600_000,
  employee_id: "e1",
  employee_name: "Flux",
  last_activity_ms: Date.now() - 60_000,
  project_id: "p",
  project_name: "Northwind API",
  running: true,
  title: "Tidy the login page",
};

test("a busy row opens its ticket, task run or work session, and a row without a screen is not a link", async ({
  page,
}) => {
  mkdirSync(SHOTS, { recursive: true });
  const site = await serveStatic(WEB_DIST);
  try {
    await scriptServer(page, {
      features: ["activity_overview", "work_sessions", "ticket_channels"],
      channels: [
        {
          conversation_id: "ch7",
          name: "7-fix-login",
          kind: "ticket",
          ticket_id: "t7",
          ticket_number: 7,
          archived: false,
          mentions: 0,
          unread: 0,
        },
      ],
      reads: {
        "activity.overview": () => ({
          type: "activity_overview",
          value: {
            busy: [
              busy("Forge", { label: "#7 Fix login", ticket_id: "t7" }),
              busy("Nexus", { label: "7-fix-login", channel_id: "ch7" }),
              busy("Flux", { label: "Tidy the login page", chat_id: "s1" }),
              busy("Bridge", { label: "Chat with Sam", chat_id: "c-old" }),
              busy("Orbit", { label: "Rotate the keys", task_id: "k1" }),
            ],
            waiting: [],
            tickets: [],
            as_of_seq: 1,
          },
        }),
        "work_session.list": () => ({ type: "work_sessions", value: { sessions: [session], as_of_seq: 1 } }),
      },
    });
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(`${site.origin}/new`);
    const trigger = page.getByRole("button", { name: "5 busy · 0 waiting for a slot" });
    await expect(trigger).toBeVisible();

    const open = async () => {
      await trigger.click();
      await expect(page.getByText("BUSY", { exact: true })).toBeVisible();
    };
    await open();
    // Every row with a place is a button; the chat that is no work session is only text.
    await expect(page.getByRole("button", { name: /^Forge/ })).toBeVisible();
    await expect(page.getByRole("button", { name: /^Nexus/ })).toBeVisible();
    await expect(page.getByRole("button", { name: /^Flux/ })).toBeVisible();
    await expect(page.getByRole("button", { name: /^Orbit/ })).toBeVisible();
    await expect(page.getByText("Chat with Sam")).toBeVisible();
    await expect(page.getByRole("button", { name: /^Bridge/ })).toHaveCount(0);
    await page.waitForLoadState("networkidle");
    await page.screenshot({ path: join(SHOTS, "activity-bar-busy.png"), animations: "disabled" });

    await page.getByRole("button", { name: /^Forge/ }).click();
    await expect(page).toHaveURL(/\/ticket\/7$/);

    await page.goto(`${site.origin}/new`);
    await open();
    await page.getByRole("button", { name: /^Nexus/ }).click();
    await expect(page).toHaveURL(/\/ticket\/7$/);

    await page.goto(`${site.origin}/new`);
    await open();
    await page.getByRole("button", { name: /^Flux/ }).click();
    await expect(page).toHaveURL(/\/session\/s1$/);

    await page.goto(`${site.origin}/new`);
    await open();
    await page.getByRole("button", { name: /^Orbit/ }).click();
    await expect(page).toHaveURL(/\/tasks\/k1$/);
  } finally {
    await site.close();
  }
});
