import type { LoadSnapshot } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  boardOnlyReason,
  composePlanFeedback,
  elapsedText,
  fileName,
  fileRows,
  isAbsolutePath,
  lastFailure,
  lastRunOf,
  lastSummaryFailed,
  markdownPaths,
  screenshotId,
  stepBatches,
  subagentDoingText,
  subagentReport,
  subagentSteps,
  usageLine,
  usageTotals,
  waitingText,
} from "./run-logic";

describe("run texts", () => {
  it("tells a subagent's latest step and how many it took", () => {
    expect(subagentDoingText({ doing: null, steps: 0 })).toBe("Starting…");
    expect(subagentDoingText({ doing: "Grep · poll", steps: 1 })).toBe("Grep · poll · 1 step");
    expect(subagentDoingText({ doing: "Bash · sleep 90", steps: 3 })).toBe("Bash · sleep 90 · 3 steps");
  });

  it("picks one subagent's own steps out of the run, in order", () => {
    const calls: { call_id: string; parent_call_id?: string }[] = [
      { call_id: "a1" },
      { call_id: "s1", parent_call_id: "a1" },
      { call_id: "a2" },
      { call_id: "t1", parent_call_id: "a2" },
      { call_id: "s2", parent_call_id: "a1" },
    ];
    expect(subagentSteps(calls, "a1").map((c) => c.call_id)).toEqual(["s1", "s2"]);
    expect(subagentSteps(calls, null)).toEqual([]);
    expect(subagentSteps(calls.slice(0, 1), undefined)).toEqual([]);
  });

  it("cuts a subagent's steps into batches in order, the last one short", () => {
    const steps = Array.from({ length: 23 }, (_, i) => ({ call_id: `s${i}` }));
    const batches = stepBatches(steps, 10);
    expect(batches.map((b) => b.length)).toEqual([10, 10, 3]);
    expect(batches[2]?.[2]?.call_id).toBe("s22");
    expect(stepBatches([], 10)).toEqual([]);
  });

  it("reads a subagent's report from the call that started it, once that call is done", () => {
    const calls = [
      { call_id: "a1", ok: true, output: "All 3 files are fixed." },
      { call_id: "a2", ok: null, output: null },
      { call_id: "a3", ok: false, output: "It stopped." },
    ];
    expect(subagentReport(calls, "a1")).toBe("All 3 files are fixed.");
    expect(subagentReport(calls, "a2")).toBeNull();
    expect(subagentReport(calls, "a3")).toBe("It stopped.");
    expect(subagentReport(calls, "zz")).toBeNull();
    expect(subagentReport(calls, null)).toBeNull();
  });

  it("finds markdown files, not web pages", () => {
    expect(markdownPaths("See docs/plan.md, (README.md) and https://x.org/a.md or plan.md#top.")).toEqual([
      "docs/plan.md",
      "README.md",
      "plan.md",
    ]);
    expect(markdownPaths("a.md b.md c.md d.md e.md")).toHaveLength(4);
  });

  it("takes only a whole id from a screenshot step", () => {
    const id = "01234567-89ab-cdef-0123-456789abcdef";
    expect(screenshotId(`Saved as screenshot ${id}.`)).toBe(id);
    expect(screenshotId("Saved as screenshot ../../etc/passwd")).toBeNull();
    expect(screenshotId(`Saved as screenshot ${id}0`)).toBeNull();
  });

  it("writes elapsed time", () => {
    expect(elapsedText(12_000)).toBe("12s");
    expect(elapsedText(72_000)).toBe("1m 12s");
    expect(elapsedText(3_780_000)).toBe("1h 03m");
  });

  it("does not count what was written to the cache as input", () => {
    const usage = {
      input_tokens: 1000,
      output_tokens: 10,
      cached_tokens: 0,
      cache_creation_tokens: 400,
      estimate: false,
      runs: 1,
      cost_micro_usd: 0,
    };
    expect(usageLine(usage)).toContain("600 in");
  });

  it("quotes plan comments before the free text", () => {
    expect(composePlanFeedback([{ quote: "step one\nstep two", text: "Split " }], " also faster ")).toBe(
      "> step one\n> step two\n\nSplit\n\nalso faster",
    );
    expect(composePlanFeedback([], "  ")).toBe("");
    expect(composePlanFeedback([], "x")).toBe("x");
  });

  it("reads the last summary attempt", () => {
    const failed = { system: { kind: "summary_failed" } };
    const saved = { system: { kind: "summary" } };
    expect(lastSummaryFailed([saved, failed, {}])).toBe(true);
    expect(lastSummaryFailed([failed, saved])).toBe(false);
    expect(lastSummaryFailed([])).toBe(false);
  });

  it("sends only a ticket's main task to the board", () => {
    expect(boardOnlyReason({ ticket_id: "t", ticket_number: 21 })).toContain("ticket #21");
    expect(boardOnlyReason({ ticket_id: "t", parent_task_id: "p" })).toBeNull();
    expect(boardOnlyReason({ ticket_id: "t", delegated_by: "e" })).toBeNull();
    expect(boardOnlyReason({})).toBeNull();
  });

  it("says why a run waits", () => {
    const load = { cpu_pct: 91.4, mem_free_mb: 2048, running: 3, hold: { kind: "cpu" } } as LoadSnapshot;
    expect(waitingText(load)).toBe("Server busy: CPU 91%, waiting");
    expect(waitingText({ ...load, hold: { kind: "memory" } })).toBe("Server busy: 2.0 GB free, waiting");
    expect(waitingText({ ...load, hold: { kind: "max_runs" } })).toBe("Server busy: 3 runs active, waiting");
    expect(waitingText({ ...load, hold: null })).toBeNull();
  });
});

