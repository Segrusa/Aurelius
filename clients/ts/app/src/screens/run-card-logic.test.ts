import type { ApprovalView, RunBrief, TokenUsage } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  approvalsFor,
  odometerEmpty,
  odometerItems,
  odometerLine,
  runLine,
  runState,
  waitsOnReply,
} from "./run-card-logic";

const usage = (over: Partial<TokenUsage> = {}): TokenUsage => ({
  runs: 1,
  input_tokens: 12_000,
  output_tokens: 2_200,
  cached_tokens: 0,
  cache_creation_tokens: 0,
  cost_micro_usd: 430_000,
  estimate: false,
  model_calls: 3,
  ...over,
});

const approval = (over: Partial<ApprovalView>) => ({ status: "pending", ...over }) as ApprovalView;

describe("run card", () => {
  it("shows the state of a run, a waiting one first", () => {
    expect(runState("running", false)).toMatchObject({ label: "Running", running: true });
    expect(runState("failed", false)).toMatchObject({ label: "Failed", variant: "error", running: false });
    expect(runState("succeeded", true)).toMatchObject({ label: "Waiting for you", variant: "warning", running: false });
    expect(runState("running", true)).toMatchObject({ label: "Waiting for you", running: true });
  });
  it("reads a state of a newer server as itself", () => {
    expect(runState("paused_by_dragons", false)).toEqual({
      label: "paused_by_dragons",
      variant: "neutral",
      running: false,
    });
  });
  it("counts calls and failures, with the tokens when known", () => {
    const brief: RunBrief = { status: "succeeded", tool_calls: 3, failed_calls: 1 };
    expect(runLine(brief, usage())).toBe("3 tool calls · 1 failed · 14.2k tokens");
    expect(runLine({ ...brief, tool_calls: 1, failed_calls: 0 }, undefined)).toBe("1 tool call");
  });
});

describe("interrupt card", () => {
  it("asks once: a run with an open approval has no reply card as well", () => {
    expect(waitsOnReply({ awaiting_user: true, run_id: "r" }, [])).toBe(true);
    expect(waitsOnReply({ awaiting_user: true, run_id: "r" }, [{ run_id: "r" }])).toBe(false);
    expect(waitsOnReply({ awaiting_user: false, run_id: "r" }, [])).toBe(false);
  });
  it("finds a discussion's approvals by conversation, by run woken and by ticket", () => {
    const list = [
      approval({ id: "here", conversation_id: "c" }),
      approval({ id: "woken", conversation_id: "main", discussion_id: "c" }),
      approval({ id: "ticket", conversation_id: "task", ticket_id: "t" }),
      approval({ id: "other", conversation_id: "x", ticket_id: "z" }),
      approval({ id: "done", conversation_id: "c", status: "approved" }),
    ];
    expect(approvalsFor(list, { conversationId: "c", ticketId: "t" }).map((a) => a.id)).toEqual([
      "here",
      "woken",
      "ticket",
    ]);
    expect(approvalsFor(list, { conversationId: "nope" })).toEqual([]);
  });
});

describe("odometer", () => {
  it("says calls, tokens in and out and the cost", () => {
    expect(odometerItems(usage())).toEqual([
      { label: "Calls", value: "3" },
      { label: "Tokens in", value: "12k" },
      { label: "Tokens out", value: "2.2k" },
      { label: "Cost", value: "$0.43" },
    ]);
    expect(odometerLine(usage())).toBe("3 calls · 12k tokens in · 2.2k tokens out · $0.43 cost");
  });
  it("leaves the calls out when they are not known, and has nothing to say without usage", () => {
    expect(odometerItems(usage({ model_calls: 0 })).map((i) => i.label)).toEqual(["Tokens in", "Tokens out", "Cost"]);
    expect(odometerEmpty(usage({ input_tokens: 0, output_tokens: 0, cached_tokens: 0 }))).toBe(true);
  });
});
