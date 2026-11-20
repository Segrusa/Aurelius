import type { ChannelView, ColumnKind, TicketView, TokenUsage } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  composerHint,
  composerPlaceholder,
  filesWrittenAcross,
  gateKeyAllowed,
  gateOf,
  inTicketsSection,
  latestRunAcross,
  ownerAnswers,
  readOnly,
  resolveNumber,
  sessionRedirect,
  sidebarTickets,
  ticketConversations,
  ticketNumberIn,
  ticketPlace,
  ticketSignal,
  ticketUsage,
} from "./ticket-session-logic";

const chan = (over: Partial<ChannelView> = {}): ChannelView => ({
  archived: false,
  conversation_id: "c",
  mentions: 0,
  name: "x",
  unread: 0,
  ...over,
});

const tv = (over: Partial<TicketView> = {}): TicketView => ({
  attachments: [],
  column: "doing",
  column_id: "c-doing",
  created_at_ms: 0,
  created_by_user: "u",
  created_by_username: "Sam",
  id: "t70",
  labels: [],
  login_user: "u",
  login_username: "Sam",
  number: 70,
  owner_name: "Pixel",
  owner_employee_id: "e1",
  position: 0,
  priority: "normal",
  project_id: "p",
  project_name: "Aurelius",
  run: {},
  title: "A ticket opens as its own session",
  updated_at_ms: 0,
  version: 1,
  ...over,
});

describe("resolveNumber", () => {
  const channels = [
    chan({ conversation_id: "g", kind: "general" }),
    chan({ conversation_id: "d70", kind: "ticket", ticket_id: "t70", ticket_number: 70 }),
    chan({ conversation_id: "d12", kind: "ticket", ticket_id: "t12", ticket_number: 12, archived: true }),
  ];

  it("finds the ticket by its channel, an archived one too", () => {
    expect(resolveNumber(70, channels, undefined)).toEqual({ ticketId: "t70", conversationId: "d70" });
    expect(resolveNumber(12, channels, undefined)).toEqual({ ticketId: "t12", conversationId: "d12" });
  });

  it("asks for a search when no channel has it and the list is not read yet", () => {
    expect(resolveNumber(5, channels, undefined)).toBe("search");
  });

  it("takes the listed ticket with exactly that number", () => {
    const listed = [tv({ id: "t50", number: 50 }), tv({ id: "t5", number: 5, conversation_id: "d5" })];
    expect(resolveNumber(5, channels, listed)).toEqual({ ticketId: "t5", conversationId: "d5" });
    expect(resolveNumber(50, [], [tv({ id: "t50", number: 50, conversation_id: null })])).toEqual({
      ticketId: "t50",
      conversationId: null,
    });
  });

  it("says missing when neither has it", () => {
    expect(resolveNumber(9999, channels, [tv({ number: 99 })])).toBe("missing");
    expect(resolveNumber(9999, [], [])).toBe("missing");
  });
});

describe("gateOf", () => {
  it("is the plan gate in plan review and the review gate in review", () => {
    expect(gateOf(tv({ column: "plan_review" }))).toBe("plan");
    expect(gateOf(tv({ column: "review" }))).toBe("review");
  });

  it("is the status box in every other open column", () => {
    for (const column of ["backlog", "todo", "doing", "testing"] as ColumnKind[]) {
      expect(gateOf(tv({ column }))).toBe("status");
    }
  });

  it("is none on Done and on an archived ticket", () => {
    expect(gateOf(tv({ column: "done" }))).toBe("none");
    expect(gateOf(tv({ column: "review", archived_at_ms: 5 }))).toBe("none");
  });
});

describe("readOnly", () => {
  it("is read-only on Done and when archived", () => {
    expect(readOnly(tv({ column: "done" }), null)).toBe(true);
    expect(readOnly(tv({ archived_at_ms: 1 }), null)).toBe(true);
  });

  it("is writable while open", () => {
    expect(readOnly(tv({ column: "doing" }), null)).toBe(false);
    expect(readOnly(tv({ column: "backlog" }), { read_only: false })).toBe(false);
  });

  it("follows the ticket, not a channel read before it left Done", () => {
    expect(readOnly(tv({ column: "doing" }), { read_only: true })).toBe(false);
    expect(readOnly(null, { read_only: true })).toBe(true);
    expect(readOnly(null, null)).toBe(false);
  });
});

