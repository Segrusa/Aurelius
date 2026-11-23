import { describe, expect, it } from "vitest";
import {
  archiveConfirmed,
  DESCRIPTION_MAX,
  descriptionCounter,
  dispatcherWarning,
  nameSavable,
} from "./workspace-settings-logic";

describe("nameSavable", () => {
  it("is false while the name is unchanged or only differs by spaces", () => {
    expect(nameSavable(" Aurelius ", "Aurelius")).toBe(false);
  });
  it("is false for an empty name", () => {
    expect(nameSavable("   ", "Aurelius")).toBe(false);
  });
  it("is true for a new name", () => {
    expect(nameSavable("Aurelius app", "Aurelius")).toBe(true);
  });
});

describe("descriptionCounter", () => {
  it("counts up to the limit", () => {
    expect(descriptionCounter("abc")).toEqual({ text: "3 / 2000", tooLong: false });
  });
  it("says how much too long a description is", () => {
    const text = "x".repeat(DESCRIPTION_MAX + 1);
    expect(descriptionCounter(text)).toEqual({ text: "Too long: 2001 of at most 2000 characters.", tooLong: true });
  });
});

describe("dispatcherWarning", () => {
  const people = [
    { id: "titan", name: "Titan", status: "active" },
    { id: "flux", name: "Flux", status: "paused" },
  ];
  it("warns when neither the workspace nor the company has a dispatcher", () => {
    expect(dispatcherWarning("", null, people)).toBe(
      "◆ No dispatcher. Started tickets wait in Todo until you choose an owner.",
    );
  });
  it("says nothing when the company's dispatcher is active", () => {
    expect(dispatcherWarning("", "titan", people)).toBeNull();
  });
  it("warns when the chosen dispatcher is not active", () => {
    expect(dispatcherWarning("flux", "titan", people)).toBe(
      "◆ Flux is paused. Started tickets wait for you to choose an owner.",
    );
  });
});

describe("archiveConfirmed", () => {
  it("needs the exact workspace name", () => {
    expect(archiveConfirmed("Aurelius", "Aurelius")).toBe(true);
    expect(archiveConfirmed("aurelius", "Aurelius")).toBe(false);
    expect(archiveConfirmed("", "Aurelius")).toBe(false);
  });
});
