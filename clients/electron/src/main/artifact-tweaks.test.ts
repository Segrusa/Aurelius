import { describe, expect, it } from "vitest";
import {
  type ArtifactTweak,
  checkPick,
  checkTweak,
  currentValue,
  oneLine,
  PICK_SELECTOR_MAX,
  PICK_TEXT_MAX,
  parseTweaks,
  TWEAK_CHOICES_MAX,
  TWEAKS_JSON_MAX,
  TWEAKS_MAX,
  tweakValue,
} from "./artifact-tweaks";

const pick = {
  selector: "body > main > button",
  tag: "button",
  text: "Choose Team",
  rect: { x: 1, y: 2, width: 3, height: 4 },
};

describe("checkPick", () => {
  it("passes a pick on, its edges rounded", () => {
    expect(checkPick({ ...pick, rect: { x: 1.4, y: 2.6, width: 3, height: 4 } })).toEqual({
      ...pick,
      rect: { x: 1, y: 3, width: 3, height: 4 },
    });
  });

  it("refuses a selector over the limit, empty, or with a newline", () => {
    expect(checkPick({ ...pick, selector: "a".repeat(PICK_SELECTOR_MAX) })).not.toBeNull();
    expect(checkPick({ ...pick, selector: "a".repeat(PICK_SELECTOR_MAX + 1) })).toBeNull();
    expect(checkPick({ ...pick, selector: "" })).toBeNull();
    expect(checkPick({ ...pick, selector: "body >\nbutton" })).toBeNull();
    expect(checkPick({ ...pick, selector: "body >\u2028button" })).toBeNull();
  });

  it("folds the text to one line of at most the limit", () => {
    const got = checkPick({ ...pick, text: `  Choose\n\n Team\t${"x".repeat(200)}` });
    expect(got?.text).toBe(`Choose Team ${"x".repeat(PICK_TEXT_MAX - 12)}`);
    expect(got?.text.length).toBeLessThanOrEqual(PICK_TEXT_MAX);
    expect(got?.text).not.toMatch(/\n/);
  });

  it("refuses what is not a pick", () => {
    expect(checkPick(null)).toBeNull();
    expect(checkPick("button")).toBeNull();
    expect(checkPick({ ...pick, tag: "BUTTON onclick" })).toBeNull();
    expect(checkPick({ ...pick, text: 3 })).toBeNull();
    expect(checkPick({ ...pick, rect: { x: 1, y: 2, width: Number.NaN, height: 4 } })).toBeNull();
    expect(checkPick({ ...pick, rect: null })).toBeNull();
  });
});

describe("oneLine", () => {
  it("turns control characters and runs of whitespace into one space", () => {
    expect(oneLine("a\u0000b\r\nc   d\u0085e", 100)).toBe("a b c d e");
    expect(oneLine("abc def", 4)).toBe("abc");
  });
});

describe("checkTweak", () => {
  it("takes the four types of the convention", () => {
    expect(checkTweak({ var: "--accent", label: "Accent", type: "color" })).toEqual({
      var: "--accent",
      label: "Accent",
      type: "color",
    });
    expect(
      checkTweak({ var: "--space", label: "Spacing", type: "length", min: 4, max: 32, step: 1, unit: "px" }),
    ).toEqual({ var: "--space", label: "Spacing", type: "length", min: 4, max: 32, step: 1, unit: "px" });
    expect(checkTweak({ var: "--scale", type: "number", min: 0.5, max: 2, step: 0.1 })).toEqual({
      var: "--scale",
      label: "scale",
      type: "number",
      min: 0.5,
      max: 2,
      step: 0.1,
    });
    expect(checkTweak({ var: "--density", type: "choice", options: ["compact", "comfortable"] })).toEqual({
      var: "--density",
      label: "density",
      type: "choice",
      options: ["compact", "comfortable"],
    });
  });

  it("takes only a var of --, then 1 to 40 lowercase letters, digits or -", () => {
    expect(checkTweak({ var: "--a", type: "color" })).not.toBeNull();
    expect(checkTweak({ var: `--${"a".repeat(40)}`, type: "color" })).not.toBeNull();
    for (const bad of ["--", `--${"a".repeat(41)}`, "accent", "-accent", "--Accent", "--a_b", "--a;b", "--a b", 3])
      expect(checkTweak({ var: bad, type: "color" })).toBeNull();
  });

  it("takes only color, length, number and choice", () => {
    for (const type of ["text", "url", "script", "", undefined]) expect(checkTweak({ var: "--a", type })).toBeNull();
  });

  it("drops an entry whose min, max or step is there and not a number", () => {
    for (const key of ["min", "max", "step"]) {
      for (const bad of ["4", null, Number.NaN, Number.POSITIVE_INFINITY, [1]]) {
        expect(checkTweak({ var: "--a", type: "number", [key]: bad })).toBeNull();
        expect(checkTweak({ var: "--a", type: "length", [key]: bad })).toBeNull();
      }
    }
    expect(checkTweak({ var: "--a", type: "number", min: 10, max: 1 })).toBeNull();
    expect(checkTweak({ var: "--a", type: "number", step: 0 })).toBeNull();
  });

  it("gives a length px by default and refuses a unit it does not know", () => {
    expect(checkTweak({ var: "--a", type: "length" })).toMatchObject({ unit: "px" });
    expect(checkTweak({ var: "--a", type: "length", unit: "rem" })).toMatchObject({ unit: "rem" });
    expect(checkTweak({ var: "--a", type: "length", unit: "px;color:red" })).toBeNull();
  });

  it("takes a choice of 1 to 8 options of at most 40 characters each", () => {
    const options = (n: number) => Array.from({ length: n }, (_, i) => `o${i}`);
    expect(checkTweak({ var: "--a", type: "choice", options: options(TWEAK_CHOICES_MAX) })).not.toBeNull();
    expect(checkTweak({ var: "--a", type: "choice", options: options(TWEAK_CHOICES_MAX + 1) })).toBeNull();
    expect(checkTweak({ var: "--a", type: "choice", options: [] })).toBeNull();
    expect(checkTweak({ var: "--a", type: "choice", options: ["a".repeat(40)] })).not.toBeNull();
    expect(checkTweak({ var: "--a", type: "choice", options: ["a".repeat(41)] })).toBeNull();
    expect(checkTweak({ var: "--a", type: "choice", options: ["a", 2] })).toBeNull();
    expect(checkTweak({ var: "--a", type: "choice", options: ["a\nb"] })).toBeNull();
    expect(checkTweak({ var: "--a", type: "choice", options: "compact" })).toBeNull();
  });

  it("keeps a label to one short line, and names it after the var without one", () => {
    expect(checkTweak({ var: "--a", type: "color", label: `Accent\n${"x".repeat(80)}` })?.label).toHaveLength(40);
    expect(checkTweak({ var: "--accent-2", type: "color", label: "  " })?.label).toBe("accent-2");
  });
});

