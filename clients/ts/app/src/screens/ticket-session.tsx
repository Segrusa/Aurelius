// A ticket as a session (`/ticket/$number`, ticket #70): one screen with the ticket's conversation, its
// progress, the decision of the moment and the ticket's data. It replaces the ticket's Team chat
// channel and the Board's panel. The server decides everything as before: who answers a message
// (`owner_answers`), and who may accept, send back or approve. This screen only places the existing
// reads and writes of the channel and the board panel around one timeline.

import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { Tab, TabList } from "@astryxdesign/core/TabList";
import { expectBody } from "@aurelius/client";
import type { AgentMode, TicketDetail, TicketPriority, TicketView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import {
  useBoard,
  useChannels,
  useEmployees,
  useKeyed,
  useMembers,
  useMessages,
  useMessagesOf,
  useTicket,
  useTicketActivity,
} from "../data";
import { useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { acceptable, filesFrom } from "../ui/attach";
import { Chip } from "../ui/chip";
import { Markdown } from "../ui/markdown";
import { initial, Mark, markColor } from "../ui/marks";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { useDelayed } from "../ui/use-delayed";
import {
  deadlineInput,
  doneCheck,
  feedbackLabel,
  latestPlan,
  ownerChoosable,
  PRIORITIES,
  priorityLabel,
  runLocked,
} from "./board-logic";
import { type SessionSlots, TicketSessionBody } from "./chat";
import { useLiveAll, useLiveStore } from "./chat-live";
import { readOnlyBar, runAuthor } from "./chat-logic";
import { artifactProblem, DESIGN_FEATURE } from "./design-logic";
import { parseDeadline, parseLabels, useModels } from "./dialogs";
import { isQuick } from "./quick-logic";
import {
  FilesWritten,
  type PropertiesDock,
  PropertiesPanel,
  PropertiesToggle,
  RunGuidelines,
  SideProperties,
  SideRow,
  SideSection,
  usePropertiesDock,
} from "./run-side";
import { FileChips } from "./run-tools";
import { foldTree, taskStateLine, treeOrder } from "./tasks-logic";
import { branchOf } from "./ticket-overview-logic";
import {
  Activity,
  Box,
  DesignRow,
  Editable,
  Followers,
  MODES,
  PlanGate,
  ReviewGate,
  StatusBox,
  time,
  useDispatcherLine,
  useTicketMenu,
} from "./ticket-panel";
import {
  composerHint,
  composerPlaceholder,
  filesWrittenAcross,
  gateOf,
  headLine,
  latestRunAcross,
  readOnly,
  resolveNumber,
  sessionRedirect,
  ticketConversations,
  ticketPlace,
  ticketUsage,
} from "./ticket-session-logic";
import { ColumnGlyph, ticketNumber, useTicketWrite } from "./tickets-kit";

const styles = stylex.create({
  screen: { display: "flex", minWidth: 0, minHeight: 0, flexGrow: 1, flexBasis: 0, flexDirection: "column" },
  body: { display: "flex", minWidth: 0, minHeight: 0, flexGrow: 1, flexBasis: 0 },
  main: { display: "flex", minWidth: 0, flexGrow: 1, flexBasis: 0, flexDirection: "column" },
  top: {
    display: "flex",
    minHeight: 52,
    flexShrink: 0,
    alignItems: "center",
    gap: 10,
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--color-border)",
    paddingBlock: 6,
    paddingInlineStart: 18,
    paddingInlineEnd: 12,
  },
  lines: { display: "flex", minWidth: 0, flexGrow: 1, flexBasis: 0, flexDirection: "column", gap: 2 },
  line1: { display: "flex", minWidth: 0, alignItems: "center", gap: 8 },
  line2: { display: "flex", minWidth: 0, alignItems: "center", gap: 12 },
  title: {
    margin: 0,
    display: "flex",
    minWidth: 0,
    alignItems: "center",
    gap: 6,
    fontSize: 13,
    fontWeight: 600,
  },
  titleText: { minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  hash: { flexShrink: 0, color: "var(--precision-text3)", fontWeight: 500 },
  subtitle: {
    minWidth: 0,
    flexGrow: 1,
    flexBasis: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  project: { display: { default: "inline", "@media (max-width: 900px)": "none" } },
  tools: { display: "flex", flexShrink: 0, gap: 2 },
  gate: {
    marginInline: "auto",
    // A status box with nothing to say renders nothing: then no empty "Your decision" either.
    display: { default: "flex", ":empty": "none" },
    width: "100%",
    maxWidth: 820,
    maxHeight: "45vh",
    flexShrink: 0,
    boxSizing: "border-box",
    flexDirection: "column",
    overflowY: "auto",
    paddingInline: 24,
    paddingBottom: 8,
  },
  gateNarrow: { paddingInline: 12 },
  centre: {
    display: "flex",
    minHeight: 0,
    flexGrow: 1,
    flexBasis: 0,
    flexDirection: "column",
    justifyContent: "center",
  },
  loading: { display: "flex", flexDirection: "column", gap: 8, padding: 24 },
  waiting: { margin: 0, padding: 24, color: "var(--precision-text3)" },
  problem: { padding: 24 },
  tabs: { marginBottom: 4 },
  value: { display: "flex", minWidth: 0, alignItems: "center", gap: 6 },
  third: { color: "var(--precision-text3)" },
  stack: { display: "flex", flexDirection: "column", gap: 8 },
  files: { display: "flex", flexDirection: "column", alignItems: "flex-start", gap: 6 },
  task: {
    display: "flex",
    width: "100%",
    height: 30,
    cursor: "pointer",
    alignItems: "center",
    gap: 8,
    borderRadius: 6,
    borderWidth: 0,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    paddingInlineEnd: 4,
    textAlign: "left",
    color: "var(--color-text-primary)",
  },
  taskTitle: {
    minWidth: 0,
    flexGrow: 1,
    flexBasis: 0,
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
  },
  taskWho: { flexShrink: 0, fontSize: 12, color: "var(--color-text-secondary)" },
  taskState: { flexShrink: 0, fontSize: 12 },
  mono: { fontFamily: "var(--font-family-code)", fontSize: 12 },
});

/** The ticket of `/ticket/N`: its channel in the cached list, else `ticket.list "#N"`. */
function useResolved(n: number) {
  const connection = useConnection();
  const { hostId } = useLinkState();
  const { storage } = usePlatform();
  const channels = useChannels();
  // A list that did not load still leaves the search.
  const known = channels.data ?? (channels.error ? [] : undefined);
  const first = known ? resolveNumber(n, known, undefined) : null;
  const listed = useKeyed(
    ["tickets", "number", n],
    async () => expectBody(await connection.call("ticket.list", { query: `#${n}` }), "tickets").value.tickets,
    first === "search",
  );
  const resolved = first === "search" ? resolveNumber(n, known, listed.data) : first;
  // Remembered per host, so a reload opens the ticket before the lists are read again.
  const key = `ticket-session.id.${hostId ?? ""}.${n}`;
  const found = resolved && typeof resolved === "object" ? resolved.ticketId : null;
  useEffect(() => {
    if (found) storage.set(key, found);
  }, [found, key, storage]);
  const remembered = storage.get(key);
  return {
    ticketId: found ?? remembered ?? null,
    missing: resolved === "missing" && !remembered,
    error: listed.error,
    retry: () => {
      void channels.refetch();
      void listed.refetch();
    },
  };
}

/**
 * `/board?ticket=<id>` and `/history?ticket=<id>` (BR6): the page shows as normal while `ticket.get`
 * gives the number, then the URL becomes the ticket's session.
 */
export function TicketRedirect({ ticketId }: { ticketId: string }) {
  const navigate = useNavigate();
  const detail = useTicket(ticketId);
  const number = detail.data?.ticket.number ?? null;
  useEffect(() => {
    if (number !== null) void navigate({ ...ticketPlace({ id: ticketId, number }), replace: true });
  }, [number, ticketId, navigate]);
  return null;
}

/** `/chat?channel=<a ticket's channel>[&message][&thread]` opens that ticket's session at the same place (BR6). */
export function ChatRedirect({
  channel,
  message,
  thread,
}: {
  channel: string | null;
  message: string | null;
  thread: string | null;
}) {
  const navigate = useNavigate();
  const channels = useChannels();
  const target = channel ? sessionRedirect("/chat", { channel }, channels.data) : null;
  const number = target && "number" in target ? target.number : null;
  useEffect(() => {
    if (number === null) return;
    const search = { ...(message ? { message } : {}), ...(thread ? { thread } : {}) };
    void navigate({ ...ticketPlace({ id: "", number }, search), replace: true });
  }, [number, message, thread, navigate]);
  return null;
}

export function TicketSessionScreen({
  number,
  message = null,
  thread = null,
}: {
  number: number;
  message?: string | null;
  thread?: string | null;
}) {
  useKeyContext("TicketSession");
  const navigate = useNavigate();
  const { status } = useLinkState();
  const resolved = useResolved(number);
  const detail = useTicket(resolved.ticketId);
  const t = detail.data?.ticket.number === number ? detail.data.ticket : undefined;
  const loading = useDelayed(!t);
  const gone = resolved.missing || (detail.error && /not.?found/i.test(detail.error.message));

  if (gone) {
    return (
      <section aria-label={`#${number}`} {...stylex.props(styles.screen)}>
        <div {...stylex.props(styles.centre)}>
          <EmptyState
            title={`Ticket #${number} not found.`}
            actions={<Button label="Open the Board" onClick={() => void navigate({ to: "/board" })} />}
          />
        </div>
      </section>
    );
  }
  const failed = detail.error ?? resolved.error;
  if (!t || !detail.data) {
    return (
      <section aria-label={`#${number}`} {...stylex.props(styles.screen)}>
        <header data-titlebar {...stylex.props(styles.top)}>
          <Heading level={1} xstyle={styles.title}>
            <span {...stylex.props(styles.hash)}>#{number}</span>
          </Heading>
        </header>
        {failed ? (
          <div {...stylex.props(styles.problem)}>
            <Banner
              status="error"
              role="alert"
              title="This ticket did not load."
              description={failed.message}
              endContent={
                <Button
                  label="Try again"
                  onClick={() => {
                    resolved.retry();
                    void detail.refetch();
                  }}
                />
              }
            />
          </div>
        ) : status !== "online" ? (
          <p {...stylex.props(styles.waiting)}>Waiting for the server…</p>
        ) : loading ? (
          <div {...stylex.props(styles.loading)}>
            <Skeleton height={48} />
            <Skeleton height={48} />
            <Skeleton height={48} />
          </div>
        ) : null}
      </section>
    );
  }
  return <Session key={t.id} detail={detail.data} t={t} message={message} thread={thread} />;
}

function Session({
  detail,
  t,
  message,
  thread,
}: {
  detail: TicketDetail;
  t: TicketView;
  message: string | null;
  thread: string | null;
}) {
  const navigate = useNavigate();
  const { features, user } = useLinkState();
  const conversation = features.includes("ticket_channels") ? (t.conversation_id ?? null) : null;
  const closed = readOnly(t, null);
  const dock = usePropertiesDock();
  // The head spans the screen; the properties dock beside the conversation, below it.
  const head = <Head t={t} conversation={conversation} me={user?.id ?? null} dock={dock} />;
  const gate = <TicketGate detail={detail} t={t} />;
  const aside = (
    <PropertiesPanel dock={dock} label="Ticket properties" wide>
      <Properties detail={detail} t={t} />
    </PropertiesPanel>
  );
  const slots: SessionSlots = {
    head: null,
    gate,
    chips: <RunChips t={t} />,
    aside,
    readOnly: closed,
    readOnlyText: readOnlyBar({ ticket_number: t.number }, t.column === "done"),
    placeholder: composerPlaceholder(t),
    hint: composerHint(t),
  };
  const onThread = (id: string | null) =>
    void navigate({
      to: "/ticket/$number",
      params: { number: String(t.number) },
      search: id ? { thread: id } : {},
    });
  return (
    <section aria-label={`#${t.number} ${t.title}`} {...stylex.props(styles.screen)}>
      {head}
      {conversation ? (
        <TicketSessionBody
          conversationId={conversation}
          ticketId={t.id}
          message={message}
          thread={thread}
          onThread={onThread}
          session={slots}
        />
      ) : (
        // An older server has no ticket conversation: the ticket's data and decision still work.
        <div {...stylex.props(styles.body)}>
          <div {...stylex.props(styles.main)}>
            <div {...stylex.props(styles.centre)}>
              <EmptyState isCompact title="This server has no ticket conversation." />
            </div>
            {gate}
          </div>
          {aside}
        </div>
      )}
    </section>
  );
}

function Head({
  t,
  conversation,
  me,
  dock,
}: {
  t: TicketView;
  conversation: string | null;
  me: string | null;
  /** The properties panel the head's button shows or hides. */
  dock: PropertiesDock;
}) {
  const { status } = useLinkState();
  const board = useBoard(null);
  const employees = useEmployees();
  const write = useTicketWrite();
  const { choosing } = useDispatcherLine(t);
  const menu = useTicketMenu(t, choosing);
  const live = useLiveAll(useLiveStore(conversation));
  const answering = [...new Set(live.flatMap((u) => runAuthor(employees.data ?? [], u.run_id) ?? []))];
  const column = board.data?.columns.find((c) => c.column.kind === t.column)?.column.name ?? t.column.replace("_", " ");
  const open = !readOnly(t, null) && status === "online";
  const line = headLine(t, me, answering);
  // The project leaves the second line on a narrow window; it is in the properties.
  const project = answering.length === 0 && line.endsWith(` · ${t.project_name}`) ? t.project_name : null;
  return (
    <header data-titlebar {...stylex.props(styles.top)}>
      {menu.dialogs}
      <div {...stylex.props(styles.lines)}>
        <div {...stylex.props(styles.line1)}>
          <Heading level={1} xstyle={styles.title}>
            <span {...stylex.props(styles.hash)}>{ticketNumber(t)}</span>
            <Editable
              label="Title"
              value={t.title}
              enabled={open}
              display={<span {...stylex.props(styles.titleText)}>{t.title}</span>}
              onSave={(title) =>
                write("ticket.update", { ticket_id: t.id, title: title.trim() }, { expectedVersion: t.version })
              }
            />
          </Heading>
          <Chip>
            <ColumnGlyph kind={t.column} />
            {column}
          </Chip>
        </div>
        <div {...stylex.props(styles.line2)}>
          <span {...stylex.props(styles.subtitle)}>
            {project ? line.slice(0, -project.length - 3) : line}
            {project ? <span {...stylex.props(styles.project)}> · {project}</span> : null}
          </span>
        </div>
      </div>
      <div {...stylex.props(styles.tools)}>
        <Menu
          label="More"
          options={menu.options}
          onPick={menu.pick}
          trigger={<IconButton variant="ghost" size="sm" label="More" icon={<Icon icon="moreHorizontal" />} />}
        />
        <PropertiesToggle dock={dock} label="Ticket properties" />
      </div>
    </header>
  );
}

/**
 * The compact session in the chat popout (O2): the same timeline, progress, gate and composer rules
 * in the narrow column, without a head or side panel. None until the ticket is read.
 */
export function useCompactSession(ticketId: string): SessionSlots | undefined {
  const detail = useTicket(ticketId);
  const t = detail.data?.ticket;
  if (!t || !detail.data) return undefined;
  return {
    gate: <TicketGate detail={detail.data} t={t} narrow />,
    readOnly: readOnly(t, null),
    readOnlyText: readOnlyBar({ ticket_number: t.number }, t.column === "done"),
    placeholder: composerPlaceholder(t),
    hint: composerHint(t),
  };
}

/** The decision of the moment, right above the composer: the column's gate, or what the column says. */
export function TicketGate({ detail, t, narrow = false }: { detail: TicketDetail; t: TicketView; narrow?: boolean }) {
  const { status } = useLinkState();
  const { choosing, line } = useDispatcherLine(t);
  const kind = gateOf(t);
  const root = useMessages(kind === "plan" ? detail.root_task?.conversation_id : null);
  const check = doneCheck(detail);
  const body =
    kind === "review" ? (
      <ReviewGate
        key={t.version}
        keyed
        session
        t={t}
        report={detail.root_task?.result ?? null}
        ready={check.kind === "ready" && status === "online"}
        whyNot={check.kind === "working" ? "Still at work: Done waits until that has finished." : null}
      />
    ) : kind === "plan" ? (
      <PlanGate
        key={t.version}
        keyed
        session
        t={t}
        plan={root.isPending ? undefined : latestPlan(root.data?.messages ?? [])}
      />
    ) : t.archived_at_ms ? null : (
      // Status in an open column; on Done only a summary to retry or a quick task's merge.
      <StatusBox t={t} choosing={choosing} dispatcherLine={line} session />
    );
  if (!body) return null;
  return (
    <section aria-label="Your decision" {...stylex.props(styles.gate, narrow && styles.gateNarrow)}>
      {body}
    </section>
  );
}

/** Mode and Model of the ticket's next run, beside Send (`ticket.update { run }`, as the board panel). */
function RunChips({ t }: { t: TicketView }) {
  const { status } = useLinkState();
  const models = useModels();
  const write = useTicketWrite();
  if (readOnly(t, null) || status !== "online" || runLocked(t)) return null;
  const update = (run: TicketView["run"]) =>
    void write("ticket.update", { ticket_id: t.id, run }, { expectedVersion: t.version });
  return (
    <>
      {isQuick(t) ? null : (
        <Menu
          label="Mode"
          value={t.run.mode ?? "default"}
          options={MODES}
          onPick={(mode) => update({ ...t.run, mode: mode === "default" ? null : (mode as AgentMode) })}
          trigger={
            <Chip name="Mode" quiet>
              {MODES.find((m) => m.key === (t.run.mode ?? "default"))?.label}
            </Chip>
          }
        />
      )}
      <Menu
        label="Model"
        value={t.run.model ?? "default"}
        options={models}
        onPick={(model) => update({ ...t.run, model: model === "default" ? null : model })}
        trigger={
          <Chip name="Model" quiet>
            {models.find((m) => m.key === (t.run.model ?? "default"))?.label ?? "Default"}
          </Chip>
        }
      />
    </>
  );
}

type Section = "properties" | "activity";

/** The properties panel's two tabs: the ticket's data, and what happened to it. */
function Properties({ detail, t }: { detail: TicketDetail; t: TicketView }) {
  const platform = usePlatform();
  const [tab, setTab] = useState<Section>("properties");
  const write = useTicketWrite();
  const closed = readOnly(t, null);
  const addFiles = async (files: { path: string }[]) => {
    if (files.length === 0) return;
    await write(
      "ticket.update",
      { ticket_id: t.id, add_files: files.map((f) => f.path) },
      { expectedVersion: t.version, done: "Files added." },
    );
  };
  const addFailed = (e: unknown) => toast(e instanceof Error ? e.message : "This did not work.");
  // A file copied or dropped on the properties goes on the ticket, as on the board's panel; never on a closed one.
  const takeFiles = (given: File[]) => {
    const { fit, problems } = acceptable(given);
    for (const problem of problems) toast(problem);
    if (fit.length > 0 && !closed) platform.keepFiles(fit).then(addFiles).catch(addFailed);
  };
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: a drop and a paste land anywhere on the panel.
    <div
      {...stylex.props(styles.stack)}
      onPaste={(e) => {
        const given = filesFrom(e.clipboardData);
        if (given.length === 0) return;
        e.preventDefault();
        takeFiles(given);
      }}
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes("Files")) e.preventDefault();
      }}
      onDrop={(e) => {
        const given = filesFrom(e.dataTransfer);
        if (given.length === 0) return;
        e.preventDefault();
        takeFiles(given);
      }}
    >
      <TabList
        role="tablist"
        aria-label="Ticket properties"
        hasDivider
        value={tab}
        onChange={(next) => setTab(next as Section)}
        xstyle={styles.tabs}
      >
        <Tab value="properties" label="Properties" panelId="ticket-properties-tab" />
        <Tab value="activity" label="Activity" panelId="ticket-properties-tab" />
      </TabList>
      <div id="ticket-properties-tab" role="tabpanel" aria-label={tab === "activity" ? "Activity" : "Properties"}>
        {tab === "activity" ? (
          <Activity id={t.id} />
        ) : (
          <PropertiesTab detail={detail} t={t} onAddFiles={() => platform.pickFiles().then(addFiles, addFailed)} />
        )}
      </div>
    </div>
  );
}

