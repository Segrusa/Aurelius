import { describe, expect, it } from "vitest";
import {
  activeSessions,
  chooserText,
  runChange,
  runChipLabel,
  SIDEBAR_SESSIONS,
  sessionFolder,
  sessionSignal,
  sessionStatus,
  sidebarSessions,
  startOfDay,
  withEarlier,
} from "./session-logic";

describe("the session's side panel", () => {
  it("says it works while a run is queued or under way, else that it waits for you", () => {
    expect(sessionStatus(true)).toBe("Working");
    expect(sessionStatus(false)).toBe("Waiting for you");
    expect(sessionStatus(true, "choosing")).toBe("Dispatcher is choosing");
    expect(sessionStatus(false, "nobody")).toBe("Waiting for your choice");
  });

  it("shows the worktree, or the project folder when it has none", () => {
    expect(sessionFolder({ worktree: "/data/worktrees/session-ab12" })).toBe("/data/worktrees/session-ab12");
    expect(sessionFolder({ worktree: null })).toBe("Project folder");
    expect(sessionFolder({})).toBe("Project folder");
  });
});

describe("a session the dispatcher gives to someone", () => {
  it("puts a chooser where the composer is until somebody takes it", () => {
    expect(chooserText({ employee_name: "Orion", dispatch: "choosing" })).toContain("You can also choose yourself");
    expect(chooserText({ employee_name: "Orion", dispatch: "nobody" })).toBe(
      "The dispatcher chose nobody. Choose who takes this session.",
    );
    expect(chooserText({ employee_name: "Flux" })).toBeNull();
  });
});

describe("sidebarSessions", () => {
  const sessions = Array.from({ length: SIDEBAR_SESSIONS + 3 }, (_, i) => ({ id: i, project_id: i % 2 ? "a" : "b" }));

  it("keeps the workspace's project only, when one is chosen", () => {
    expect(sidebarSessions(sessions, "a").every((s) => s.project_id === "a")).toBe(true);
  });

  it("lists at most a few, the latest first as the server sends them", () => {
    const shown = sidebarSessions(sessions, null);
    expect(shown).toHaveLength(SIDEBAR_SESSIONS);
    expect(shown[0]?.id).toBe(0);
  });

  it("never leaves out a session that is active, even past the limit", () => {
    const many = Array.from({ length: SIDEBAR_SESSIONS + 3 }, (_, i) => ({
      id: i,
      project_id: "a",
      running: i === SIDEBAR_SESSIONS + 1,
    }));
    const shown = sidebarSessions(many, null);
    expect(shown).toHaveLength(SIDEBAR_SESSIONS + 1);
    expect(shown.at(-1)?.id).toBe(SIDEBAR_SESSIONS + 1);
  });

  it("by default keeps today's sessions and the active ones, with no limit", () => {
    const today = 1000;
    const list = [
      ...Array.from({ length: SIDEBAR_SESSIONS + 2 }, (_, i) => ({
        id: i,
        project_id: "a",
        last_activity_ms: today + i,
      })),
      { id: "yesterday", project_id: "a", last_activity_ms: today - 1 },
      { id: "running", project_id: "a", last_activity_ms: 1, running: true },
      { id: "choose", project_id: "a", last_activity_ms: 1, dispatch: "nobody" as const },
    ];
    const shown = sidebarSessions(list, null, today).map((s) => s.id);
    expect(shown).toHaveLength(SIDEBAR_SESSIONS + 4);
    expect(shown).not.toContain("yesterday");
    expect(shown).toContain("running");
    expect(shown).toContain("choose");
  });

  it("sorts the most recently used first, whatever order the server sends", () => {
    const list = [
      { id: "old", project_id: "a", last_activity_ms: 10 },
      { id: "new", project_id: "a", last_activity_ms: 30 },
      { id: "mid", project_id: "a", last_activity_ms: 20 },
    ];
    expect(sidebarSessions(list, null).map((s) => s.id)).toEqual(["new", "mid", "old"]);
    expect(sidebarSessions(list, null, 0).map((s) => s.id)).toEqual(["new", "mid", "old"]);
  });

  it("counts opening a session as using it, also for today's view", () => {
    const list = [
      { id: "busy", project_id: "a", last_activity_ms: 30 },
      { id: "opened", project_id: "a", last_activity_ms: 5 },
    ];
    const usedAt = (s: (typeof list)[number]) => Math.max(s.last_activity_ms, s.id === "opened" ? 40 : 0);
    expect(sidebarSessions(list, null, null, usedAt).map((s) => s.id)).toEqual(["opened", "busy"]);
    expect(sidebarSessions(list, null, 35, usedAt).map((s) => s.id)).toEqual(["opened"]);
  });

  it("shows earlier ones only on request, keeping all of today's and the list's order", () => {
    const id = (s: string) => s;
    const ordered = ["t1", "t2", "y1", "y2", "y3"];
    expect(withEarlier(ordered, ["t1", "t2"], ["t1", "y1"], id, false)).toEqual({
      list: ["t1", "t2"],
      hasEarlier: true,
    });
    expect(withEarlier(ordered, ["t1", "t2"], ["t1", "y1"], id, true)).toEqual({
      list: ["t1", "t2", "y1"],
      hasEarlier: true,
    });
    expect(withEarlier(ordered, ["t1", "t2"], ["t1"], id, false).hasEarlier).toBe(false);
  });

  it("starts today at local midnight", () => {
    const noon = new Date(2026, 10, 18, 12, 30).getTime();
    expect(startOfDay(noon)).toBe(new Date(2026, 10, 18).getTime());
  });
});

