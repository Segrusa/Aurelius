import type { EmployeeRunLine, RemoteClientView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  agoText,
  choiceToStore,
  compactFieldOf,
  compactThresholdOf,
  defaultEffortLabel,
  defaultModelLabel,
  deviceHint,
  effortOptions,
  employeeRunHint,
  inSettingsArea,
  pressedChoice,
  remoteAccessDevices,
  runDefaultValue,
  screenUseRows,
  sectionKeyOf,
  sectionsFor,
  settingMatches,
  uptimeText,
  withRunDefault,
} from "./settings-logic";

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

describe("settings sections per server", () => {
  const sections = [{ key: "run-defaults" }, { key: "employee-runs", replacedBy: "user_run_defaults" }];

  it("drops Model & effort where My defaults replaced it", () => {
    expect(sectionsFor(sections, ["user_run_defaults"]).map((s) => s.key)).toEqual(["run-defaults"]);
  });

  it("keeps it on an old server", () => {
    expect(sectionsFor(sections, []).map((s) => s.key)).toEqual(["run-defaults", "employee-runs"]);
  });
});

describe("settings redesign", () => {
  const now = Date.UTC(2026, 10, 25, 12, 0);
  const min = 60_000;

  it("opens the regrouped page for an old link and leaves a current one alone", () => {
    expect(sectionKeyOf("app")).toBe("device");
    expect(sectionKeyOf("run-defaults")).toBe("defaults");
    expect(sectionKeyOf("servers")).toBe("server");
    expect(sectionKeyOf("mcp")).toBe("tools");
    expect(sectionKeyOf("compaction")).toBe("load");
    expect(sectionKeyOf("account")).toBe("account");
    expect(sectionKeyOf("employee-runs")).toBe("employee-runs");
  });

  it("finds an entry by its label or one of its words, whatever the case", () => {
    expect(settingMatches("Runs & load", "load limits cpu compaction", "")).toBe(true);
    expect(settingMatches("Runs & load", "load limits cpu compaction", "COMPACT")).toBe(true);
    expect(settingMatches("Runs & load", "load limits cpu compaction", " runs ")).toBe(true);
    expect(settingMatches("Backups", "restore", "tailscale")).toBe(false);
  });

  it("says how long ago in the words of the design", () => {
    expect(agoText(null, now)).toBe("never");
    expect(agoText(now - 20_000, now)).toBe("now");
    expect(agoText(now - 4 * min, now)).toBe("4 min ago");
    expect(agoText(now - 125 * min, now)).toBe("2 h ago");
    expect(agoText(now - 30 * 60 * min, now)).toBe("yesterday");
    expect(agoText(now - 9 * 24 * 60 * min, now)).toBe("9 days ago");
  });

  it("says how long the server runs", () => {
    expect(uptimeText(undefined, now)).toBe("Unknown");
    expect(uptimeText(now - 12 * min, now)).toBe("12 min");
    expect(uptimeText(now - 5 * 60 * min, now)).toBe("5 h");
    expect(uptimeText(now - 3 * 24 * 60 * min, now)).toBe("3 days");
  });

  it("shows Default as what it comes to, and stores the default again when that is picked", () => {
    expect(pressedChoice("default", "ask")).toBe("ask");
    expect(pressedChoice("default", null)).toBeNull();
    expect(pressedChoice("plan", "ask")).toBe("plan");
    expect(choiceToStore("ask", "ask")).toBe("default");
    expect(choiceToStore("high", "medium")).toBe("high");
    expect(choiceToStore("high", null)).toBe("high");
  });
});
