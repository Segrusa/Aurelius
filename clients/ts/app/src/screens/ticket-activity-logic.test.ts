import type { ActivityView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import { mergeByTime, rowFiles, workInFilter } from "./ticket-activity-logic";

const step = (tool: string, input: unknown, offset = 0, ok: boolean | null = true): ActivityView => ({
  call_id: `${tool}-${JSON.stringify(input)}`,
  tool,
  input,
  offset,
  ok,
});

describe("mergeByTime", () => {
  it("puts each work row before the first message after it", () => {
    const merged = mergeByTime(
      [{ created_at_ms: 100 }, { created_at_ms: 300 }],
      [{ atMs: 100 }, { atMs: 200 }, { atMs: Number.POSITIVE_INFINITY }],
    );
    expect(merged.map((r) => (r.kind === "message" ? `m${r.m.created_at_ms}` : `w${r.w.atMs}`))).toEqual([
      "m100",
      "w100",
      "w200",
      "m300",
      "wInfinity",
    ]);
  });
});

describe("the filter", () => {
  it("counts work as Changes and keeps it out of Messages", () => {
    expect(workInFilter("all")).toBe(true);
    expect(workInFilter("changes")).toBe(true);
    expect(workInFilter("messages")).toBe(false);
  });
});

describe("rowFiles", () => {
  it("gives each file once, in the order it was first touched", () => {
    const reads = [
      step("Read", { file_path: "src/a.ts" }),
      step("Read", { file_path: "src/a.ts" }),
      step("Read", { file_path: "b.ts" }),
    ];
    expect(rowFiles(reads)).toEqual(["src/a.ts", "b.ts"]);
  });
});
