import type { ToolOption } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  asksFirst,
  inLevel,
  isDefaultPolicy,
  mcpRule,
  nameAfterProfile,
  policyLine,
  profileOption,
  samePolicy,
  serverOn,
  setAsksFirst,
  setServer,
  setTool,
  toolOn,
} from "./employee-tools-logic";

const tool = (name: string): ToolOption => ({ name, label: name, group: "Files", levels: ["inherit"] });
const all = [tool("Read"), tool("Bash")];

describe("the tool list of an employee", () => {
  it("has everything on without a list", () => {
    expect(toolOn({}, "Bash")).toBe(true);
    expect(serverOn({}, "github")).toBe(true);
    expect(isDefaultPolicy({})).toBe(true);
    expect(policyLine({})).toBeNull();
  });

  it("makes a list when one goes off and drops it when all are on again", () => {
    const off = setTool({}, all, "Bash", false);
    expect(off.tools).toEqual(["Read"]);
    expect(toolOn(off, "Bash")).toBe(false);
    expect(setTool(off, all, "Bash", true).tools).toBeNull();
  });

  it("stops asking for a tool that goes off", () => {
    const asks = setAsksFirst({}, "Bash", true);
    expect(asksFirst(asks, "Bash")).toBe(true);
    expect(setTool(asks, all, "Bash", false).approval_tools).toEqual([]);
  });

  it("spells a server like Claude Code and stops asking when it goes off", () => {
    expect(mcpRule("my.srv")).toBe("mcp__my_srv__*");
    const asks = setAsksFirst({}, mcpRule("a"), true);
    const off = setServer(asks, ["a", "b"], "a", false);
    expect(off.mcp_servers).toEqual(["b"]);
    expect(off.approval_tools).toEqual([]);
    expect(setServer(off, ["a", "b"], "a", true).mcp_servers).toBeNull();
  });

  it("compares without looking at the order", () => {
    expect(samePolicy({ tools: ["a", "b"] }, { tools: ["b", "a"], approval_tools: [] })).toBe(true);
    expect(samePolicy({}, { tools: [] })).toBe(false);
  });

  it("says briefly what is set", () => {
    expect(policyLine({ tools: ["Read"], mcp_servers: ["a", "b"], approval_tools: ["Read"] })).toBe(
      "1 on, 2 MCP servers, 1 asks first",
    );
  });

  it("knows what a level has", () => {
    expect(inLevel(tool("Read"), "inherit")).toBe(true);
    expect(inLevel(tool("Read"), "read_only")).toBe(false);
  });
});

describe("start profiles", () => {
  const profile = { key: "qa", label: "QA", summary: "", name: "Quinn", role: "QA", description: "", instructions: "" };

  it("says who started from it", () => {
    expect(profileOption(profile)).toBe("QA");
    expect(profileOption({ ...profile, hired: ["Quinn"] })).toBe("QA · hired as Quinn");
  });

  it("offers a suggested name only when it is free and nothing was typed", () => {
    expect(nameAfterProfile("", null, "Quinn", [])).toBe("Quinn");
    expect(nameAfterProfile("", null, "Quinn", ["quinn"])).toBe("");
    expect(nameAfterProfile("Mine", null, "Quinn", [])).toBe("Mine");
    expect(nameAfterProfile("Quinn", "Quinn", "Dara", [])).toBe("Dara");
    expect(nameAfterProfile("Quinn", "Quinn", null, [])).toBe("");
  });
});
