import { Button } from "@astryxdesign/core/Button";
import { Selector } from "@astryxdesign/core/Selector";
import { TextArea } from "@astryxdesign/core/TextArea";
import { expectBody } from "@aurelius/client";
import type { AccessLevel, EmployeeSummary, TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useAllTasks, useEmployees, useKeyed } from "../data";
import { useAction } from "../keys/provider";
import { usePlatform } from "../platform";
import { initial, Mark, markColor } from "../ui/marks";
import { toast } from "../ui/toast";
import { EmployeeGuidelinesTab } from "./employee-guidelines";
import { EmployeeInboxTab, EmployeeMemoryTab } from "./employee-inbox";
import {
  ACCESS_LEVELS,
  accessOf,
  asChoice,
  canPin,
  canReportTo,
  choiceLine,
  type EmployeeTab,
  employeeState,
  employeeTabs,
  inheritedRun,
  position,
  togglePin,
} from "./employee-logic";
import { pinKey, usePins } from "./employees";
import { Confirm, FormDialog, failureOf, Loading, shortDay } from "./org-kit";
import { overlays } from "./overlays-store";
import { useFolderChooser } from "./run-tools";
import { PageTabs, SaveBar, Segments, SettingsPage, Status, type StatusTone } from "./settings-frame";
import { Chip, Foot, Group, Row, SmallButton, Switch, TextInput, useSetting } from "./settings-kit";
import { taskStateLine } from "./tasks-logic";

const styles = stylex.create({
  hero: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 14 },
  heroText: { minWidth: 0, flex: 1 },
  heroName: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: 10,
    margin: 0,
    fontSize: 19,
    fontWeight: 650,
  },
  sub: { color: "var(--precision-text3)" },
  acts: { display: "flex", flexWrap: "wrap", justifyContent: "flex-end", gap: 8 },
  panel: { display: "flex", flexDirection: "column", gap: 26 },
  task: {
    display: "flex",
    alignItems: "center",
    gap: 12,
    paddingBlock: 10,
    paddingInline: 16,
    color: "var(--color-text-primary)",
    textDecoration: "none",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    borderBlockStartWidth: { default: 1, ":first-child": 0 },
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--precision-line2)",
  },
  taskText: { display: "flex", minWidth: 0, flex: 1, flexDirection: "column" },
  taskTitle: { overflow: "hidden", fontWeight: 500, textOverflow: "ellipsis", whiteSpace: "nowrap" },
  taskMeta: { fontSize: 12, color: "var(--precision-text3)" },
  ticket: { fontFamily: "var(--font-family-code)", fontSize: 11.5 },
  card: {
    overflow: "hidden",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 12,
    backgroundColor: "var(--color-background-card)",
    boxShadow: "var(--precision-card-shadow)",
    listStyleType: "none",
    margin: 0,
    padding: 0,
  },
  sec: { display: "flex", flexDirection: "column", gap: 8 },
  secHead: { display: "flex", minHeight: 28, alignItems: "center", gap: 8, paddingInline: 2 },
  secTitle: { margin: 0, fontSize: 12.5, fontWeight: 600, color: "var(--color-text-secondary)" },
  secNote: { fontSize: 12, color: "var(--precision-text3)" },
  grow: { flex: 1 },
  two: { display: "grid", gridTemplateColumns: { default: "1fr 1fr", "@media (max-width: 720px)": "1fr" } },
  field: {
    display: "flex",
    flexDirection: "column",
    gap: 6,
    paddingBlock: 12,
    paddingInline: 16,
    fontWeight: 500,
    borderBlockStartWidth: { default: 1, ":nth-child(-n+2)": 0 },
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--precision-line2)",
    borderInlineStartWidth: { default: 0, ":nth-child(even)": 1 },
    borderInlineStartStyle: "solid",
    borderInlineStartColor: "var(--precision-line2)",
  },
  wide: { display: "flex", flexDirection: "column", gap: 6, paddingBlock: 12, paddingInline: 16 },
  hint: { fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
  mono: { fontFamily: "var(--font-family-code)", fontSize: 12 },
  empty: { margin: 0, paddingBlock: 14, paddingInline: 16, color: "var(--precision-text3)" },
  version: { display: "flex", alignItems: "center", gap: 12, paddingBlock: 10 },
  versionText: { minWidth: 0, flex: 1, display: "flex", flexDirection: "column" },
});

type Ask = "browser" | "screen" | "archive" | null;

