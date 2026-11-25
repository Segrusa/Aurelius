import { describe, expect, it } from "vitest";
import { consolidationScopes, thousands } from "./knowledge-logic";

describe("memory", () => {
  it("consolidates the project and each employee once", () => {
    const items = [
      { scope: "project" },
      { scope: "employee", employee_id: "alex" },
      { scope: "employee", employee_id: "alex" },
      { scope: "global" },
    ];
    expect(consolidationScopes("p", items)).toEqual([
      { kind: "project", id: "p" },
      { kind: "employee", id: "alex" },
    ]);
    expect(consolidationScopes("p", [])).toEqual([{ kind: "project", id: "p" }]);
    expect(thousands(1234)).toBe("1,234");
  });
});
