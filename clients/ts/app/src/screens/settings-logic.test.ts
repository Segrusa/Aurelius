import type { EmployeeRunLine } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  defaultEffortLabel,
  effortOptions,
  employeeRunHint,
  parseClock,
  recurrenceForm,
  recurrenceOf,
  recurrenceText,
  screenUseRows,
} from "./settings-logic";

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

describe("screenUseRows", () => {
  it("counts one screen's own pages as one, most opened first", () => {
    const rows = screenUseRows({
      board: { opens: 3, last_ms: 5 },
      "employees.01a1a741-0000-7000-8000-000000000000": { opens: 2, last_ms: 7 },
      "employees.01a1a741-0000-7000-8000-000000000001": { opens: 2, last_ms: 9 },
    });
    expect(rows).toEqual([
      ["employees (their own pages)", { opens: 4, last_ms: 9 }],
      ["board", { opens: 3, last_ms: 5 }],
    ]);
  });
});

describe("model and effort per employee", () => {
  const high = { mode: "value", value: "high" } as const;

  it("says what Default comes to", () => {
    expect(defaultEffortLabel({ company: { effort: high } }, ["high"], "medium")).toBe("High (company default)");
    expect(defaultEffortLabel({ company: { effort: high } }, ["low"], "medium")).toBe("Medium (model's default)");
    expect(defaultEffortLabel({ company: {} }, [], null)).toBe("Model's default");
  });

  it("offers only what the model has, and keeps a stored level visible", () => {
    const options = effortOptions(["low", "max"], "Model's default", "xhigh");
    expect(options.map((o) => o.value)).toEqual(["inherit", "low", "max", "xhigh"]);
    expect(options[3]?.label).toBe("Extra high (not supported)");
    expect(effortOptions(["low"], "x", null).map((o) => o.value)).toEqual(["inherit", "low"]);
  });

  it("says where the model comes from and what a chat changes", () => {
    const run = {
      model: "claude-opus-5-5",
      model_source: "company",
      supported_efforts: ["low"],
      unsupported_effort: "max",
      chat_effort: "low",
      chat_model: null,
    } as EmployeeRunLine;
    expect(employeeRunHint(run, (m) => m)).toBe(
      "claude-opus-5-5 · company default. Max is not supported by this model, so it is not used. Its chat is set to Low",
    );
  });
});
