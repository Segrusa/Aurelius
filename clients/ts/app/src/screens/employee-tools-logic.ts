// An employee's tool policy and start profile, without a screen (C2, C6). The tool list itself is gone (OPR-4).

import type { StartProfile, ToolPolicy } from "@aurelius/protocol";

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
