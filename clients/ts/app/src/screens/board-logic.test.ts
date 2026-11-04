import type { TicketView, TimelineCard } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  activityDetail,
  activityText,
  beforeFor,
  columnShown,
  columnsProblem,
  deadlineInput,
  deadlineText,
  decideToo,
  dispatcherGaveUp,
  dispatcherOf,
  doneCheck,
  dropEffect,
  feedbackLabel,
  inMyTab,
  isChoosing,
  latestPlan,
  loginWait,
  movedCard,
  needsMe,
  needsYou,
  ownerChoosable,
  shownCards,
  taskCounts,
  ticketNeeds,
} from "./board-logic";

const ticket = (over: Partial<TicketView> = {}): TicketView => ({
  id: "t",
  number: 1,
  title: "T",
  column: "doing",
  column_id: "c",
  priority: "normal",
  labels: [],
  attachments: [],
  created_at_ms: 0,
  created_by_user: "u1",
  created_by_username: "sam",
  login_user: "u1",
  login_username: "sam",
  position: 0,
  project_id: "p",
  project_name: "P",
  run: {},
  updated_at_ms: 0,
  version: 1,
  ...over,
});
const card = (title: string, actor = "user", detail = ""): TimelineCard => ({
  title,
  actor,
  detail,
  at_ms: 0,
  correlation_id: "",
  kind: "activity",
  label: "",
  seq: 0,
});
const anyone = { owner: { kind: "anyone" as const }, query: "", needsOnly: false, needs: () => false };

describe("a drop", () => {
  it("follows the table", () => {
    expect(dropEffect("done", "doing", false)).toEqual({ kind: "refused", why: "Done tickets are closed." });
    expect(dropEffect("doing", "doing", false)).toEqual({ kind: "reorder" });
    expect(dropEffect("backlog", "todo", false)).toEqual({ kind: "start" });
    expect(dropEffect("todo", "backlog", true)).toEqual({ kind: "back_to_backlog", confirm: true });
    expect(dropEffect("todo", "backlog", false)).toEqual({ kind: "back_to_backlog", confirm: false });
    expect(dropEffect("review", "backlog", false)).toEqual({ kind: "back_to_backlog", confirm: true });
    expect(dropEffect("plan_review", "backlog", false)).toEqual({ kind: "reject" });
    expect(dropEffect("plan_review", "doing", false)).toEqual({ kind: "plan" });
    expect(dropEffect("review", "doing", false)).toEqual({ kind: "send_back" });
    expect(dropEffect("review", "done", false)).toEqual({ kind: "mark_done" });
  });

  it("says why it may not", () => {
    const why = (a: Parameters<typeof dropEffect>[0], b: Parameters<typeof dropEffect>[1]) => {
      const e = dropEffect(a, b, false);
      return e.kind === "refused" ? e.why : e.kind;
    };
    expect(why("doing", "testing")).toMatch(/Testing moves/);
    expect(why("backlog", "doing")).toBe("Start first.");
    expect(why("doing", "done")).toBe("Only from Review.");
    expect(why("doing", "review")).toBe("The owner hands it in.");
    expect(why("doing", "plan_review")).toMatch(/engine/);
    expect(why("todo", "doing")).toMatch(/owner is chosen/);
    expect(why("doing", "todo")).toBe("Not from Doing.");
    expect(why("review", "todo")).toBe("Only from Backlog.");
  });

  it("goes above a neighbour, unless nothing changes", () => {
    const cards = [{ id: "a" }, { id: "b" }, { id: "c" }];
    expect(beforeFor(cards, "a", "c")).toBe("c");
    expect(beforeFor(cards, "a", "b")).toBeUndefined();
    expect(beforeFor(cards, "a", "a")).toBeUndefined();
    expect(beforeFor(cards, "a", null)).toBeNull();
    expect(beforeFor(cards, "c", null)).toBeUndefined();
    expect(beforeFor(cards, "x", "a")).toBe("a");
  });

  it("moves a card at once on screen", () => {
    const start = [
      { key: "a", tickets: [{ id: "1" }, { id: "2" }] },
      { key: "b", tickets: [{ id: "3" }] },
    ];
    expect(movedCard(start, "1", "b", "3").map((c) => c.tickets.map((t) => t.id))).toEqual([["2"], ["1", "3"]]);
    expect(movedCard(start, "1", "b", null).map((c) => c.tickets.map((t) => t.id))).toEqual([["2"], ["3", "1"]]);
    expect(movedCard(start, "9", "b", null)).toBe(start);
  });
});

