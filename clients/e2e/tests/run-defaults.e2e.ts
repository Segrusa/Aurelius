// One model setting and one label (ticket #93, OPR-5 point 5): on a server with `user_run_defaults` every
// run chip says whose default applies and what it comes to, and Settings has only My defaults. Screenshots
// go to `AURELIUS_SHOTS`, else target/ticket-93.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { boardOf, message, messages, scriptServer, ticket } from "./fake-server";

const SHOTS = process.env.AURELIUS_SHOTS ?? join(REPO, "target/ticket-93");
const NEW_SERVER = [
  "user_run_defaults",
  "run_defaults_of",
  "work_sessions",
  "ticket_discussion",
  "ticket_channels",
  "employee_run_overview",
];
const flux = { id: "e1", name: "Flux", role: "engineer", title: "Frontend Developer", status: "active", agents: [] };
const project = { id: "p", name: "Northwind API", status: "active", created_at_ms: 0, employees_active: 1, version: 1 };
const session = {
  conversation_id: "s1",
  created_at_ms: Date.now() - 3_600_000,
  employee_id: "e1",
  employee_name: "Flux",
  last_activity_ms: Date.now() - 60_000,
  project_id: "p",
  project_name: "Northwind API",
  running: false,
  title: "Why does the login page flash on load?",
};
const mine = {
  defaults: {},
  model: "claude-opus-5-5",
  model_default_effort: "high",
  supported_efforts: ["low", "high"],
};
const kims = {
  defaults: { effort: "low", mode: "plan" },
  model: "claude-sonnet-5-5",
  model_default_effort: "high",
  supported_efforts: ["low", "high"],
};
const own = ticket("t30", 30, "todo", {
  title: "Tidy the login page",
  owner_employee_id: "e1",
  owner_name: "Flux",
  conversation_id: "d30",
});
const kimsTicket = ticket("t31", 31, "todo", {
  title: "Rotate the API keys",
  owner_employee_id: "e1",
  owner_name: "Flux",
  login_user: "u-kim",
  login_username: "Kim",
  created_by_user: "u-kim",
  created_by_username: "Kim",
  conversation_id: "d31",
});
const detail = (t: typeof own) => ({ as_of_seq: 1, ticket: t, root_task: null, tasks: [] });

async function world(page: Page, features: string[]) {
  return scriptServer(page, {
    features,
    projects: [project],
    employees: [flux],
    tickets: [own, kimsTicket],
    board: boardOf([own, kimsTicket]),
    details: { t30: detail(own), t31: detail(kimsTicket) },
    feeds: { d30: messages("d30", []), d31: messages("d31", []) },
    conversations: {
      d30: messages("d30", []),
      d31: messages("d31", []),
      s1: messages("s1", [
        message("m1", "user", "Why does the login page flash on load?", { author_user: { id: "u", username: "Sam" } }),
        message("m2", "assistant", "The theme is applied after the first paint.", { author: "Flux" }),
      ]),
    },
    reads: {
      "user.run_defaults": () => ({ type: "user_run_defaults", value: mine }),
      "user.run_defaults_of": () => ({ type: "user_run_defaults", value: kims }),
      "work_session.list": () => ({ type: "work_sessions", value: { sessions: [session], as_of_seq: 1 } }),
      "connection.list": () => ({
        type: "connections",
        value: {
          as_of_seq: 1,
          defaults: [],
          connections: [
            {
              id: "c1",
              provider: "claude_code",
              display_name: "Claude Code",
              auth_kind: "subscription",
              billing: "subscription",
              status: "ready",
              has_secret: false,
              models: ["claude-opus-5-5", "claude-sonnet-5-5"],
            },
          ],
        },
      }),
    },
  });
}

/** A run chip by its name ("Model"); its text is the name and the value, "ModelYour default · Opus 5.5". */
const chip = (page: Page, name: "Model" | "Effort" | "Mode") => page.getByRole("button", { name, exact: true }).first();

