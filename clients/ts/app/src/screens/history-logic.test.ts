import { describe, expect, it } from "vitest";
import { areasOf } from "../data";
import {
  ALL_PROJECTS,
  dayLabel,
  groupByDay,
  historyFilters,
  historyProject,
  narrowed,
  nextPage,
  ownerName,
  periodRange,
  projectInUrl,
  summaryShown,
} from "./history-logic";

// Saturday 14 November 2026, 15:30 local time.
const NOW = new Date(2026, 10, 14, 15, 30);
const at = (month: number, day: number, hour = 12) => new Date(2026, month, day, hour).getTime();

describe("the History's filters in the URL", () => {
  it("keeps what is valid and drops the rest", () => {
    expect(historyFilters({ project: "p1", employee: "e1", period: "7d", q: "export", ticket: "t1" })).toEqual({
      project: "p1",
      employee: "e1",
      period: "7d",
      q: "export",
      ticket: "t1",
    });
    expect(historyFilters({ period: "yesterday", q: "  ", project: 3 })).toEqual({});
    // All time is the default, so it is not written.
    expect(historyFilters({ period: "all" })).toEqual({});
  });

  it("takes days only for a custom period, and only real days", () => {
    expect(historyFilters({ period: "custom", from: "2026-11-01", to: "2026-11-07" })).toEqual({
      period: "custom",
      from: "2026-11-01",
      to: "2026-11-07",
    });
    expect(historyFilters({ period: "7d", from: "2026-11-01" })).toEqual({ period: "7d" });
    expect(historyFilters({ period: "custom", from: "1 Nov", to: "2026-13-45" })).toEqual({ period: "custom" });
  });

  it("lets All projects win over the workspace's project", () => {
    // No project in the URL: the workspace's project, if one is chosen.
    expect(historyProject({}, "w1")).toBe("w1");
    expect(historyProject({}, null)).toBeNull();
    // A project in the URL wins; "all" means every project, also inside a workspace.
    expect(historyProject({ project: "p1" }, "w1")).toBe("p1");
    expect(historyProject({ project: ALL_PROJECTS }, "w1")).toBeNull();
    // Choosing All projects writes "all" only when a workspace would otherwise narrow it.
    expect(projectInUrl(ALL_PROJECTS, "w1")).toBe(ALL_PROJECTS);
    expect(projectInUrl(ALL_PROJECTS, null)).toBeUndefined();
    expect(projectInUrl("p1", "w1")).toBe("p1");
    expect(narrowed({ project: ALL_PROJECTS })).toBe(false);
  });

  it("knows when something narrows the list", () => {
    expect(narrowed({})).toBe(false);
    expect(narrowed({ ticket: "t1" })).toBe(false);
    expect(narrowed({ period: "all" })).toBe(false);
    expect(narrowed({ q: "x" })).toBe(true);
    expect(narrowed({ period: "30d" })).toBe(true);
  });
});

describe("the period", () => {
  it("counts whole days, today included", () => {
    expect(periodRange({ period: "7d" }, NOW)).toEqual({ from_ms: new Date(2026, 10, 8).getTime() });
    expect(periodRange({ period: "30d" }, NOW)).toEqual({ from_ms: new Date(2026, 9, 16).getTime() });
    expect(periodRange({}, NOW)).toEqual({});
  });

  it("includes the last chosen day", () => {
    expect(periodRange({ period: "custom", from: "2026-11-01", to: "2026-11-07" }, NOW)).toEqual({
      from_ms: new Date(2026, 10, 1).getTime(),
      to_ms: new Date(2026, 10, 8).getTime(),
    });
    expect(periodRange({ period: "custom", to: "2026-11-07" }, NOW)).toEqual({
      to_ms: new Date(2026, 10, 8).getTime(),
    });
  });
});

describe("rows per day", () => {
  it("names today and yesterday, else the date", () => {
    expect(dayLabel(at(10, 14, 0), NOW, "en-GB")).toBe("Today");
    expect(dayLabel(at(10, 13, 23), NOW, "en-GB")).toBe("Yesterday");
    // The exact punctuation is the runtime's (ICU); the parts are ours.
    const thisYear = dayLabel(at(10, 9), NOW, "en-GB");
    expect(thisYear).toMatch(/Mon.*9 Nov/);
    expect(thisYear).not.toContain("2026");
    expect(dayLabel(new Date(2025, 11, 31, 12).getTime(), NOW, "en-GB")).toMatch(/Wed.*31 Dec 2025/);
  });

  it("groups newest first, one group per day", () => {
    const rows = [
      { done_at_ms: at(10, 14, 15), n: 4 },
      { done_at_ms: at(10, 14, 9), n: 3 },
      { done_at_ms: at(10, 13), n: 2 },
      { done_at_ms: at(10, 2), n: 1 },
    ];
    const groups = groupByDay(rows, NOW, "en-GB");
    expect(groups.map((g) => [g.label, g.entries.map((e) => e.n)])).toEqual([
      ["Today", [4, 3]],
      ["Yesterday", [2]],
      [dayLabel(at(10, 2), NOW, "en-GB"), [1]],
    ]);
  });
});

describe("a row", () => {
  it("says where its summary stands, never an endless wait", () => {
    expect(summaryShown({ summary: "It shipped.", summary_state: "ready" })).toEqual({
      text: "It shipped.",
      kind: "summary",
    });
    expect(summaryShown({ summary_state: "summarizing" })).toEqual({ text: "Summarizing…", kind: "busy" });
    expect(summaryShown({ summary_state: "not_available" })).toEqual({ text: "Summary not available", kind: "failed" });
    expect(summaryShown({ summary_state: "before_history" })).toEqual({
      text: "No summary (finished before History)",
      kind: "quiet",
    });
  });

  it("names a missing owner with a dash", () => {
    expect(ownerName({ owner_name: "Ann" })).toBe("Ann");
    expect(ownerName({})).toBe("—");
  });

  it("pages on the last row's Done moment and number", () => {
    const entries = [
      { done_at_ms: 20, number: 9 },
      { done_at_ms: 10, number: 4 },
    ];
    expect(nextPage({ has_more: true, entries })).toEqual({ before_done_at_ms: 10, before_number: 4 });
    expect(nextPage({ has_more: false, entries })).toBeUndefined();
    expect(nextPage({ has_more: true, entries: [] })).toBeUndefined();
  });

  it("refreshes on a ticket event, so a new Done or summary shows without a reload", () => {
    expect(areasOf("ticket.updated")).toContain("tickets");
    expect(areasOf("ticket.accepted")).toContain("tickets");
  });
});
