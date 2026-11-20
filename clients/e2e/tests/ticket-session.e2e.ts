// A ticket as a session (ticket #70): the flows of user-stories.md §9 against a scripted server, and
// the screenshots of ui-ontwerp.md §12 (into `AURELIUS_SHOTS`, else target/ticket-70).

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { boardOf, channel, message, messages, scriptServer, task, ticket } from "./fake-server";

const SHOTS = process.env.AURELIUS_SHOTS ?? join(REPO, "target/ticket-70");
const FEATURES = [
  "ticket_channels",
  "ticket_discussion",
  "chat_threads",
  "quick_replies",
  "ticket_follow",
  "ticket_responsible",
  "run_cards",
];
const pixel = { id: "e1", name: "Pixel", role: "designer", title: "UI/UX Designer", status: "active", agents: [] };
const flux = { id: "e2", name: "Flux", role: "engineer", title: "Frontend Developer", status: "active", agents: [] };
const project = { id: "p", name: "Aurelius", status: "active", created_at_ms: 0, employees_active: 2, version: 1 };

const owned = {
  owner_employee_id: "e1",
  owner_name: "Pixel",
  root_task_id: "k70",
  conversation_id: "d70",
  responsible_user_id: "u",
  responsible_username: "Sam",
  project_name: "Aurelius",
  labels: ["ui", "desktop"],
  description: "Every ticket opens as its own session.",
};
const ticketChannel = (id: string, number: number, column: string, over: Record<string, unknown> = {}) =>
  channel(id, `#${number}-ticket`, {
    kind: "ticket",
    ticket_id: `t${number}`,
    ticket_number: number,
    ticket_column: column,
    ...over,
  });
const usage = {
  type: "usage_totals",
  value: {
    own: { input_tokens: 0, output_tokens: 0, cached_tokens: 0, cache_creation_tokens: 0, estimate: true, runs: 0 },
    subtasks: [],
    total: {
      input_tokens: 182_000,
      output_tokens: 24_000,
      cached_tokens: 1_200_000,
      cache_creation_tokens: 41_000,
      cost_micro_usd: 3_120_000,
      estimate: true,
      runs: 6,
    },
  },
};

type Ticket = ReturnType<typeof ticket> & { conversation_id?: string };

interface World {
  tickets: Record<string, Ticket>;
  feeds: Record<string, ReturnType<typeof messages>>;
}

