import { describe, expect, it } from "vitest";
import { isDefaultPolicy, nameAfterProfile, profileOption, samePolicy } from "./employee-tools-logic";

describe("the tool policy of an employee", () => {
  it("has everything on without a list", () => {
    expect(isDefaultPolicy({})).toBe(true);
    expect(isDefaultPolicy({ tools: ["Read"] })).toBe(false);
  });

  it("compares without looking at the order", () => {
    expect(samePolicy({ tools: ["a", "b"] }, { tools: ["b", "a"], approval_tools: [] })).toBe(true);
    expect(samePolicy({}, { tools: [] })).toBe(false);
  });
});

describe("start profiles", () => {
  const profile = { key: "qa", label: "QA", summary: "", name: "Quinn", role: "QA", description: "", instructions: "" };

  it("says who started from it", () => {
    expect(profileOption(profile)).toBe("QA");
    expect(profileOption({ ...profile, hired: ["Quinn"] })).toBe("QA · hired as Quinn");
  });

  it("offers a suggested name only when it is free and nothing was typed", () => {
    expect(nameAfterProfile("", null, "Quinn", [])).toBe("Quinn");
    expect(nameAfterProfile("", null, "Quinn", ["quinn"])).toBe("");
    expect(nameAfterProfile("Mine", null, "Quinn", [])).toBe("Mine");
    expect(nameAfterProfile("Quinn", "Quinn", "Dara", [])).toBe("Dara");
    expect(nameAfterProfile("Quinn", "Quinn", null, [])).toBe("");
  });
});
