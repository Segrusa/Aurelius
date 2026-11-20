import type { ActivityView, ChannelView, MessageView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  applyReaction,
  audienceLine,
  authorKey,
  authorOf,
  bareName,
  before,
  canMakeTicket,
  clock,
  completeMention,
  dayLabel,
  decisionOf,
  dispatchOf,
  fold,
  indexOfByte,
  isSystemLine,
  lineMark,
  localDay,
  markMentions,
  mentionSuggestions,
  mentions,
  mentionsEveryone,
  mentionsMe,
  newMessagesAt,
  planTitle,
  prefill,
  quickReplies,
  reactionOn,
  readOnlyBar,
  recommendedSplit,
  replies,
  replyLabel,
  rowActions,
  saveOf,
  splitBlocks,
  statusLine,
  streamTo,
  systemText,
  threadLabel,
  threadLine,
  ticketFrom,
  typingMention,
  unknownMentions,
  unknownWarning,
  webSearchSummary,
} from "./chat-logic";

const msg = (over: Partial<MessageView> = {}): MessageView => ({
  id: "m",
  activity: [],
  created_at_ms: 0,
  files: [],
  ordinal: 1,
  queued: false,
  role: "assistant",
  sources: [],
  status: "complete",
  text: "hi",
  uploads: [],
  ...over,
});

const chan = (over: Partial<ChannelView>): ChannelView => ({
  archived: false,
  conversation_id: "c",
  mentions: 0,
  name: "x",
  unread: 0,
  ...over,
});

const step = (tool: string, ok: boolean | null, offset = 0): ActivityView => ({
  call_id: `${tool}${offset}`,
  input: {},
  offset,
  ok,
  tool,
});

describe("grouping", () => {
  const a = msg({ id: "a", author: "Flux", created_at_ms: 0 });
  const day = (ms: number) => localDay(ms, 0);
  it("groups one author within five minutes", () => {
    const b = msg({ id: "b", author: "Flux", created_at_ms: 4 * 60_000 });
    expect(before([a, b], 1, null, day).grouped).toBe(true);
    const late = msg({ id: "b", author: "Flux", created_at_ms: 5 * 60_000 });
    expect(before([a, late], 1, null, day).grouped).toBe(false);
  });
  it("a day or the new line breaks a group", () => {
    const b = msg({ id: "b", author: "Flux", created_at_ms: 86_400_000 });
    expect(before([a, b], 1, null, day)).toEqual({ day: true, new: false, grouped: false });
    const c = msg({ id: "c", author: "Flux", created_at_ms: 1000 });
    expect(before([a, c], 1, "c", day)).toEqual({ day: false, new: true, grouped: false });
  });
  it("system lines and update cards never group", () => {
    expect(authorKey(msg({ system: { kind: "moved" } }))).toBeNull();
    expect(authorKey(msg({ author: "Flux", update: "plan" }))).toBeNull();
    expect(authorKey(msg({ author: "Flux", update: "group" }))).toBe("employee:Flux");
    expect(authorKey(msg({ author: " " }))).toBeNull();
    expect(authorKey(msg({ role: "user" }))).toBe("member");
  });
  it("places the new line", () => {
    expect(newMessagesAt([1, 2, 3], 1, 2)).toBe(1);
    expect(newMessagesAt([1, 2, 3], null, 1)).toBe(2);
    expect(newMessagesAt([1, 2, 3], null, 0)).toBeNull();
    expect(newMessagesAt([1], null, 5)).toBeNull();
  });
});

describe("days and clock", () => {
  it("names days", () => {
    const today = localDay(Date.UTC(2026, 10, 4), 0);
    expect(dayLabel(today, today)).toBe("Today");
    expect(dayLabel(today - 1, today)).toBe("Yesterday");
    expect(dayLabel(localDay(Date.UTC(2026, 9, 27), 0), today)).toBe("Tue 27 Oct");
    expect(dayLabel(localDay(Date.UTC(2025, 9, 27), 0), today)).toBe("Mon 27 Oct 2025");
  });
  it("reads the clock in local time", () => {
    expect(clock(Date.UTC(2026, 0, 1, 23, 30), 60)).toBe("00:30");
    expect(clock(Date.UTC(2026, 0, 1, 0, 30), -60)).toBe("23:30");
  });
});

