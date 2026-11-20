// A ticket's talk as a chat: the bar above a message, and one question in the Inbox.

import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { boardOf, channel, message, messages, notification, scriptServer, task, ticket } from "./fake-server";

const SHOTS = process.env.AURELIUS_CHAT_SHOTS ? join(REPO, "clients/e2e/test-results/shots") : null;
const shot = async (page: Page, name: string) => {
  if (!SHOTS) return;
  await page.waitForTimeout(250);
  await page.screenshot({ path: join(SHOTS, `${name}.png`) });
  await page.emulateMedia({ colorScheme: "dark" });
  await page.waitForTimeout(150);
  await page.screenshot({ path: join(SHOTS, `${name}-dark.png`) });
  await page.emulateMedia({ colorScheme: "light" });
};
const pixel = { id: "e2", name: "Pixel", role: "designer", title: "Designer", status: "active" };

test("a ticket reads as a chat: the bar above a message is icons and takes a reaction at one click", async ({
  page,
}) => {
  const site = await serveStatic(WEB_DIST);
  try {
    const open = ticket("t1", 35, "doing", { title: "Trim the texts", conversation_id: "d1", owner_name: "Pixel" });
    const talk = messages("d1", [
      message("q", "assistant", "Which group may I remove first?", { update: "group", author: "Pixel" }),
    ]);
    const { writes } = await scriptServer(page, {
      employees: [pixel],
      features: ["ticket_channels", "ticket_discussion", "chat_threads", "chat_reactions", "message_edits"],
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
    });
    // An old link to the Board's panel opens the ticket's session (ticket #70): the talk and the properties.
    await page.goto(`${site.origin}/board?ticket=t1`);
    await expect(page).toHaveURL(/\/ticket\/35$/);
    const panel = page.getByRole("complementary", { name: "Ticket properties" });
    await expect(panel.getByText("No description.")).toBeVisible();

    const chat = page.getByRole("main");
    await chat.getByText("Which group may I remove first?").hover();
    const bar = chat.locator("[data-row-bar]");
    await expect(bar).toBeVisible();
    await expect(bar.getByRole("button", { name: "Reply in thread" })).toBeVisible();
    await expect(bar).not.toContainText("☺");
    await expect(bar).not.toContainText("⋯");
    await shot(page, "ticket-chat");
    await bar.getByRole("button", { name: "React with 👍" }).click();
    await expect
      .poll(() => writes.find((w) => w.method === "message.react")?.params)
      .toMatchObject({ emoji: "👍", on: true });
    await bar.getByRole("button", { name: "Add a reaction" }).click();
    await expect(page.getByRole("button", { name: "React with 🚀" })).toBeVisible();
    await shot(page, "picker");
    await page.getByRole("button", { name: "React with 🚀" }).click();
    await expect.poll(() => writes.filter((w) => w.method === "message.react").length).toBe(2);
    await chat.getByRole("textbox", { name: /^Message/ }).fill("The first one");
    await page.keyboard.press("Enter");
    await expect
      .poll(() => writes.find((w) => w.method === "group_chat.send")?.params)
      .toMatchObject({ channel: "d1", text: "The first one" });
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
