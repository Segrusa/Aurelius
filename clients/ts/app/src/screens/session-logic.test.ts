import { describe, expect, it } from "vitest";
import { runChange, runChipLabel, SIDEBAR_SESSIONS, sessionPlace, sidebarSessions } from "./session-logic";

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
