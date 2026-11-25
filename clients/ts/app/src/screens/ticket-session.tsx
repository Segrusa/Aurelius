// A ticket as a session (`/ticket/$number`, ticket #70): one screen with the ticket's conversation, its
// progress, the decision of the moment and the ticket's data. It replaces the ticket's Team chat
// channel and the Board's panel. The server decides everything as before: who answers a message
// (`owner_answers`), and who may accept, send back or approve. This screen only places the existing
// reads and writes of the channel and the board panel around one timeline.
//
// Ticket #74 lays it out as the ticket page: the description and what every task of the ticket does
// now above the talk (a ticket can have a main task and several delegations at once), the talk as
// its Activity with a filter, what runs this second above the composer, and on the right the plan of
// the main task with its delegations, then the ticket's properties and files.

import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { Tab, TabList } from "@astryxdesign/core/TabList";
import { expectBody } from "@aurelius/client";
import type { AgentMode, TaskView, TicketDetail, TicketPriority, TicketView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useNavigate } from "@tanstack/react-router";
import { type ReactNode, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
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
import { designPagesAcross } from "./artifact-logic";
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
import { readOnlyBar } from "./chat-logic";
import { parseDeadline, parseLabels, useModels } from "./dialogs";
import { isQuick } from "./quick-logic";
import {
  DesignPagesSection,
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
import { taskStateLine } from "./tasks-logic";
import { ActivityPanel, type TicketTab, TicketTabs, useTicketAsks } from "./ticket-activity";
import { branchOf, latestTodos } from "./ticket-overview-logic";
import { PageSection, type Person, useTicketWork } from "./ticket-page";
import { planStep } from "./ticket-page-logic";
import {
  Box,
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
  latestRunAcross,
  readOnly,
  resolveNumber,
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
    minHeight: 44,
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
  olderPanel: { minHeight: 0, flexGrow: 1, flexBasis: 0, overflowY: "auto" },
  count: { marginInlineStart: 4, color: "var(--precision-text3)" },
  // DESCRIPTION_LINES: StyleX needs the literal here.
  folded: { display: "-webkit-box", WebkitBoxOrient: "vertical", WebkitLineClamp: 3, overflow: "hidden" },
  foldButton: { marginTop: 4 },
});

/** How many lines of the description the ticket page shows until "Show more". */
export const DESCRIPTION_LINES = 3;

/**
 * The ticket's description (editable while the ticket is open) and the feedback the owner works
 * on, at the end of the Activity tab (ticket #83).
 */
function PageTop({ t }: { t: TicketView }) {
  const { status } = useLinkState();
  const write = useTicketWrite();
  const open = !readOnly(t, null) && status === "online";
  const activity = useTicketActivity(t.review_feedback ? t.id : null);
  // Folded to DESCRIPTION_LINES lines per ticket; "Show more" only when the text is longer.
  const [unfoldedFor, setUnfoldedFor] = useState<string | null>(null);
  const unfolded = unfoldedFor === t.id;
  const [long, setLong] = useState(false);
  const folded = useRef<HTMLDivElement>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the text changes what overflows.
  useLayoutEffect(() => {
    const box = folded.current;
    if (!box || unfolded) return;
    const measure = () => setLong(box.scrollHeight > box.clientHeight + 1);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(box);
    return () => observer.disconnect();
  }, [t.description, unfolded]);
  return (
    <PageSection title="Description">
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
            <div ref={folded} {...stylex.props(!unfolded && styles.folded)}>
              <Markdown text={t.description} />
            </div>
          ) : (
            <span {...stylex.props(styles.third)}>No description.</span>
          )
        }
        footer={
          t.description?.trim() && (long || unfolded) ? (
            <div {...stylex.props(styles.foldButton)}>
              <Button
                size="sm"
                variant="ghost"
                aria-expanded={unfolded}
                label={unfolded ? "Show less" : "Show more"}
                onClick={() => setUnfoldedFor(unfolded ? null : t.id)}
              />
            </div>
          ) : null
        }
        onSave={(description) =>
          write("ticket.update", { ticket_id: t.id, description }, { expectedVersion: t.version })
        }
      />
    </PageSection>
  );
}

