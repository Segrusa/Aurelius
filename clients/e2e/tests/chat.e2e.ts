import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { _electron, expect, type Page, test } from "@playwright/test";
import { REPO, serveStatic, TestDaemon, WEB_DIST } from "./daemon";
import { approval, channel, message, messages, scriptServer, task, ticket } from "./fake-server";

const FEATURES = [
  "ticket_channels",
  "chat_threads",
  "chat_reactions",
  "message_edits",
  "quick_replies",
  "ticket_discussion",
];
const person = { id: "e1", name: "Flux", role: "engineer", title: "Engineer", status: "active" };
const person2 = { id: "e2", name: "Fiona", role: "qa", title: "QA", status: "active", reports_to: "e1" };

async function open(page: Page, script: Parameters<typeof scriptServer>[1], path = "/chat") {
  const server = await serveStatic(WEB_DIST);
  const fake = await scriptServer(page, { employees: [person, person2], features: FEATURES, ...script });
  await page.goto(`${server.origin}${path}`);
  return { ...fake, stop: server.close };
}

const general = channel("g", "general", { kind: "general" });
const alpha = channel("a", "alpha", { unread: 2, mentions: 1 });
const feed = (id: string, list: unknown[]) => messages(id, list);
const bottom = (page: Page) => page.getByRole("textbox", { name: /^Message/ });
const writesOf = (writes: Record<string, unknown>[], method: string) => writes.filter((w) => w.method === method);

