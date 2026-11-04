import { describe, expect, it } from "vitest";
import { messageLink, parseLink, ticketLink } from "./links-logic";

describe("aurelius links", () => {
  it("read back what they write", () => {
    expect(parseLink(ticketLink("h-1", "t-2"))).toEqual({ host: "h-1", kind: "ticket", id: "t-2" });
    expect(parseLink(messageLink("h-1", "m-2"))).toEqual({ host: "h-1", kind: "message", id: "m-2" });
  });

  it("refuse any other form", () => {
    for (const text of [
      "https://x/ticket/1",
      "aurelius://ticket/1",
      "aurelius://h/ticket/1/more",
      "aurelius://h/other/1",
      "aurelius://h/ticket/",
      "aurelius://../ticket/1",
      "aurelius://h/ticket/a b",
    ]) {
      expect(parseLink(text)).toBeNull();
    }
  });
});
