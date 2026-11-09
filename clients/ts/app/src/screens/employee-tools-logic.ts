// What the tool list of an employee computes, without a screen (C2, C6).

import type { AccessLevel, StartProfile, ToolOption, ToolPolicy } from "@aurelius/protocol";

/** The approval rule for a whole MCP server, spelled like Claude Code spells its tools. */
export const mcpRule = (server: string): string => `mcp__${server.replace(/[^A-Za-z0-9_-]/g, "_")}__*`;

export const toolOn = (p: ToolPolicy, name: string): boolean => p.tools == null || p.tools.includes(name);

export const serverOn = (p: ToolPolicy, name: string): boolean => p.mcp_servers == null || p.mcp_servers.includes(name);

export const asksFirst = (p: ToolPolicy, rule: string): boolean => (p.approval_tools ?? []).includes(rule);

/** Whether this level has the tool at all. */
export const inLevel = (option: ToolOption, access: AccessLevel): boolean => option.levels.includes(access);

/** Nothing set: what is stored for an employee with all of its level. */
export const isDefaultPolicy = (p: ToolPolicy): boolean =>
  p.tools == null && p.mcp_servers == null && (p.approval_tools ?? []).length === 0;

const canonical = (p: ToolPolicy) =>
  JSON.stringify([
    p.tools == null ? null : [...p.tools].sort(),
    [...(p.approval_tools ?? [])].sort(),
    p.mcp_servers == null ? null : [...p.mcp_servers].sort(),
  ]);

export const samePolicy = (a: ToolPolicy, b: ToolPolicy): boolean => canonical(a) === canonical(b);

const without = (list: string[] | null | undefined, name: string) => (list ?? []).filter((n) => n !== name);

function toggled(current: string[] | null | undefined, all: string[], name: string, on: boolean): string[] | null {
  const base = current ?? all;
  const next = on ? [...without(base, name), name] : without(base, name);
  // Everything on again is no list: a tool added later is then on as well.
  return all.every((n) => next.includes(n)) ? null : next;
}

/** A tool switched off no longer asks. */
export function setTool(p: ToolPolicy, all: ToolOption[], name: string, on: boolean): ToolPolicy {
  const tools = toggled(
    p.tools,
    all.map((t) => t.name),
    name,
    on,
  );
  return { ...p, tools, approval_tools: on ? (p.approval_tools ?? []) : without(p.approval_tools, name) };
}

export function setServer(p: ToolPolicy, servers: string[], name: string, on: boolean): ToolPolicy {
  return {
    ...p,
    mcp_servers: toggled(p.mcp_servers, servers, name, on),
    approval_tools: on ? (p.approval_tools ?? []) : without(p.approval_tools, mcpRule(name)),
  };
}

export function setAsksFirst(p: ToolPolicy, rule: string, on: boolean): ToolPolicy {
  return { ...p, approval_tools: on ? [...without(p.approval_tools, rule), rule] : without(p.approval_tools, rule) };
}

/** For the Tools button and the employee page; none when it has all of its level. */
export function policyLine(p: ToolPolicy): string | null {
  if (isDefaultPolicy(p)) return null;
  const asks = (p.approval_tools ?? []).length;
  return [
    p.tools == null ? null : `${p.tools.length} on`,
    p.mcp_servers == null ? null : `${p.mcp_servers.length} MCP server${p.mcp_servers.length === 1 ? "" : "s"}`,
    asks > 0 ? `${asks} ask${asks === 1 ? "s" : ""} first` : null,
  ]
    .filter((part) => part !== null)
    .join(", ");
}

/** The picker's line: who already started from this profile. */
export const profileOption = (p: StartProfile): string =>
  (p.hired ?? []).length > 0 ? `${p.label} · hired as ${(p.hired ?? []).join(", ")}` : p.label;

/** The name a profile leaves: what was typed stays, a suggestion another employee has is not offered. */
export function nameAfterProfile(
  current: string,
  earlier: string | null,
  next: string | null,
  taken: string[],
): string {
  const typed = current.trim() !== "" && current !== earlier;
  if (typed) return current;
  if (next === null || taken.some((n) => n.toLowerCase() === next.toLowerCase())) return "";
  return next;
}
