import { expectBody } from "@aurelius/client";
import type { DefaultsView, EmployeeSummary, RunSettings, Setting2 } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useEmployees, useKeyed } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { Button, initial, Mark, markColor } from "../ui/kit";
import { Dialog } from "../ui/overlay";
import { toast } from "../ui/toast";
import { cycle } from "./dialog-logic";
import { useModels } from "./dialogs";
import { type AgentMode, MODE_LABELS, MODES, reportsCandidates } from "./employee-logic";
import { AREA, FIELD, failureOf, shortDay } from "./org-kit";
import { useFolderChooser } from "./run-tools";

const EFFORTS = [
  ["inherit", "Inherit"],
  ["low", "Low · Fastest answers"],
  ["medium", "Medium · Balanced"],
  ["high", "High · Thinks longer"],
  ["xhigh", "Extra high · For hard problems"],
  ["max", "Max · Deepest reasoning, slowest"],
] as const;

const asChoice = (s: Setting2 | undefined) => (s?.mode === "value" ? s.value : "inherit");
const asSetting = (v: string): Setting2 => (v === "inherit" ? { mode: "inherit" } : { mode: "value", value: v });

const LABEL = "flex flex-col gap-1 text-[12px] text-text3";

export function EmployeeDialog({ employee, onClose }: { employee: EmployeeSummary | null; onClose: () => void }) {
  useKeyContext("AureliusDialog", true);
  const connection = useConnection();
  const client = useQueryClient();
  const navigate = useNavigate();
  const folders = useFolderChooser();
  const { features } = useLinkState();
  const models = useModels();
  const employees = useEmployees();
  const [name, setName] = useState(employee?.name ?? "");
  const [role, setRole] = useState(employee?.role ?? "");
  const [title, setTitle] = useState(employee?.title ?? "");
  const [reportsTo, setReportsTo] = useState(employee?.reports_to ?? "");
  const [instructions, setInstructions] = useState(employee?.instructions ?? "");
  const [folder, setFolder] = useState("");
  const [mode, setMode] = useState<AgentMode>(employee?.agent_mode ?? "ask");
  const [sequential, setSequential] = useState(employee?.sequential_tasks ?? false);
  const [more, setMore] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const defaults = useKeyed(
    ["settings", "connections", "defaults", employee?.id],
    async () => expectBody(await connection.call("connection.list"), "connections").value.defaults,
    employee !== null,
  );
  const stored = (purpose: "employee" | "subagent"): RunSettings =>
    (defaults.data ?? []).find(
      (d: DefaultsView) => d.purpose === purpose && d.scope.kind === "employee" && d.scope.id === employee?.id,
    )?.settings ?? {};
  const [picks, setPicks] = useState<{ model?: string; effort?: string; subModel?: string; subEffort?: string }>({});
  const model = picks.model ?? asChoice(stored("employee").model);
  const effort = picks.effort ?? asChoice(stored("employee").effort);
  const subModel = picks.subModel ?? asChoice(stored("subagent").model);
  const subEffort = picks.subEffort ?? asChoice(stored("subagent").effort);

  const hasVersions = features.includes("instruction_proposals");
  const versions = useKeyed(
    ["employees", "instructions", employee?.id],
    async () =>
      expectBody(
        await connection.call("employee.instruction_versions", { employee_id: employee?.id as string }),
        "employee_instruction_versions",
      ).value,
    employee !== null && hasVersions && more,
  );
  const candidates = reportsCandidates(employees.data ?? [], employee?.id ?? null);
  const refresh = () => client.invalidateQueries({ queryKey: ["employees"] });

  const save = async () => {
    if (busy) return;
    if (name.trim() === "") return setError("Enter a name.");
    if (role.trim() === "") return setError("Give the employee a role. It can be anything.");
    setBusy(true);
    setError(null);
    try {
      if (!employee) {
        const receipt = await connection.write(
          "employee.create",
          {
            name: name.trim(),
            role: role.trim(),
            instructions,
            ...(title.trim() ? { title: title.trim() } : {}),
            ...(reportsTo ? { reports_to: reportsTo } : {}),
            ...(folder.trim() ? { folder: folder.trim() } : {}),
          },
          { retry: false },
        );
        await refresh();
        toast("Employee added.");
        onClose();
        const made = receipt.result;
        if (made.kind === "command" && made.value.type === "employee_created") {
          void navigate({ to: "/employees/$id", params: { id: made.value.employee_id } });
        }
        return;
      }
      const scope = { kind: "employee" as const, id: employee.id };
      if (picks.model !== undefined || picks.effort !== undefined) {
        await connection.write(
          "connection.set_defaults",
          {
            scope,
            purpose: "employee",
            settings: { ...stored("employee"), model: asSetting(model), effort: asSetting(effort) },
          },
          { retry: false },
        );
      }
      if (picks.subModel !== undefined || picks.subEffort !== undefined) {
        await connection.write(
          "connection.set_defaults",
          {
            scope,
            purpose: "subagent",
            settings: { ...stored("subagent"), model: asSetting(subModel), effort: asSetting(subEffort) },
          },
          { retry: false },
        );
      }
      if (mode !== employee.agent_mode && employee.conversation_id) {
        await connection.write(
          "conversation.set_agent_mode",
          { conversation_id: employee.conversation_id, mode },
          { retry: false },
        );
      }
      await connection.write(
        "employee.update",
        {
          employee_id: employee.id,
          name: name.trim(),
          role: role.trim(),
          title: title.trim(),
          instructions,
          sequential_tasks: sequential,
        },
        { retry: false, expectedVersion: employee.version },
      );
      if ((employee.reports_to ?? "") !== reportsTo) {
        await connection.write(
          "employee.set_reports_to",
          { employee_id: employee.id, ...(reportsTo ? { reports_to: reportsTo } : {}) },
          { retry: false },
        );
      }
      await refresh();
      await client.invalidateQueries({ queryKey: ["settings"] });
      onClose();
    } catch (e) {
      setError(failureOf(e));
      setBusy(false);
    }
  };
  useAction("ConfirmPrimary", () => void save());
  useAction("ToggleManagerSequential", () => setSequential(!sequential));
  useAction("CycleReportsTo", () => setReportsTo(cycle(["", ...candidates.map((c) => c.id)], reportsTo)));
  useAction("CycleTaskEffort", () =>
    setPicks({
      ...picks,
      effort: cycle(
        EFFORTS.map(([k]) => k as string),
        effort,
      ),
    }),
  );
  useAction("CycleSubagentModel", () =>
    setPicks({
      ...picks,
      subModel: cycle(
        models.map((m) => (m.key === "default" ? "inherit" : m.key)),
        subModel,
      ),
    }),
  );
  useAction("ChooseDialogFolder", () => folders.choose("Choose a folder for this employee", setFolder));

  const restore = async (version: number) => {
    if (!employee) return;
    try {
      await connection.write("employee.restore_instructions", { employee_id: employee.id, version }, { retry: false });
      await refresh();
      onClose();
    } catch (e) {
      setError(failureOf(e));
    }
  };
  const heading = employee ? "Edit employee" : "Add employee";
  return (
    <Dialog open onClose={onClose} title={heading} width={592} top>
      <div className="flex h-[52px] items-center gap-[10px] border-b border-line2 px-[18px]">
        <Mark
          letter={initial(name || "?")}
          color={employee ? markColor(employee.id) : "#3A3D44"}
          size={26}
          radius={7}
        />
        <span className="text-[14px] font-semibold">{heading}</span>
      </div>
      <div className="flex max-h-[68vh] flex-col gap-[14px] overflow-y-auto px-[18px] py-4">
        {employee ? (
          <p className="m-0 text-[12px] text-text3">
            Changes apply to new runs; a running run keeps its configuration.
          </p>
        ) : null}
        <div className="grid grid-cols-2 gap-[10px]">
          <label className={LABEL}>
            Name
            <input
              data-autofocus
              placeholder="Name, for example Alex"
              className={FIELD}
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <label className={LABEL}>
            Role (free text)
            <input
              placeholder="Free-form role, for example Product partner"
              className={FIELD}
              value={role}
              onChange={(e) => setRole(e.target.value)}
            />
          </label>
          <label className={LABEL}>
            Title on the org chart
            <input
              placeholder="Optional, e.g. Head of design"
              className={FIELD}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </label>
          <label className={LABEL}>
            Reports to
            <select className={FIELD} value={reportsTo} onChange={(e) => setReportsTo(e.target.value)}>
              <option value="">Nobody (top of the org chart)</option>
              {candidates.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </label>
        </div>
        <label className={LABEL}>
          Instructions
          <textarea rows={7} className={AREA} value={instructions} onChange={(e) => setInstructions(e.target.value)} />
        </label>
        {employee ? null : (
          <label className={LABEL}>
            Own folder (optional)
            <span className="flex gap-2">
              <input
                placeholder="No folder chosen"
                className={`${FIELD} font-mono`}
                value={folder}
                onChange={(e) => setFolder(e.target.value)}
              />
              <Button onClick={() => folders.choose("Choose a folder for this employee", setFolder)}>Choose…</Button>
              {folders.dialog}
            </span>
            <span>Where its chat works. In a task the project folder applies.</span>
          </label>
        )}
        {employee ? (
          <>
            <div className="grid grid-cols-3 gap-[10px]">
              <label className={LABEL}>
                Default mode
                <select className={FIELD} value={mode} onChange={(e) => setMode(e.target.value as AgentMode)}>
                  {MODES.map((m) => (
                    <option key={m} value={m}>
                      {MODE_LABELS[m]}
                    </option>
                  ))}
                </select>
              </label>
              <label
                className={LABEL}
                title="The default for this employee's chat and tasks. A chat or task can override it for itself (Ctrl+M) without changing this."
              >
                Model
                <select className={FIELD} value={model} onChange={(e) => setPicks({ ...picks, model: e.target.value })}>
                  {models.map((m) => (
                    <option key={m.key} value={m.key === "default" ? "inherit" : m.key}>
                      {m.key === "default" ? "Inherit" : m.label}
                    </option>
                  ))}
                </select>
              </label>
              <label className={LABEL}>
                Effort
                <select
                  className={FIELD}
                  value={effort}
                  onChange={(e) => setPicks({ ...picks, effort: e.target.value })}
                >
                  {EFFORTS.map(([k, l]) => (
                    <option key={k} value={k}>
                      {l}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            <button
              type="button"
              aria-expanded={more}
              onClick={() => setMore(!more)}
              className="cursor-pointer self-start border-0 bg-transparent p-0 text-[12px] text-text2 hover:text-text"
            >
              {more ? "Less ▴" : "More ▾"} Advanced: Subagents, run one at a time{sequential ? " (on)" : ""}
              {hasVersions ? ", instruction versions" : ""}
            </button>
            {more ? (
              <div className="flex flex-col gap-3">
                <div className="grid grid-cols-2 gap-[10px]">
                  <label className={LABEL}>
                    Subagents of this employee
                    <select
                      className={FIELD}
                      value={subModel}
                      onChange={(e) => setPicks({ ...picks, subModel: e.target.value })}
                    >
                      {models.map((m) => (
                        <option key={m.key} value={m.key === "default" ? "inherit" : m.key}>
                          {m.key === "default" ? "Inherit" : m.label}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className={LABEL}>
                    Effort of subagents
                    <select
                      className={FIELD}
                      value={subEffort}
                      onChange={(e) => setPicks({ ...picks, subEffort: e.target.value })}
                    >
                      {EFFORTS.map(([k, l]) => (
                        <option key={k} value={k}>
                          {k === "inherit" ? "Effort: follows the employee" : l}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                <label className="flex cursor-pointer items-start gap-2">
                  <input
                    type="checkbox"
                    className="mt-[3px] accent-(--acc-text)"
                    checked={sequential}
                    onChange={(e) => setSequential(e.target.checked)}
                  />
                  <span>
                    Run tasks one at a time
                    <span className="block text-[12px] text-text3">
                      Tasks wait in line and start one after the other.
                    </span>
                  </span>
                </label>
                {hasVersions ? (
                  <div className="flex flex-col gap-1">
                    <p className="m-0 text-[12px] text-text3">Earlier instructions</p>
                    {versions.data?.versions.map((v) => (
                      <div
                        key={v.version}
                        className="flex items-center gap-2 rounded-[6px] border border-line px-2 py-[6px] text-[12px]"
                      >
                        <span className="flex-none font-medium">Version {v.version}</span>
                        <span className="flex-none text-text3">
                          {shortDay(v.created_at_ms)} ·{" "}
                          {v.version === versions.data?.current_version
                            ? "current"
                            : v.restored_from != null
                              ? `restored from version ${v.restored_from}`
                              : v.proposal_id
                                ? "approved proposal"
                                : "as it was"}
                        </span>
                        <span className="min-w-0 flex-1 truncate text-text3">
                          {v.instructions.split("\n")[0] || "(no instructions)"}
                        </span>
                        {v.version !== versions.data?.current_version ? (
                          <Button className="h-6 px-2 text-[12px]" onClick={() => void restore(v.version)}>
                            Restore
                          </Button>
                        ) : null}
                      </div>
                    ))}
                  </div>
                ) : null}
              </div>
            ) : null}
          </>
        ) : null}
        {error ? (
          <p role="alert" className="m-0 text-red">
            {error}
          </p>
        ) : null}
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Cancel</Button>
        <Button kind="primary" disabled={busy} onClick={() => void save()}>
          {employee ? "Save" : "Add"}
        </Button>
      </div>
    </Dialog>
  );
}