describe("ownerAnswers (BR1)", () => {
  it("is the owner in Todo, Doing, Plan review, Testing and Review", () => {
    for (const column of ["todo", "doing", "plan_review", "testing", "review"] as ColumnKind[]) {
      expect(ownerAnswers(tv({ column }))).toBe("Pixel");
    }
  });

  it("is nobody in Backlog and Done", () => {
    expect(ownerAnswers(tv({ column: "backlog" }))).toBeNull();
    expect(ownerAnswers(tv({ column: "done" }))).toBeNull();
  });

  it("is nobody when archived or without an owner", () => {
    expect(ownerAnswers(tv({ archived_at_ms: 1 }))).toBeNull();
    expect(ownerAnswers(tv({ owner_name: null, owner_employee_id: null }))).toBeNull();
  });
});

describe("the composer's words", () => {
  it("messages the owner while the owner answers", () => {
    expect(composerPlaceholder(tv())).toBe("Message Pixel (owner)… · @ to mention");
    expect(composerHint(tv())).toBe("@Name asks that employee; without a name, Pixel (owner) answers.");
  });

  it("messages the ticket and says no one is woken otherwise", () => {
    const backlog = tv({ column: "backlog" });
    expect(composerPlaceholder(backlog)).toBe("Message #70 · @ to mention");
    expect(composerHint(backlog)).toBe("@Name asks that employee; without a name, no one is woken.");
    expect(composerPlaceholder(tv({ owner_name: null }))).toBe("Message #70 · @ to mention");
  });
});

describe("sessionRedirect (BR6)", () => {
  it("asks to resolve the id of /board?ticket= and /history?ticket=", () => {
    expect(sessionRedirect("/board", { ticket: "t70" })).toEqual({ ticketId: "t70" });
    expect(sessionRedirect("/history", { ticket: "t70" })).toEqual({ ticketId: "t70" });
    expect(sessionRedirect("/board", {})).toBeNull();
    expect(sessionRedirect("/tasks", { ticket: "t70" })).toBeNull();
  });
});

describe("ticketUsage", () => {
  it("maps the server's totals onto the side panel's", () => {
    const u: TokenUsage = {
      input_tokens: 10,
      output_tokens: 20,
      cached_tokens: 30,
      cache_creation_tokens: 40,
      cost_micro_usd: 50,
      estimate: true,
      runs: 2,
    };
    expect(ticketUsage(u)).toEqual({ input: 10, output: 20, cached: 30, written: 40, micro: 50 });
  });

  it("reads a missing cost as 0, and no totals as none", () => {
    const u = {
      input_tokens: 1,
      output_tokens: 2,
      cached_tokens: 0,
      cache_creation_tokens: 0,
      estimate: false,
      runs: 1,
    };
    expect(ticketUsage({ ...u, cost_micro_usd: null })).toEqual({
      input: 1,
      output: 2,
      cached: 0,
      written: 0,
      micro: 0,
    });
    expect(ticketUsage(null)).toBeNull();
    expect(ticketUsage(undefined)).toBeNull();
  });
});

describe("the ticket's conversations", () => {
  it("are the discussion and every task's conversation, once each", () => {
    expect(
      ticketConversations("d70", [{ conversation_id: "k1" }, { conversation_id: null }, { conversation_id: "k1" }]),
    ).toEqual(["d70", "k1"]);
    expect(ticketConversations(null, [{ conversation_id: "k2" }])).toEqual(["k2"]);
  });

  const write = (file_path: string, ok: boolean | null = true) => ({ tool: "claude.Write", input: { file_path }, ok });

  it("unite the files written, in order and without doubles", () => {
    expect(
      filesWrittenAcross([
        [{ activity: [write("a.ts"), write("b.ts")] }],
        [{ activity: [write("b.ts"), write("c.ts"), write("d.ts", false)] }],
        undefined,
      ]),
    ).toEqual(["a.ts", "b.ts", "c.ts"]);
    expect(filesWrittenAcross([])).toEqual([]);
  });

  it("give the latest run across them", () => {
    expect(
      latestRunAcross([
        [
          { run_id: "r1", created_at_ms: 1 },
          { run_id: "r3", created_at_ms: 3 },
        ],
        [
          { run_id: "r4", created_at_ms: 4 },
          { run_id: null, created_at_ms: 9 },
        ],
        undefined,
      ]),
    ).toBe("r4");
    expect(latestRunAcross([[{ run_id: null, created_at_ms: 1 }]])).toBeNull();
  });
});

describe("ticketPlace", () => {
  it("opens the session by number", () => {
    expect(ticketPlace({ id: "t70", number: 70 })).toEqual({ to: "/ticket/$number", params: { number: "70" } });
    expect(ticketPlace({ id: "t70", number: 70 }, { message: "m1" })).toEqual({
      to: "/ticket/$number",
      params: { number: "70" },
      search: { message: "m1" },
    });
  });

  it("goes through the Board's redirect when only the id is known", () => {
    expect(ticketPlace({ id: "t70" })).toEqual({ to: "/board", search: { ticket: "t70" } });
    expect(ticketPlace({ id: "t70", number: null })).toEqual({ to: "/board", search: { ticket: "t70" } });
  });
});