test.describe("scripted", () => {
  test("channels, drafts and sending", async ({ page }) => {
    const { writes, stop } = await open(page, {
      // A ticket's channel is never listed: the ticket opens as its session (ticket #70).
      channels: [
        general,
        alpha,
        channel("t", "#21-screens", { kind: "ticket", ticket_number: 21, archived: true }),
        channel("x", "gone", { archived: true }),
      ],
      feeds: {
        general: feed("g", [
          message("m1", "user", "Morning all", { author_user: { id: "u", username: "Sam", created_at_ms: 0 } }),
        ]),
        a: feed("a", []),
        t: feed("t", []),
        x: feed("x", []),
      },
      refuse: (w) =>
        (w.params as { text?: string }).text === "key abc"
          ? { code: "looks_like_secret", message: "held", matched_items: ["API key"] }
          : (w.params as { text?: string }).text === "nope"
            ? { code: "invalid", message: "Refused by the server." }
            : null,
    });
    try {
      await expect(page.getByText("Morning all")).toBeVisible();
      const row = page.getByRole("button", { name: /alpha/ });
      await expect(row).toContainText("@1");
      await bottom(page).fill("half a thought");
      await row.click();
      await expect(page.getByRole("heading", { name: "Talk with your whole team here." })).toBeVisible();
      await expect(bottom(page)).toHaveValue("");
      await page.getByRole("button", { name: /general/ }).click();
      await expect(bottom(page)).toHaveValue("half a thought");

      await bottom(page).fill("line one");
      await bottom(page).press("Shift+Enter");
      await expect(writesOf(writes, "group_chat.send")).toHaveLength(0);
      await bottom(page).fill("hello team");
      await bottom(page).press("Enter");
      await expect(bottom(page)).toHaveValue("");
      await expect.poll(() => writesOf(writes, "group_chat.send").length).toBe(1);
      expect(writesOf(writes, "group_chat.send")[0]?.params).toMatchObject({
        text: "hello team",
        channel: null,
        thread_parent: null,
      });

      // A refusal puts the words back.
      await bottom(page).fill("nope");
      await bottom(page).press("Enter");
      await expect(page.locator('[role="alert"]:not([data-astryx-live-region])')).toHaveText("Refused by the server.");
      await expect(bottom(page)).toHaveValue("nope");

      // A secret is held until it is confirmed.
      await bottom(page).fill("key abc");
      await bottom(page).press("Enter");
      await expect(page.getByText("API key")).toBeVisible();
      await page.getByRole("button", { name: /Send anyway/ }).click();
      await expect
        .poll(() => writesOf(writes, "group_chat.send").at(-1)?.params)
        .toMatchObject({ text: "key abc", confirm_secret: true });

      // Read-only says why.
      await expect(page.getByRole("button", { name: /Ticket 21/ })).toHaveCount(0);
      await page.getByRole("button", { name: /Archived \(1\)/ }).click();
      await page.getByRole("button", { name: /^gone/ }).click();
      await expect(page.getByText("This project is archived · read-only.")).toBeVisible();
      await expect(bottom(page)).toHaveCount(0);
    } finally {
      await stop();
    }
  });

  test("mentions complete from the keyboard and warn about nobody", async ({ page }) => {
    const { writes, stop } = await open(page, { channels: [general], feeds: { general: feed("g", []) } });
    try {
      await bottom(page).fill("ask @f");
      const list = page.getByRole("listbox", { name: "People" });
      await expect(list.getByRole("option")).toHaveCount(2);
      await bottom(page).press("ArrowDown");
      await bottom(page).press("Enter");
      await expect(bottom(page)).toHaveValue("ask @Fiona ");
      await expect(list).toHaveCount(0);
      expect(writesOf(writes, "group_chat.send")).toHaveLength(0);
      await bottom(page).fill("@f");
      await bottom(page).press("Escape");
      await expect(list).toHaveCount(0);
      await expect(bottom(page)).toBeFocused();
      await bottom(page).fill("@Nobody hi");
      await expect(page.getByText("Nobody active is called @Nobody: it wakes no one.")).toBeVisible();
    } finally {
      await stop();
    }
  });

  test("a streaming answer grows, a finished one takes over, and only its row renders", async ({ page }) => {
    const list: unknown[] = [message("m1", "assistant", "Earlier **answer**", { author: "Flux" })];
    const { push, stop } = await open(page, { channels: [general], feeds: { general: feed("g", list) } });
    try {
      await expect(page.getByText("Earlier")).toBeVisible();
      const frame = (text: string, activity: unknown[] = []) =>
        push({ type: "stream", run_id: "r9", conversation_id: "g", message_id: "s1", text, activity });
      frame("");
      await expect(page.getByText("Thinking…")).toBeVisible();
      await page.evaluate(() => {
        (window as unknown as { kept: Element | null }).kept = document.querySelector("[data-message-body]");
      });
      frame("First block.\n\nSecond");
      await expect(page.getByText("Second")).toBeVisible();
      await expect(page.getByText("Writing…")).toBeVisible();
      frame("First block.\n\nSecond block.", [
        { call_id: "c1", tool: "Bash", input: { command: "cargo test" }, offset: 0, ok: null },
      ]);
      // A chat says that someone is writing, not what they are doing.
      await expect(page.getByText("Running a command", { exact: true })).toHaveCount(0);
      expect(await page.evaluate(() => (window as unknown as { kept: Element }).kept.isConnected)).toBe(true);

      // The saved message takes over: once, without its stream beside it.
      list.push(message("s1", "assistant", "First block.\n\nSecond block.", { author: "Flux", ordinal: 2 }));
      push({ type: "event", seq: 5, kind: "message.created", at_ms: Date.now(), payload: {} });
      await expect(page.getByText("Writing…")).toHaveCount(0);
      await expect(page.getByText("Second block.")).toHaveCount(1);
    } finally {
      await stop();
    }
  });

  test("reactions, threads and a message's actions", async ({ page }) => {
    const root = message("m1", "assistant", "Shall I ship, @Fiona?", {
      author: "Flux",
      ordinal: 1,
      thread_reply_count: 2,
      thread_unread: 1,
      thread_last_reply_ms: Date.now() - 60_000,
      thread_repliers: ["Sam"],
      reactions: [{ emoji: "👍", count: 1, mine: false, by: ["Ava"] }],
    });
    const mine = message("m2", "user", "My own words", {
      ordinal: 2,
      created_at_ms: Date.now() - 1000,
      editable: true,
      author_user: { id: "u", username: "Sam", created_at_ms: 0 },
    });
    const { writes, stop } = await open(page, {
      channels: [general],
      feeds: { general: feed("g", [root, mine]) },
      threads: {
        m1: {
          as_of_seq: 1,
          conversation_id: "g",
          root,
          replies: [message("r1", "user", "Reply one", { thread_parent: "m1" })],
        },
      },
    });
    try {
      await expect(page.locator(".mention", { hasText: "@Fiona" })).toBeVisible();
      const pill = page.getByRole("button", { name: /👍/ }).first();
      await expect(pill).toContainText("1");
      await pill.click();
      await expect(pill).toContainText("2");
      await expect
        .poll(() => writesOf(writes, "message.react")[0]?.params)
        .toMatchObject({ emoji: "👍", on: true, message_id: "m1" });

      await page.getByRole("button", { name: /Thread, 2 replies/ }).click();
      const panel = page.getByRole("complementary", { name: "Thread" });
      await expect(panel.getByText("Reply one")).toBeVisible();
      await panel.getByRole("textbox").fill("In the thread");
      await panel.getByRole("textbox").press("Enter");
      await expect
        .poll(() => writesOf(writes, "group_chat.send")[0]?.params)
        .toMatchObject({ text: "In the thread", thread_parent: "m1" });
      await panel.getByRole("button", { name: "Close thread" }).click();
      await expect(panel).toHaveCount(0);

      const row = page.getByText("My own words");
      await row.hover();
      await page.getByRole("button", { name: "More actions" }).last().click();
      await page.getByRole("menuitem", { name: "Edit" }).click();
      const edit = page.getByRole("textbox", { name: "Edit message" });
      await edit.fill("");
      await expect(page.getByText("To remove it, delete the message.")).toBeVisible();
      await edit.fill("Better words");
      await edit.press("Enter");
      await expect
        .poll(() => writesOf(writes, "message.edit")[0]?.params)
        .toMatchObject({ text: "Better words", message_id: "m2" });
    } finally {
      await stop();
    }
  });

  test("a channel is a chat: no run details, only an approval of a woken run", async ({ page }) => {
    const { stop } = await open(page, {
      features: [...FEATURES, "run_cards"],
      channels: [general],
      approvals: [
        approval("a9", { conversation_id: "main", discussion_id: "g", requester: "Bo" }),
        approval("a8", { conversation_id: "elsewhere", requester: "Cy" }),
      ],
      feeds: {
        general: feed("g", [
          message("m1", "assistant", "All done here.", {
            update: "group",
            author: "Flux",
            run_id: "r1",
            run: { status: "succeeded", tool_calls: 3, failed_calls: 0 },
            usage: {
              input_tokens: 2000,
              output_tokens: 500,
              cached_tokens: 0,
              cache_creation_tokens: 0,
              estimate: false,
            },
          }),
        ]),
      },
      reads: {
        "usage.get_totals": () => {
          const total = {
            runs: 1,
            input_tokens: 2000,
            output_tokens: 500,
            cached_tokens: 0,
            cache_creation_tokens: 0,
            cost_micro_usd: 50000,
            estimate: false,
            model_calls: 2,
          };
          return { type: "usage_totals", value: { own: total, subtasks: [], total } };
        },
      },
    });
    try {
      await expect(page.getByText("All done here.")).toBeVisible();
      await expect(page.getByText(/tool calls/)).toHaveCount(0);
      await expect(page.getByText(/tokens in/)).toHaveCount(0);
      await expect(page.getByText("Bo wants to run a command")).toBeVisible();
      await expect(page.getByText("Cy wants to run a command")).toHaveCount(0);
    } finally {
      await stop();
    }
  });

  test("a ticket's session keeps the todo list, open tasks and its properties beside the talk", async ({ page }) => {
    const todos = [
      { content: "Find the causes", activeForm: "Finding the causes", status: "completed" },
      { content: "Fix the prompt", activeForm: "Fixing the prompt", status: "in_progress" },
      { content: "Run the suites", activeForm: "Running the suites", status: "pending" },
    ];
    const { stop } = await open(
      page,
      {
        channels: [general, channel("t", "#21-screens", { kind: "ticket", ticket_number: 21, ticket_id: "t1" })],
        feeds: { general: feed("g", []), t: feed("t", [message("m1", "user", "How far are we?")]) },
        details: {
          t1: {
            as_of_seq: 1,
            ticket: ticket("t1", 21, "doing", {
              kind: "quick",
              conversation_id: "t",
              root_task_id: "root",
              owner_name: "Flux",
            }),
            root_task: task("root", { conversation_id: "rc" }),
            tasks: [
              task("root", { conversation_id: "rc" }),
              task("sub", { title: "Write the tests", parent_task_id: "root", employee_name: "Fiona" }),
              task("old", { title: "Read the code", parent_task_id: "root", status: "succeeded" }),
            ],
          },
        },
        conversations: {
          rc: messages("rc", [
            message("p1", "assistant", "On it.", {
              activity: [{ call_id: "c1", tool: "TodoWrite", input: { todos }, offset: 0, ok: true }],
            }),
          ]),
        },
      },
      "/chat?channel=t",
    );
    try {
      await expect(page.getByText("How far are we?")).toBeVisible();
      const card = page.getByRole("region", { name: "Progress" });
      await expect(card.getByRole("list", { name: "Todo list" }).getByRole("listitem")).toHaveText([
        "Find the causes",
        "Fixing the prompt",
        "Run the suites",
      ]);
      await expect(card).toContainText("1 of 3");
      await expect(card).toContainText("Open tasks · 1 of 2 done");
      const sub = card.getByRole("link", { name: /Write the tests/ });
      await expect(sub).toHaveAttribute("href", /\/tasks\/sub$/);
      await expect(card.getByRole("link", { name: /Read the code/ })).toHaveCount(0);
      // The posts stay words only.
      await expect(page.getByRole("article").getByText("Fixing the prompt")).toHaveCount(0);

      // The old channel link opened the ticket's session; its properties replace the overview.
      await expect(page).toHaveURL(/\/ticket\/21$/);
      const panel = page.getByRole("complementary", { name: "Ticket properties" });
      await expect(panel).toContainText("quick/ticket-21");
      await expect(panel.getByRole("button", { name: /Read the code/ })).toBeVisible();
      await page.getByRole("button", { name: "Hide ticket properties" }).click();
      await expect(panel).toHaveCount(0);
      await page.getByRole("button", { name: "Show ticket properties" }).click();
      await expect(panel).toBeVisible();

      await expect(card.getByRole("button", { name: /Working: Flux, Fiona/ })).toBeVisible();
      await card.getByRole("button", { name: /Working/ }).click();
      await expect(card.getByRole("list", { name: "Todo list" })).toHaveCount(0);
      await expect(card).toContainText("Fixing the prompt");
    } finally {
      await stop();
    }
  });

  test("a question and a plan get their own answer, and the bell leads back to them", async ({ page }) => {
    const question = approval("q1", {
      tool: "claude.AskUserQuestion",
      conversation_id: "g",
      arguments: {
        questions: [
          {
            question: "Which colour?",
            header: "Colour",
            options: [
              { label: "Green", description: "The design" },
              { label: "Blue", description: "" },
            ],
          },
        ],
      },
    });
    const plan = approval("p1", {
      tool: "claude.ExitPlanMode",
      conversation_id: "g",
      arguments: { plan: "## Ship it\nStep one" },
    });
    const { writes, stop } = await open(page, {
      channels: [general],
      feeds: { general: feed("g", [message("m1", "user", "Go", {})]) },
      approvals: [question, plan],
    });
    try {
      await page.getByRole("button", { name: "Send answer" }).click();
      await expect(page.getByText("Answer every question first, or skip.")).toBeVisible();
      await page.getByRole("radio", { name: /Green/ }).check();
      await page.getByRole("button", { name: "Send answer" }).click();
      await expect
        .poll(() => writesOf(writes, "approval.answer")[0]?.params)
        .toMatchObject({ approval_id: "q1", answers: { "Which colour?": "Green" } });

      await page.getByRole("button", { name: "Request changes" }).click();
      await expect(page.getByText("Say what should change, then ask for changes.")).toBeVisible();
      await page.getByRole("textbox", { name: "What should change?" }).fill("Smaller");
      await page.getByRole("button", { name: "Request changes" }).click();
      await expect
        .poll(() => writesOf(writes, "approval.decide_plan")[0]?.params)
        .toMatchObject({ approval_id: "p1", decision: "revise", feedback: "Smaller" });

      await page
        .getByRole("button", { name: /^Inbox/ })
        .first()
        .click();
      // The bell lists the question; its row leads to the conversation it was asked in.
      await page.getByRole("dialog", { name: "Inbox" }).getByRole("button", { name: /Flux/ }).first().click();
      await expect(page.getByRole("dialog", { name: "Inbox" })).toHaveCount(0);
      await expect(page).toHaveURL(/\/chat/);
      await expect(page.getByRole("button", { name: /^Allow once/ })).toHaveCount(0);
    } finally {
      await stop();
    }
  });

  test("an answer and a plan decision read as cards, not as a grey line", async ({ page }) => {
    const answered = message(
      "m2",
      "assistant",
      "Answered:\n\n- What happens to the chat history in the database?\n  **Delete it for good**\n- What exactly goes?\n  **1-on-1 + Team chat (Recommended)**",
      { update: "status", author: "You", ordinal: 2 },
    );
    const revised = message("m3", "assistant", "Asked for changes to the plan: Smaller steps, please.", {
      update: "status",
      author: "You",
      ordinal: 3,
    });
    const { stop } = await open(page, {
      channels: [general],
      feeds: { general: feed("g", [message("m1", "user", "Go", {}), answered, revised]) },
    });
    try {
      const card = page.getByRole("region", { name: /^You answered 2 questions/ });
      await expect(card).toContainText("What exactly goes?");
      await expect(card).toContainText("Delete it for good");
      await expect(card.getByText("Recommended", { exact: true })).toBeVisible();
      await expect(card).not.toContainText("**");
      const plan = page.getByRole("region", { name: /^You asked for changes to the plan/ });
      await expect(plan).toContainText("Smaller steps, please.");
      await page.screenshot({ path: join(REPO, "target/decision-card/decision-card.png") });
    } finally {
      await stop();
    }
  });

  test("2,000 messages scroll without long frames and render few rows", async ({ page }) => {
    const many = Array.from({ length: 2000 }, (_, i) =>
      message(`m${i}`, i % 2 ? "assistant" : "user", `Message ${i} with **bold** text and \`code\`.\n\n- one\n- two`, {
        author: i % 2 ? "Flux" : undefined,
        ordinal: i + 1,
        created_at_ms: Date.now() - (2000 - i) * 400_000,
      }),
    );
    const { stop } = await open(page, { channels: [general], feeds: { general: feed("g", many) } });
    try {
      await expect(page.getByText("Message 1999", { exact: false })).toBeVisible();
      const scroller = '[data-virtuoso-scroller="true"]';
      const result = await page.evaluate(async (selector) => {
        const el = document.querySelector(selector) as HTMLElement;
        const frames: number[] = [];
        await new Promise<void>((done) => {
          const start = performance.now();
          let last = start;
          const tick = (now: number) => {
            frames.push(now - last);
            last = now;
            const t = (now - start) / 4000;
            el.scrollTop = el.scrollHeight * (1 - Math.min(1, t)) * 0.5;
            if (t < 1) requestAnimationFrame(tick);
            else done();
          };
          requestAnimationFrame(tick);
        });
        frames.shift();
        return {
          frames: frames.length,
          over33: frames.filter((f) => f > 33).length,
          worst: Math.round(Math.max(...frames)),
          rows: document.querySelectorAll("[data-item-index]").length,
        };
      }, scroller);
      console.log("chat scroll", JSON.stringify(result));
      expect(result.rows).toBeLessThan(120);
      expect(result.over33 / result.frames).toBeLessThan(0.05);
    } finally {
      await stop();
    }
  });
});