describe("sessionSignal", () => {
  it("says a session works while a run of it is queued or under way", () => {
    expect(sessionSignal({ running: true })).toBe("working");
  });

  it("counts the dispatcher choosing as working, and its choice of nobody as needing you", () => {
    expect(sessionSignal({ running: false, dispatch: "choosing" })).toBe("working");
    expect(sessionSignal({ running: false, dispatch: "nobody" })).toBe("needs_you");
  });

  it("gives an idle session no signal", () => {
    expect(sessionSignal({ running: false })).toBeNull();
    expect(sessionSignal({ running: false, dispatch: null })).toBeNull();
    expect(sessionSignal({ running: false, unseen: false })).toBeNull();
  });

  it("marks a finished answer you have not opened yet as done, below working and needing you", () => {
    expect(sessionSignal({ running: false, unseen: true })).toBe("done");
    expect(sessionSignal({ running: true, unseen: true })).toBe("working");
    expect(sessionSignal({ running: false, dispatch: "nobody", unseen: true })).toBe("needs_you");
  });
});

describe("activeSessions", () => {
  it("counts the sessions that work now", () => {
    expect(activeSessions([{ running: true }, { running: false, dispatch: "choosing" }, { running: false }])).toBe(2);
    expect(activeSessions([])).toBe(0);
  });
});

describe("the run chips", () => {
  it("say the choice, or that the employee's own applies", () => {
    expect(runChipLabel(null)).toBe("Employee's default");
    expect(runChipLabel("high")).toBe("high");
    expect(runChipLabel("opus", (m) => m.toUpperCase())).toBe("OPUS");
  });

  it("send the picked one and keep the other", () => {
    const current = { model: "opus", effort: "high" };
    expect(runChange("c1", current, { effort: "low" })).toEqual({
      conversation_id: "c1",
      model: "opus",
      effort: "low",
    });
    expect(runChange("c1", current, { model: "default" })).toEqual({
      conversation_id: "c1",
      model: null,
      effort: "high",
    });
    expect(runChange("c1", {}, { model: "sonnet" })).toEqual({ conversation_id: "c1", model: "sonnet", effort: null });
  });
});
