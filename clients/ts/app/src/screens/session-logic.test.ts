import { describe, expect, it } from "vitest";
import {
  chooserText,
  runChange,
  runChipLabel,
  SIDEBAR_SESSIONS,
  sessionFolder,
  sessionPlace,
  sessionStatus,
  sessionWho,
  sidebarSessions,
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

describe("sessionPlace", () => {
  it("names the employee, the project and the folder it works in", () => {
    expect(sessionPlace({ employee_name: "Orion", project_name: "Planora", branch: null })).toBe(
      "Orion · Planora · project folder",
    );
    expect(sessionPlace({ employee_name: "Orion", project_name: "Planora", branch: "session/1a2b3c4d" })).toBe(
      "Orion · Planora · own worktree on session/1a2b3c4d",
    );
  });
});

describe("a session the dispatcher gives to someone", () => {
  it("names the dispatcher while it chooses, nobody after it chose nobody, else the employee", () => {
    expect(sessionWho({ employee_name: "Orion", dispatch: "choosing" })).toBe("Orion is choosing who takes it");
    expect(sessionWho({ employee_name: "Orion", dispatch: "nobody" })).toBe("Nobody yet");
    expect(sessionWho({ employee_name: "Flux" })).toBe("Flux");
    expect(sessionPlace({ employee_name: "Orion", project_name: "Planora", branch: null, dispatch: "choosing" })).toBe(
      "Orion is choosing who takes it · Planora · project folder",
    );
  });

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