const shot = (page: Page, name: string) => page.screenshot({ path: join(SHOTS, name), animations: "disabled" });

test("every run chip says Your default with its value, Kim's default on Kim's ticket, and the menu links to My defaults", async ({
  page,
}) => {
  mkdirSync(SHOTS, { recursive: true });
  const site = await serveStatic(WEB_DIST);
  try {
    await world(page, NEW_SERVER);
    await page.setViewportSize({ width: 1440, height: 900 });

    // New session: model and effort of My defaults, no bare Default.
    await page.goto(`${site.origin}/new`);
    await expect(chip(page, "Model")).toHaveText("ModelYour default · Opus 5.5");
    await expect(chip(page, "Effort")).toHaveText("EffortYour default · High");
    await chip(page, "Model").click();
    await expect(page.getByRole("menuitem", { name: "Use your default" })).toBeVisible();
    await expect(page.getByRole("menuitem", { name: "Change my defaults" })).toBeVisible();
    await shot(page, "desktop-new-session-model-menu.png");
    await page.getByRole("menuitem", { name: "Sonnet 5.5" }).click();
    await expect(chip(page, "Model")).toHaveText("ModelSonnet 5.5");

    // A work session: the composer chips and the properties.
    await page.goto(`${site.origin}/session/s1`);
    await expect(chip(page, "Model")).toHaveText("ModelYour default · Opus 5.5");
    await expect(page.getByText("Employee's default")).toHaveCount(0);
    await shot(page, "desktop-session-composer-and-properties.png");

    // My own ticket, then Kim's: whose default it is, and Kim's values.
    await page.goto(`${site.origin}/ticket/30`);
    await expect(page.getByRole("complementary", { name: "Ticket properties" })).toContainText(
      "ModelYour default · Opus 5.5EffortYour default · HighModeYour default · Ask",
    );
    await page.getByRole("tab", { name: "Timeline" }).click();
    await expect(chip(page, "Model")).toHaveText("ModelYour default · Opus 5.5");
    await expect(chip(page, "Mode")).toHaveText("ModeYour default · Ask");
    await expect(page.getByText("Owner's default")).toHaveCount(0);
    await shot(page, "desktop-ticket-own.png");
    await page.goto(`${site.origin}/ticket/31`);
    await page.getByRole("tab", { name: "Timeline" }).click();
    await expect(chip(page, "Model")).toHaveText("ModelKim's default · Sonnet 5.5");
    await expect(chip(page, "Effort")).toHaveText("EffortKim's default · Low");
    await chip(page, "Model").click();
    await expect(page.getByRole("menuitem", { name: "Use Kim's default" })).toBeVisible();
    await shot(page, "desktop-ticket-kims.png");

    // "Change my defaults" opens Settings › You › My defaults; Model & effort is gone from the sidebar.
    await page.getByRole("menuitem", { name: "Change my defaults" }).click();
    await expect(page).toHaveURL(/\/settings\/run-defaults$/);
    const nav = page.getByRole("navigation", { name: "Settings" });
    await expect(nav.getByRole("link", { name: "My defaults" })).toBeVisible();
    await expect(nav.getByRole("link", { name: "Model & effort" })).toHaveCount(0);
    await shot(page, "desktop-settings-my-defaults.png");
  } finally {
    await site.close();
  }
});

test("an old server keeps Model & effort and the old labels", async ({ page }) => {
  mkdirSync(SHOTS, { recursive: true });
  const site = await serveStatic(WEB_DIST);
  try {
    await world(page, ["work_sessions", "ticket_discussion", "employee_run_overview"]);
    await page.goto(`${site.origin}/session/s1`);
    await expect(chip(page, "Model")).toHaveText("ModelEmployee's default");
    await page.goto(`${site.origin}/settings/account`);
    const nav = page.getByRole("navigation", { name: "Settings" });
    await expect(nav.getByRole("link", { name: "Model & effort" })).toBeVisible();
    await shot(page, "desktop-old-server-settings.png");
  } finally {
    await site.close();
  }
});