function PropertiesTab({ detail, t, onAddFiles }: { detail: TicketDetail; t: TicketView; onAddFiles: () => void }) {
  const navigate = useNavigate();
  const { features, status, user } = useLinkState();
  const employees = useEmployees();
  const members = useMembers();
  const models = useModels();
  const write = useTicketWrite();
  const open = !readOnly(t, null) && status === "online";
  const activity = useTicketActivity(t.review_feedback ? t.id : null);
  const update = (params: Record<string, unknown>) =>
    write("ticket.update", { ticket_id: t.id, ...params } as never, { expectedVersion: t.version });
  const tasks = useMemo(() => foldTree(treeOrder(detail.tasks), {}), [detail.tasks]);
  const me = user?.id ?? null;
  const responsibleItems = [
    { key: "me", label: "Me" },
    ...(members.data ?? [])
      .filter((u) => !u.disabled_at_ms && u.id !== me)
      .map((u) => ({ key: u.id, label: u.username })),
    { key: "none", label: "No one" },
  ];
  const branch = branchOf(t);
  const modeLabel = MODES.find((m) => m.key === (t.run.mode ?? "default"))?.label ?? "Owner's default";
  const modelLabel = models.find((m) => m.key === (t.run.model ?? "default"))?.label ?? "Default";
  const usage = useTicketUsage(t);
  return (
    <>
      <SideProperties usage={usage}>
        <SideRow name="Priority">
          {open ? (
            <Menu
              label="Priority"
              value={t.priority}
              options={PRIORITIES.map((p) => ({ key: p, label: priorityLabel(p) }))}
              onPick={(priority: TicketPriority) => void update({ priority })}
              trigger={<Chip quiet>{priorityLabel(t.priority)}</Chip>}
            />
          ) : (
            priorityLabel(t.priority)
          )}
        </SideRow>
        <SideRow name="Owner">
          {open && ownerChoosable(t) ? (
            <Menu
              label="Owner"
              {...(t.owner_employee_id ? { value: t.owner_employee_id } : {})}
              options={(employees.data ?? [])
                .filter((e) => e.status === "active")
                .map((e) => ({ key: e.id, label: e.name }))}
              onPick={(id) => void write("ticket.assign", { ticket_id: t.id, employee_id: id })}
              trigger={<Chip quiet>{t.owner_name ?? "No owner"}</Chip>}
            />
          ) : (
            <span {...stylex.props(styles.value)}>
              {t.owner_name ? (
                <Mark
                  letter={initial(t.owner_name)}
                  color={markColor(t.owner_employee_id ?? "")}
                  size={14}
                  radius={4}
                />
              ) : null}
              {t.owner_name ?? "No owner"}
            </span>
          )}
        </SideRow>
        {features.includes("ticket_responsible") ? (
          <SideRow name="Responsible">
            {open ? (
              <Menu
                label="Responsible"
                options={responsibleItems}
                onPick={(key) =>
                  void write("ticket.set_responsible", {
                    ticket_id: t.id,
                    responsible_user_id: key === "none" ? null : key === "me" ? me : key,
                  })
                }
                trigger={
                  <Chip quiet>
                    {t.responsible_user_id && t.responsible_user_id === me
                      ? "You"
                      : (t.responsible_username ?? "No one")}
                  </Chip>
                }
              />
            ) : t.responsible_user_id && t.responsible_user_id === me ? (
              "You"
            ) : (
              (t.responsible_username ?? "No one")
            )}
          </SideRow>
        ) : null}
        <SideRow name="Mode · Model" title={`${modeLabel} · ${modelLabel}`}>
          {`${modeLabel} · ${modelLabel}`}
        </SideRow>
        <SideRow name="Labels">
          <Editable
            label="Labels"
            value={t.labels.join(", ")}
            enabled={open}
            placeholder="backend, ui"
            display={t.labels.length > 0 ? t.labels.join(", ") : <span {...stylex.props(styles.third)}>None</span>}
            onSave={(text) => update({ labels: parseLabels(text) })}
          />
        </SideRow>
        <SideRow name="Deadline">
          <Editable
            label="Deadline"
            value={t.deadline_ms ? deadlineInput(t.deadline_ms, -new Date().getTimezoneOffset()) : ""}
            enabled={open}
            placeholder="2026-11-04 or 2026-11-04 17:00"
            display={t.deadline_ms ? time(t.deadline_ms) : <span {...stylex.props(styles.third)}>None</span>}
            onSave={async (text) => {
              try {
                const ms = parseDeadline(text, -new Date().getTimezoneOffset());
                return await update(ms === null ? { clear_deadline: true } : { deadline_ms: ms });
              } catch (e) {
                toast((e as Error).message);
                return false;
              }
            }}
          />
        </SideRow>
        <SideRow name="Project">{t.project_name}</SideRow>
        {features.includes(DESIGN_FEATURE) ? (
          <DesignRow
            url={t.design_url ?? null}
            enabled={open}
            onSave={async (text) => {
              const problem = artifactProblem(text);
              if (problem) {
                toast(problem);
                return false;
              }
              return update({ design_url: text.trim() });
            }}
          />
        ) : null}
        <SideRow name="Runs on">
          {t.login_user === me ? "Your Claude login" : `${t.login_username}'s Claude login`}
        </SideRow>
        <SideRow name="Created by">{`${t.created_by_username} · ${time(t.created_at_ms)}`}</SideRow>
        {features.includes("ticket_follow") ? <Followers id={t.id} /> : null}
      </SideProperties>
      <SideSection title="Description">
        <div {...stylex.props(styles.stack)}>
          {t.review_feedback && t.column === "doing" ? (
            <Box>
              <span {...stylex.props(styles.third)}>{feedbackLabel(activity.data)}</span>
              <span>{t.review_feedback}</span>
            </Box>
          ) : null}
          <Editable
            label="Description"
            value={t.description ?? ""}
            enabled={open}
            multiline
            display={
              t.description?.trim() ? (
                <Markdown text={t.description} />
              ) : (
                <span {...stylex.props(styles.third)}>No description.</span>
              )
            }
            onSave={(description) => update({ description })}
          />
        </div>
      </SideSection>
      <SideSection title="Files">
        <span {...stylex.props(styles.files)}>
          <FileChips files={t.attachments} />
          {t.attachments.length === 0 && readOnly(t, null) ? <span {...stylex.props(styles.third)}>None</span> : null}
          {readOnly(t, null) ? null : (
            <Button label="Add files…" isDisabled={status !== "online"} onClick={onAddFiles} />
          )}
        </span>
      </SideSection>
      {tasks.length > 0 ? (
        <SideSection title={`Tasks (${detail.tasks.length})`}>
          {tasks.map(({ task, depth }) => {
            const state = taskStateLine(task);
            return (
              <button
                key={task.id}
                type="button"
                onClick={() => void navigate({ to: "/tasks/$taskId", params: { taskId: task.id } })}
                {...stylex.props(styles.task)}
                style={{ paddingLeft: 4 + 16 * depth }}
              >
                <span {...stylex.props(styles.taskTitle)}>
                  {task.id === t.root_task_id ? `Main task · ${task.title}` : task.title}
                </span>
                {task.employee_name ? <span {...stylex.props(styles.taskWho)}>{task.employee_name}</span> : null}
                <span {...stylex.props(styles.taskState)} style={{ color: state.color }}>
                  {state.text}
                </span>
              </button>
            );
          })}
        </SideSection>
      ) : null}
      {branch || t.merge_commit ? (
        <SideSection title="Work">
          {branch ? (
            <SideRow name="Branch" title={branch}>
              <span {...stylex.props(styles.mono)}>{branch}</span>
            </SideRow>
          ) : null}
          {t.merge_commit ? (
            <SideRow name="Merged" title={t.merge_commit}>
              <span {...stylex.props(styles.mono)}>{t.merge_commit.slice(0, 8)}</span>
            </SideRow>
          ) : null}
        </SideSection>
      ) : null}
      <RunsOfTicket detail={detail} t={t} />
    </>
  );
}

