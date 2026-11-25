import type { ActivityView, CodeBranch, CodeCommit } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  autoDoneDue,
  clock,
  type FinishInput,
  finishKind,
  finishLines,
  finishSteps,
  isMergeAsk,
  MERGE_ASK,
  mergeAsk,
  mergeCommitOf,
  ticketBranch,
} from "./review-finish-logic";

const branch = (over: Partial<CodeBranch>): CodeBranch => ({
  ahead: 2,
  at_ms: 1,
  author: "Forge",
  behind: 0,
  head: "h-branch",
  name: "feat/ticket-87-tickets-everywhere",
  subject: "s",
  ...over,
});

const msg = (id: string, role: "user" | "assistant", text: string, at = 1) => ({
  id,
  role,
  text,
  created_at_ms: at,
  system: null,
  deleted_at_ms: null,
  author: role === "user" ? "Sebastien" : "Forge",
});

const input = (over: Partial<FinishInput>): FinishInput => ({
  column: "review",
  asked: false,
  running: false,
  branch: { merged: false, ahead: 2 },
  mergedCommit: false,
  ...over,
});

describe("the merge request", () => {
  it("the button's own text and a typed request both ask to merge", () => {
    expect(isMergeAsk(MERGE_ASK)).toBe(true);
    expect(isMergeAsk("please merge it into main")).toBe(true);
    expect(isMergeAsk("Merge to master")).toBe(true);
    expect(isMergeAsk("Explain what you did")).toBe(false);
    expect(isMergeAsk("Did you merge main into your branch? Check the tests.")).toBe(false);
  });

  it("counts only while it is the board member's latest word", () => {
    expect(mergeAsk([msg("a", "user", MERGE_ASK, 5), msg("b", "assistant", "Merging…", 6)])).toEqual({
      id: "a",
      atMs: 5,
      author: "Sebastien",
    });
    expect(mergeAsk([msg("a", "user", MERGE_ASK), msg("b", "user", "Wait, explain first")])).toBeNull();
    expect(mergeAsk([])).toBeNull();
  });

  it("a deleted request asks nothing", () => {
    expect(mergeAsk([{ ...msg("a", "user", MERGE_ASK), deleted_at_ms: 9 }])).toBeNull();
  });
});

describe("the moment of the card", () => {
  it("is not shown outside Review and Done", () => {
    expect(finishKind(input({ column: "doing" }))).toBeNull();
  });

  it("waits for the decision until merging is asked", () => {
    expect(finishKind(input({}))).toBe("decide");
  });

  it("merges while the owner's run goes", () => {
    expect(finishKind(input({ asked: true, running: true }))).toBe("merging");
  });

  it("is merged when main has the merge commit or the branch has nothing ahead", () => {
    expect(finishKind(input({ asked: true, mergedCommit: true }))).toBe("merged");
    expect(finishKind(input({ asked: true, branch: { merged: true, ahead: 0 } }))).toBe("merged");
  });

  it("stopped when the run ended and the branch still has work main lacks", () => {
    expect(finishKind(input({ asked: true, branch: { merged: false, ahead: 2 } }))).toBe("conflict");
  });

  it("goes back to the decision when git cannot tell", () => {
    expect(finishKind(input({ asked: true, branch: null }))).toBe("decide");
  });

  it("is done on Done whatever else holds", () => {
    expect(finishKind(input({ column: "done", asked: true, running: true }))).toBe("done");
  });
});

describe("the finish line", () => {
  const states = (kind: Parameters<typeof finishSteps>[0], merged = false, pushed = false) =>
    finishSteps(kind, { merged, pushed }).map((s) => s.state);

  it("follows the design per moment", () => {
    expect(states("decide")).toEqual(["ask", "open", "open", "open"]);
    expect(states("merging")).toEqual(["done", "now", "open", "open"]);
    expect(states("merged", true)).toEqual(["done", "done", "open", "ask"]);
    expect(states("conflict")).toEqual(["done", "stopped", "open", "open"]);
    expect(states("done", true)).toEqual(["done", "done", "open", "done"]);
  });

  it("push is optional and done once main is on its remote", () => {
    const push = finishSteps("merged", { merged: true, pushed: true })[2];
    expect(push).toEqual({ label: "Push", state: "done", optional: true });
  });

  it("a ticket done without a merge leaves Merge open", () => {
    expect(states("done", false)).toEqual(["done", "open", "open", "done"]);
  });
});

