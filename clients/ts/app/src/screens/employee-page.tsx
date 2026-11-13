import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Layout, LayoutContent, LayoutPanel } from "@astryxdesign/core/Layout";
import { MetadataList, MetadataListItem } from "@astryxdesign/core/MetadataList";
import { StatusDot } from "@astryxdesign/core/StatusDot";
import { Tab, TabList } from "@astryxdesign/core/TabList";
import { expectBody } from "@aurelius/client";
import type { EmployeeSummary, TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useAllTasks, useEmployees } from "../data";
import { hidden } from "../hidden";
import { useAction } from "../keys/provider";
import { usePlatform } from "../platform";
import { initial, Mark, markColor } from "../ui/marks";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { useOpenChat } from "./employee-chat";
import { EmployeeDialog } from "./employee-dialog";
import { EmployeeGuidelinesTab } from "./employee-guidelines";
import { EmployeeInboxTab, EmployeeMemoryTab } from "./employee-inbox";
import {
  type AgentMode,
  accessLabel,
  accessOf,
  asChoice,
  canPin,
  choiceLine,
  type EmployeeTab,
  effortName,
  employeeState,
  employeeTabs,
  inheritedRun,
  instructionsFold,
  MODE_LABELS,
  nextMode,
  position,
  threadGroup,
  togglePin,
} from "./employee-logic";
import { policyLine } from "./employee-tools-logic";
import { pinKey, usePins } from "./employees";
import { Confirm, Eyebrow, failureOf, Hint, Loading, Page, PageHead, shortDay } from "./org-kit";
import { overlays } from "./overlays-store";
import { useFolderChooser } from "./run-tools";
import { modelName, useSetting } from "./settings-kit";
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
    // INSTRUCTIONS_PREVIEW_LINES: StyleX needs the literal here.
    WebkitLineClamp: 12,
  },
  // "Show all": the whole text, persona and rules, selectable.
  unfolded: { display: "block", overflow: "visible", userSelect: "text", WebkitLineClamp: "unset" },
  tabPanel: {
    display: "flex",
    minWidth: 0,
    flexDirection: "column",
    gap: 16,
    // A Table bleeds over its container's padding, and here that would be over the tabs.
    "--container-padding-block-start": "0px",
    "--container-padding-block-end": "0px",
    "--container-padding-inline-start": "0px",
    "--container-padding-inline-end": "0px",
  },
  panel: { display: "flex", minWidth: 0, flexDirection: "column", gap: 12 },
  panelTitle: { display: "flex", margin: 0, fontWeight: 500 },
  counts: { marginInlineStart: "auto", fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
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

const PANEL = "employee-panel";

export function EmployeePage({ id, tab: wanted }: { id: string; tab?: EmployeeTab }) {
  const connection = useConnection();
  const client = useQueryClient();
  const navigate = useNavigate();
  const platform = usePlatform();
  const folders = useFolderChooser();
  const { hostName, features } = useLinkState();
  const employees = useEmployees();
  const openChat = useOpenChat();
  const connections = useSetting(
    "connections",
    async () => expectBody(await connection.call("connection.list"), "connections").value,
  );
  const tasks = useAllTasks();
  const employee = employees.data?.find((e) => e.id === id && e.status !== "archived") ?? null;
  const [editing, setEditing] = useState(false);
  const [ask, setAsk] = useState<Ask>(null);
  const [showEarlier, setShowEarlier] = useState(false);
  const [allInstructions, setAllInstructions] = useState(false);
  const [pins, setPins] = usePins(pinKey(hostName, "pinned_employees"));
  const mine = useMemo(
    () =>
      (tasks.data ?? [])
        .filter((t) => t.employee_id === id && t.conversation_id)
        .sort((a, b) => Number(isOngoing(b)) - Number(isOngoing(a)) || b.updated_at_ms - a.updated_at_ms),
    [tasks.data, id],
  );
  const now = Date.now();
  const byGroup = (g: string) => mine.filter((t) => threadGroup(isOngoing(t), false, t.updated_at_ms, now) === g);
  const tabs = employeeTabs(features.includes("employee_queue"), features.includes("guidelines"));
  const tab = tabs.find((t) => t.key === wanted)?.key ?? tabs[0]?.key ?? "settings";
  const pickTab = (key: string) =>
    void navigate({ to: "/employees/$id", params: { id }, search: { tab: key as EmployeeTab }, replace: true });
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
  const own =
    e.settings ??
    connections.data?.defaults?.find(
      (d) => d.purpose === "employee" && d.scope.kind === "employee" && d.scope.id === e.id,
    )?.settings ??
    {};
  const inherited = inheritedRun(connections.data);
  const providerChoice = asChoice(own.connection);
  const providerName =
    connections.data?.connections?.find((c) => c.id === providerChoice)?.display_name ?? "A removed connection";
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
            {features.includes("employee_chats") && e.status === "active" ? (
              <Button size="sm" label="Chat" onClick={() => openChat(e.id)} />
            ) : null}
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
                  <span {...stylex.props(styles.status)} style={{ color: state.color }}>
                    <StatusDot variant={state.variant} isPulsing={state.isPulsing} label={state.label} />
                    {state.label}
                  </span>
                </div>
              </div>
              <TabList role="tablist" aria-label={`About ${e.name}`} hasDivider value={tab} onChange={pickTab}>
                {tabs.map((t) => (
                  <Tab key={t.key} value={t.key} label={t.label} panelId={PANEL} />
                ))}
              </TabList>
              <div id={PANEL} role="tabpanel" {...stylex.props(styles.tabPanel)}>
                {tab === "inbox" ? (
                  <EmployeeInboxTab employeeId={e.id} name={e.name} logs={features.includes("activity_log")} />
                ) : null}
                {tab === "memory" ? <EmployeeMemoryTab employeeId={e.id} name={e.name} /> : null}
                {tab === "guidelines" ? <EmployeeGuidelinesTab employeeId={e.id} name={e.name} /> : null}
                {tab === "settings" ? (
                  <>
                    <MetadataList orientation="horizontal" label={{ position: "start" }}>
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
                    <MetadataList orientation="horizontal" label={{ position: "start" }}>
                      <MetadataListItem label="Provider">
                        {providerChoice === "inherit" ? choiceLine("inherit", inherited.provider) : providerName}
                      </MetadataListItem>
                      <MetadataListItem label="Model">
                        {choiceLine(asChoice(own.model), inherited.model && modelName(inherited.model), modelName)}
                      </MetadataListItem>
                      <MetadataListItem label="Effort">
                        {choiceLine(asChoice(own.effort), inherited.effort && effortName(inherited.effort), effortName)}
                      </MetadataListItem>
                      {features.includes("employee_access") ? (
                        <MetadataListItem label="Access">{accessLabel(accessOf(e))}</MetadataListItem>
                      ) : null}
                      {features.includes("employee_tools") &&
                      !hidden("EmployeeTools") &&
                      policyLine(e.tool_policy ?? {}) ? (
                        <MetadataListItem label="Tools">{policyLine(e.tool_policy ?? {})}</MetadataListItem>
                      ) : null}
                    </MetadataList>
                    {e.description ? (
                      <section {...stylex.props(styles.section)}>
                        <Eyebrow>DESCRIPTION</Eyebrow>
                        <p {...stylex.props(styles.instructions)}>{e.description}</p>
                      </section>
                    ) : null}
                    {e.instructions ? (
                      <section {...stylex.props(styles.section)}>
                        <Eyebrow>INSTRUCTIONS</Eyebrow>
                        <p {...stylex.props(styles.instructions, allInstructions && styles.unfolded)}>
                          {e.instructions}
                        </p>
                        {instructionsFold(e.instructions) ? (
                          <div>
                            <Button
                              size="sm"
                              variant="ghost"
                              aria-expanded={allInstructions}
                              label={allInstructions ? "Show less" : "Show all instructions"}
                              onClick={() => setAllInstructions(!allInstructions)}
                            />
                          </div>
                        ) : null}
                      </section>
                    ) : null}
                  </>
                ) : null}
              </div>
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
