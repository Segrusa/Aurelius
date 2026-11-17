import type { ChannelView, EmployeeChatView, EmployeeSummary } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import { chatAge, dockAway, dockChannels, dockChats, dockTickets, launcherAway, parseDock } from "./chat-dock-logic";

const chat = (name: string, unread = 0) =>
  ({
    conversation_id: `c-${name}`,
    employee_id: name,
    employee_name: name,
    unread,
    last_activity_ms: 0,
  }) as EmployeeChatView;
const employee = (name: string, status = "active") => ({ id: name, name, status }) as unknown as EmployeeSummary;

describe("the chat popout", () => {
  it("stays away where a chat fills the screen", () => {
    expect(dockAway("/chat")).toBe(true);
    expect(dockAway("/talk/abc")).toBe(true);
    expect(dockAway("/board")).toBe(false);
    expect(dockAway("/chatter")).toBe(false);
    // A ticket's session has its own composer where the dock would sit.
    expect(dockAway("/ticket/70")).toBe(true);
  });

  it("hides only its launcher where a message box sits in that corner", () => {
    expect(launcherAway("/board", true)).toBe(true);
    expect(launcherAway("/tasks/t1", false)).toBe(true);
    expect(launcherAway("/tasks", false)).toBe(false);
    expect(launcherAway("/board", false)).toBe(false);
  });

  it("lists chats, and active employees without one, by name", () => {
    const list = dockChats(
      [chat("Flux", 2), chat("Atlas", 1)],
      [employee("Flux"), employee("Pixel"), employee("Old", "archived")],
      "",
    );
    expect(list.chats.map((c) => c.employee_name)).toEqual(["Flux", "Atlas"]);
    expect(list.others.map((e) => e.name)).toEqual(["Pixel"]);
    expect(list.unread).toBe(3);
    expect(dockChats([chat("Flux")], [employee("Pixel")], "PIX")).toMatchObject({
      chats: [],
      others: [{ name: "Pixel" }],
    });
  });

  it("lists open tickets with their title, column and owner, and finds any by search", () => {
    const channel = (n: number, over: Partial<ChannelView> = {}) =>
      ({
        conversation_id: `d${n}`,
        kind: "ticket",
        archived: false,
        name: `#${n}-slug`,
        ticket_id: `t${n}`,
        ticket_number: n,
        ticket_column: "plan_review",
        unread: 0,
        mentions: 0,
        last_activity_ms: n,
        ...over,
      }) as ChannelView;
    const list = dockTickets(
      [channel(1), channel(2, { archived: true }), { ...channel(3), kind: "project" }],
      [{ id: "t1", title: "Trim the texts", owner_name: "Pixel" }],
      "",
    );
    expect(list.map((t) => [t.title, t.line])).toEqual([["#1 Trim the texts", "Plan review · Pixel"]]);
    expect(list[0]?.number).toBe(1);
    const many = Array.from({ length: 20 }, (_, i) => channel(i + 1));
    expect(dockTickets(many, [], "")).toHaveLength(12);
    expect(dockTickets(many, [], "#17 ").map((t) => t.ticketId)).toEqual(["t17"]);
  });

  it("says how old a chat is", () => {
    expect(chatAge(0, 30_000)).toBe("now");
    expect(chatAge(0, 5 * 60_000)).toBe("5m");
    expect(chatAge(0, 3 * 3_600_000)).toBe("3h");
    expect(chatAge(0, 2 * 86_400_000)).toBe("2d");
    expect(chatAge(0, 8 * 86_400_000)).toBe("1w");
  });

  it("reads back what it stored, and survives anything else", () => {
    expect(parseDock(JSON.stringify({ open: true, conversation: "c1", ticket: "t1" }))).toEqual({
      open: true,
      conversation: "c1",
      ticket: "t1",
    });
    expect(parseDock("{bad")).toEqual({ open: false, conversation: null, ticket: null });
    expect(parseDock(null)).toEqual({ open: false, conversation: null, ticket: null });
  });
});

describe("the channels in the popout", () => {
  const channel = (name: string, kind: string) =>
    ({ conversation_id: `c-${name}`, name, kind, archived: false, unread: 0, mentions: 0 }) as unknown as ChannelView;
  const all = [channel("General", "general"), channel("Apollo", "project"), channel("#1-fix", "ticket")];
  it("lists general and the projects, not the tickets", () => {
    expect(dockChannels(all, "").map((c) => c.name)).toEqual(["General", "Apollo"]);
  });
  it("narrows by the search", () => {
    expect(dockChannels(all, "apo").map((c) => c.name)).toEqual(["Apollo"]);
  });
});
