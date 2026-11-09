// A chat that is only a chat: the bar above a message, the popout, and one question in the Inbox.

import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { boardOf, channel, message, messages, notification, scriptServer, task, ticket } from "./fake-server";

const SHOTS = process.env.AURELIUS_POPOUT_SHOTS ? join(REPO, "clients/e2e/test-results/shots") : null;
const shot = async (page: Page, name: string) => {
  if (!SHOTS) return;
  await page.waitForTimeout(250);
  await page.screenshot({ path: join(SHOTS, `${name}.png`) });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.waitForTimeout(150);
  await page.screenshot({ path: join(SHOTS, `${name}-dark.png`) });
  await page.emulateMedia({ colorScheme: "light" });
};
const flux = { id: "e1", name: "Flux", role: "engineer", title: "Engineer", status: "active" };
const pixel = { id: "e2", name: "Pixel", role: "designer", title: "Designer", status: "active" };
const chats = () => ({
  type: "employee_chats",
  value: {
    as_of_seq: 1,
    chats: [
      {
        conversation_id: "c1",
        employee_id: "e1",
        employee_name: "Flux",
        unread: 2,
        last_activity_ms: Date.now() - 300_000,
        preview: "The new build is ready.",
      },
    ],
  },
});

test("the bar above a message is icons, not symbols, and takes a reaction at one click", async ({ page }) => {
  const site = await serveStatic(WEB_DIST);
  try {
    const { writes } = await scriptServer(page, {
      employees: [flux],
      features: ["chat_threads", "chat_reactions", "message_edits", "ticket_discussion"],
      channels: [channel("g", "general", { kind: "general" })],
      feeds: {
        general: messages("g", [message("m1", "assistant", "All done here.", { author: "Flux", role_kind: "x" })]),
      },
    });
    await page.goto(`${site.origin}/chat`);
    await page.getByText("All done here.").hover();
    const bar = page.locator("[data-row-bar]");
    await expect(bar).toBeVisible();
    await expect(bar.getByRole("button", { name: "Reply in thread" })).toBeVisible();
    await expect(bar).not.toContainText("☺");
    await expect(bar).not.toContainText("⋯");
    await shot(page, "bar");
    await bar.getByRole("button", { name: "React with 👍" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "message.react")?.params)
      .toMatchObject({ emoji: "👍", on: true });

    await bar.getByRole("button", { name: "Add a reaction" }).click();
    await expect(page.getByRole("button", { name: "React with 🚀" })).toBeVisible();
    await shot(page, "picker");
    await page.getByRole("button", { name: "React with 🚀" }).click();
    await expect.poll(() => writes.filter((w) => w.method === "message.react").length).toBe(2);
    await expect(page.getByText("—")).toHaveCount(0);
  } finally {
    await site.close();
  }
});

test("the popout lists the chats, opens one, sends and keeps out of the way", async ({ page }) => {
  const site = await serveStatic(WEB_DIST);
  try {
    const { writes } = await scriptServer(page, {
      employees: [flux, pixel],
      features: ["employee_chats"],
      tickets: [ticket("t1", 3, "doing", { title: "Open me" })],
      conversations: {
        c1: messages("c1", [
          message("m1", "user", "Is it ready?"),
          message("m2", "assistant", "The new build is ready."),
        ]),
      },
      reads: { "employee.chats": chats },
    });
    await page.goto(`${site.origin}/inbox`);
    const launcher = page.getByRole("button", { name: "Open chat, 2 unread" });
    await expect(launcher).toBeVisible();
    await shot(page, "launcher");
    await launcher.click();
    const popout = page.getByRole("region", { name: "Chat popout" });
    await expect(popout.getByText("The new build is ready.")).toBeVisible();
    await expect(popout.getByText("Start a chat")).toBeVisible();
    await shot(page, "popout-list");

    await popout.getByRole("button", { name: /Flux/ }).click();
    await expect(popout.getByText("Is it ready?")).toBeVisible();
    await popout.getByPlaceholder("Message Flux").fill("Thanks");
    await page.keyboard.press("Enter");
    await expect
      .poll(() => writes.find((w) => w.method === "chat.send")?.params)
      .toMatchObject({ conversation_id: "c1", text: "Thanks" });
    await shot(page, "popout-chat");

    // It comes back where it was, and a full-screen chat replaces it.
    await page.reload();
    await expect(page.getByRole("region", { name: "Chat popout" }).getByText("Is it ready?")).toBeVisible();
    await page.getByRole("button", { name: "Open full screen" }).click();
    await expect(page).toHaveURL(/\/talk\/c1/);
    await expect(page.getByRole("region", { name: "Chat popout" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: /Open chat/ })).toHaveCount(0);
  } finally {
    await site.close();
  }
});