describe("what needs a board member", () => {
  it("per column and state", () => {
    expect(needsYou(ticket({ column: "review" }), false, 0)).toBe("review");
    expect(needsYou(ticket({ column: "plan_review" }), false, 0)).toBe("plan_review");
    expect(needsYou(ticket({ column: "backlog" }), false, 3)).toBeNull();
    expect(needsYou(ticket({ column: "doing", root_task_status: "failed" }), false, 0)).toBe("failed");
    expect(needsYou(ticket({ column: "doing" }), false, 2)).toBe("approval");
    expect(needsYou(ticket({ column: "todo" }), false, 0)).toBe("choose_owner");
    expect(needsYou(ticket({ column: "todo" }), true, 0)).toBeNull();
  });

  it("a login only its own; the rest the Responsible, or everyone without one", () => {
    const waits = ticket({ column: "doing", needs: "login", login_user: "u2" });
    expect(ticketNeeds(waits, "u2", false, 0)).toBe("connect_login");
    expect(ticketNeeds(waits, "u1", false, 0)).toBeNull();
    const review = ticket({ column: "review", responsible_user_id: "u2" });
    expect(needsMe("review", review, "u2")).toBe(true);
    expect(needsMe("review", review, "u1")).toBe(false);
    expect(needsMe("review", { ...review, responsible_disabled: true }, "u1")).toBe(true);
    expect(needsMe("review", ticket({ column: "review" }), "u1")).toBe(true);
    expect(needsMe(null, review, "u2")).toBe(false);
  });

  it("counts a task only when its ticket needs this member", () => {
    const task = { ticket_id: "t", parent_task_id: null } as never;
    expect(taskCounts(task, false, undefined, () => false)).toBe(true);
    expect(taskCounts(task, true, undefined, () => false)).toBe(true);
    expect(taskCounts(task, true, ticket(), () => false)).toBe(false);
    expect(taskCounts(task, true, ticket(), () => true)).toBe(true);
    expect(taskCounts({ ticket_id: "t", parent_task_id: "p" } as never, true, ticket(), () => false)).toBe(true);
  });

  it("words who decides and whose login it waits for", () => {
    expect(decideToo(ticket({ responsible_user_id: "u", responsible_username: "Alex" }))).toBe(
      "Alex is responsible. You can decide too.",
    );
    expect(decideToo(ticket())).toBeNull();
    const waits = ticket({ needs: "login", login_user: "u2", login_username: "Alex" });
    expect(loginWait(waits, "u2", false)).toBe("◆ Waiting for your Claude login");
    expect(loginWait(waits, "u1", false)).toBe("Waiting for Alex's Claude login");
    expect(loginWait(waits, "u1", true)).toBe("Waiting: Alex is disabled");
    expect(loginWait(ticket(), "u1", false)).toBeNull();
  });
});

describe("the dispatcher", () => {
  const e = (id: string, status: string) => ({ id, name: id, status }) as never;
  it("is the project's, else the company's, only while active", () => {
    expect(dispatcherOf("a", "b", [e("a", "active"), e("b", "active")])).toEqual({
      kind: "project",
      id: "a",
      name: "a",
    });
    expect(dispatcherOf(null, "b", [e("b", "active")])).toEqual({ kind: "company", id: "b", name: "b" });
    expect(dispatcherOf("a", "b", [e("a", "paused"), e("b", "active")])).toEqual({
      kind: "inactive",
      name: "a",
      state: "paused",
    });
    expect(dispatcherOf(null, "gone", []).kind).toBe("inactive");
    expect(dispatcherOf(null, null, []).kind).toBe("none");
  });

  it("gave up when the system said so after the last start", () => {
    expect(dispatcherGaveUp([card("ticket.updated", "system"), card("ticket.started")])).toBe(true);
    expect(dispatcherGaveUp([card("ticket.started"), card("ticket.updated", "system")])).toBe(false);
    expect(dispatcherGaveUp([card("ticket.updated", "user")])).toBe(false);
    expect(isChoosing(ticket({ column: "todo" }), true, false)).toBe(true);
    expect(isChoosing(ticket({ column: "todo" }), true, true)).toBe(false);
    expect(isChoosing(ticket({ column: "todo", owner_employee_id: "e" }), true, false)).toBe(false);
  });
});

describe("a deadline on a card", () => {
  const now = Date.UTC(2026, 10, 4, 12, 0);
  it("says how near it is", () => {
    expect(deadlineText(Date.UTC(2026, 10, 20), now, false, 0)).toEqual({ text: "Due Nov 20", tone: "muted" });
    expect(deadlineText(Date.UTC(2026, 10, 4, 17), now, false, 0)).toEqual({ text: "Due today 17:00", tone: "soon" });
    expect(deadlineText(Date.UTC(2026, 10, 5, 9), now, false, 0)).toEqual({ text: "Due tomorrow 09:00", tone: "soon" });
    expect(deadlineText(Date.UTC(2026, 9, 29), now, false, 0)).toEqual({ text: "Overdue · Oct 29", tone: "over" });
    expect(deadlineText(Date.UTC(2026, 9, 29), now, true, 0)).toEqual({ text: "Due Oct 29", tone: "muted" });
  });

  it("goes back to what can be typed", () => {
    expect(deadlineInput(Date.UTC(2026, 10, 4, 23, 59), 0)).toBe("2026-11-04");
    expect(deadlineInput(Date.UTC(2026, 10, 4, 16, 0), 60)).toBe("2026-11-04 17:00");
  });
});