describe("who wrote it", () => {
  it("has three kinds", () => {
    const roleOf = (n: string) => (n === "Flux" ? "Engineer" : null);
    expect(authorOf(msg({ author_user: { id: "u", username: "Sam", created_at_ms: 0 } }), roleOf)).toEqual({
      name: "Sam",
      kind: "board member",
      human: true,
    });
    expect(authorOf(msg({ role: "user" }), roleOf).name).toBe("You");
    expect(authorOf(msg({ role: "user" }), roleOf, "Flux", "Sam").name).toBe("Sam");
    expect(authorOf(msg({ author: "Flux" }), roleOf)).toEqual({ name: "Flux", kind: "Engineer", human: false });
    expect(authorOf(msg({}), roleOf)).toEqual({ name: "Aurelius", kind: "employee", human: false });
    expect(authorOf(msg({}), roleOf, "Flux")).toEqual({ name: "Flux", kind: "Engineer", human: false });
  });
  it("tells a line from an answer", () => {
    expect(isSystemLine(msg({ update: "system" }), false)).toBe(true);
    expect(isSystemLine(msg({ update: "compacted", author: "Aurelius" }), false)).toBe(true);
    expect(isSystemLine(msg({ update: "status" }), true)).toBe(true);
    expect(isSystemLine(msg({ update: "status" }), false)).toBe(false);
    expect(isSystemLine(msg({ update: "group" }), true)).toBe(true);
    expect(isSystemLine(msg({ update: "group", author: "Flux" }), true)).toBe(false);
    expect(isSystemLine(msg({ update: "group", status: "partial" }), true)).toBe(false);
    expect(isSystemLine(msg({}), true)).toBe(false);
  });
});

describe("mentions", () => {
  const people = [
    { name: "Flux", kind: "Engineer", employee: true },
    { name: "Fiona", kind: "QA", employee: true },
    { name: "Sam", kind: "board member", employee: false },
  ];
  it("sees what is being typed", () => {
    expect(typingMention("hi @fl")).toEqual({ at: 3, word: "fl" });
    expect(typingMention("(@fl")).toEqual({ at: 1, word: "fl" });
    expect(typingMention("a@b")).toBeNull();
    expect(typingMention("@fl x")).toBeNull();
  });
  it("suggests by prefix, none once complete", () => {
    expect(mentionSuggestions("@f", people)?.found.map((p) => p.name)).toEqual(["Flux", "Fiona"]);
    expect(mentionSuggestions("@flux", people)).toBeNull();
    expect(mentionSuggestions("@zz", people)).toBeNull();
    expect(completeMention("hi @f", 3, "Flux")).toBe("hi @Flux ");
  });
  it("lists and warns", () => {
    expect(mentions("@a mail@b.c (@B. @a")).toEqual(["a", "B"]);
    expect(unknownMentions("@Flux @Old @Nobody", people, ["Old"])).toEqual(["Old", "Nobody"]);
    expect(unknownWarning([])).toBeNull();
    expect(unknownWarning(["X"])).toBe("Nobody active is called @X: it wakes no one.");
    expect(unknownWarning(["X", "Y"])).toBe("Nobody active is called @X, @Y: they wake no one.");
  });
  it("marks known names outside code and links", () => {
    expect(markMentions("hi @Flux and @nobody", ["Flux"], null)).toBe("hi [@Flux](#@Flux) and @nobody");
    expect(markMentions("@sam!", ["Sam"], "Sam")).toBe("**[@sam](#@sam)**!");
    expect(markMentions("`@Flux` a@Flux [@Flux](x)", ["Flux"], null)).toBe("`@Flux` a@Flux [@Flux](x)");
    expect(markMentions("```\n@Flux\n```\n@Flux", ["Flux"], null)).toBe("```\n@Flux\n```\n[@Flux](#@Flux)");
  });
});