function scripted(world: World) {
  const detail = (id: string) => {
    const t = world.tickets[id];
    if (!t) return undefined;
    return {
      as_of_seq: 1,
      ticket: { ...t, root_task_id: `k${t.number}` },
      root_task: task(`k${t.number}`, {
        title: t.title,
        employee_id: "e1",
        employee_name: "Pixel",
        conversation_id: `c${t.number}`,
        ticket_id: t.id,
        result: t.column === "review" ? "## Report\n\nThe session screen is built.\n\n- head\n- gate\n- panel" : null,
        status: t.column === "done" ? "succeeded" : t.column === "review" ? "review" : "running",
      }),
      tasks: [
        task(`k${t.number}`, {
          title: t.title,
          employee_name: "Pixel",
          conversation_id: `c${t.number}`,
          ticket_id: t.id,
          status: t.column === "done" ? "succeeded" : t.column === "review" ? "review" : "running",
        }),
        task(`s${t.number}`, {
          title: "Build the head",
          employee_id: "e2",
          employee_name: "Flux",
          parent_task_id: `k${t.number}`,
          conversation_id: `cs${t.number}`,
          ticket_id: t.id,
          status: "succeeded",
        }),
      ],
    };
  };
  const tickets = () => Object.values(world.tickets);
  return {
    features: FEATURES,
    employees: [pixel, flux],
    projects: [project],
    company: { company: {}, members: [] },
    get board() {
      return boardOf(tickets());
    },
    get tickets() {
      return tickets();
    },
    channels: [
      ...tickets().map((t) =>
        ticketChannel(t.conversation_id as string, t.number, t.column, {
          archived: t.column === "done",
          last_activity_ms: t.number,
        }),
      ),
    ],
    feeds: world.feeds,
    conversations: {
      c70: messages("c70", [
        message("w1", "assistant", "Wrote the head.", {
          run_id: "r1",
          created_at_ms: Date.now() - 200_000,
          activity: [{ tool: "claude.Write", input: { file_path: "screens/ticket-session.tsx" }, ok: true }],
        }),
      ]),
      cs70: messages("cs70", [
        message("w2", "assistant", "Wrote the logic.", {
          run_id: "r2",
          created_at_ms: Date.now() - 100_000,
          activity: [
            { tool: "claude.Write", input: { file_path: "screens/ticket-session-logic.ts" }, ok: true },
            { tool: "claude.Edit", input: { file_path: "screens/ticket-session.tsx" }, ok: true },
          ],
        }),
      ]),
      c71: messages("c71", [message("pl", "assistant", "1. First the head.\n2. Then the gate.", { update: "plan" })]),
    },
    get details() {
      return Object.fromEntries(Object.keys(world.tickets).map((id) => [id, detail(id)]));
    },
    reads: {
      "usage.get_totals": () => usage,
      "ticket.followers": () => ({ type: "ticket_followers", value: { followers: [] } }),
      "user.list": () => ({ type: "users", value: { users: [{ id: "u", username: "Sam", created_at_ms: 0 }] } }),
      "activity.overview": () => ({ type: "activity_overview", value: { tickets: [], employees: [], as_of_seq: 1 } }),
      "ticket.list": (request: Record<string, unknown>) => {
        const query = (request.params as { query?: string | null }).query ?? "";
        const n = /^#(\d+)$/.exec(query)?.[1];
        const list = n ? tickets().filter((t) => t.number === Number(n)) : tickets();
        return { type: "tickets", value: { tickets: list, as_of_seq: 1 } };
      },
    },
  };
}

/** The script with these parts added; never spread: its getters read the world at each request. */
const plus = <T extends object>(script: T, more: Record<string, unknown>): T => Object.assign(script, more);

const feedOf = (id: string, list: unknown[]) => messages(id, list);
const sam = { author_user: { id: "u", username: "Sam", created_at_ms: 0 } };
const box = (page: Page) => page.getByRole("textbox", { name: /^Message/ });

