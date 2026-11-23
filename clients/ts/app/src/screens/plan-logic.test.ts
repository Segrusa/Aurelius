import type { MessageView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import { planSummary } from "./chat-logic";
import {
  decidedLine,
  findQuote,
  latestPlan,
  newPlanWaiting,
  planDecision,
  planFromSearch,
  planLabel,
  planStatus,
  shouldAutoOpenPlan,
} from "./plan-logic";

const msg = (over: Partial<MessageView>): MessageView =>
  ({ id: "m", role: "assistant", author: "Titan", text: "", created_at_ms: 0, ...over }) as MessageView;
const plan = (id: string, over: Partial<MessageView> = {}) =>
  msg({ id, update: "plan", text: `# Plan ${id}`, approval_id: `a-${id}`, run_id: "r1", ...over });
const decided = (id: string, kind: string, over: Partial<MessageView> = {}) =>
  msg({
    id,
    update: "status",
    author: "You",
    text: "",
    run_id: "r1",
    created_at_ms: 5,
    decision: { kind, feedback: "smaller" },
    ...over,
  } as Partial<MessageView>);

describe("plan summary", () => {
  it("counts the top-level items under a Steps heading and the distinct files", () => {
    const text = [
      "# Plan: add a panel",
      "Touches `src/a.ts` and `src/b.tsx:12`.",
      "## Steps",
      "1. Read `src/a.ts`",
      "   - a sub item that does not count",
      "2. Write `docs/x.md`",
      "- third",
      "## Risks",
      "- not a step",
    ].join("\n");
    expect(planSummary(text)).toEqual({ steps: 3, sections: 0, files: 3, line: "3 steps · 3 files" });
  });

  it("prefers a Steps heading over a Plan title that has no list of its own", () => {
    expect(planSummary("# Plan\n## Context\n- a\n- b\n## Steps\n- one").steps).toBe(1);
  });

  it("falls back to the number of ## sections", () => {
    expect(planSummary("# Title\n## One\ntext\n## Two\ntext").line).toBe("2 sections");
  });

  it("ignores code that is not a path and code inside fences", () => {
    const text = "## Plan\n- use `npm test` and `true`\n```\n`src/hidden.ts`\n```\n- `Cargo.toml`";
    expect(planSummary(text)).toMatchObject({ steps: 2, files: 1, line: "2 steps · 1 file" });
  });

  it("says nothing for a plan without structure", () => {
    expect(planSummary("Just do it.").line).toBe("");
  });
});

describe("opening a plan by itself", () => {
  const design = { kind: "design", messageId: "d", path: "x.html" } as const;
  const waiting = { id: "p1", pending: true };

  it("opens a waiting plan in an empty dock, once", () => {
    expect(shouldAutoOpenPlan(null, waiting, new Set())).toBe(true);
    expect(shouldAutoOpenPlan(null, waiting, new Set(["p1"]))).toBe(false);
  });

  it("never takes over a design or another plan", () => {
    expect(shouldAutoOpenPlan(design, waiting, new Set())).toBe(false);
    expect(shouldAutoOpenPlan({ kind: "plan", messageId: "p0" }, waiting, new Set())).toBe(false);
  });

  it("leaves a decided plan alone", () => {
    expect(shouldAutoOpenPlan(null, { id: "p1", pending: false }, new Set())).toBe(false);
  });

  it("shows the pill only while the dock shows something else", () => {
    expect(newPlanWaiting(design, waiting)).toBe("p1");
    expect(newPlanWaiting({ kind: "plan", messageId: "p1" }, waiting)).toBeNull();
    expect(newPlanWaiting(null, waiting)).toBeNull();
    expect(newPlanWaiting(design, { id: "p1", pending: false })).toBeNull();
  });

  it("finds the newest plan and whether it waits", () => {
    const list = [plan("p1"), msg({ id: "x" }), plan("p2")];
    expect(latestPlan(list, new Set(["a-p2"]))).toEqual({ id: "p2", pending: true });
    expect(latestPlan(list, new Set())).toEqual({ id: "p2", pending: false });
    expect(latestPlan([msg({ id: "x" })], new Set())).toBeNull();
  });
});

describe("a plan's decision", () => {
  it("is the first plan decision after it in the same run", () => {
    const list = [
      plan("p1"),
      decided("other", "plan_rejected", { run_id: "r2" }),
      decided("d1", "plan_changes"),
      plan("p2"),
      decided("d2", "plan_approved_edits", { created_at_ms: 9 }),
    ];
    expect(planDecision(list, "p1")).toMatchObject({ decision: { verdict: "changes" }, at: 5 });
    expect(planDecision(list, "p2")).toMatchObject({ decision: { verdict: "approved" }, at: 9 });
  });

  it("is none when the next plan of the run comes first", () => {
    expect(planDecision([plan("p1"), plan("p2"), decided("d", "plan_approved")], "p1")).toBeNull();
  });

  it("names the status in words", () => {
    const edits = planDecision([plan("p"), decided("d", "plan_approved_edits")], "p")?.decision ?? null;
    const auto = planDecision([plan("p"), decided("d", "plan_approved")], "p")?.decision ?? null;
    expect(planStatus(true, null).label).toBe("Waiting for you");
    expect(planStatus(false, edits)).toEqual({ label: "Approved · edits", tone: "success" });
    expect(planStatus(false, auto).label).toBe("Approved · auto");
    expect(planLabel(false, auto)).toBe("PLAN · APPROVED");
    expect(planLabel(true, null)).toBe("PLAN · WAITING FOR YOU");
    if (auto) expect(decidedLine(auto, "14:40")).toBe("You approved this plan at 14:40 · auto mode");
  });
});

describe("a comment's quote in the text", () => {
  it("is found across the line breaks a selection adds between blocks", () => {
    const text = "Keep the dock width.Also for plans.";
    const range = findQuote(text, "dock width.\n\nAlso");
    expect(range).toEqual([9, 24]);
    expect(range && text.slice(...range)).toBe("dock width.Also");
  });

  it("is null when the text does not hold it", () => {
    expect(findQuote("abc", "x")).toBeNull();
    expect(findQuote("abc", "  ")).toBeNull();
  });
});

describe("a plan link", () => {
  it("opens the plan the link names when the feed holds it", () => {
    const list = [plan("p1"), msg({ id: "x" })];
    expect(planFromSearch("?plan=p1", list)).toBe("p1");
    expect(planFromSearch("?plan=x", list)).toBeNull();
    expect(planFromSearch("", list)).toBeNull();
  });
});
