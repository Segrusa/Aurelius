import { describe, expect, it } from "vitest";
import { linkedTicket, messageLink, parseLink, ticketLink } from "./links-logic";

describe("aurelius links", () => {
  it("read back what they write", () => {
    expect(parseLink(ticketLink("h-1", "t-2"))).toEqual({ host: "h-1", kind: "ticket", id: "t-2" });
    expect(parseLink(messageLink("h-1", "m-2"))).toEqual({ host: "h-1", kind: "message", id: "m-2" });
    expect(parseLink(ticketLink("h-1", "70"))).toEqual({ host: "h-1", kind: "ticket", id: "70" });
  });

  it("open a ticket by its number, and an old link by its id", () => {
    expect(linkedTicket("70")).toEqual({ number: 70 });
    expect(linkedTicket("0b1c-2d")).toEqual({ id: "0b1c-2d" });
    expect(linkedTicket("4f1e3c2a-1b2c-4d5e-8f90-123456789abc")).toEqual({
      id: "4f1e3c2a-1b2c-4d5e-8f90-123456789abc",
    });
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