test.describe("a ticket's session", () => {
  let site: { origin: string; close: () => void };
  test.beforeAll(async () => {
    site = await serveStatic(WEB_DIST);
  });
  test.afterAll(() => site.close());

  test("1. creating a ticket opens its session", async ({ page }) => {
    const world: World = { tickets: {}, feeds: {} };
    const script = scripted(world);
    await scriptServer(
      page,
      plus(script, {
        results: {
          "ticket.create": () => ({ kind: "command", value: { type: "ticket_created", ticket_id: "t80" } }),
        },
        onWrite: (w: Record<string, unknown>) => {
          if (w.method !== "ticket.create") return;
          world.tickets.t80 = ticket("t80", 80, "todo", { ...owned, conversation_id: "d80", title: "From the dialog" });
          world.feeds.d80 = feedOf("d80", []);
          script.channels.push(ticketChannel("d80", 80, "todo"));
        },
      }),
    );
    await page.goto(`${site.origin}/new`);
    await expect(page.getByRole("heading", { level: 1, name: "New session" })).toBeVisible();
    await page.keyboard.press("Alt+t");
    await page.getByLabel("Title").fill("From the dialog");
    await page.keyboard.press("Control+Enter");
    await expect(page).toHaveURL(/\/ticket\/80$/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("#80");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("From the dialog");
    await expect(box(page)).toHaveAttribute("placeholder", "Message Pixel (owner)… · @ to mention");
  });

  test("2. a message without a name gets the owner's answer in the timeline", async ({ page }) => {
    const world: World = {
      tickets: { t70: ticket("t70", 70, "doing", { ...owned, title: "A ticket opens as its own session" }) },
      feeds: { d70: feedOf("d70", [message("m1", "user", "Design first, then Flux builds.", sam)]) },
    };
    const { writes } = await scriptServer(
      page,
      plus(scripted(world), {
        onWrite: (w: Record<string, unknown>) => {
          if (w.method !== "group_chat.send") return;
          const text = (w.params as { text: string }).text;
          world.feeds.d70 = feedOf("d70", [
            ...(world.feeds.d70?.messages ?? []),
            message("m2", "user", text, sam),
            message("m3", "assistant", "On it: the head comes first.", {
              author: "Pixel",
            }),
          ]);
        },
      }),
    );
    await page.goto(`${site.origin}/ticket/70`);
    await expect(page.getByText("Design first, then Flux builds.")).toBeVisible();
    await expect(page.getByText("@Name asks that employee; without a name, Pixel (owner) answers.")).toBeVisible();
    await box(page).fill("How far is the head?");
    await box(page).press("Enter");
    await expect
      .poll(() => writes.find((w) => w.method === "group_chat.send")?.params)
      .toMatchObject({
        text: "How far is the head?",
        channel: "d70",
        thread_parent: null,
      });
    await expect(page.getByText("On it: the head comes first.")).toBeVisible();
    // The ticket's data is beside the talk.
    const aside = page.getByRole("complementary", { name: "Ticket properties" });
    await expect(aside.getByText("ui, desktop")).toBeVisible();
    await aside.getByRole("button", { name: /Files written/ }).click();
    await expect(aside.getByText("screens/ticket-session-logic.ts")).toBeVisible();
    await expect(aside.getByText("Guidelines in the runs")).toHaveCount(0);
    await aside.getByRole("button", { name: "Usage" }).hover();
    await expect(page.getByRole("tooltip")).toContainText("182k in · 24k out");
  });

  test("3. Review → Accept → the session is read-only", async ({ page }) => {
    const world: World = {
      tickets: {
        t70: ticket("t70", 70, "review", {
          ...owned,
          title: "A ticket opens as its own session",
          root_task_status: "review",
        }),
      },
      feeds: { d70: feedOf("d70", [message("m1", "assistant", "Ready for review.", { author: "Pixel" })]) },
    };
    const { writes } = await scriptServer(
      page,
      plus(scripted(world), {
        onWrite: (w: Record<string, unknown>) => {
          if (w.method !== "ticket.accept") return;
          world.tickets.t70 = { ...(world.tickets.t70 as Ticket), column: "done", version: 2 };
        },
      }),
    );
    await page.goto(`${site.origin}/ticket/70`);
    const gate = page.getByRole("region", { name: "Your decision" });
    await expect(gate.getByText("In review · your decision")).toBeVisible();
    await expect(gate.getByText("The session screen is built.")).toBeVisible();
    // The composer under it is the reply; the timeline is the discussion.
    await expect(gate.getByRole("button", { name: /^Reply to/ })).toHaveCount(0);
    await expect(gate.getByRole("button", { name: "Open discussion" })).toHaveCount(0);
    await gate.getByRole("button", { name: /^Mark done|^Accept/ }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "ticket.accept")?.params)
      .toMatchObject({ ticket_id: "t70" });
    await expect(page.getByText("#70 is Done · read-only.")).toBeVisible();
    await expect(box(page)).toHaveCount(0);
    await expect(page.getByRole("region", { name: "Your decision" })).toHaveCount(0);
    await expect(page.getByText("Ready for review.")).toBeVisible();
    await expect(page.getByRole("button", { name: "Add files…" })).toHaveCount(0);
  });

  test("4. old /board?ticket= and /history?ticket= links open the session", async ({ page }) => {
    const world: World = {
      tickets: { t70: ticket("t70", 70, "doing", { ...owned, title: "A ticket opens as its own session" }) },
      feeds: { d70: feedOf("d70", [message("m1", "user", "Linked message.", sam)]) },
    };
    await scriptServer(page, scripted(world));
    await page.goto(`${site.origin}/board?ticket=t70`);
    await expect(page).toHaveURL(/\/ticket\/70$/);
    await page.goto(`${site.origin}/history?ticket=t70`);
    await expect(page).toHaveURL(/\/ticket\/70$/);
  });

  test("the sidebar lists the open tickets; a row opens the session", async ({ page }) => {
    const world: World = {
      tickets: {
        t70: ticket("t70", 70, "doing", { ...owned, title: "A ticket opens as its own session" }),
        t12: ticket("t12", 12, "done", { ...owned, title: "An old ticket", conversation_id: "d12" }),
      },
      feeds: { d70: feedOf("d70", []), d12: feedOf("d12", []) },
    };
    await scriptServer(page, scripted(world));
    await page.goto(`${site.origin}/new`);
    const side = page.getByRole("navigation", { name: "Main" });
    await expect(side.getByText("Tickets", { exact: true })).toBeVisible();
    await expect(side.getByRole("link", { name: /#12 An old ticket/ })).toHaveCount(0);
    await side.getByRole("link", { name: /#70 A ticket opens as its own session/ }).click();
    await expect(page).toHaveURL(/\/ticket\/70$/);
  });

  test("the sidebar shows today's tickets, the earlier ones on request, and its section folds", async ({ page }) => {
    const yesterday = Date.now() - 2 * 24 * 3600_000;
    const world: World = {
      tickets: {
        t70: ticket("t70", 70, "doing", { ...owned, title: "A ticket opens as its own session" }),
        t60: ticket("t60", 60, "doing", {
          ...owned,
          title: "An older open ticket",
          conversation_id: "d60",
          updated_at_ms: yesterday,
        }),
      },
      feeds: { d70: feedOf("d70", []), d60: feedOf("d60", []) },
    };
    await scriptServer(page, scripted(world));
    await page.goto(`${site.origin}/new`);
    const side = page.getByRole("navigation", { name: "Main" });
    const today = side.getByRole("link", { name: /#70 A ticket opens/ });
    const older = side.getByRole("link", { name: /#60 An older open ticket/ });
    await expect(today).toBeVisible();
    await expect(older).toHaveCount(0);

    await side.getByRole("button", { name: "Show earlier" }).click();
    await expect(older).toBeVisible();
    await side.getByRole("button", { name: "Show today only" }).click();
    await expect(older).toHaveCount(0);

    const header = side.getByRole("button", { name: /^Tickets/ });
    await expect(header).toHaveAttribute("aria-expanded", "true");
    await header.click();
    await expect(header).toHaveAttribute("aria-expanded", "false");
    await expect(today).toHaveCount(0);
    // Folded stays folded after a reload.
    await page.reload();
    await expect(side.getByRole("button", { name: /^Tickets/ })).toHaveAttribute("aria-expanded", "false");
    await expect(today).toHaveCount(0);
    await side.getByRole("button", { name: /^Tickets/ }).click();
    await expect(today).toBeVisible();
  });

  test("a Board card opens the session", async ({ page }) => {
    const world: World = {
      tickets: { t70: ticket("t70", 70, "doing", { ...owned, title: "A ticket opens as its own session" }) },
      feeds: { d70: feedOf("d70", []) },
    };
    await scriptServer(page, scripted(world));
    await page.goto(`${site.origin}/board`);
    await page.getByRole("button", { name: /#70 A ticket opens/ }).click();
    await expect(page).toHaveURL(/\/ticket\/70$/);
    // Board stays the place it came from in the sidebar.
    await expect(page.getByRole("link", { name: "Board" })).toHaveAttribute("aria-current", "page");
  });

  test("an unknown number says so", async ({ page }) => {
    await scriptServer(page, scripted({ tickets: {}, feeds: {} }));
    await page.goto(`${site.origin}/ticket/9999`);
    await expect(page.getByRole("heading", { name: "Ticket #9999 not found." })).toBeVisible();
    await expect(page.getByRole("button", { name: "Open the Board" })).toBeVisible();
  });
});

test("shots", async ({ page }) => {
  mkdirSync(SHOTS, { recursive: true });
  const site = await serveStatic(WEB_DIST);
  const live = Date.now() - 30_000;
  const world: World = {
    tickets: {
      t70: ticket("t70", 70, "doing", {
        ...owned,
        title: "A ticket opens as its own session",
        root_task_status: "running",
      }),
      t71: ticket("t71", 71, "plan_review", {
        ...owned,
        conversation_id: "d71",
        title: "Plan the dock's compact session",
      }),
      t72: ticket("t72", 72, "review", {
        ...owned,
        conversation_id: "d72",
        title: "Team chat without tickets",
        root_task_status: "review",
      }),
      t73: ticket("t73", 73, "done", {
        ...owned,
        conversation_id: "d73",
        title: "Old links redirect",
        done_at_ms: live,
      }),
    },
    feeds: {
      d70: feedOf("d70", [
        message("a1", "user", "@Pixel design first, then Flux builds.", { ...sam, created_at_ms: live - 600_000 }),
        message("a2", "assistant", "ui-ontwerp.md is committed on feat/ticket-70-ticket-session.", {
          author: "Pixel",
          created_at_ms: live - 500_000,
        }),
        message("a3", "assistant", "Orion delegated “Build ticket session” to Flux", {
          created_at_ms: live - 400_000,
          system: { kind: "delegated", task_id: "s70" },
        }),
        message("a4", "assistant", "Building the head and the gate.", {
          author: "Flux",
          status: "partial",
          run_id: "r9",
          created_at_ms: live,
        }),
      ]),
      d71: feedOf("d71", [message("b1", "assistant", "The plan is ready for you.", { author: "Pixel" })]),
      d72: feedOf("d72", [message("c1", "assistant", "Done: the Team chat lists no tickets.", { author: "Pixel" })]),
      d73: feedOf("d73", [message("e1", "assistant", "Merged and closed.", { author: "Pixel" })]),
    },
  };
  const script = scripted(world);
  script.conversations.c71 = messages("c71", [
    message(
      "pl",
      "assistant",
      "## Plan\n\n1. Reuse the channel's feed.\n2. Put the gate above the composer.\n3. Dock: the compact session.",
      {
        update: "plan",
      },
    ),
  ]);
  await scriptServer(
    page,
    plus(script, {
      reads: {
        ...script.reads,
        "employee.list": () => ({
          type: "employees",
          value: { employees: [pixel, { ...flux, agents: [{ run_id: "r9" }] }], as_of_seq: 1 },
        }),
      },
    }),
  );
  const shot = async (name: string) => {
    await page.waitForLoadState("networkidle");
    await page.screenshot({ path: join(SHOTS, name), animations: "disabled" });
  };
  try {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto(`${site.origin}/ticket/70`);
    await expect(page.getByText("ui-ontwerp.md is committed")).toBeVisible();
    await expect(page.getByRole("complementary", { name: "Ticket properties" })).toBeVisible();
    await shot("doing.png");

    await page.goto(`${site.origin}/ticket/72`);
    await expect(page.getByText("In review · your decision")).toBeVisible();
    await shot("review.png");

    await page.goto(`${site.origin}/ticket/71`);
    await expect(page.getByText("Plan review · your decision")).toBeVisible();
    await shot("plan-review.png");

    await page.goto(`${site.origin}/ticket/73`);
    await expect(page.getByText("#73 is Done · read-only.")).toBeVisible();
    await shot("done-readonly.png");

    await page.goto(`${site.origin}/ticket/9999`);
    await expect(page.getByRole("heading", { name: "Ticket #9999 not found." })).toBeVisible();
    await shot("not-found.png");

    await page.setViewportSize({ width: 820, height: 900 });
    await page.goto(`${site.origin}/ticket/70`);
    await expect(page.getByText("ui-ontwerp.md is committed")).toBeVisible();
    await page.getByRole("button", { name: "Show ticket properties" }).click();
    await expect(page.getByRole("dialog", { name: "Ticket properties" })).toBeVisible();
    await shot("narrow-sheet.png");
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog", { name: "Ticket properties" })).toHaveCount(0);
  } finally {
    site.close();
  }
});
