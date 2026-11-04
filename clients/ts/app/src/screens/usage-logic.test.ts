import { describe, expect, it } from "vitest";
import { barHeight, bars, clampPercent, costText, limitTone, subscriptionFor, tokensShort } from "./usage-logic";

describe("usage numbers", () => {
  it("shortens tokens", () => {
    expect(tokensShort(999)).toBe("999");
    expect(tokensShort(1000)).toBe("1k");
    expect(tokensShort(1234)).toBe("1.2k");
    expect(tokensShort(2_500_000)).toBe("2.50M");
    expect(tokensShort(1_030_000_000)).toBe("1.03B");
  });

  it("writes a cost, or why there is none", () => {
    expect(costText(null, false)).toBe("cost not reported");
    expect(costText(0, false)).toBe("no cost");
    expect(costText(5_000, false)).toBe("$0.0050");
    expect(costText(133_610_000, true)).toBe("$133.61 (estimate)");
  });

  it("colours a limit by how close it is", () => {
    expect(limitTone(69)).toBe("bg-acc-text");
    expect(limitTone(70)).toBe("bg-amber");
    expect(limitTone(90)).toBe("bg-red");
    expect(clampPercent(120)).toBe(100);
  });

  it("draws bars relative to the largest day", () => {
    expect(barHeight(0, 0)).toBe(2);
    expect(barHeight(1, 1000)).toBe(2);
    const noon = new Date(2026, 10, 4, 12).getTime();
    const day = new Date(2026, 10, 4).getTime();
    const [a, b] = bars(
      [
        { day_start_ms: day - 86_400_000, tokens: 50 },
        { day_start_ms: day, tokens: 100 },
      ],
      noon,
    );
    expect(a).toMatchObject({ height: 70, today: false });
    expect(b).toMatchObject({ height: 140, today: true });
  });

  it("does not show another member's subscription", () => {
    expect(subscriptionFor(undefined, "sam")).toBeNull();
    expect(subscriptionFor({ fetched_at_ms: 0 }, "sam")).toBeNull();
    expect(subscriptionFor({ fetched_at_ms: 1, login_user: "kim" }, "sam")).toBeNull();
    expect(subscriptionFor({ fetched_at_ms: 1 }, "sam")).toBeNull();
    expect(subscriptionFor({ fetched_at_ms: 1, login_user: "sam" }, null)).toBeNull();
    expect(subscriptionFor({ fetched_at_ms: 1, login_user: "sam" }, "sam")).not.toBeNull();
  });
});