const isOngoing = (t: TaskView) => !["succeeded", "failed", "cancelled"].includes(t.status) || Boolean(t.live_run);

/** A task as one row: its state dot, title, ticket and where it stands, and a chip at the right. */
function TaskRow({ task }: { task: TaskView }) {
  const line = taskStateLine(task);
  const tone: StatusTone = task.live_run
    ? "work"
    : task.status === "waiting"
      ? "you"
      : isOngoing(task)
        ? "wait"
        : "plain";
  const chip = task.live_run ? (
    <Chip>Running</Chip>
  ) : task.status === "succeeded" ? (
    <Chip tone="ok">Done</Chip>
  ) : task.status === "failed" ? (
    <Chip tone="bad">Failed</Chip>
  ) : null;
  return (
    <li>
      <Link to="/tasks/$taskId" params={{ taskId: task.id }} {...stylex.props(styles.task)}>
        <Status tone={tone} label={line.text} dotOnly />
        <span {...stylex.props(styles.taskText)}>
          <span {...stylex.props(styles.taskTitle)}>{task.title}</span>
          <span {...stylex.props(styles.taskMeta)}>
            {task.ticket_number ? <span {...stylex.props(styles.ticket)}>#{task.ticket_number} · </span> : null}
            {line.text} · {shortDay(task.updated_at_ms)}
          </span>
        </span>
        {chip}
      </Link>
    </li>
  );
}

function Section({
  title,
  note,
  action,
  children,
}: {
  title: string;
  note?: string;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section {...stylex.props(styles.sec)}>
      <div {...stylex.props(styles.secHead)}>
        <h2 {...stylex.props(styles.secTitle)}>{title}</h2>
        {note ? <span {...stylex.props(styles.secNote)}>{note}</span> : null}
        <span {...stylex.props(styles.grow)} />
        {action}
      </div>
      {children}
    </section>
  );
}

const TASKS_SHOWN = 8;

function WorkTab({ e, queue, logs }: { e: EmployeeSummary; queue: boolean; logs: boolean }) {
  const tasks = useAllTasks();
  const [all, setAll] = useState(false);
  const mine = useMemo(
    () =>
      (tasks.data ?? [])
        .filter((t) => t.employee_id === e.id && t.conversation_id)
        .sort((a, b) => Number(isOngoing(b)) - Number(isOngoing(a)) || b.updated_at_ms - a.updated_at_ms),
    [tasks.data, e.id],
  );
  const active = mine.filter(isOngoing).length;
  const shown = all ? mine : mine.slice(0, TASKS_SHOWN);
  return (
    <div {...stylex.props(styles.panel)}>
      {queue ? (
        <Section title={`Waiting for ${e.name}`}>
          <EmployeeInboxTab employeeId={e.id} name={e.name} logs={logs} />
        </Section>
      ) : null}
      <Section title="Tasks" note={`${active} active · ${mine.length}`}>
        <ul {...stylex.props(styles.card)}>
          {tasks.data && mine.length === 0 ? (
            <li {...stylex.props(styles.empty)}>Tasks you give {e.name} show here, from every workspace.</li>
          ) : null}
          {shown.map((t) => (
            <TaskRow key={t.id} task={t} />
          ))}
          {mine.length > TASKS_SHOWN && !all ? (
            <Foot>
              <SmallButton kind="ghost" onClick={() => setAll(true)}>{`Show all ${mine.length} tasks`}</SmallButton>
            </Foot>
          ) : null}
        </ul>
      </Section>
    </div>
  );
}

const NOBODY = "nobody";

/** Profile: name, role, title, boss, description and instructions, edited in place with a save bar. */
function ProfileTab({ e, all }: { e: EmployeeSummary; all: EmployeeSummary[] }) {
  const connection = useConnection();
  const client = useQueryClient();
  const { features } = useLinkState();
  const saved = {
    name: e.name,
    role: e.role,
    title: e.title ?? "",
    boss: e.reports_to ?? "",
    description: e.description ?? "",
    instructions: e.instructions ?? "",
  };
  const [draft, setDraft] = useState(saved);
  const [busy, setBusy] = useState(false);
  const [versions, setVersions] = useState(false);
  const dirty = (Object.keys(saved) as (keyof typeof saved)[]).some((k) => draft[k] !== saved[k]);
  const set = (key: keyof typeof saved) => (value: string) => setDraft({ ...draft, [key]: value });
  const bosses = all.filter((b) => (b.status !== "archived" && canReportTo(all, e.id, b.id)) || b.id === e.reports_to);
  const save = async () => {
    if (draft.name.trim() === "" || draft.role.trim() === "") return toast("A name and a role are needed.");
    setBusy(true);
    try {
      await connection.write(
        "employee.update",
        {
          employee_id: e.id,
          name: draft.name.trim(),
          role: draft.role.trim(),
          title: draft.title.trim(),
          description: draft.description.trim(),
          instructions: draft.instructions,
        },
        { retry: false, expectedVersion: e.version },
      );
      if (draft.boss !== saved.boss) {
        await connection.write(
          "employee.set_reports_to",
          { employee_id: e.id, ...(draft.boss ? { reports_to: draft.boss } : {}) },
          { retry: false },
        );
      }
      await client.invalidateQueries({ queryKey: ["employees"] });
      toast("Saved.");
    } catch (error) {
      toast(failureOf(error));
    } finally {
      setBusy(false);
    }
  };
  const hasVersions = features.includes("instruction_proposals");
  return (
    <div {...stylex.props(styles.panel)}>
      <Section title="Who">
        <div {...stylex.props(styles.card, styles.two)}>
          <div {...stylex.props(styles.field)}>
            <TextInput label="Name" showLabel value={draft.name} onChange={(ev) => set("name")(ev.target.value)} />
          </div>
          <div {...stylex.props(styles.field)}>
            <TextInput label="Role" showLabel value={draft.role} onChange={(ev) => set("role")(ev.target.value)} />
          </div>
          <div {...stylex.props(styles.field)}>
            <TextInput
              label="Title on the org chart"
              showLabel
              placeholder="Optional, e.g. Head of design"
              value={draft.title}
              onChange={(ev) => set("title")(ev.target.value)}
            />
          </div>
          <div {...stylex.props(styles.field)}>
            <Selector
              label="Reports to"
              size="sm"
              variant="secondary"
              value={draft.boss || NOBODY}
              options={[
                ...bosses.map((b) => ({ value: b.id, label: `${b.name} · ${b.role}` })),
                { value: NOBODY, label: "Nobody (top of the org chart)" },
              ]}
              onChange={(value) => set("boss")(value === NOBODY ? "" : value)}
            />
          </div>
        </div>
      </Section>
      <Section title="Description" note={`Other employees read this to decide when to hand work to ${e.name}`}>
        <div {...stylex.props(styles.card, styles.wide)}>
          <TextArea
            label="Description"
            isLabelHidden
            rows={3}
            value={draft.description}
            onChange={set("description")}
          />
        </div>
      </Section>
      <Section
        title="Instructions"
        note={`${draft.instructions.length.toLocaleString()} characters`}
        action={
          hasVersions ? (
            <SmallButton kind="ghost" onClick={() => setVersions(true)}>
              Earlier versions
            </SmallButton>
          ) : undefined
        }
      >
        <div {...stylex.props(styles.card, styles.wide)}>
          <TextArea
            label="Instructions"
            isLabelHidden
            rows={11}
            value={draft.instructions}
            onChange={set("instructions")}
          />
          <span {...stylex.props(styles.hint)}>
            Changes apply to new runs. A run that is going keeps what it started with.
          </span>
        </div>
      </Section>
      {dirty ? (
        <SaveBar
          actions={
            <>
              <SmallButton kind="ghost" disabled={busy} onClick={() => setDraft(saved)}>
                Discard
              </SmallButton>
              <SmallButton kind="primary" disabled={busy} onClick={() => void save()}>
                Save
              </SmallButton>
            </>
          }
        >
          {`You changed the profile of ${e.name}.`}
        </SaveBar>
      ) : null}
      {versions ? <VersionsDialog e={e} onClose={() => setVersions(false)} /> : null}
    </div>
  );
}

function VersionsDialog({ e, onClose }: { e: EmployeeSummary; onClose: () => void }) {
  const connection = useConnection();
  const client = useQueryClient();
  const versions = useKeyed(
    ["employees", "versions", e.id],
    async () =>
      expectBody(
        await connection.call("employee.instruction_versions", { employee_id: e.id }),
        "employee_instruction_versions",
      ).value,
  );
  const restore = async (version: number) => {
    try {
      await connection.write("employee.restore_instructions", { employee_id: e.id, version }, { retry: false });
      await client.invalidateQueries({ queryKey: ["employees"] });
      toast(`Version ${version} is back, as a new version.`);
      onClose();
    } catch (error) {
      toast(failureOf(error));
    }
  };
  const current = versions.data?.current_version;
  return (
    <FormDialog
      title={`Earlier instructions of ${e.name}`}
      width={560}
      onClose={onClose}
      footer={<Button variant="primary" label="Done" onClick={onClose} />}
    >
      {versions.isPending ? <Loading rows={3} /> : null}
      <ul {...stylex.props(styles.card)}>
        {(versions.data?.versions ?? []).map((v) => (
          <Row
            key={v.version}
            title={
              <>
                Version {v.version}
                {v.version === current ? <Chip tone="ok">Current</Chip> : null}
              </>
            }
            hint={`${
              v.restored_from != null
                ? `Restored from version ${v.restored_from}`
                : v.proposal_id
                  ? "Approved proposal"
                  : "Edited"
            } · ${shortDay(v.created_at_ms)}`}
          >
            {v.version === current ? null : <SmallButton onClick={() => void restore(v.version)}>Restore</SmallButton>}
          </Row>
        ))}
      </ul>
    </FormDialog>
  );
}

/** Access: what the employee may use, where its chats work, and pausing or archiving it. */
function AccessTab({
  e,
  onAsk,
  onPause,
  setAllowed,
}: {
  e: EmployeeSummary;
  onAsk: (ask: Ask) => void;
  onPause: () => void;
  setAllowed: (method: "conversation.set_browser_allowed" | "conversation.set_screen_allowed", on: boolean) => void;
}) {
  const connection = useConnection();
  const client = useQueryClient();
  const platform = usePlatform();
  const folders = useFolderChooser();
  const { features } = useLinkState();
  const connections = useSetting(
    "connections",
    async () => expectBody(await connection.call("connection.list"), "connections").value,
  );
  const own =
    e.settings ??
    connections.data?.defaults?.find(
      (d) => d.purpose === "employee" && d.scope.kind === "employee" && d.scope.id === e.id,
    )?.settings ??
    {};
  const inherited = inheritedRun(connections.data);
  const provider = asChoice(own.connection);
  const refresh = () => client.invalidateQueries({ queryKey: ["employees"] });
  const act = (run: () => Promise<unknown>) =>
    run()
      .then(refresh)
      .catch((error: unknown) => toast(failureOf(error)));
  const setAccess = (access_level: AccessLevel) =>
    act(() =>
      connection.write(
        "employee.update",
        {
          employee_id: e.id,
          name: e.name,
          role: e.role,
          title: e.title ?? "",
          description: e.description ?? "",
          instructions: e.instructions ?? "",
          access_level,
        },
        { retry: false, expectedVersion: e.version },
      ),
    );
  const setProvider = (value: string) =>
    act(() =>
      connection.write(
        "connection.set_defaults",
        {
          scope: { kind: "employee", id: e.id },
          purpose: "employee",
          settings: { ...own, connection: value === "inherit" ? { mode: "inherit" } : { mode: "value", value } },
        },
        { retry: false },
      ),
    ).then(() => client.invalidateQueries({ queryKey: ["settings"] }));
  return (
    <div {...stylex.props(styles.panel)}>
      <Group title={`How ${e.name} runs`}>
        {features.includes("employee_access") ? (
          <Row title="Files" hint={ACCESS_LEVELS.find((l) => l.value === accessOf(e))?.hint}>
            <Segments
              label="Files"
              options={ACCESS_LEVELS.map((l) => ({ value: l.value, label: l.label }))}
              value={accessOf(e)}
              onChange={(value) => void setAccess(value)}
            />
          </Row>
        ) : null}
        <Row title="Browser">
          <Switch
            label="Browser"
            on={e.browser_allowed}
            onChange={(on) => (on ? onAsk("browser") : setAllowed("conversation.set_browser_allowed", false))}
          />
        </Row>
        <Row title="See your screen">
          {e.screen_allowed ? null : <Chip>Asks each time</Chip>}
          <Switch
            label="See your screen"
            on={e.screen_allowed}
            onChange={(on) => (on ? onAsk("screen") : setAllowed("conversation.set_screen_allowed", false))}
          />
        </Row>
        <Row title="Provider">
          <Selector
            label="Provider"
            isLabelHidden
            size="sm"
            variant="secondary"
            width={220}
            value={provider}
            options={[
              { value: "inherit", label: choiceLine("inherit", inherited.provider) },
              ...(connections.data?.connections ?? []).map((c) => ({ value: c.id, label: c.display_name })),
            ]}
            onChange={(value) => void setProvider(value)}
          />
        </Row>
        <Row title="Model, effort and mode" hint="Each board member sets these once for every employee.">
          <Link to="/settings/$section" params={{ section: "defaults" }}>
            My defaults ›
          </Link>
        </Row>
      </Group>
      <Group title="Own folder">
        <Row
          title={e.folder ? <span {...stylex.props(styles.mono)}>{e.folder}</span> : "No folder of its own"}
          hint="For chats. In a task the workspace folder applies."
        >
          {platform.openFolder && e.folder ? (
            <SmallButton kind="ghost" onClick={() => void platform.openFolder?.(e.folder as string)}>
              Open
            </SmallButton>
          ) : null}
          <SmallButton
            onClick={() =>
              folders.choose(`Choose a folder for ${e.name}`, (path) => {
                void act(() => connection.write("employee.set_folder", { employee_id: e.id, path }, { retry: false }));
              })
            }
          >
            Choose…
          </SmallButton>
        </Row>
      </Group>
      <Group title="Stop or remove">
        <Row
          title={e.status === "paused" ? `Resume ${e.name}` : `Pause ${e.name}`}
          hint={e.status === "paused" ? "Takes up new work again." : "Starts no new work. Waiting items stay."}
        >
          <SmallButton onClick={onPause}>{e.status === "paused" ? "Resume" : "Pause"}</SmallButton>
        </Row>
        <Row title={`Archive ${e.name}`} hint="Leaves the lists. History and conversations are kept.">
          <SmallButton kind="danger" onClick={() => onAsk("archive")}>
            Archive…
          </SmallButton>
        </Row>
      </Group>
      {folders.dialog}
    </div>
  );
}

/** Settings › Employees › one employee: who it is, what it does, and what it may use. */
export function EmployeePage({ id, tab: wanted }: { id: string; tab?: EmployeeTab }) {
  const connection = useConnection();
  const client = useQueryClient();
  const navigate = useNavigate();
  const { hostName, features } = useLinkState();
  const employees = useEmployees();
  const employee = employees.data?.find((e) => e.id === id && e.status !== "archived") ?? null;
  const [ask, setAsk] = useState<Ask>(null);
  const [pins, setPins] = usePins(pinKey(hostName, "pinned_employees"));
  const queueOn = features.includes("employee_queue");
  const queue = useKeyed(
    ["employees", "queue", id],
    async () => expectBody(await connection.call("employee.queue", { employee_id: id }), "employee_queue").value,
    queueOn,
  );
  const memories = useKeyed(
    ["memory", "employee", id],
    async () => expectBody(await connection.call("memory.list", { employee_id: id }), "memory").value.items,
  );
  const tabs = employeeTabs(features.includes("guidelines"));
  const tab = tabs.find((t) => t.key === wanted)?.key ?? "work";
  const pickTab = (key: EmployeeTab) =>
    void navigate({ to: "/employees/$id", params: { id }, search: { tab: key }, replace: true });
  const refresh = () => client.invalidateQueries({ queryKey: ["employees"] });
  const act = async (run: () => Promise<unknown>, done?: string) => {
    try {
      await run();
      await refresh();
      if (done) toast(done);
    } catch (e) {
      toast(failureOf(e));
    }
  };
  useAction("EditItem", () => pickTab("profile"), employee !== null);
  useAction("Back", () => void navigate({ to: "/employees" }));
  const crumbs = [{ label: "Employees", to: "/employees" }];

  if (employees.data && !employee) {
    return (
      <SettingsPage title="Not found" crumbs={crumbs} width="mid">
        <p {...stylex.props(styles.empty)}>This employee no longer exists (archived?). Esc goes back.</p>
      </SettingsPage>
    );
  }
  if (!employee) return <Loading />;
  const e: EmployeeSummary = employee;
  const state = employeeState(e);
  const tone: StatusTone =
    state.label === "Working"
      ? "work"
      : state.label === "Waiting for you"
        ? "you"
        : state.label === "Paused"
          ? "paused"
          : state.label === "Queued"
            ? "wait"
            : "plain";
  const boss = employees.data?.find((b) => b.id === e.reports_to);
  const pinned = pins.includes(e.id);
  const setAllowed = (
    method: "conversation.set_browser_allowed" | "conversation.set_screen_allowed",
    allowed: boolean,
  ) => {
    if (e.conversation_id)
      void act(() =>
        connection.write(method, { conversation_id: e.conversation_id as string, allowed }, { retry: false }),
      );
  };
  const pause = () => {
    const paused = e.status === "paused";
    void act(
      () =>
        connection.write(
          "employee.set_status",
          { employee_id: e.id, status: paused ? "active" : "paused" },
          { retry: false },
        ),
      paused ? "Employee resumed." : "Employee paused: no new work starts.",
    );
  };
  const waiting = queue.data?.items.length ?? 0;
  const inUse = (memories.data ?? []).filter((m) => m.status === "active").length;
  return (
    <SettingsPage title={e.name} crumbs={crumbs} width="mid">
      <div {...stylex.props(styles.hero)}>
        <Mark letter={initial(e.name)} color={markColor(e.id)} size={40} radius={10} />
        <div {...stylex.props(styles.heroText)}>
          <h2 {...stylex.props(styles.heroName)}>
            {e.name} <Status tone={tone} label={state.label} />
          </h2>
          <div {...stylex.props(styles.sub)}>
            {position(e)}
            {boss ? ` · reports to ${boss.name}` : ""}
          </div>
        </div>
        <div {...stylex.props(styles.acts)}>
          <Button
            size="sm"
            label={pinned ? "Pinned" : "Pin"}
            aria-pressed={pinned}
            tooltip={pinned ? "Pinned in the list" : "Pin to the top of the list"}
            onClick={() => {
              if (!canPin(pins, e.id)) toast("Only 5 employees can be pinned. Unpin one first.");
              else setPins(togglePin(pins, e.id));
            }}
          />
          <Button size="sm" label={e.status === "paused" ? "Resume" : "Pause"} onClick={pause} />
          <Button
            size="sm"
            variant="primary"
            label="Give work"
            onClick={() => overlays.open("give", { title: "", description: "", employeeId: e.id })}
          />
        </div>
      </div>
      <PageTabs
        label={`About ${e.name}`}
        tabs={tabs.map((t) => ({
          value: t.key,
          label: t.label,
          ...(t.key === "work" && waiting > 0 ? { count: waiting, warn: true } : {}),
          ...(t.key === "memory" && inUse > 0 ? { count: inUse } : {}),
        }))}
        value={tab}
        onChange={pickTab}
      />
      <div role="tabpanel" aria-label={tabs.find((t) => t.key === tab)?.label}>
        {tab === "work" ? <WorkTab e={e} queue={queueOn} logs={features.includes("activity_log")} /> : null}
        {tab === "profile" ? <ProfileTab key={e.version} e={e} all={employees.data ?? []} /> : null}
        {tab === "memory" ? <EmployeeMemoryTab employeeId={e.id} name={e.name} /> : null}
        {tab === "guidelines" ? <EmployeeGuidelinesTab employeeId={e.id} name={e.name} /> : null}
        {tab === "access" ? <AccessTab e={e} onAsk={setAsk} onPause={pause} setAllowed={setAllowed} /> : null}
      </div>
      {ask === "browser" ? (
        <Confirm
          title="Allow the browser?"
          body="This agent can then use your own Chrome, which is signed in to your accounts. Every browser action is still shown to you first, also in Auto mode. Task workers never get the browser."
          action="Allow browser"
          onClose={() => setAsk(null)}
          onConfirm={() => {
            setAsk(null);
            setAllowed("conversation.set_browser_allowed", true);
          }}
        />
      ) : null}
      {ask === "screen" ? (
        <Confirm
          title="Let this agent see your screen?"
          body="It can then take screenshots of your screen or of one window without asking, whenever it thinks that helps. A screenshot shows anything that is open, also private things. Off, it still asks you for each screenshot. Task workers never get this."
          action="Allow screen"
          onClose={() => setAsk(null)}
          onConfirm={() => {
            setAsk(null);
            setAllowed("conversation.set_screen_allowed", true);
          }}
        />
      ) : null}
      {ask === "archive" ? (
        <Confirm
          danger
          title={`Archive ${e.name}?`}
          body={`${e.name} starts no new work and leaves the lists. Conversations and history are kept.`}
          action="Archive"
          onClose={() => setAsk(null)}
          onConfirm={() => {
            setAsk(null);
            void act(
              () =>
                connection.write("employee.set_status", { employee_id: e.id, status: "archived" }, { retry: false }),
              "Archived. The history is kept.",
            ).then(() => navigate({ to: "/employees" }));
          }}
        />
      ) : null}
    </SettingsPage>
  );
}