test("the Team chat over a real daemon: send, and a diagram under the strict page policy", async () => {
  const daemon = new TestDaemon();
  await daemon.start();
  const profile = mkdtempSync(join(tmpdir(), "aurelius-chat-"));
  const app = await _electron.launch({
    args: [
      join(REPO, "clients/electron"),
      `--user-data-dir=${profile}`,
      ...(process.env.AURELIUS_ELECTRON_ARGS?.split(" ") ?? []),
    ],
    env: { ...process.env, AURELIUS_DATA_DIR: daemon.data, AURELIUS_ENV: "dev", ELECTRON_RENDERER_URL: "" } as Record<
      string,
      string
    >,
  });
  try {
    const page = await app.firstWindow();
    await page.getByLabel("Name").fill("sam");
    await page.getByLabel("Password").fill("a-long-enough-password");
    await page.getByRole("button", { name: "Create" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "New session" })).toBeVisible();
    await page.keyboard.press("Control+8");
    const box = page.getByRole("textbox", { name: /^Message/ });
    await box.fill("```mermaid\ngraph TD\n  A[Start] --> B[Finish]\n```");
    await box.press("Enter");
    const diagram = page.getByRole("img", { name: "Mermaid diagram" });
    await expect(diagram).toBeVisible({ timeout: 20_000 });
    expect(await diagram.evaluate((img: HTMLImageElement) => img.naturalWidth)).toBeGreaterThan(0);
    await box.fill("```mermaid\nnot a diagram at all ((\n```");
    await box.press("Enter");
    await expect(page.getByText("not a diagram at all")).toBeVisible({ timeout: 20_000 });
  } finally {
    await app.close();
    await daemon.stop();
    rmSync(profile, { recursive: true, force: true });
  }
});
