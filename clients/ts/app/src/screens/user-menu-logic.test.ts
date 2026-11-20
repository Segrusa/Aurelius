import { describe, expect, it } from "vitest";
import { planLabel } from "./user-menu-logic";

describe("planLabel", () => {
  it("capitalises the plan the login reports", () => {
    expect(planLabel("max")).toBe("Max");
    expect(planLabel("pro")).toBe("Pro");
  });

  it("splits a plan with a tier into words", () => {
    expect(planLabel("max_20x")).toBe("Max 20x");
  });

  it("shows nothing when the plan is unknown", () => {
    expect(planLabel(null)).toBeNull();
    expect(planLabel(undefined)).toBeNull();
    expect(planLabel("  ")).toBeNull();
  });
});