/** The employees of a ticket's tasks, each once, in the order they joined: the Timeline's chips. */
function ticketPeople(tasks: readonly TaskView[]): Person[] {
  const out: Person[] = [];
  for (const task of tasks)
    if (task.employee_name && !out.some((p) => p.name === task.employee_name)) out.push({ name: task.employee_name });
  return out;
}

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

export function TicketSessionScreen({
  number,
  message = null,
  thread = null,
  design = null,
}: {
  number: number;
  message?: string | null;
  thread?: string | null;
  /** A kept design file (by id) that opens in the design panel at once. */
  design?: string | null;
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
  return <Session key={t.id} detail={detail.data} t={t} message={message} thread={thread} design={design} />;
}

function Session({
  detail,
  t,
  message,
  thread,
  design,
}: {
  detail: TicketDetail;
  t: TicketView;
  message: string | null;
  thread: string | null;
  design: string | null;
}) {
  const navigate = useNavigate();
  const { features } = useLinkState();
  const conversation = features.includes("ticket_channels") ? (t.conversation_id ?? null) : null;
  const closed = readOnly(t, null);
  const dock = usePropertiesDock();
  const gate = <TicketGate detail={detail} t={t} />;
  const top = <PageTop t={t} />;
  const work = useTicketWork(detail.tasks);
  const asks = useTicketAsks(detail, t, work);
  // A link to a message or a thread opens the Timeline; else the Activity is what a ticket opens on.
  const [tab, setTab] = useState<TicketTab>(message || thread ? "timeline" : "activity");
  const [who, setWho] = useState<string | null>(null);
  const people = useMemo(() => ticketPeople(detail.tasks), [detail.tasks]);
  // The head spans the screen; the properties dock beside the conversation, below it.
  const head = (
    <Head
      t={t}
      dock={dock}
      tabs={conversation ? <TicketTabs tab={tab} onTab={setTab} questions={asks.size} /> : null}
    />
  );
  const panel = (
    <ActivityPanel
      detail={detail}
      t={t}
      work={work}
      asks={asks}
      gate={gate}
      description={top}
      onTimeline={(name) => {
        setWho(name);
        setTab("timeline");
      }}
    />
  );
  const aside = (
    <PropertiesPanel dock={dock} label="Ticket properties" wide>
      <Properties detail={detail} t={t} />
    </PropertiesPanel>
  );
  const slots: SessionSlots = {
    design,
    head: null,
    top: null,
    work: work.blocks,
    ...(tab === "activity" ? { panel } : {}),
    newestFirst: true,
    people,
    who,
    onWho: setWho,
    live: null,
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
            <div {...stylex.props(styles.olderPanel)}>{panel}</div>
          </div>
          {aside}
        </div>
      )}
    </section>
  );
}

function Head({
  t,
  dock,
  tabs,
}: {
  t: TicketView;
  /** The properties panel the head's button shows or hides. */
  dock: PropertiesDock;
  /** Activity and Timeline under the title (ticket #83). */
  tabs: ReactNode;
}) {
  const { status } = useLinkState();
  const board = useBoard(null);
  const write = useTicketWrite();
  const { choosing } = useDispatcherLine(t);
  const menu = useTicketMenu(t, choosing);
  const column = board.data?.columns.find((c) => c.column.kind === t.column)?.column.name ?? t.column.replace("_", " ");
  const open = !readOnly(t, null) && status === "online";
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
        {tabs}
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

/** The decision of the moment, right above the composer: the column's gate, or what the column says. */
export function TicketGate({ detail, t }: { detail: TicketDetail; t: TicketView }) {
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
    <section aria-label="Your decision" {...stylex.props(styles.gate)}>
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

type Section = "properties" | "files";

/** The details panel: tabs for the ticket's data and its files (its plan and history are the Activity and Timeline tabs). */
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
        <Tab
          value="files"
          label="Files"
          endContent={
            t.attachments.length > 0 ? <span {...stylex.props(styles.count)}>{t.attachments.length}</span> : null
          }
          panelId="ticket-properties-tab"
        />
      </TabList>
      <div id="ticket-properties-tab" role="tabpanel" aria-label={tab === "files" ? "Files" : "Properties"}>
        {tab === "files" ? (
          <FilesTab detail={detail} t={t} onAddFiles={() => platform.pickFiles().then(addFiles, addFailed)} />
        ) : (
          <PropertiesTab detail={detail} t={t} />
        )}
      </div>
    </div>
  );
}