describe("folding", () => {
  const lines = (n: number) => Array.from({ length: n }, (_, i) => `l${i}`).join("\n");
  it("keeps twenty lines", () => {
    expect(fold(lines(20))).toBeNull();
    expect(fold(`${lines(20)}\n\n\n`)).toBeNull();
    const f = fold(lines(25));
    expect(f?.hidden).toBe(5);
    expect(f?.shown.split("\n")).toHaveLength(20);
  });
  it("closes a code block it cuts open", () => {
    const f = fold(`  ~~~~ts\n${lines(30)}\n  ~~~~`);
    expect(f?.shown.endsWith("\n  ~~~~")).toBe(true);
    expect(fold(`\`\`\`\na\n\`\`\`\n${lines(30)}`)?.shown.endsWith("```")).toBe(false);
  });
  it("splits blocks outside code only", () => {
    expect(splitBlocks("a\nb\n\nc")).toEqual(["a\nb\n", "c"]);
    expect(splitBlocks("```\na\n\nb\n```\n\nc")).toEqual(["```\na\n\nb\n```\n", "c"]);
  });
  it("titles a plan", () => {
    expect(planTitle("\n## My plan\nx")).toBe("My plan");
    expect(planTitle("")).toBe("Plan");
  });
});

describe("web search", () => {
  it("turns results into links", () => {
    const out = 'Search\nLinks: [{"title":"A [b]","url":"https://www.x.org/p"}]\nrest';
    expect(webSearchSummary(out)).toBe("- [A b](<https://www.x.org/p>) · x.org");
    expect(webSearchSummary("plain")).toBe("plain");
    expect(webSearchSummary("Links: [{bad}]")).toBe("Links: [{bad}]");
  });
});

describe("channels", () => {
  it("names a channel and words the read-only bar", () => {
    expect(bareName(chan({ kind: "ticket", name: "#42-login" }))).toBe("42-login");
    expect(readOnlyBar(chan({ ticket_number: 12 }), true)).toBe("#12 is Done · read-only.");
    expect(readOnlyBar(chan({ ticket_number: 12 }), false)).toBe("#12 is archived · read-only.");
    expect(readOnlyBar(chan({}), false)).toBe("This project is archived · read-only.");
  });
});

describe("threads and reactions", () => {
  const caps = { threads: true, reactions: true, readOnly: false };
  it("offers what fits", () => {
    expect(rowActions(msg(), caps)).toEqual({ react: true, reply: true });
    expect(rowActions(msg({ thread_parent: "r" }), caps)).toEqual({ react: true, reply: false });
    expect(rowActions(msg({ status: "partial" }), caps)).toEqual({ react: false, reply: false });
    expect(rowActions(msg({ system: { kind: "moved" } }), caps).react).toBe(false);
    expect(rowActions(msg(), { ...caps, readOnly: true }).react).toBe(false);
    expect(rowActions(msg(), { ...caps, threads: false }).reply).toBe(false);
  });
  it("reads a thread line", () => {
    const today = localDay(Date.UTC(2026, 10, 4), 0);
    const last = Date.UTC(2026, 10, 4, 11, 2);
    expect(replies(1)).toBe("1 reply");
    expect(threadLine(msg({ thread_reply_count: 3, thread_last_reply_ms: last }), 0, today)).toBe(
      "↳ 3 replies · last 11:02",
    );
    expect(threadLine(msg({ thread_reply_count: 0 }), 0, today)).toBeNull();
    expect(threadLabel(msg({ thread_reply_count: 3, thread_unread: 2, thread_last_reply_ms: last }), 0, today)).toBe(
      "Thread, 3 replies, 2 unread, last reply 11:02",
    );
  });
  it("routes a stream", () => {
    expect(streamTo(null, "r")).toBe("channel");
    expect(streamTo("r", "r")).toBe("thread");
    expect(streamTo("r", null)).toBe("elsewhere");
  });
  it("toggles a reaction", () => {
    expect(reactionOn([{ emoji: "👍", count: 1, mine: true }], "👍")).toBe(false);
    expect(applyReaction([], "👍", true, "Sam")).toEqual([{ emoji: "👍", count: 1, mine: true, by: ["Sam"] }]);
    expect(applyReaction([{ emoji: "👍", count: 1, mine: false, by: ["Ava"] }], "👍", true, "Sam")[0]).toEqual({
      emoji: "👍",
      count: 2,
      mine: true,
      by: ["Ava", "Sam"],
    });
    expect(applyReaction([{ emoji: "👍", count: 1, mine: true, by: ["Sam"] }], "👍", false, "Sam")).toEqual([]);
    expect(
      applyReaction([{ emoji: "👍", count: 2, mine: true, by: ["Ava", "Sam"] }], "👍", false, "Sam")[0]?.by,
    ).toEqual(["Ava"]);
  });
});