describe("gateKeyAllowed", () => {
  it("decides from the keyboard unless a filled text field has the focus", () => {
    expect(gateKeyAllowed(null)).toBe(true);
    expect(gateKeyAllowed({ tagName: "BUTTON" })).toBe(true);
    expect(gateKeyAllowed({ tagName: "TEXTAREA", value: "" })).toBe(true);
    expect(gateKeyAllowed({ tagName: "TEXTAREA", value: "please also" })).toBe(false);
    expect(gateKeyAllowed({ tagName: "INPUT", value: "x" })).toBe(false);
  });
});

describe("ticketNumberIn", () => {
  it("reads the number a line names, for a link straight to the session", () => {
    expect(ticketNumberIn("Sam opened #12 Fix the login")).toBe(12);
    expect(ticketNumberIn("#7")).toBe(7);
    expect(ticketNumberIn("a ticket")).toBeNull();
  });
});

describe("the sidebar's tickets", () => {
  const none = () => null;

  it("lists the open tickets of the workspace, the last changed first", () => {
    const tickets = [
      tv({ id: "old", updated_at_ms: 1 }),
      tv({ id: "new", updated_at_ms: 3, column: "review" }),
      tv({ id: "other", updated_at_ms: 2, project_id: "q" }),
    ];
    expect(sidebarTickets(tickets, "p", none).map((t) => t.id)).toEqual(["new", "old"]);
    expect(sidebarTickets(tickets, null, none).map((t) => t.id)).toEqual(["new", "other", "old"]);
  });

  it("puts a ticket you opened last first, before one that changed earlier", () => {
    const tickets = [tv({ id: "changed", updated_at_ms: 30 }), tv({ id: "opened", updated_at_ms: 5 })];
    const usedAt = (t: TicketView) => Math.max(t.updated_at_ms, t.id === "opened" ? 40 : 0);
    expect(sidebarTickets(tickets, null, none, null, usedAt).map((t) => t.id)).toEqual(["opened", "changed"]);
    expect(sidebarTickets(tickets, null, none, 35, usedAt).map((t) => t.id)).toEqual(["opened"]);
  });

  it("leaves out the Backlog, Done and archived tickets", () => {
    const tickets = [
      tv({ id: "backlog", column: "backlog" }),
      tv({ id: "done", column: "done" }),
      tv({ id: "archived", archived_at_ms: 5 }),
      tv({ id: "todo", column: "todo" }),
    ];
    expect(sidebarTickets(tickets, null, none).map((t) => t.id)).toEqual(["todo"]);
  });

  it("lists the Done tickets beside the open ones, as the finished sessions are (ticket #75)", () => {
    const tickets = [
      tv({ id: "backlog", column: "backlog" }),
      tv({ id: "done", column: "done" }),
      tv({ id: "done-archived", column: "done", archived_at_ms: 5 }),
      tv({ id: "todo", column: "todo" }),
    ];
    expect(tickets.filter(inTicketsSection).map((t) => t.id)).toEqual(["done", "todo"]);
  });

  it("keeps an older ticket past the limit when it needs you or works", () => {
    const tickets = Array.from({ length: 10 }, (_, i) => tv({ id: `t${i}`, updated_at_ms: 100 - i }));
    const shown = sidebarTickets(tickets, null, (t) => (t.id === "t9" ? "needs_you" : null));
    expect(shown.map((t) => t.id)).toEqual(["t0", "t1", "t2", "t3", "t4", "t5", "t6", "t7", "t9"]);
  });

  it("by default keeps today's tickets and the ones that need you or work, with no limit", () => {
    const today = 1000;
    const tickets = [
      ...Array.from({ length: 10 }, (_, i) => tv({ id: `today${i}`, updated_at_ms: today + i })),
      tv({ id: "yesterday", updated_at_ms: today - 1 }),
      tv({ id: "waiting", updated_at_ms: 5 }),
    ];
    const shown = sidebarTickets(tickets, null, (t) => (t.id === "waiting" ? "needs_you" : null), today);
    expect(shown).toHaveLength(11);
    expect(shown.map((t) => t.id)).not.toContain("yesterday");
    expect(shown.at(-1)?.id).toBe("waiting");
  });

  it("marks a ticket that needs you before one whose main task runs", () => {
    expect(ticketSignal({ root_task_status: "running" }, true)).toBe("needs_you");
    expect(ticketSignal({ root_task_status: "running" }, false)).toBe("working");
    expect(ticketSignal({ root_task_status: "queued" }, false)).toBe("working");
    expect(ticketSignal({ root_task_status: "review" }, false)).toBeNull();
    expect(ticketSignal({}, false)).toBeNull();
  });
});
