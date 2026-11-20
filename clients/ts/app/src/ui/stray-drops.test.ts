import { describe, expect, it } from "vitest";
import { strayDrop } from "./stray-drops";

describe("strayDrop", () => {
  it("refuses a file that nothing took, also over a text field", () => {
    expect(strayDrop(["Files"], false, false)).toBe(true);
    expect(strayDrop(["Files"], false, true)).toBe(true);
  });

  it("leaves a drop a drop zone took", () => {
    expect(strayDrop(["Files"], true, false)).toBe(false);
  });

  it("refuses a link outside a text field and lets a text field take it as text", () => {
    expect(strayDrop(["text/uri-list", "text/plain"], false, false)).toBe(true);
    expect(strayDrop(["text/uri-list", "text/plain"], false, true)).toBe(false);
  });

  it("leaves plain text alone", () => {
    expect(strayDrop(["text/plain"], false, false)).toBe(false);
  });
});