describe("progress", () => {
  it("reads quick replies", () => {
    const with_ = (options: unknown) => [
      { ...step("mcp__aurelius__suggest_replies", true), input: { question: " Q? ", options } },
    ];
    expect(quickReplies(with_(["a", "b", "c", "d"]))).toEqual({ question: "Q?", options: ["a", "b", "c"] });
    expect(quickReplies(with_(["a"]))).toBeNull();
    expect(replyLabel("x".repeat(40))).toHaveLength(40);
    expect(replyLabel("x".repeat(41))).toBe(`${"x".repeat(39)}…`);
  });
});

describe("system lines", () => {
  it("words a line without text", () => {
    expect(systemText({ kind: "moved", by: "Sam", from: "Todo", to: "Doing" }, "", 12)).toBe(
      "Sam moved the ticket from Todo to Doing",
    );
    expect(systemText({ kind: "moved", action: "opened", by: "Sam" }, "", 12)).toBe("#12 opened · Sam");
    expect(systemText({ kind: "moved" }, " given ", null)).toBe("given");
    expect(systemText({ kind: "limit" }, "", null)).toBe("Limit reached: nobody was not woken.");
  });
  it("marks a line", () => {
    expect(lineMark({ kind: "moved" }, "")).toEqual({ mark: "→ ", failed: false });
    expect(lineMark({ kind: "reported" }, "It failed")).toEqual({ mark: "▶ ✕ ", failed: true });
    expect(lineMark({ kind: "limit" }, "Paused: x")).toEqual({ mark: "⏸ ", failed: false });
    expect(lineMark({ kind: "moved", action: "tested_failed" }, "").failed).toBe(true);
  });
});

describe("tickets and edits", () => {
  it("prefills a ticket", () => {
    const p = prefill("# Title\n\nbody", "Flux", "#general", "11:02");
    expect(p.title).toBe("Title");
    expect(p.description).toBe("> # Title\n>\n> body\n\nFrom Flux in #general · 11:02");
    expect(prefill("", "a", "b", "c").title).toBe("From a message");
    expect(canMakeTicket(msg())).toBe(true);
    expect(canMakeTicket(msg({ text: " " }))).toBe(false);
  });
  it("saves an edit", () => {
    expect(saveOf("a", " ")).toEqual({ kind: "off" });
    expect(saveOf("a", " a ")).toEqual({ kind: "close" });
    expect(saveOf("a", "b")).toEqual({ kind: "send", text: "b" });
  });
});

describe("an answer in order", () => {
  it("counts bytes where a step happened", () => {
    expect(indexOfByte("é€x", 2)).toBe(1);
    expect(indexOfByte("é€x", 5)).toBe(2);
    expect(indexOfByte("😀x", 4)).toBe(2);
  });
});

