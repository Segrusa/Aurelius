import type { EmployeeChatView, EmployeeSummary } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import { chatAge, dockAway, dockChats, launcherAway, parseDock } from "./chat-dock-logic";

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

  it("says how old a chat is", () => {
    expect(chatAge(0, 30_000)).toBe("now");
    expect(chatAge(0, 5 * 60_000)).toBe("5m");
    expect(chatAge(0, 3 * 3_600_000)).toBe("3h");
    expect(chatAge(0, 2 * 86_400_000)).toBe("2d");
    expect(chatAge(0, 8 * 86_400_000)).toBe("1w");
  });

  it("reads back what it stored, and survives anything else", () => {
    expect(parseDock(JSON.stringify({ open: true, conversation: "c1" }))).toEqual({ open: true, conversation: "c1" });
    expect(parseDock("{bad")).toEqual({ open: false, conversation: null });
    expect(parseDock(null)).toEqual({ open: false, conversation: null });
  });
});
