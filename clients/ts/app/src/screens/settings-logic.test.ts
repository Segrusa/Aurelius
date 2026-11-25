import type { EmployeeRunLine, RemoteClientView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  compactFieldOf,
  compactThresholdOf,
  defaultEffortLabel,
  defaultModelLabel,
  deviceHint,
  effortOptions,
  employeeRunHint,
  inSettingsArea,
  parseClock,
  recurrenceForm,
  recurrenceOf,
  recurrenceText,
  remoteAccessDevices,
  runDefaultValue,
  screenUseRows,
  withRunDefault,
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

describe("compaction threshold", () => {
  const limits = { threshold_tokens: 120_000, default_tokens: 120_000, min_tokens: 20_000, max_tokens: 1_000_000 };

  it("shows and reads thousands of tokens", () => {
    expect(compactFieldOf(120_000)).toBe("120");
    expect(compactThresholdOf("400", limits)).toBe(400_000);
    expect(compactThresholdOf(" 20 ", limits)).toBe(20_000);
    expect(compactThresholdOf("1000", limits)).toBe(1_000_000);
  });

  it("takes 0 as never", () => {
    expect(compactThresholdOf("0", limits)).toBe(0);
  });

  it("refuses what the server would refuse", () => {
    expect(compactThresholdOf("19", limits)).toBe("Choose 0 (never) or 20 to 1000.");
    expect(compactThresholdOf("1001", limits)).toBe("Choose 0 (never) or 20 to 1000.");
    expect(compactThresholdOf("", limits)).toBe("Type a whole number of thousands of tokens.");
    expect(compactThresholdOf("1.5", limits)).toBe("Type a whole number of thousands of tokens.");
    expect(compactThresholdOf("-5", limits)).toBe("Type a whole number of thousands of tokens.");
  });
});

describe("settings area", () => {
  it("holds Settings and the Company pages that moved into it", () => {
    for (const path of [
      "/settings",
      "/settings/company",
      "/employees",
      "/employees/org",
      "/employees/e-1",
      "/knowledge",
      "/knowledge/guidelines",
      "/knowledge/guidelines/g-1",
      "/projects",
    ]) {
      expect(inSettingsArea(path), path).toBe(true);
    }
  });

  it("leaves a project and the rest of the app in the app", () => {
    for (const path of ["/inbox", "/board", "/projects/p-1", "/talk/c-1", "/employeesx", "/settingsx"]) {
      expect(inSettingsArea(path), path).toBe(false);
    }
  });
});

describe("paired devices", () => {
  const device = (name: string, paired_by?: string): RemoteClientView => ({
    id: name,
    name,
    platform: "ios",
    capabilities: ["read"],
    created_at_ms: 1,
    last_seen_ms: 5,
    connected: false,
    push: false,
    ...(paired_by ? { paired_by } : {}),
  });

  it("says where a device stands", () => {
    const seen = (ms: number | null | undefined) => `t${ms}`;
    expect(deviceHint(device("a"), seen)).toBe("ios · Last seen t5");
    expect(deviceHint({ ...device("a"), connected: true, push: true }, seen)).toBe(
      "ios · Connected now · Notifications on",
    );
  });

  it("leaves a board member's devices to Account when the server has own_devices", () => {
    const clients = [device("mine", "u1"), device("old")];
    expect(remoteAccessDevices(clients, true).map((c) => c.name)).toEqual(["old"]);
    expect(remoteAccessDevices(clients, false).map((c) => c.name)).toEqual(["mine", "old"]);
  });
});

describe("my run defaults", () => {
  it("shows default for an unset field and clears a field set back to default", () => {
    const defaults = { model: "claude-opus-5-5", mode: "plan" as const };
    expect(runDefaultValue(defaults, "model")).toBe("claude-opus-5-5");
    expect(runDefaultValue(defaults, "effort")).toBe("default");
    expect(withRunDefault(defaults, "effort", "high")).toEqual({
      model: "claude-opus-5-5",
      mode: "plan",
      effort: "high",
    });
    expect(withRunDefault(defaults, "model", "default")).toEqual({ mode: "plan" });
  });

  it("names the connection's model as the default model", () => {
    expect(defaultModelLabel("claude-opus-5-5", (m) => m.toUpperCase())).toBe(
      "Default · CLAUDE-OPUS-5-5 (connection's default)",
    );
    expect(defaultModelLabel(null, (m) => m)).toBe("Default · the connection's model");
  });
});
