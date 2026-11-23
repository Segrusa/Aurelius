import { describe, expect, it } from "vitest";
import { runnable } from "./open-file";

describe("runnable", () => {
  it("a plain text file opens", () => {
    expect(runnable("/w/src/api.rs", 0o644, "linux")).toBe(false);
    expect(runnable("C:\\w\\notes.md", 0o666, "win32")).toBe(false);
  });

  it("an executable bit makes a file runnable outside Windows", () => {
    expect(runnable("/w/build", 0o755, "linux")).toBe(true);
    expect(runnable("/w/build", 0o744, "darwin")).toBe(true);
    expect(runnable("C:\\w\\notes.md", 0o777, "win32")).toBe(false);
  });

  it("a runnable extension is refused whatever its mode or case", () => {
    expect(runnable("/w/launch.desktop", 0o644, "linux")).toBe(true);
    expect(runnable("/w/setup.SH", 0o644, "linux")).toBe(true);
    expect(runnable("C:\\w\\run.Bat", 0o666, "win32")).toBe(true);
    expect(runnable("C:\\w\\tool.js", 0o666, "win32")).toBe(true);
    expect(runnable("/w/Open.command", 0o644, "darwin")).toBe(true);
  });
});