describe("links between messages", () => {
  it("finds the ticket made from a message", () => {
    const line = msg({
      id: "l",
      text: "Made #52",
      system: { kind: "ticket_created", message_id: "m", ticket_id: "t" },
    });
    expect(ticketFrom([msg({ id: "m" }), line], "m")).toEqual({ ticket: "t", label: "#52" });
    expect(ticketFrom([line], "other")).toBeNull();
  });
  it("sees a call by name", () => {
    expect(mentionsMe("hi @sam!", "Sam")).toBe(true);
    expect(mentionsMe("hi", "Sam")).toBe(false);
    expect(mentionsMe("@sam", null)).toBe(false);
  });
});

describe("@Everyone", () => {
  const people = [{ name: "Flux", kind: "Frontend", employee: true }];
  it("is offered only when the server has it", () => {
    expect(mentionSuggestions("@e", people)).toBeNull();
    expect(mentionSuggestions("@e", people, true)?.found.map((p) => p.name)).toEqual(["Everyone"]);
    expect(mentionSuggestions("@", people, true)?.found.map((p) => p.name)).toEqual(["Everyone", "Flux"]);
    expect(mentionSuggestions("@everyone", people, true)).toBeNull();
  });
  it("is found as a whole word in any case", () => {
    expect(mentionsEveryone("hi @everyone, look")).toBe(true);
    expect(mentionsEveryone("(@EVERYONE)")).toBe(true);
    expect(mentionsEveryone("mail@everyone.com")).toBe(false);
    expect(mentionsEveryone("@Everyoneelse")).toBe(false);
  });
  it("is no unknown name when offered", () => {
    expect(unknownMentions("@Everyone", people, [], true)).toEqual([]);
    expect(unknownMentions("@Everyone", people, [], false)).toEqual(["Everyone"]);
  });
  it("shows as a mention when known", () => {
    expect(markMentions("@everyone go", ["Everyone"], null)).toBe("[@everyone](#@everyone) go");
  });
  it("names the coordinator and who reads along", () => {
    expect(audienceLine({ employees: 5, paused: 1, board_members: 2, coordinator: "Orion" })).toBe(
      "Orion picks who follows up; 5 employees can read along (1 paused), 2 board members are told",
    );
    expect(audienceLine({ employees: 1, paused: 0, board_members: 1, coordinator: "Ann" })).toBe(
      "Ann picks who follows up; 1 employee can read along, 1 board member is told",
    );
    expect(audienceLine({ employees: 3, paused: 0, board_members: 0, coordinator: "Ann" })).toBe(
      "Ann picks who follows up; 3 employees can read along",
    );
  });
  it("keeps the old line on an older server", () => {
    expect(audienceLine({ employees: 5, paused: 1, board_members: 2 })).toBe(
      "Wakes 5 employees (1 paused) and tells 2 board members",
    );
    expect(audienceLine({ employees: 1, paused: 0, board_members: 1 })).toBe(
      "Wakes 1 employee and tells 1 board member",
    );
    expect(audienceLine({ employees: 0, paused: 2, board_members: 0 })).toBe("Wakes no employees (2 paused)");
  });
});

describe("statusLine", () => {
  it("says the context is being compacted while it is", () => {
    expect(statusLine("", false, true)).toBe("Compacting context…");
    expect(statusLine("half an answer", false, true)).toBe("Compacting context…");
    expect(statusLine("", false, false)).toBe("Thinking…");
    expect(statusLine("text", false, false)).toBe("Writing…");
    expect(statusLine("", true, true)).toBe("Waiting for your approval");
  });
});

