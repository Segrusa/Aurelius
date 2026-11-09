import { describe, expect, it } from "vitest";
import { ARTIFACT_URL_MAX, artifactProblem } from "./design-logic";

describe("artifactProblem", () => {
  it("takes both kinds of artifact link, trimmed", () => {
    expect(artifactProblem("https://claude.ai/artifact/9sfwWLbDbQerBZ3jmBSkXS")).toBeNull();
    expect(artifactProblem(" https://claude.ai/code/artifact/8f14e45f-ceea-467f-a22a-2b7a2c55e3b1 ")).toBeNull();
  });

  it("empty clears the link", () => {
    expect(artifactProblem("  ")).toBeNull();
  });

  it("refuses anything else, with what it should be", () => {
    for (const bad of [
      "http://claude.ai/artifact/a",
      "https://claude.ai/chat/a",
      "https://claude.ai/artifact/",
      "https://claude.ai/artifact/a?b=1",
      "https://example.com/artifact/a",
      "not a link",
    ]) {
      expect(artifactProblem(bad)).toContain("https://claude.ai/artifact/");
    }
  });

  it("refuses a link that is too long", () => {
    expect(artifactProblem(`https://claude.ai/artifact/${"a".repeat(ARTIFACT_URL_MAX)}`)).toContain("at most");
  });
});