describe("lastFailure", () => {
  const m = (role: string, status: string, text: string) => ({ role, status, text });

  it("says why the last answer did not come, without the advice for a chat", () => {
    expect(
      lastFailure([
        m("user", "complete", "go"),
        m("assistant", "failed", "No connection. Send the message again to retry."),
      ]),
    ).toBe("No connection.");
  });

  it("is nothing once a later answer came", () => {
    expect(lastFailure([m("assistant", "failed", "x"), m("assistant", "complete", "ok")])).toBeNull();
    expect(lastFailure([])).toBeNull();
  });
});

describe("usageTotals", () => {
  it("adds up the answers that used tokens and skips the rest", () => {
    const u = (input: number, micro: number | null) => ({
      input_tokens: input,
      output_tokens: 2,
      cached_tokens: 3,
      cache_creation_tokens: 4,
      cost_micro_usd: micro,
      estimate: true,
      runs: 1,
    });
    expect(usageTotals([{ usage: u(10, 500) }, { usage: null }, {}, { usage: u(5, null) }])).toEqual({
      input: 15,
      output: 4,
      cached: 6,
      written: 8,
      micro: 500,
    });
  });
});

describe("lastRunOf", () => {
  it("is the latest message that came from a run", () => {
    expect(lastRunOf([{ run_id: "r1" }, { run_id: null }, { run_id: "r2" }, {}])).toBe("r2");
    expect(lastRunOf([{}, { run_id: null }])).toBeNull();
  });
});

describe("written files", () => {
  it("shows a file by its name, on either separator", () => {
    expect(fileName("/home/x/target/wt/crates/daemon/tests/artifact_drafts.rs")).toBe("artifact_drafts.rs");
    expect(fileName("C:\\w\\src\\main.ts")).toBe("main.ts");
    expect(fileName("src/api.rs")).toBe("api.rs");
  });

  it("adds the folder only where two files share a name", () => {
    expect(fileRows(["/w/a/mod.rs", "/w/b/mod.rs", "/w/b/lib.rs"])).toEqual([
      { path: "/w/a/mod.rs", name: "mod.rs", folder: "a" },
      { path: "/w/b/mod.rs", name: "mod.rs", folder: "b" },
      { path: "/w/b/lib.rs", name: "lib.rs", folder: null },
    ]);
  });

  it("opens only absolute paths", () => {
    expect(isAbsolutePath("/w/a.rs")).toBe(true);
    expect(isAbsolutePath("C:\\w\\a.rs")).toBe(true);
    expect(isAbsolutePath("\\\\server\\share\\a.rs")).toBe(true);
    expect(isAbsolutePath("src/a.rs")).toBe(false);
  });
});
