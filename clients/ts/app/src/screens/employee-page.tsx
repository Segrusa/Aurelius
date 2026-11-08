import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Layout, LayoutContent, LayoutPanel } from "@astryxdesign/core/Layout";
import { MetadataList, MetadataListItem } from "@astryxdesign/core/MetadataList";
import type { EmployeeSummary, TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useAllTasks, useEffectiveSets, useEmployees } from "../data";
import { useAction } from "../keys/provider";
import { usePlatform } from "../platform";
import { initial, Mark, markColor } from "../ui/marks";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { EmployeeDialog } from "./employee-dialog";
import { type AgentMode, canPin, MODE_LABELS, nextMode, position, threadGroup, togglePin } from "./employee-logic";
import { employeeState, pinKey, usePins } from "./employees";
import { Confirm, Eyebrow, failureOf, Hint, Loading, Page, PageHead, shortDay } from "./org-kit";
import { overlays } from "./overlays-store";
import { useFolderChooser } from "./run-tools";
import { taskStateLine } from "./tasks-logic";

const styles = stylex.create({
  taskCard: {
    display: "flex",
    flexDirection: "column",
    gap: 3,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: { default: "var(--color-background-surface)", ":hover": "var(--color-overlay-hover)" },
    paddingInline: 12,
    paddingBlock: 10,
    color: "var(--color-text-primary)",
    textDecoration: "none",
  },
  truncate: { overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis" },
  strong: { fontWeight: 500 },
  small: { fontSize: 12, color: "var(--precision-text3)" },
  crumb: {
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
    textDecoration: "none",
  },
  separator: { color: "var(--precision-text3)" },
  note: { margin: 0, padding: 24, color: "var(--color-text-secondary)" },
  headLink: {
    fontSize: 12,
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
    textDecoration: "none",
  },
  main: { display: "flex", minWidth: 0, flexDirection: "column", gap: 16 },
  identity: { display: "flex", alignItems: "center", gap: 14 },
  identityText: { minWidth: 0 },
  name: { margin: 0, fontSize: 20, fontWeight: 600, letterSpacing: "-0.01em" },
  position: { margin: 0, color: "var(--color-text-secondary)" },
  status: { display: "inline-flex", alignItems: "center", gap: 6, fontSize: 12 },
  dot: { width: 6, height: 6, borderRadius: "50%" },
  on: { color: "var(--color-text-accent)" },
  section: { display: "flex", flexDirection: "column", gap: 8 },
  instructions: {
    display: "-webkit-box",
    margin: 0,
    overflow: "hidden",
    lineHeight: 1.6,
    whiteSpace: "pre-wrap",
    color: "var(--color-text-secondary)",
    WebkitBoxOrient: "vertical",
    WebkitLineClamp: 12,
  },
  noTasks: { display: "flex", flexDirection: "column", gap: 4, color: "var(--color-text-secondary)" },
  noTasksTitle: { margin: 0, fontWeight: 500 },
  link: { color: "var(--color-text-accent)", textDecoration: "none" },
  panel: { display: "flex", minWidth: 0, flexDirection: "column", gap: 12 },
  panelTitle: { display: "flex", margin: 0, fontWeight: 500 },
  counts: { marginInlineStart: "auto", fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
  guidelines: {
    display: "flex",
    flexDirection: "column",
    gap: 4,
    marginBlockStart: 8,
    borderBlockStartWidth: 1,
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--precision-line2)",
    paddingBlockStart: 12,
  },
  guidelineLink: {
    overflow: "hidden",
    fontSize: 12,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
    textDecoration: "none",
  },
  manage: { fontSize: 12, color: "var(--color-text-accent)", textDecoration: "none" },
  sunk: { backgroundColor: "var(--precision-sunk)" },
});

type Ask = "browser" | "screen" | "archive" | null;

const isOngoing = (t: TaskView) => !["succeeded", "failed", "cancelled"].includes(t.status) || Boolean(t.live_run);

function TaskCard({ task }: { task: TaskView }) {
  const line = taskStateLine(task);
  return (
    <Link to="/tasks/$taskId" params={{ taskId: task.id }} {...stylex.props(styles.taskCard)}>
      <span {...stylex.props(styles.truncate, styles.strong)}>{task.title}</span>
      <span {...stylex.props(styles.truncate, styles.small)}>
        {task.ticket_number ? `#${task.ticket_number} · ` : ""}
        <span style={{ color: line.color }}>{line.text}</span> · {shortDay(task.updated_at_ms)}
      </span>
    </Link>
  );
}

function EmployeeKeys({
  onBrowser,
  onScreen,
  onMode,
  onFolder,
  onPick,
}: {
  onBrowser: () => void;
  onScreen: () => void;
  onMode: () => void;
  onFolder: () => void;
  onPick: (key: string) => void;
}) {
  useAction("TogglePause", () => onPick("pause"));
  useAction("ArchiveItem", () => onPick("archive"));
  useAction("OpenProjectFolder", () => onPick("open"));
  useAction("ToggleBrowser", onBrowser);
  useAction("ToggleScreen", onScreen);
  useAction("CycleAgentMode", onMode);
  useAction("ChangeProjectFolder", onFolder);
  return null;
}

export function EmployeePage({ id }: { id: string }) {
  const connection = useConnection();
  const client = useQueryClient();
  const navigate = useNavigate();
  const platform = usePlatform();
  const folders = useFolderChooser();
  const { hostName } = useLinkState();
  const employees = useEmployees();
  const tasks = useAllTasks();
  const employee = employees.data?.find((e) => e.id === id && e.status !== "archived") ?? null;
  const [editing, setEditing] = useState(false);
  const [ask, setAsk] = useState<Ask>(null);
  const [showEarlier, setShowEarlier] = useState(false);
  const [pins, setPins] = usePins(pinKey(hostName, "pinned_employees"));
  const pairs = useMemo((): [string, null][] => [[id, null]], [id]);
  const sets = useEffectiveSets(pairs);
  const set = sets.data?.get(`${id}|`);
  const mine = useMemo(
    () =>
      (tasks.data ?? [])
        .filter((t) => t.employee_id === id && t.conversation_id)
        .sort((a, b) => Number(isOngoing(b)) - Number(isOngoing(a)) || b.updated_at_ms - a.updated_at_ms),
    [tasks.data, id],
  );
  const now = Date.now();
  const byGroup = (g: string) => mine.filter((t) => threadGroup(isOngoing(t), false, t.updated_at_ms, now) === g);
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
  useAction("EditItem", () => setEditing(true), employee !== null);
  useAction("Back", () => void navigate({ to: "/employees" }));

  if (employees.data && !employee) {
    return (
      <Page>
        <PageHead
          title={
            <Link to="/employees" {...stylex.props(styles.crumb)}>
              Employees
            </Link>
          }
        />
        <p {...stylex.props(styles.note)}>This employee no longer exists (archived?). Esc goes back.</p>
      </Page>
    );
  }
  if (!employee) return <Loading />;
  const e: EmployeeSummary = employee;
  const state = employeeState(e);
  const boss = employees.data?.find((b) => b.id === e.reports_to);
  const pinned = pins.includes(e.id);
  const setMode = () => {
    if (!e.conversation_id) return;
    const mode: AgentMode = nextMode(e.agent_mode);
    void act(() =>
      connection.write(
        "conversation.set_agent_mode",
        { conversation_id: e.conversation_id as string, mode },
        { retry: false },
      ),
    );
  };
  const setAllowed = (
    method: "conversation.set_browser_allowed" | "conversation.set_screen_allowed",
    allowed: boolean,
  ) =>
    e.conversation_id
      ? act(() => connection.write(method, { conversation_id: e.conversation_id as string, allowed }, { retry: false }))
      : Promise.resolve();
  const browser = () =>
    e.browser_allowed ? void setAllowed("conversation.set_browser_allowed", false) : setAsk("browser");
  const screen = () =>
    e.screen_allowed ? void setAllowed("conversation.set_screen_allowed", false) : setAsk("screen");
  const menu = [
    { key: "edit", label: "Edit employee…" },
    { key: "pin", label: pinned ? "Unpin from the list" : "Pin to the list" },
    { key: "mode", label: "Switch mode" },
    { key: "browser", label: e.browser_allowed ? "Browser off" : "Browser on" },
    { key: "screen", label: e.screen_allowed ? "Screen off" : "Screen on" },
    { key: "folder", label: "Choose workspace folder…" },
    ...(platform.openFolder && e.folder ? [{ key: "open", label: "Open workspace folder" }] : []),
    { key: "pause", label: e.status === "paused" ? "Resume employee" : "Pause employee" },
    { key: "archive", label: "Archive employee…", danger: true },
  ];
  const pick = (key: string) => {
    if (key === "edit") setEditing(true);
    else if (key === "pin") {
      if (!canPin(pins, e.id)) toast("Only 5 employees can be pinned. Unpin one first.");
      else setPins(togglePin(pins, e.id));
    } else if (key === "mode") setMode();
    else if (key === "browser") browser();
    else if (key === "screen") screen();
    else if (key === "folder")
      folders.choose(`Choose a folder for ${e.name}`, (path) => {
        void act(() => connection.write("employee.set_folder", { employee_id: e.id, path }, { retry: false }));
      });
    else if (key === "open" && e.folder) void platform.openFolder?.(e.folder);
    else if (key === "pause") {
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
    } else if (key === "archive") setAsk("archive");
  };
  const give = () => overlays.open("give", { title: "", description: "", employeeId: e.id });
  const past = byGroup("past");
  const earlier = byGroup("earlier");
  const active = byGroup("active");
  return (
    <Page>
      <Layout
        header={
          <PageHead
            title={
              <>
                <Link to="/employees" {...stylex.props(styles.crumb)}>
                  Employees
                </Link>
                <span {...stylex.props(styles.separator)}>›</span>
                <span {...stylex.props(styles.truncate)}>{e.name}</span>
              </>
            }
          >
            <Link to="/chat" {...stylex.props(styles.headLink)}>
              Talk in Team chat
            </Link>
            <Button size="sm" label="Edit" onClick={() => setEditing(true)} />
            <Menu
              label={`More for ${e.name}`}
              options={menu}
              onPick={pick}
              trigger={<IconButton size="sm" label={`More for ${e.name}`} icon="⋯" />}
            />
            <Button size="sm" variant="primary" label="Give work" onClick={give} />
          </PageHead>
        }
        content={
          <LayoutContent padding={8}>
            <div {...stylex.props(styles.main)}>
              <div {...stylex.props(styles.identity)}>
                <Mark letter={initial(e.name)} color={markColor(e.id)} size={44} radius={11} />
                <div {...stylex.props(styles.identityText)}>
                  <Heading level={1} maxLines={1} xstyle={styles.name}>
                    {e.name}
                  </Heading>
                  <p {...stylex.props(styles.position, styles.truncate)}>
                    {position(e)}
                    {boss ? ` · reports to ${boss.name}` : ""}
                  </p>
                </div>
              </div>
              <MetadataList orientation="horizontal" label={{ position: "start" }}>
                <MetadataListItem label="Status">
                  <span {...stylex.props(styles.status)} style={{ color: state.color }}>
                    <span {...stylex.props(styles.dot)} style={{ backgroundColor: state.dot }} />
                    {state.label}
                  </span>
                </MetadataListItem>
                <MetadataListItem label="Mode">
                  <Button
                    size="sm"
                    variant="ghost"
                    label={`Mode: ${MODE_LABELS[e.agent_mode]}`}
                    tooltip="Its mode for new work"
                    onClick={setMode}
                  >
                    {MODE_LABELS[e.agent_mode]}
                  </Button>
                </MetadataListItem>
                <MetadataListItem label="Browser">
                  <Button
                    size="sm"
                    variant="ghost"
                    label={`Browser ${e.browser_allowed ? "on" : "off"}`}
                    onClick={browser}
                    xstyle={e.browser_allowed ? styles.on : undefined}
                  >
                    {e.browser_allowed ? "On" : "Off"}
                  </Button>
                </MetadataListItem>
                <MetadataListItem label="Screen">
                  <Button
                    size="sm"
                    variant="ghost"
                    label={`Screen: ${e.screen_allowed ? "allowed" : "asks"}`}
                    onClick={screen}
                    xstyle={e.screen_allowed ? styles.on : undefined}
                  >
                    {e.screen_allowed ? "Allowed" : "Asks"}
                  </Button>
                </MetadataListItem>
              </MetadataList>
              {e.instructions ? (
                <section {...stylex.props(styles.section)}>
                  <Eyebrow>INSTRUCTIONS</Eyebrow>
                  <p {...stylex.props(styles.instructions)}>{e.instructions}</p>
                </section>
              ) : null}
              {mine.length === 0 ? (
                <div {...stylex.props(styles.noTasks)}>
                  <p {...stylex.props(styles.noTasksTitle)}>{e.name} has no tasks yet.</p>
                  <Hint>
                    Tasks you give {e.name} show on the right, each with its conversation. Talk with {e.name} in the
                    Team chat.
                  </Hint>
                </div>
              ) : null}
              <Link to="/chat" {...stylex.props(styles.link)}>
                Talk with {e.name} in Team chat →
              </Link>
            </div>
          </LayoutContent>
        }
        end={
          <LayoutPanel
            hasDivider
            role="complementary"
            label="Tasks"
            width={380}
            padding={4}
            {...stylex.props(styles.sunk)}
          >
            <div {...stylex.props(styles.panel)}>
              <p {...stylex.props(styles.panelTitle)}>
                Tasks{" "}
                <span {...stylex.props(styles.counts)}>
                  {active.length} active · {mine.length}
                </span>
              </p>
              {tasks.data && mine.length === 0 ? (
                <Hint>Tasks you give this employee show here, from every project.</Hint>
              ) : null}
              {tasks.data && mine.length > 0 && active.length === 0 ? <Hint>Nothing active</Hint> : null}
              {active.map((t) => (
                <TaskCard key={t.id} task={t} />
              ))}
              {past.length > 0 ? <Eyebrow>Past</Eyebrow> : null}
              {past.map((t) => (
                <TaskCard key={t.id} task={t} />
              ))}
              {earlier.length > 0 ? (
                <div>
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-expanded={showEarlier}
                    label={`${showEarlier ? "▾" : "▸"} Earlier · ${earlier.length}`}
                    onClick={() => setShowEarlier(!showEarlier)}
                  />
                </div>
              ) : null}
              {showEarlier ? earlier.map((t) => <TaskCard key={t.id} task={t} />) : null}
              <div {...stylex.props(styles.guidelines)}>
                <p title="What its runs get on top of its instructions" {...stylex.props(styles.noTasksTitle)}>
                  Guidelines{set ? ` · ${set.guidelines.length} · ${set.total_chars} / ${set.limit_chars}` : ""}
                </p>
                {set?.guidelines.map((g) => (
                  <Link
                    key={g.id}
                    to="/knowledge/guidelines/$id"
                    params={{ id: g.id }}
                    {...stylex.props(styles.guidelineLink)}
                  >
                    {g.title}
                  </Link>
                ))}
                <Link to="/knowledge/$tab" params={{ tab: "guidelines" }} {...stylex.props(styles.manage)}>
                  Manage links →
                </Link>
              </div>
            </div>
          </LayoutPanel>
        }
      />
      {folders.dialog}
      <EmployeeKeys
        onPick={pick}
        onBrowser={browser}
        onScreen={screen}
        onMode={setMode}
        onFolder={() => pick("folder")}
      />
      {editing ? <EmployeeDialog employee={e} onClose={() => setEditing(false)} /> : null}
      {ask === "browser" ? (
        <Confirm
          title="Allow the browser?"
          body="This agent can then use your own Chrome, which is signed in to your accounts. Every browser action is still shown to you first, also in Auto mode. Task workers never get the browser."
          action="Allow browser"
          onClose={() => setAsk(null)}
          onConfirm={() => {
            setAsk(null);
            void setAllowed("conversation.set_browser_allowed", true);
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
            void setAllowed("conversation.set_screen_allowed", true);
          }}
        />
      ) : null}
      {ask === "archive" ? (
        <Confirm
          danger
          title={`Archive ${e.name}?`}
          body="The employee stops starting work and disappears from the lists. Conversations and history are kept."
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
    </Page>
  );
}