describe("parseTweaks", () => {
  it("reads the list of the convention", () => {
    const json = JSON.stringify([
      { var: "--accent", label: "Accent", type: "color" },
      { var: "--space", label: "Spacing", type: "length", min: 4, max: 32, step: 1, unit: "px" },
      { var: "--radius", label: "Radius", type: "length", min: 0, max: 24, unit: "px" },
      { var: "--density", type: "choice", options: ["compact", "comfortable"] },
    ]);
    expect(parseTweaks(json).map((t) => t.var)).toEqual(["--accent", "--space", "--radius", "--density"]);
  });

  it("drops invalid entries and keeps the rest", () => {
    const json = JSON.stringify([
      { var: "--a", type: "color" },
      { var: "bad", type: "color" },
      7,
      { var: "--b", type: "color" },
    ]);
    expect(parseTweaks(json).map((t) => t.var)).toEqual(["--a", "--b"]);
  });

  it("reads at most 20 entries", () => {
    const json = JSON.stringify(Array.from({ length: 30 }, (_, i) => ({ var: `--v${i}`, type: "color" })));
    const got = parseTweaks(json);
    expect(got).toHaveLength(TWEAKS_MAX);
    expect(got.at(-1)?.var).toBe("--v19");
  });

  it("keeps a var named twice only once, the first", () => {
    const json = JSON.stringify([
      { var: "--a", type: "color", label: "First" },
      { var: "--a", type: "number", label: "Second" },
    ]);
    expect(parseTweaks(json)).toEqual([{ var: "--a", type: "color", label: "First" }]);
  });

  it("gives none for anything that is not a JSON list within the limit", () => {
    expect(parseTweaks(null)).toEqual([]);
    expect(parseTweaks("not json")).toEqual([]);
    expect(parseTweaks('{"var":"--a","type":"color"}')).toEqual([]);
    expect(parseTweaks(`[${" ".repeat(TWEAKS_JSON_MAX)}]`)).toEqual([]);
  });

  it("never evaluates: a function-like text stays text and is dropped", () => {
    expect(parseTweaks('[{"var":"--a","type":"color","label":"x"}, alert(1)]')).toEqual([]);
  });
});

describe("tweakValue", () => {
  const color: ArtifactTweak = { var: "--accent", label: "Accent", type: "color" };
  const space: ArtifactTweak = { var: "--space", label: "Spacing", type: "length", min: 4, max: 32, unit: "px" };
  const scale: ArtifactTweak = { var: "--scale", label: "Scale", type: "number", min: 0, max: 2 };
  const density: ArtifactTweak = {
    var: "--density",
    label: "Density",
    type: "choice",
    options: ["compact", "comfortable"],
  };

  it("takes a colour only as hex", () => {
    expect(tweakValue(color, "#1F6FEB")).toBe("#1f6feb");
    expect(tweakValue(color, "#abc")).toBe("#abc");
    for (const bad of ["red", "rgb(1,2,3)", "#12345", "#1f6feb;x", "#1f6feb)", "url(x)", 3])
      expect(tweakValue(color, bad)).toBeNull();
  });

  it("takes a length as a number with the tweak's unit, within min and max", () => {
    expect(tweakValue(space, "20px")).toBe("20px");
    expect(tweakValue(space, "4.5px")).toBe("4.5px");
    for (const bad of ["20", "20rem", "2px", "40px", "20 px", "calc(20px)", "20px;color:red", "px"])
      expect(tweakValue(space, bad)).toBeNull();
  });

  it("takes a number within min and max", () => {
    expect(tweakValue(scale, "1.25")).toBe("1.25");
    expect(tweakValue(scale, "3")).toBeNull();
    expect(tweakValue(scale, "1e1")).toBeNull();
    expect(tweakValue(scale, "")).toBeNull();
  });

  it("takes a choice only from its options", () => {
    expect(tweakValue(density, "compact")).toBe("compact");
    expect(tweakValue(density, "spacious")).toBeNull();
  });
});

describe("currentValue", () => {
  it("keeps what getComputedStyle gave on one short line", () => {
    expect(currentValue("  #15764c ")).toBe("#15764c");
    expect(currentValue("a\nb")).toBe("a b");
    expect(currentValue("x".repeat(500))).toHaveLength(100);
    expect(currentValue(undefined)).toBe("");
  });
});