describe("the ticket's branch and its merge", () => {
  it("picks the branch named after the ticket with work ahead, then the newest", () => {
    const code = {
      branches: [
        branch({ name: "main", is_main: true, ahead: 0 }),
        branch({ name: "old", ticket: { number: 87, ticket_id: "t", title: "x" }, ahead: 0, at_ms: 9 }),
        branch({ name: "work", ticket: { number: 87, ticket_id: "t", title: "x" }, ahead: 1, at_ms: 2 }),
        branch({ name: "other", ticket: { number: 12, ticket_id: "u", title: "y" } }),
      ],
    };
    expect(ticketBranch(code, { number: 87 })?.name).toBe("work");
    expect(ticketBranch(code, { number: 5 })).toBeNull();
  });

  it("a quick task's own branch goes first", () => {
    const code = { branches: [branch({ name: "quick/ticket-5", ahead: 0 })] };
    expect(ticketBranch(code, { number: 5, kind: "quick" })?.name).toBe("quick/ticket-5");
  });

  const commit = (over: Partial<CodeCommit>): CodeCommit => ({
    at_ms: 1,
    author: "a",
    hash: "c1",
    subject: "x",
    parents: ["p1"],
    ...over,
  });

  it("finds the merge commit by the branch, or by the ticket once the branch is gone", () => {
    const merge = commit({ hash: "m", subject: "Merge feat/ticket-87-tickets-everywhere", parents: ["p", "h"] });
    const code = { commits: [commit({}), merge] };
    expect(mergeCommitOf(code, { number: 87 }, branch({}))?.hash).toBe("m");
    expect(mergeCommitOf(code, { number: 87 }, null)?.hash).toBe("m");
    expect(mergeCommitOf(code, { number: 8 }, null)).toBeNull();
  });

  it("a plain commit that mentions the branch is not its merge", () => {
    const code = { commits: [commit({ subject: "Merge feat/ticket-87-tickets-everywhere", parents: ["p"] })] };
    expect(mergeCommitOf(code, { number: 87 }, branch({}))).toBeNull();
  });
});

describe("what the owner does while merging", () => {
  const call = (id: string, tool: string, input: unknown, ok: boolean | null): ActivityView => ({
    call_id: id,
    tool,
    input,
    offset: 0,
    ok,
  });

  it("lists commands with their command and leaves lookups out", () => {
    const lines = finishLines([
      call("1", "Bash", { command: "git status" }, true),
      call("2", "Bash", { command: "git merge --no-ff feat/x" }, true),
      call("3", "Bash", { command: "cargo xtask gen-dart --check" }, null),
      call("4", "TodoWrite", { todos: [] }, true),
    ]);
    expect(lines).toEqual([
      { id: "2", state: "done", text: "Ran", command: "git merge --no-ff feat/x" },
      { id: "3", state: "now", text: "Running", command: "cargo xtask gen-dart --check" },
    ]);
  });

  it("a failed call is marked and only the newest steps stay", () => {
    const many = Array.from({ length: 9 }, (_, i) => call(String(i), "Bash", { command: `step ${i}` }, i !== 8));
    const lines = finishLines(many, 3);
    expect(lines.map((l) => l.id)).toEqual(["6", "7", "8"]);
    expect(lines.at(-1)?.state).toBe("failed");
  });
});

describe("small parts", () => {
  it("the clock counts minutes and seconds", () => {
    expect(clock(72_000)).toBe("1:12");
    expect(clock(3_729_000)).toBe("1:02:09");
    expect(clock(-5)).toBe("0:00");
  });

  it("mark done when merged fires only once merged, wanted and allowed", () => {
    expect(autoDoneDue("merged", true, true)).toBe(true);
    expect(autoDoneDue("merged", false, true)).toBe(false);
    expect(autoDoneDue("merged", true, false)).toBe(false);
    expect(autoDoneDue("merging", true, true)).toBe(false);
  });
});
