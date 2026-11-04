import { describe, expect, it } from "vitest";
import { filterGroups, type PaletteItem, score } from "./palette-logic";

const item = (label: string, detail = ""): PaletteItem => ({ id: label, label, detail, mark: "", run: () => {} });

describe("the palette", () => {
  it("finds every word, the start of a word first", () => {
    expect(score("", "anything")).toBe(1);
    expect(score("nope", "Northwind API")).toBe(0);
    expect(score("api north", "Northwind API")).toBeGreaterThan(0);
    expect(score("north", "Northwind")).toBeGreaterThan(score("orth", "Northwind"));
    expect(score("142", "#142 Fix")).toBeGreaterThan(score("42", "#142 Fix"));
  });

  it("orders by match, drops empty groups and caps long ones", () => {
    const many = Array.from({ length: 30 }, (_, i) => item(`Ticket ${i}`));
    const groups = filterGroups("ticket", [
      { label: "Tickets", items: many },
      { label: "Projects", items: [item("Website")] },
    ]);
    expect(groups.map((g) => g.label)).toEqual(["Tickets"]);
    expect(groups[0]?.items).toHaveLength(8);
    const ranked = filterGroups("api", [{ label: "P", items: [item("Rapid"), item("API docs")] }]);
    expect(ranked[0]?.items.map((i) => i.label)).toEqual(["API docs", "Rapid"]);
    const exact = filterGroups("settings", [{ label: "C", items: [item("Settings: Backups"), item("Settings")] }]);
    expect(exact[0]?.items[0]?.label).toBe("Settings");
  });
});
