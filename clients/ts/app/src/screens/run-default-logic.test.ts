import { describe, expect, it } from "vitest";
import { chipLabel, defaultLabel, resetLabel, resolvedDefaults, whoseDefault } from "./run-default-logic";

const name = (v: string) => v.toUpperCase();

describe("run default labels", () => {
  it("say your default on your own work and the starter's on another board member's", () => {
    expect(whoseDefault(null, "me")).toBe("Your default");
    expect(whoseDefault({ id: "me", username: "sam" }, "me")).toBe("Your default");
    expect(whoseDefault({ id: "kim-id", username: "Kim" }, "me")).toBe("Kim's default");
  });

  it("add the value once it is known", () => {
    expect(defaultLabel("Your default", "Opus 5.5")).toBe("Your default · Opus 5.5");
    expect(defaultLabel("Kim's default", null)).toBe("Kim's default");
  });

  it("start the menu with Use your default", () => {
    expect(resetLabel("Your default")).toBe("Use your default");
    expect(resetLabel("Kim's default")).toBe("Use Kim's default");
  });

  it("show only the value for a choice of this run", () => {
    expect(chipLabel("claude-sonnet-5-5", name, "Your default · X")).toBe("CLAUDE-SONNET-5-5");
    expect(chipLabel(null, name, "Your default · X")).toBe("Your default · X");
    expect(chipLabel("default", name, "Your default · X")).toBe("Your default · X");
  });

  it("resolve the chosen values, else the model's and Ask", () => {
    const view = { defaults: { effort: null, mode: null }, model: "opus", model_default_effort: "high" };
    expect(resolvedDefaults(view, name, name, name)).toEqual({ model: "OPUS", effort: "HIGH", mode: "ASK" });
    const own = { defaults: { effort: "low", mode: "plan" as const }, model: "opus", model_default_effort: "high" };
    expect(resolvedDefaults(own, name, name, name)).toEqual({ model: "OPUS", effort: "LOW", mode: "PLAN" });
    expect(resolvedDefaults(null, name, name, name)).toEqual({ model: null, effort: null, mode: "ASK" });
  });
});