describe("the cards of a column", () => {
  it("filter on owner, find and needs", () => {
    const cards = [
      ticket({ id: "1", number: 21, title: "Screenshots", owner_employee_id: "e" }),
      ticket({ id: "2", number: 22, title: "Build", labels: ["ui"] }),
    ];
    const ids = (f: Partial<typeof anyone> | object) =>
      shownCards("doing", cards, { ...anyone, ...f } as never, false).cards.map((t) => t.id);
    expect(ids({ owner: { kind: "none" } })).toEqual(["2"]);
    expect(ids({ owner: { kind: "employee", id: "e" } })).toEqual(["1"]);
    expect(ids({ query: "shot" })).toEqual(["1"]);
    expect(ids({ query: "#22" })).toEqual(["2"]);
    expect(ids({ query: "UI" })).toEqual(["2"]);
    expect(ids({ needsOnly: true, needs: (t: TicketView) => t.id === "2" })).toEqual(["2"]);
  });

  it("in Done show the newest twenty, the rest behind Show older", () => {
    const done = Array.from({ length: 25 }, (_, i) => ticket({ id: String(i), done_at_ms: i }));
    const { cards, older } = shownCards("done", done, anyone, false);
    expect(cards).toHaveLength(20);
    expect(cards[0]?.id).toBe("24");
    expect(older).toBe(5);
    expect(shownCards("done", done, anyone, true).cards).toHaveLength(25);
  });
});

describe("a ticket's panel", () => {
  it("words the activity, never a raw event name", () => {
    expect(activityText(card("ticket.created"))).toBe("A board member created the ticket");
    expect(activityText(card("ticket.moved", "employee"))).toBe("The owner moved it");
    expect(activityText(card("ticket.tested", "system", "passed"))).toBe("Tests passed");
    expect(activityText(card("ticket.updated", "system"))).toMatch(/Nobody chose/);
    expect(activityText(card("ticket.odd_thing"))).toBe("A board member: odd thing");
    expect(activityText(card("Handed in the report"))).toBe("Handed in the report");
    expect(activityDetail(card("x", "user", "ticket"))).toBeNull();
    expect(activityDetail(card("x", "user", "Some words"))).toBe("Some words");
  });

  it("names the feedback by the newest decision", () => {
    expect(feedbackLabel([card("ticket.sent_back"), card("ticket.plan_approved")])).toBe("Sent back");
    expect(feedbackLabel([card("ticket.plan_revised")])).toBe("New plan asked");
    expect(feedbackLabel(undefined)).toBe("Feedback");
  });

  it("lets the owner be chosen only before the work starts", () => {
    expect(ownerChoosable(ticket({ column: "backlog" }))).toBe(true);
    expect(ownerChoosable(ticket({ column: "todo", root_task_status: "draft" }))).toBe(true);
    expect(ownerChoosable(ticket({ column: "todo", root_task_status: "queued" }))).toBe(false);
    expect(ownerChoosable(ticket({ column: "doing" }))).toBe(false);
  });

  it("holds Done back while tasks still work", () => {
    expect(doneCheck(undefined).kind).toBe("loading");
    const t = ticket({ root_task_id: "root" });
    expect(doneCheck({ ticket: t, tasks: [{ id: "root", status: "running" } as never], as_of_seq: 0 }).kind).toBe(
      "ready",
    );
    expect(doneCheck({ ticket: t, tasks: [{ id: "k", status: "running" } as never], as_of_seq: 0 }).kind).toBe(
      "working",
    );
    expect(doneCheck({ ticket: t, tasks: [], root_task: { live_run: "running" } as never, as_of_seq: 0 }).kind).toBe(
      "working",
    );
  });

  it("finds the newest plan", () => {
    expect(latestPlan([{ update: "plan", text: "old" }, { update: "plan", text: "new" }, { text: "x" }])).toBe("new");
    expect(latestPlan([{ text: "x" }])).toBeNull();
  });
});

describe("My tickets", () => {
  it("sorts into the tabs", () => {
    const t = ticket({ responsible_user_id: "me", created_by_user: "other" });
    expect(inMyTab("responsible", t, [], "me", false)).toBe(true);
    expect(inMyTab("created", t, [], "me", false)).toBe(false);
    expect(inMyTab("following", t, ["t"], "me", false)).toBe(true);
    expect(inMyTab("waiting", t, [], "me", true)).toBe(true);
  });

  it("hides Testing on a server without it", () => {
    expect(columnShown("testing", false)).toBe(false);
    expect(columnShown("testing", true)).toBe(true);
    expect(columnShown("doing", false)).toBe(true);
  });
});

describe("column names", () => {
  it("needs a name, and each its own", () => {
    expect(columnsProblem([{ name: "", title: "Doing" }])).toBe("Give Doing a name.");
    expect(
      columnsProblem([
        { name: "Work", title: "Todo" },
        { name: "work", title: "Doing" },
      ]),
    ).toBe("Each column needs its own name.");
    expect(columnsProblem([{ name: "Work", title: "Todo" }])).toBeNull();
  });
});
