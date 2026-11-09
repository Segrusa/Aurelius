import { Button } from "@astryxdesign/core/Button";
import { CheckboxInput } from "@astryxdesign/core/CheckboxInput";
import { Field } from "@astryxdesign/core/Field";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { TextArea } from "@astryxdesign/core/TextArea";
import { TextInput } from "@astryxdesign/core/TextInput";
import { expectBody } from "@aurelius/client";
import type {
  AccessLevel,
  DefaultsView,
  EmployeeSummary,
  RunSettings,
  Setting2,
  StartProfile,
  ToolPolicy,
} from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useEmployees, useKeyed } from "../data";
import { hidden } from "../hidden";
import { useAction, useKeyContext } from "../keys/provider";
import { initial, Mark, markColor } from "../ui/marks";
import { toast } from "../ui/toast";
import { cycle } from "./dialog-logic";
import { useModels } from "./dialogs";
import {
  ACCESS_LEVELS,
  type AgentMode,
  accessOf,
  asChoice,
  effortName,
  inheritedRun,
  inheritLabel,
  MODE_LABELS,
  MODES,
  modelAfterProvider,
  modelsFor,
  reportsCandidates,
  usableConnections,
} from "./employee-logic";
import { EmployeeTools } from "./employee-tools";
import { isDefaultPolicy, nameAfterProfile, profileOption, samePolicy } from "./employee-tools-logic";
import { DialogSelector, FormDialog, failureOf, Hint, Problem, shortDay } from "./org-kit";
import { useFolderChooser } from "./run-tools";
import { modelName, useSetting } from "./settings-kit";

const EFFORTS = [
  ["inherit", "Inherit"],
  ["low", "Low · Fastest answers"],
  ["medium", "Medium · Balanced"],
  ["high", "High · Thinks longer"],
  ["xhigh", "Extra high · For hard problems"],
  ["max", "Max · Deepest reasoning, slowest"],
] as const;

const asSetting = (v: string): Setting2 => (v === "inherit" ? { mode: "inherit" } : { mode: "value", value: v });

// Selector takes no empty value: "nobody" and "inherit" need a word of their own.
const NOBODY = "nobody";
const BLANK = "blank";

const styles = stylex.create({
  two: { display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 10 },
  three: { display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: 10 },
  folder: { display: "flex", alignItems: "flex-end", gap: 8 },
  grow: { minWidth: 0, flex: 1 },
  mono: { fontFamily: "var(--font-family-code)" },
  stack: { display: "flex", flexDirection: "column", gap: 12 },
  field: { display: "flex", flexDirection: "column", gap: 6 },
  versions: { display: "flex", flexDirection: "column", gap: 4 },
  version: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    paddingInline: 8,
    paddingBlock: 6,
    fontSize: 12,
  },
  versionNumber: { flexShrink: 0, fontWeight: 500 },
  versionNote: { flexShrink: 0, color: "var(--precision-text3)" },
  versionText: {
    minWidth: 0,
    flex: 1,
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--precision-text3)",
  },
});

