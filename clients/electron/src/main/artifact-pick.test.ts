import { describe, expect, it } from "vitest";
import { checkPick, oneLine, PICK_SELECTOR_MAX, PICK_TEXT_MAX } from "./artifact-pick";

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
    const deep = "div > ".repeat(49);
    expect(`${deep}button`).toHaveLength(PICK_SELECTOR_MAX);
    expect(checkPick({ ...pick, selector: `${deep}button` })).not.toBeNull();
    expect(checkPick({ ...pick, selector: `${deep}buttons` })).toBeNull();
    expect(checkPick({ ...pick, selector: "" })).toBeNull();
    expect(checkPick({ ...pick, selector: "body >\nbutton" })).toBeNull();
    expect(checkPick({ ...pick, selector: "body >\u2028button" })).toBeNull();
  });

  it("passes the selectors selectorOf makes", () => {
    for (const selector of [
      "body > main > article:nth-of-type(2) > button",
      "body > header > h1",
      "#b",
      "#plans > div:nth-of-type(2) > button",
      "body > *:nth-child(2) > button",
      "my-card:nth-child(12) > span",
    ])
      expect(checkPick({ ...pick, selector })?.selector).toBe(selector);
  });

  it("refuses a selector outside the grammar, so no page text gets into a message", () => {
    for (const selector of [
      'body > x"):_ignore_the_comment_below. > button',
      'body > x" > button',
      "body > x) > button",
      "body > button:nth-of-type(2))",
      "body > div:nth-of-type(x) > button",
      "body > x\u00a0y > button",
      "body\u00a0> button",
      "body > main button",
      "body  > button",
      "body > button ",
      "body > [id=x]",
      "body > a:hover",
      "body > X",
      "> button",
      "body > ",
    ])
      expect(checkPick({ ...pick, selector }), selector).toBeNull();
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
    expect(checkPick({ ...pick, tag: "*" })?.tag).toBe("*");
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