/**
 * What all of the ticket's runs wrote, which guidelines the latest one had and what they used
 * (Titan's decision for ticket #70): files and guidelines over the discussion and every task's
 * conversation, read only while this tab shows (usage sits behind the info icon of the Properties
 * heading). Each block hides when there is nothing to show: never a zero or a spinner.
 */
function RunsOfTicket({ detail, t }: { detail: TicketDetail; t: TicketView }) {
  const ids = useMemo(() => ticketConversations(t.conversation_id, detail.tasks), [t.conversation_id, detail.tasks]);
  const reads = useMessagesOf(ids);
  const lists = reads.map((r) => r.data?.messages);
  const files = filesWrittenAcross(lists);
  const run = latestRunAcross(lists);
  return (
    <>
      <FilesWritten files={files} />
      <RunGuidelines runId={run} title="Guidelines in the runs" />
    </>
  );
}

/** What all of the ticket's runs used (`usage.get_totals { ticket_id }`), only on a server with `run_cards`. */
function useTicketUsage(t: TicketView) {
  const connection = useConnection();
  const { features } = useLinkState();
  const totals = useKeyed(
    // The same read as the run card's odometer, so both share it.
    ["usage", "odometer", t.id, null],
    async () =>
      expectBody(await connection.call("usage.get_totals", { ticket_id: t.id, conversation_id: null }), "usage_totals")
        .value,
    features.includes("run_cards"),
  );
  return features.includes("run_cards") ? ticketUsage(totals.data?.total) : null;
}