export function EmployeeDialog({ employee, onClose }: { employee: EmployeeSummary | null; onClose: () => void }) {
  useKeyContext("AureliusDialog", true);
  const connection = useConnection();
  const client = useQueryClient();
  const navigate = useNavigate();
  const folders = useFolderChooser();
  const { features } = useLinkState();
  const models = useModels();
  const list = useSetting(
    "connections",
    async () => expectBody(await connection.call("connection.list"), "connections").value,
  );
  const connections = usableConnections(list.data?.connections ?? []);
  const inherited = inheritedRun(list.data);
  const hasAccess = features.includes("employee_access");
  // Hidden, its block is gone but the policy stays in the state: save only sends a change.
  const hasTools = features.includes("employee_tools") && !hidden("EmployeeTools");
  const employees = useEmployees();
  const [name, setName] = useState(employee?.name ?? "");
  const [role, setRole] = useState(employee?.role ?? "");
  const [title, setTitle] = useState(employee?.title ?? "");
  const [description, setDescription] = useState(employee?.description ?? "");
  const [reportsTo, setReportsTo] = useState(employee?.reports_to ?? "");
  const [instructions, setInstructions] = useState(employee?.instructions ?? "");
  const [folder, setFolder] = useState("");
  const [mode, setMode] = useState<AgentMode>(employee?.agent_mode ?? "ask");
  const [sequential, setSequential] = useState(employee?.sequential_tasks ?? false);
  const [access, setAccess] = useState<AccessLevel>(accessOf(employee));
  const [policy, setPolicy] = useState<ToolPolicy>(employee?.tool_policy ?? {});
  const [profile, setProfile] = useState<StartProfile | null>(null);
  const [more, setMore] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const profiles = useKeyed(
    ["employees", "start_profiles"],
    async () => expectBody(await connection.call("employee.start_profiles"), "employee_start_profiles").value.profiles,
    employee === null && features.includes("employee_profiles"),
  );
  // Everything comes from the profile; only a name that was typed stays.
  const chooseProfile = (key: string) => {
    const next = (profiles.data ?? []).find((p) => p.key === key) ?? null;
    const taken = (employees.data ?? []).map((e) => e.name);
    setName(nameAfterProfile(name, profile?.name ?? null, next?.name ?? null, taken));
    setProfile(next);
    setRole(next?.role ?? "");
    setTitle(next?.title ?? "");
    setDescription(next?.description ?? "");
    setInstructions(next?.instructions ?? "");
    setAccess(next?.access_level ?? "inherit");
    setPolicy(next?.tool_policy ?? {});
  };

  const defaults = useKeyed(
    ["settings", "connections", "defaults", employee?.id],
    async () => expectBody(await connection.call("connection.list"), "connections").value.defaults,
    employee !== null,
  );
  const stored = (purpose: "employee" | "subagent"): RunSettings =>
    (defaults.data ?? []).find(
      (d: DefaultsView) => d.purpose === purpose && d.scope.kind === "employee" && d.scope.id === employee?.id,
    )?.settings ?? {};
  const [picks, setPicks] = useState<{
    provider?: string;
    model?: string;
    effort?: string;
    subModel?: string;
    subEffort?: string;
  }>({});
  const provider = picks.provider ?? asChoice(stored("employee").connection);
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
    setBusy(true);
    setError(null);
    try {
      if (!employee) {
        const receipt = await connection.write(
          "employee.create",
          {
            name: name.trim(),
            // A role says how they work; without one they are just an employee.
            role: role.trim() || "Employee",
            instructions,
            ...(description.trim() ? { description: description.trim() } : {}),
            ...(title.trim() ? { title: title.trim() } : {}),
            ...(reportsTo ? { reports_to: reportsTo } : {}),
            ...(folder.trim() ? { folder: folder.trim() } : {}),
            ...(access !== "inherit" ? { access_level: access } : {}),
            ...(!isDefaultPolicy(policy) ? { tool_policy: policy } : {}),
            // With a profile, what was cleared here must not come back from the catalogue.
            ...(profile
              ? {
                  start_profile: profile.key,
                  title: title.trim(),
                  description: description.trim(),
                  access_level: access,
                  tool_policy: policy,
                }
              : {}),
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
      if (picks.provider !== undefined || picks.model !== undefined || picks.effort !== undefined) {
        await connection.write(
          "connection.set_defaults",
          {
            scope,
            purpose: "employee",
            settings: {
              ...stored("employee"),
              connection: asSetting(provider),
              model: asSetting(model),
              effort: asSetting(effort),
            },
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
          description: description.trim(),
          instructions,
          sequential_tasks: sequential,
          // Only a change: it takes more rights than an ordinary edit on a phone.
          ...(access !== accessOf(employee) ? { access_level: access } : {}),
          ...(!samePolicy(policy, employee.tool_policy ?? {}) ? { tool_policy: policy } : {}),
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
  useAction("ToggleManagerSequential", () => setSequential(!sequential), !hidden("SequentialTasks"));
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
  const modelOptions = models.map((m) => ({
    value: m.key === "default" ? "inherit" : m.key,
    label: m.key === "default" ? "Inherit" : m.label,
  }));
  const ownModels = [
    { value: "inherit", label: inheritLabel(inherited.model && modelName(inherited.model)) },
    ...modelsFor(connections, provider).map((m) => ({ value: m, label: modelName(m) })),
    // A model no connection lists any more still shows as what is stored.
    ...(model === "inherit" || modelsFor(connections, provider).includes(model)
      ? []
      : [{ value: model, label: modelName(model) }]),
  ];
  const providerOptions = [
    { value: "inherit", label: inheritLabel(inherited.provider) },
    ...connections.map((c) => ({ value: c.id, label: c.display_name })),
  ];
  const chooseProvider = (next: string) =>
    setPicks({ ...picks, provider: next, model: modelAfterProvider(connections, next, model) });
  const accessField = hasAccess ? (
    <div {...stylex.props(styles.field)}>
      <Field isGroupLabel label="Access" inputID="employee-access">
        <SegmentedControl
          label="Access"
          layout="fill"
          value={access}
          onChange={(next) => setAccess(next as AccessLevel)}
        >
          {ACCESS_LEVELS.map((l) => (
            <SegmentedControlItem key={l.value} value={l.value} label={l.label} />
          ))}
        </SegmentedControl>
      </Field>
      <Hint>
        {ACCESS_LEVELS.find((l) => l.value === access)?.hint ?? "A level of a newer server: its runs are read only."}
      </Hint>
    </div>
  ) : null;
  return (
    <FormDialog
      title={heading}
      width={592}
      top
      onClose={onClose}
      start={
        <Mark
          letter={initial(name || "?")}
          color={employee ? markColor(employee.id) : "#3A3D44"}
          size={26}
          radius={7}
        />
      }
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button variant="primary" label={employee ? "Save" : "Add"} isDisabled={busy} onClick={() => void save()} />
        </>
      }
    >
      {employee ? <Hint>Changes apply to new runs; a running run keeps its configuration.</Hint> : null}
      {!employee && (profiles.data ?? []).length > 0 ? (
        <DialogSelector
          label="Start from"
          value={profile?.key ?? BLANK}
          onChange={chooseProfile}
          description={profile?.summary ?? "A profile fills in the fields below; change whatever you like."}
          options={[
            { value: BLANK, label: "Blank" },
            ...(profiles.data ?? []).map((p) => ({ value: p.key, label: profileOption(p) })),
          ]}
        />
      ) : null}
      <div {...stylex.props(styles.two)}>
        <TextInput hasAutoFocus label="Name" placeholder="Name, for example Alex" value={name} onChange={setName} />
        <TextInput
          label="Role (free text)"
          placeholder="Free-form role, for example Product partner"
          value={role}
          onChange={setRole}
        />
        <TextInput
          label="Title on the org chart"
          placeholder="Optional, e.g. Head of design"
          value={title}
          onChange={setTitle}
        />
        <DialogSelector
          label="Reports to"
          value={reportsTo || NOBODY}
          onChange={(v) => setReportsTo(v === NOBODY ? "" : v)}
          options={[
            { value: NOBODY, label: "Nobody (top of the org chart)" },
            ...candidates.map((c) => ({ value: c.id, label: c.name })),
          ]}
        />
      </div>
      <TextInput
        label="Description"
        description="Shown to other employees so they know when to hand work to this one. Empty: they see the first line of the instructions."
        placeholder="For example: reviews Rust changes before they merge"
        value={description}
        onChange={setDescription}
      />
      <TextArea label="Instructions" rows={7} value={instructions} onChange={setInstructions} />
      {employee ? null : (
        <>
          {accessField}
          {hasTools ? <EmployeeTools policy={policy} access={access} onChange={setPolicy} /> : null}
          <div {...stylex.props(styles.folder)}>
            <span {...stylex.props(styles.grow)}>
              <TextInput
                width="100%"
                label="Own folder (optional)"
                placeholder="No folder chosen"
                description="Where its chat works. In a task the project folder applies."
                value={folder}
                onChange={setFolder}
                controlXstyle={styles.mono}
              />
            </span>
            <Button label="Choose…" onClick={() => folders.choose("Choose a folder for this employee", setFolder)} />
          </div>
          {folders.dialog}
        </>
      )}
      {employee ? (
        <>
          <div {...stylex.props(styles.three)}>
            <DialogSelector
              label="Default mode"
              value={mode}
              onChange={(v) => setMode(v as AgentMode)}
              options={MODES.map((m) => ({ value: m, label: MODE_LABELS[m] }))}
            />
            <DialogSelector
              label="Provider"
              labelTooltip="Which connection this employee's runs use. Runs stay on your own Claude login."
              value={provider}
              onChange={chooseProvider}
              options={providerOptions}
            />
            <DialogSelector
              label="Model"
              labelTooltip="The default for this employee's chat and tasks. A chat or task can override it for itself (Ctrl+M) without changing this."
              value={model}
              onChange={(v) => setPicks({ ...picks, model: v })}
              options={ownModels}
            />
          </div>
          <div {...stylex.props(styles.field)}>
            <Field isGroupLabel label="Effort" inputID="employee-effort">
              <SegmentedControl
                label="Effort"
                layout="fill"
                value={effort}
                onChange={(next) => setPicks({ ...picks, effort: next })}
              >
                {EFFORTS.map(([value, label]) => (
                  <SegmentedControlItem key={value} value={value} label={label.split(" · ")[0] as string} />
                ))}
              </SegmentedControl>
            </Field>
            {effort === "inherit" ? (
              <Hint>
                {inherited.effort
                  ? `Inherits ${effortName(inherited.effort)} from the company default.`
                  : "Inherits the default of the model."}
              </Hint>
            ) : null}
          </div>
          {accessField}
          {hasTools ? <EmployeeTools policy={policy} access={access} onChange={setPolicy} /> : null}
          <div>
            <Button
              size="sm"
              variant="ghost"
              aria-expanded={more}
              label={`${more ? "Less ▴" : "More ▾"} Advanced: Subagents${hidden("SequentialTasks") ? "" : `, run one at a time${sequential ? " (on)" : ""}`}${hasVersions ? ", instruction versions" : ""}`}
              onClick={() => setMore(!more)}
            />
          </div>
          {more ? (
            <div {...stylex.props(styles.stack)}>
              <div {...stylex.props(styles.two)}>
                <DialogSelector
                  label="Subagents of this employee"
                  value={subModel}
                  onChange={(v) => setPicks({ ...picks, subModel: v })}
                  options={modelOptions}
                />
                <DialogSelector
                  label="Effort of subagents"
                  value={subEffort}
                  onChange={(v) => setPicks({ ...picks, subEffort: v })}
                  options={EFFORTS.map(([value, label]) => ({
                    value,
                    label: value === "inherit" ? "Effort: follows the employee" : label,
                  }))}
                />
              </div>
              {hidden("SequentialTasks") ? null : (
                <CheckboxInput
                  label="Run tasks one at a time"
                  description="Tasks wait in line and start one after the other."
                  value={sequential}
                  onChange={(checked) => setSequential(checked)}
                />
              )}
              {hasVersions ? (
                <div {...stylex.props(styles.versions)}>
                  <Hint>Earlier instructions</Hint>
                  {versions.data?.versions.map((v) => (
                    <div key={v.version} {...stylex.props(styles.version)}>
                      <span {...stylex.props(styles.versionNumber)}>Version {v.version}</span>
                      <span {...stylex.props(styles.versionNote)}>
                        {shortDay(v.created_at_ms)} ·{" "}
                        {v.version === versions.data?.current_version
                          ? "current"
                          : v.restored_from != null
                            ? `restored from version ${v.restored_from}`
                            : v.proposal_id
                              ? "approved proposal"
                              : "as it was"}
                      </span>
                      <span {...stylex.props(styles.versionText)}>
                        {v.instructions.split("\n")[0] || "(no instructions)"}
                      </span>
                      {v.version !== versions.data?.current_version ? (
                        <Button size="sm" label="Restore" onClick={() => void restore(v.version)} />
                      ) : null}
                    </div>
                  ))}
                </div>
              ) : null}
            </div>
          ) : null}
        </>
      ) : null}
      {error ? <Problem>{error}</Problem> : null}
    </FormDialog>
  );
}