function PropertiesTab({ detail, t }: { detail: TicketDetail; t: TicketView }) {
  const { features, status, user } = useLinkState();
  const employees = useEmployees();
  const members = useMembers();
  const models = useModels();
  const write = useTicketWrite();
  const open = !readOnly(t, null) && status === "online";
  const update = (params: Record<string, unknown>) =>
    write("ticket.update", { ticket_id: t.id, ...params } as never, { expectedVersion: t.version });
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
  const root = detail.root_task ?? null;
  return (
    <>
      <TicketDesign detail={detail} t={t} />
      <SideSection title="Run">
        <SideRow name="Status">
          {root ? <RootState task={root} /> : <span {...stylex.props(styles.third)}>Not started</span>}
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
        <SideRow name="Mode · Model" title={`${modeLabel} · ${modelLabel}`}>
          {`${modeLabel} · ${modelLabel}`}
        </SideRow>
        {root ? <SideRow name="Started">{time(root.created_at_ms)}</SideRow> : null}
        <SideRow name="Runs on">
          {t.login_user === me ? "Your Claude login" : `${t.login_username}'s Claude login`}
        </SideRow>
      </SideSection>
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
        <SideRow name="Project">{t.project_name}</SideRow>
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
        <SideRow name="Created by">{`${t.created_by_username} · ${time(t.created_at_ms)}`}</SideRow>
        {features.includes("ticket_follow") ? <Followers id={t.id} /> : null}
      </SideProperties>
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
      <RunsOfTicket detail={detail} t={t} part="guidelines" />
    </>
  );
}

/** The ticket's files and what its runs wrote; files dropped or pasted on the panel land here too. */
function FilesTab({ detail, t, onAddFiles }: { detail: TicketDetail; t: TicketView; onAddFiles: () => void }) {
  const { status } = useLinkState();
  return (
    <>
      <SideSection title="Files">
        <span {...stylex.props(styles.files)}>
          <FileChips files={t.attachments} />
          {t.attachments.length === 0 && readOnly(t, null) ? <span {...stylex.props(styles.third)}>None</span> : null}
          {readOnly(t, null) ? null : (
            <Button label="Add files…" isDisabled={status !== "online"} onClick={onAddFiles} />
          )}
        </span>
      </SideSection>
      <RunsOfTicket detail={detail} t={t} part="files" />
    </>
  );
}

/** The main task's state with the step of its plan: "Running · step 3 of 4". */
function RootState({ task }: { task: TaskView }) {
  const messages = useMessages(task.conversation_id ?? null);
  const step = planStep(latestTodos([], messages.data?.messages ?? []));
  const state = taskStateLine(task);
  return (
    <span style={{ color: state.color }}>
      {step && state.text === "Running" ? `${state.text} · step ${step.at} of ${step.total}` : state.text}
    </span>
  );
}

/**
 * What all of the ticket's runs wrote, which guidelines the latest one had and what they used
 * (Titan's decision for ticket #70; since ticket #74 the files sit on the Files tab): files and guidelines over the discussion and every task's
 * conversation, read only while this tab shows (usage sits behind the info icon of the Properties
 * heading). Each block hides when there is nothing to show: never a zero or a spinner.
 */
function RunsOfTicket({ detail, t, part }: { detail: TicketDetail; t: TicketView; part: "files" | "guidelines" }) {
  const ids = useMemo(() => ticketConversations(t.conversation_id, detail.tasks), [t.conversation_id, detail.tasks]);
  const reads = useMessagesOf(ids);
  const lists = reads.map((r) => r.data?.messages);
  const files = filesWrittenAcross(lists);
  const run = latestRunAcross(lists);
  return (
    <>
      {part === "files" ? <FilesWritten files={files} /> : <RunGuidelines runId={run} title="Guidelines in the runs" />}
    </>
  );
}

/** The design the ticket's discussion and its tasks made, when there is one (`DesignPagesSection`). */
function TicketDesign({ detail, t }: { detail: TicketDetail; t: TicketView }) {
  const ids = useMemo(() => ticketConversations(t.conversation_id, detail.tasks), [t.conversation_id, detail.tasks]);
  const reads = useMessagesOf(ids);
  const pages = designPagesAcross(reads.map((r) => r.data?.messages));
  return <DesignPagesSection pages={pages} />;
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
