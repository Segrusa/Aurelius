import { describe, expect, it } from "vitest";
import {
  admits,
  canBecomeTicket,
  consolidationScopes,
  ideaPrefill,
  nextAuthor,
  splitIdeas,
  thousands,
} from "./knowledge-logic";

describe("ideas", () => {
  const you = { author: "You", employee_id: null };
  const iris = { author: "Iris", employee_id: "id" };
  const irisGone = { author: "Iris", employee_id: null };
  it("filters by author", () => {
    const list = [you, iris, irisGone];
    expect(list.filter((i) => admits("anyone", i))).toHaveLength(3);
    expect(list.filter((i) => admits("you", i))).toHaveLength(1);
    expect(list.filter((i) => admits("employees", i))).toHaveLength(2);
    expect([nextAuthor("anyone"), nextAuthor("you"), nextAuthor("employees")]).toEqual(["you", "employees", "anyone"]);
  });
  it("splits yours and theirs", () => {
    const [a, b, c, d] = [
      { ...iris, id: "a" },
      { ...you, id: "b" },
      { author: "Kai", employee_id: null, id: "c" },
      { ...you, id: "d" },
    ];
    const s = splitIdeas([a, b, c, d]);
    expect(s.yours.map((x) => x.id)).toEqual(["b", "d"]);
    expect(s.employees.map((x) => x.id)).toEqual(["a", "c"]);
  });
  it("becomes one ticket", () => {
    expect(canBecomeTicket({})).toBe(true);
    expect(canBecomeTicket({ task_id: "t" })).toBe(false);
    expect(canBecomeTicket({ ticket_id: "t" })).toBe(false);
    expect(ideaPrefill({ title: "T", note: "N", kind: "bug" })).toEqual({
      title: "T",
      description: "N",
      labels: "bug",
    });
    expect(ideaPrefill({ title: "T", note: null, kind: "feature" })).toEqual({
      title: "T",
      description: "",
      labels: "",
    });
  });
});

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
