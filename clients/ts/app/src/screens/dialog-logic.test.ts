import { describe, expect, it } from "vitest";
import {
  cycle,
  DIALOG_TOP,
  defaultAssignee,
  parseDeadline,
  parseLabels,
  rootsOf,
  titleOfGoal,
  topCentered,
} from "./dialog-logic";

describe("a deadline", () => {
  it("is empty, a date (the end of that day) or a date and time, in local time", () => {
    expect(parseDeadline("  ", 0)).toBeNull();
    expect(parseDeadline("2026-11-04", 0)).toBe(Date.UTC(2026, 10, 4, 23, 59));
    expect(parseDeadline("2026-11-04 17:00", 0)).toBe(Date.UTC(2026, 10, 4, 17, 0));
    expect(parseDeadline("2026-11-04T17:00", 60)).toBe(Date.UTC(2026, 10, 4, 16, 0));
  });

  it("refuses what is no date", () => {
    for (const bad of [
      "tomorrow",
      "2026-13-01",
      "2026-02-30",
      "2026-11-04 25:00",
      "2026-11-04 17",
      "2026-11",
      "1969-01-01",
      "2026-11-04 17:00 x",
    ]) {
      expect(() => parseDeadline(bad, 0), bad).toThrow(/Write the date/);
    }
  });
});

describe("the rest", () => {
  it("splits labels, without doubles", () => {
    expect(parseLabels(" backend, ui ,,backend")).toEqual(["backend", "ui"]);
  });

  it("makes a title of a goal, as the domain crate does", () => {
    expect(titleOfGoal("  Compare hosting\nwith details")).toBe("Compare hosting");
    const title = titleOfGoal("x".repeat(80));
    expect([...title]).toHaveLength(61);
    expect(title.endsWith("…")).toBe(true);
    expect(titleOfGoal("x".repeat(60))).toBe("x".repeat(60));
  });

  it("gives work to the top of the org chart", () => {
    const ceo = { id: "ceo", status: "active", reports_to: null };
    const cto = { id: "cto", status: "active", reports_to: "ceo" };
    const orphan = { id: "orphan", status: "active", reports_to: "gone" };
    const gone = { id: "gone", status: "archived", reports_to: null };
    expect(rootsOf([ceo, cto]).map((e) => e.id)).toEqual(["ceo"]);
    expect(rootsOf([ceo, orphan, gone]).map((e) => e.id)).toEqual(["ceo", "orphan"]);
    expect(defaultAssignee([], null)).toBeNull();
    expect(defaultAssignee([ceo, cto], "cto")).toBe("ceo");
    expect(defaultAssignee([ceo, orphan, cto], "cto")).toBe("cto");
    expect(defaultAssignee([ceo, orphan, gone], "gone")).toBe("ceo");
  });

  it("cycles around", () => {
    expect(cycle(["a", "b"], "a")).toBe("b");
    expect(cycle(["a", "b"], "b")).toBe("a");
    expect(cycle(["a", "b"], "c")).toBe("a");
  });
});

describe("a dialog pinned near the top", () => {
  it("keeps its top and is centred by its width, never off the left edge", () => {
    expect(topCentered(640)).toEqual({ top: DIALOG_TOP, start: "max(0px, calc((100vw - 640px) / 2))" });
  });
});