describe("the board member's decisions in a run", () => {
  const status = (text: string) => msg({ update: "status", author: "You", text });

  it("reads each question with its answer", () => {
    const d = decisionOf(
      status(
        "Answered:\n\n- What happens to the history?\n  **Delete it**\n- What goes?\n  **1-on-1 + Team chat (Recommended)**",
      ),
    );
    expect(d).toEqual({
      kind: "answered",
      answers: [
        { question: "What happens to the history?", answer: "Delete it" },
        { question: "What goes?", answer: "1-on-1 + Team chat (Recommended)" },
      ],
    });
  });

  it("keeps an answer over several lines in one piece", () => {
    const d = decisionOf(status("Answered:\n\n- Why?\n  **First line\n- not a new item**"));
    expect(d).toEqual({ kind: "answered", answers: [{ question: "Why?", answer: "First line\n- not a new item" }] });
  });

  it("reads a plan decision with its feedback", () => {
    expect(decisionOf(status("Approved the plan."))).toMatchObject({ verdict: "approved", feedback: null });
    expect(decisionOf(status("Asked for changes to the plan: smaller steps"))).toMatchObject({
      verdict: "changes",
      feedback: "smaller steps",
    });
    expect(decisionOf(status("Rejected the plan."))).toMatchObject({ verdict: "rejected", feedback: null });
  });

  it("reads the server's decision field before the text", () => {
    const answered = msg({
      update: "status",
      author: "You",
      text: "a sentence this client cannot read",
      decision: { kind: "answered", answers: [{ question: "What goes?", answer: " All of it " }] },
    });
    expect(decisionOf(answered)).toEqual({
      kind: "answered",
      answers: [{ question: "What goes?", answer: "All of it" }],
    });
    const changes = msg({ update: "status", text: "x", decision: { kind: "plan_changes", feedback: "smaller steps" } });
    expect(decisionOf(changes)).toEqual({
      kind: "plan",
      verdict: "changes",
      title: "You asked for changes to the plan",
      feedback: "smaller steps",
    });
    const edits = msg({ update: "status", text: "x", decision: { kind: "plan_approved_edits", feedback: "ignored" } });
    expect(decisionOf(edits)).toMatchObject({ verdict: "approved", feedback: null });
  });

  it("shows a decision kind it does not know as the plain line", () => {
    const later = msg({
      update: "status",
      author: "You",
      text: "Approved the plan.",
      decision: { kind: "plan_paused" },
    });
    expect(decisionOf(later)).toBeNull();
  });

  it("leaves other messages alone", () => {
    expect(decisionOf(msg({ text: "Answered:\n\n- q\n  **a**" }))).toBeNull();
    expect(decisionOf(msg({ update: "status", author: "Forge", text: "Approved the plan." }))).toBeNull();
    expect(decisionOf(status("Something else"))).toBeNull();
  });

  it("splits off a trailing (Recommended) only", () => {
    expect(recommendedSplit("Keep it (Recommended)")).toEqual({ text: "Keep it", recommended: true });
    expect(recommendedSplit("(Recommended) first, then more")).toEqual({
      text: "(Recommended) first, then more",
      recommended: false,
    });
  });
});

describe("dispatchOf", () => {
  const line = (text: string, over: Partial<MessageView> = {}) =>
    msg({ update: "system", author: "Aurelius", text, ...over });
  it("reads the dispatcher choosing", () => {
    expect(dispatchOf(line("Orion is choosing who takes this…"))).toEqual({ kind: "choosing", by: "Orion" });
  });
  it("reads who it chose and why", () => {
    expect(dispatchOf(line("Orion chose Flux: This is front-end work."))).toEqual({
      kind: "chose",
      by: "Orion",
      chosen: "Flux",
      reason: "This is front-end work.",
    });
  });
  it("reads a board member's choice without a reason", () => {
    expect(dispatchOf(line("grace chose Flux."))).toEqual({ kind: "chose", by: "grace", chosen: "Flux", reason: null });
  });
  it("leaves other lines alone", () => {
    expect(dispatchOf(line("The dispatcher chose nobody. Choose who takes this session."))).toBeNull();
    expect(dispatchOf(line("Orion chose Flux: x", { update: "status" }))).toBeNull();
    expect(dispatchOf(line("Orion chose Flux: x", { author: "Orion" }))).toBeNull();
    expect(dispatchOf(line("Orion chose Flux: x", { system: { kind: "moved" } }))).toBeNull();
  });
});
