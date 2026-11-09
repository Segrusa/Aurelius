import { Button } from "@astryxdesign/core/Button";
import { CheckboxInput } from "@astryxdesign/core/CheckboxInput";
import { expectBody } from "@aurelius/client";
import type { AccessLevel, ToolOption, ToolPolicy } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useKeyed } from "../data";
import {
  asksFirst,
  inLevel,
  mcpRule,
  policyLine,
  serverOn,
  setAsksFirst,
  setServer,
  setTool,
  toolOn,
} from "./employee-tools-logic";
import { Hint } from "./org-kit";

const styles = stylex.create({
  stack: { display: "flex", flexDirection: "column", gap: 12 },
  group: { display: "flex", flexDirection: "column", gap: 4 },
  row: { display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 },
  head: { fontSize: 12, fontWeight: 500, color: "var(--color-text-secondary)" },
});

const askLabel = (name: string) => `${name} asks first`;

/** Tools, approvals and MCP servers of an employee: a list that folds out (C2). */
export function EmployeeTools({
  policy,
  access,
  onChange,
}: {
  policy: ToolPolicy;
  access: AccessLevel;
  onChange: (next: ToolPolicy) => void;
}) {
  const connection = useConnection();
  const { features } = useLinkState();
  const [open, setOpen] = useState(false);
  const wanted = features.includes("employee_tools");
  const options = useKeyed(
    ["employees", "tool_options"],
    async () => expectBody(await connection.call("employee.tool_options"), "employee_tool_options").value.tools,
    wanted && open,
  );
  const mcp = useKeyed(
    ["settings", "mcp"],
    async () => expectBody(await connection.call("mcp.list"), "mcp_servers").value,
    wanted && open,
  );
  if (!wanted) return null;
  const tools = options.data ?? [];
  const servers = (mcp.data?.servers ?? []).filter((s) => s.enabled).map((s) => s.name);
  const groups = [...new Set(tools.map((t) => t.group))];
  const line = policyLine(policy);
  const row = (tool: ToolOption) => {
    const allowed = inLevel(tool, access);
    const on = allowed && toolOn(policy, tool.name);
    return (
      <div key={tool.name} {...stylex.props(styles.row)}>
        <CheckboxInput
          label={tool.label}
          value={on}
          isDisabled={!allowed}
          onChange={(checked) => onChange(setTool(policy, tools, tool.name, checked))}
        />
        <CheckboxInput
          label={askLabel(tool.label)}
          isLabelHidden
          value={asksFirst(policy, tool.name)}
          isDisabled={!on}
          onChange={(checked) => onChange(setAsksFirst(policy, tool.name, checked))}
        />
      </div>
    );
  };
  return (
    <div {...stylex.props(styles.stack)}>
      <div>
        <Button
          size="sm"
          variant="ghost"
          aria-expanded={open}
          label={`${open ? "Less ▴" : "More ▾"} Tools: ${line ?? "all of its access"}`}
          onClick={() => setOpen(!open)}
        />
      </div>
      {open ? (
        <div {...stylex.props(styles.stack)}>
          <Hint>
            A tool that is off is not there for its runs. A tool that asks first goes to a board member in every mode,
            Auto too, and holds at once for a run already going.
          </Hint>
          {groups.map((group) => (
            <div key={group} {...stylex.props(styles.group)}>
              <div {...stylex.props(styles.row, styles.head)}>
                <span>{group}</span>
                <span>Asks first</span>
              </div>
              {tools.filter((t) => t.group === group).map(row)}
            </div>
          ))}
          <div {...stylex.props(styles.group)}>
            <div {...stylex.props(styles.row, styles.head)}>
              <span>MCP servers</span>
              <span>Asks first</span>
            </div>
            {access !== "inherit" ? (
              <Hint>This access level has no MCP servers.</Hint>
            ) : servers.length === 0 ? (
              <Hint>No MCP servers are on for this company.</Hint>
            ) : (
              servers.map((name) => {
                const on = serverOn(policy, name);
                return (
                  <div key={name} {...stylex.props(styles.row)}>
                    <CheckboxInput
                      label={name}
                      value={on}
                      onChange={(checked) => onChange(setServer(policy, servers, name, checked))}
                    />
                    <CheckboxInput
                      label={askLabel(name)}
                      isLabelHidden
                      value={asksFirst(policy, mcpRule(name))}
                      isDisabled={!on}
                      onChange={(checked) => onChange(setAsksFirst(policy, mcpRule(name), checked))}
                    />
                  </div>
                );
              })
            )}
          </div>
        </div>
      ) : null}
    </div>
  );
}