test("a ticket reads as a chat: in its panel and in the popout", async ({ page }) => {
  const site = await serveStatic(WEB_DIST);
  try {
    const open = ticket("t1", 35, "doing", { title: "Trim the texts", conversation_id: "d1", owner_name: "Pixel" });
    const talk = messages("d1", [
      message("q", "assistant", "Which group may I remove first?", { update: "group", author: "Pixel" }),
    ]);
    const { writes } = await scriptServer(page, {
      employees: [pixel],
      features: ["employee_chats", "ticket_channels", "ticket_discussion", "chat_threads", "chat_reactions"],
      tickets: [open],
      board: boardOf([open]),
      channels: [
        channel("d1", "#35-trim", {
          kind: "ticket",
          ticket_id: "t1",
          ticket_number: 35,
          ticket_column: "doing",
          unread: 1,
          mentions: 1,
        }),
      ],
      details: { t1: { ticket: open, root_task: null, tasks: [], as_of_seq: 1 } },
      feeds: { d1: talk },
      conversations: { d1: talk },
      reads: { "employee.chats": chats },
    });
    await page.goto(`${site.origin}/board?ticket=t1`);
    const panel = page.getByRole("complementary", { name: /Trim the texts/ });
    await panel.getByRole("tab", { name: /Discussion/ }).click();
    await panel.getByText("Which group may I remove first?").hover();
    await expect(panel.locator("[data-row-bar]")).toBeVisible();
    await shot(page, "ticket-panel");
    await panel.getByRole("textbox", { name: /^Message/ }).fill("The first one");
    await page.keyboard.press("Enter");
    await expect
      .poll(() => writes.find((w) => w.method === "group_chat.send")?.params)
      .toMatchObject({ channel: "d1", text: "The first one" });

    await page.goto(`${site.origin}/inbox`);
    await page.getByRole("button", { name: /Open chat/ }).click();
    const popout = page.getByRole("region", { name: "Chat popout" });
    await expect(popout.getByText("Tickets", { exact: true })).toBeVisible();
    await expect(popout.getByText("Doing · Pixel")).toBeVisible();
    await shot(page, "popout-tickets");
    await popout.getByRole("button", { name: /#35 Trim the texts/ }).click();
    await expect(popout.getByText("Which group may I remove first?")).toBeVisible();
    await expect(popout.getByRole("textbox", { name: /^Message/ })).toBeVisible();
    await shot(page, "popout-ticket");
  } finally {
    await site.close();
  }
});

test("a question on a ticket is one row in the Inbox, readable and answerable there", async ({ page }) => {
  const site = await serveStatic(WEB_DIST);
  try {
    const open = ticket("t1", 35, "doing", { title: "Trim the texts", conversation_id: "d1" });
    const waiting = task("w1", {
      title: "Trim the texts",
      status: "waiting",
      waiting_reason: "input",
      ticket_id: "t1",
      ticket_number: 35,
      employee_name: "Pixel",
      conversation_id: "tc",
    });
    const { writes } = await scriptServer(page, {
      employees: [pixel],
      tickets: [open],
      tasks: [waiting],
      notifications: [
        { ...notification(1), reason: "mention", ticket_id: "t1", ticket_number: 35, title: "Pixel mentioned you" },
      ],
      details: { t1: { ticket: open, root_task: null, tasks: [], as_of_seq: 1 } },
      conversations: {
        tc: messages("tc", []),
        d1: messages("d1", [
          message("q", "tool", "@Sam Which group may I remove first?", {
            author: "Pixel",
            run_id: "r1",
            awaiting_user: true,
          }),
        ]),
      },
    });
    await page.goto(`${site.origin}/inbox`);
    await expect(page.getByText("Pixel mentioned you")).toHaveCount(0);
    await page.getByText("Trim the texts").first().click();
    await expect(page.getByText("Which group may I remove first?")).toBeVisible();
    await shot(page, "inbox-question");
    await page.getByRole("textbox", { name: "Your answer to Pixel" }).fill("The first one");
    await page.getByRole("button", { name: "Send answer" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "ticket.post")?.params)
      .toMatchObject({ ticket_id: "t1", text: "The first one" });
  } finally {
    await site.close();
  }
});
