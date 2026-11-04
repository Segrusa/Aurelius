import { describe, expect, it } from "vitest";
import { parseClock, recurrenceForm, recurrenceOf, recurrenceText } from "./settings-logic";

describe("recurrence", () => {
  it("reads a clock", () => {
    expect(parseClock("09:30")).toBe(570);
    expect(parseClock("9.05")).toBe(545);
    expect(parseClock("24:00")).toBeNull();
    expect(parseClock("noon")).toBeNull();
  });

  it("says what is wrong", () => {
    const form = recurrenceForm(null);
    expect(recurrenceOf({ ...form, kind: "every_hours", hours: "0" })).toBe("Hours must be between 1 and 168.");
    expect(recurrenceOf({ ...form, kind: "every_hours", hours: "x" })).toBe("Enter the number of hours.");
    expect(recurrenceOf({ ...form, time: "soon" })).toBe("Enter the time as HH:MM.");
    expect(recurrenceOf({ ...form, kind: "weekly", weekdays: [] })).toBe("Pick at least one weekday.");
  });

  it("goes there and back", () => {
    const weekly = { kind: "weekly" as const, at_minute: 545, weekdays: [0, 4] };
    expect(recurrenceOf(recurrenceForm(weekly))).toEqual(weekly);
    expect(recurrenceText(weekly)).toBe("Mon, Fri at 09:05");
    expect(recurrenceText({ kind: "every_hours", hours: 1 })).toBe("Every hour");
    expect(recurrenceText({ kind: "daily", at_minute: 60 })).toBe("Daily at 01:00");
  });
});
