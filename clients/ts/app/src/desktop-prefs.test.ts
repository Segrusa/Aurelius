import { describe, expect, it } from "vitest";
import { adoptDesktopPrefs } from "./desktop-prefs";

function memory(start: Record<string, string> = {}) {
  const map = new Map(Object.entries(start));
  return { map, get: (k: string) => map.get(k) ?? null, set: (k: string, v: string) => void map.set(k, v) };
}

describe("taking over the desktop app's preferences", () => {
  it("copies what it knows", () => {
    const s = memory();
    adoptDesktopPrefs(s, {
      uiJson: '{"appearance":"light","last_given_to":"e1","board_members":{"x":1}}',
      screenUseJson: '{"screens":{}}',
    });
    expect(s.get("appearance")).toBe("light");
    expect(s.get("last_given_to")).toBe("e1");
    expect(s.get("screen_use")).toBe('{"screens":{}}');
    expect(s.get("board_members")).toBeNull();
  });

  it("happens once and never overwrites a choice made here", () => {
    const s = memory({ appearance: "dark" });
    adoptDesktopPrefs(s, { uiJson: '{"appearance":"light"}', screenUseJson: null });
    expect(s.get("appearance")).toBe("dark");
    s.map.delete("appearance");
    adoptDesktopPrefs(s, { uiJson: '{"appearance":"light"}', screenUseJson: null });
    expect(s.get("appearance")).toBeNull();
  });

  it("survives a damaged or strange file", () => {
    for (const uiJson of ["not json", "null", '{"appearance":"neon","last_given_to":5}', null]) {
      const s = memory();
      adoptDesktopPrefs(s, { uiJson, screenUseJson: null });
      expect(s.get("appearance")).toBeNull();
      expect(s.get("last_given_to")).toBeNull();
    }
  });
});
