import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import type { MessagesSnapshot } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { type ReactNode, useEffect, useMemo, useRef } from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { useConnection, useLinkState } from "../connection";
import { useApprovals, useEmployees, useMessages } from "../data";
import { CHAT_FILES_FEATURE } from "../ui/attach-logic";
import { useDelayed } from "../ui/use-delayed";
import { ApprovalCard } from "./approval-card";
import { DockedArtifact, DockOpenerProvider, targetKey, useArtifactDock, useOpenDesign } from "./artifact-dock";
import type { ArtifactSend } from "./artifact-feedback";
import { newestDesignFile } from "./artifact-logic";
import { Composer } from "./chat";
import { useLiveAll, useLiveStore } from "./chat-live";
import { before, localDay, quickReplies, streamTo } from "./chat-logic";
import { type ChatContext, ChatProvider, DayLine, FeedIdsProvider, LiveRow, MessageRow } from "./chat-message";
import { usePlanDock } from "./plan-dock";
import { approvalsFor } from "./run-card-logic";
import { ThreadWaiting } from "./scheduled";

const styles = stylex.create({
  screen: { display: "flex", minWidth: 0, minHeight: 0, flexGrow: 1, flexBasis: 0, flexDirection: "column" },
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
  title: { margin: 0, fontSize: 13, fontWeight: 600 },
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
  // Below the head: the conversation, and a session's properties panel beside it.
  body: { display: "flex", minWidth: 0, minHeight: 0, flexGrow: 1, flexBasis: 0 },
  main: { display: "flex", minWidth: 0, flexGrow: 1, flexBasis: 0, flexDirection: "column" },
  feedBox: { minHeight: 0, flexGrow: 1, flexBasis: 0 },
  feed: { height: "100%", scrollbarGutter: "stable both-edges" },
  feedColumn: { marginInline: "auto", width: "100%", maxWidth: 820, paddingInline: 24 },
  topGap: { paddingTop: 8 },
  approvalGap: { paddingTop: 12 },
  edge: { height: 14 },
  emptyView: { boxSizing: "border-box", height: "100%", overflowY: "auto", paddingBlock: 24 },
  loading: { display: "flex", flexDirection: "column", gap: 8, padding: 24 },
  problem: { padding: 24 },
  closed: {
    marginInline: "auto",
    width: "100%",
    maxWidth: 820,
    paddingInline: 24,
    paddingBottom: 16,
    color: "var(--color-text-secondary)",
  },
});

const FEED_PARTS = {
  Header: function FeedTop() {
    return <div {...stylex.props(styles.edge)} />;
  },
  Footer: function FeedEnd() {
    return <div {...stylex.props(styles.edge)} />;
  },
};

const NOBODY: never[] = [];
const offset = () => -new Date().getTimezoneOffset();

type Row =
  | { kind: "message"; m: ChatContext["messages"][number]; day: boolean; grouped: boolean }
  | { kind: "live"; id: string }
  | { kind: "approval"; id: string };

/** What a message from the composer carries; a frame may send it its own way. */
export interface ChatMessage {
  text: string;
  images: string[];
  files: string[];
  confirmSecret: boolean;
}

/**
 * What another kind of conversation with one employee (a work session, a task's run) shows instead of a
 * chat's words.
 */
export interface ChatFrame {
  /** The head's title; the employee's name when not given. Also the screen's name. */
  title?: string;
  /** The head's left side instead of the title alone (a task's project, ticket and state). */
  head?: ReactNode;
  /** What the conversation is, in "this session can only be read". */
  noun?: string;
  empty: { title: string; description: string };
  /** Instead of the empty state: what there is without messages (a finished task's result). */
  emptyView?: ReactNode;
  /** Pending approvals of this task show here too, also those of a run outside this conversation. */
  taskId?: string;
  /** The run under way, when the feed does not show it yet (a task's run waiting on an approval). */
  activeRun?: string | null;
  /** How the composer's message goes; `chat.send` when not given. */
  send?: (message: ChatMessage) => Promise<unknown>;
  /** Controls in the composer's bottom row, from what the conversation runs on now. */
  footer?: (snapshot: MessagesSnapshot) => ReactNode;
  /** What stands where the composer is, while nothing can be written (a session nobody took yet). */
  instead?: ReactNode;
  /** Buttons at the right of the head (a session's properties). */
  actions?: ReactNode;
  /** Beside the conversation, below the head: a session's properties panel. */
  aside?: ReactNode;
  /** A kept design file (by id) that opens in the design panel at once. */
  design?: string | null;
  /** A design session: without `design`, its newest page opens in the design panel at once. */
  designSession?: boolean;
}

/**
 * The chat itself: only the two of them read it. `frame`: a work session, which is this same conversation with its own head and empty state.
 */
export function EmployeeChat({ conversationId, frame }: { conversationId: string; frame?: ChatFrame }) {
  const { user, features } = useLinkState();
  const connection = useConnection();
  const feed = useMessages(conversationId);
  const approvals = useApprovals();
  const employees = useEmployees();
  const store = useLiveStore(conversationId);
  const live = useLiveAll(store);
  const list = useMemo(() => feed.data?.messages ?? [], [feed.data]);
  const handle = useRef<VirtuosoHandle>(null);
  const atBottom = useRef(true);
  const employee = employees.data?.find((e) => e.id === feed.data?.employee_id);
  const name = employee?.name ?? "Employee";
  // An artifact's viewer sends with this chat's own send (below): one ordinary message, as typed.
  const composerSend = useRef<((message: ChatMessage) => Promise<unknown>) | null>(null);
  const artifactSend = useMemo<ArtifactSend>(
    () => ({
      to: name,
      send: (text) =>
        composerSend.current?.({ text, images: [], files: [], confirmSecret: false }) ??
        Promise.reject(new Error("This chat cannot send yet.")),
    }),
    [name],
  );

  // An artifact opens in a panel at the right, in place of the session's properties.
  const dock = useArtifactDock();
  const opening = frame?.design ?? (frame?.designSession ? (newestDesignFile(list)?.id ?? null) : null);
  useOpenDesign(dock.open, list, opening);

  // A finished message ends its stream.
  useEffect(() => {
    store.settle(new Set(list.filter((m) => m.status !== "partial").map((m) => m.id)));
  }, [list, store]);

  const taskId = frame?.taskId;
  const pending = useMemo(
    () => approvalsFor(approvals.data ?? [], { conversationId, taskId }),
    [approvals.data, conversationId, taskId],
  );
  const approvalMap = useMemo(() => new Map(pending.map((a) => [a.id, a])), [pending]);
  usePlanDock(dock, list, approvalMap);
  const onCard = useMemo(() => new Set(list.flatMap((m) => m.approval_id ?? [])), [list]);
  const inList = useMemo(() => new Set(list.map((m) => m.id)), [list]);
  const rows = useMemo<Row[]>(() => {
    const dayOf = (ms: number) => localDay(ms, offset());
    const out: Row[] = list.map((m, i) => {
      const b = before(list, i, null, dayOf);
      return { kind: "message", m, day: b.day, grouped: b.grouped };
    });
    for (const u of live) {
      if (!inList.has(u.message_id) && streamTo(u.thread_parent, null) === "channel") {
        out.push({ kind: "live", id: u.message_id });
      }
    }
    for (const a of pending) if (!onCard.has(a.id)) out.push({ kind: "approval", id: a.id });
    return out;
  }, [list, live, inList, pending, onCard]);

  const employeesData = employees.data;
  const lastAnswerId = useMemo(
    () => [...list].reverse().find((m) => !m.update && m.role === "assistant")?.id ?? null,
    [list],
  );
  const ctx = useMemo<ChatContext>(
    () => ({
      store,
      caps: { threads: false, reactions: false, readOnly: false },
      features,
      people: NOBODY,
      me: user?.username ?? null,
      place: name,
      ticketNumber: null,
      roleOf: (who) => {
        const e = employeesData?.find((x) => x.name === who);
        return e ? (e.title ?? e.role) : null;
      },
      approvals: approvalMap,
      openThread: () => undefined,
      messages: list,
      lastAnswer: lastAnswerId,
      conversationId,
      channel: null,
      retryTarget: { employeeId: null },
      speaker: name,
      flash: null,
      artifactSend,
      openArtifact: dock.open,
      dockTarget: dock.target,
    }),
    [
      store,
      features,
      user,
      name,
      employeesData,
      approvalMap,
      list,
      lastAnswerId,
      conversationId,
      artifactSend,
      dock.open,
      dock.target,
    ],
  );

  const running =
    [...list].reverse().find((m) => m.status === "partial" && m.run_id)?.run_id ??
    live.at(-1)?.run_id ??
    frame?.activeRun ??
    null;
  const lastAnswer = [...list].reverse().find((m) => !m.update);
  const quick =
    features.includes("quick_replies") &&
    lastAnswer?.role === "assistant" &&
    lastAnswer.status === "complete" &&
    !pending.some((a) => a.run_id === lastAnswer.run_id)
      ? quickReplies(lastAnswer.activity)
      : null;
  const loading = useDelayed(feed.isPending);
  const closed = employee && employee.status !== "active" ? employee.status : null;
  const noun = frame ? (frame.noun ?? "session") : "chat";
  const send =
    frame?.send ??
    ((message: ChatMessage) =>
      connection.write(
        "chat.send",
        {
          conversation_id: conversationId,
          text: message.text,
          images: message.images,
          ...(message.files.length > 0 ? { files: message.files } : {}),
          confirm_secret: message.confirmSecret,
        },
        { retry: false },
      ));
  composerSend.current = send;

  return (
    <section aria-label={frame?.title ?? `Chat with ${name}`} {...stylex.props(styles.screen)}>
      <header data-titlebar {...stylex.props(styles.top)}>
        {frame?.head ?? (
          <Heading level={1} xstyle={styles.title}>
            {frame?.title || name}
          </Heading>
        )}
        {/* A session's head has the title only; a chat says the employee's role, or that it answers. */}
        <span {...stylex.props(styles.subtitle)}>
          {frame ? null : live.length > 0 ? `${name} answering` : employee ? (employee.title ?? employee.role) : ""}
        </span>
        {frame?.actions}
      </header>
      <div {...stylex.props(styles.body)}>
        <div {...stylex.props(styles.main)}>
          <ThreadWaiting conversationId={conversationId} />
          <ChatProvider value={ctx}>
            <FeedIdsProvider value={{ conversationId }}>
              <div {...stylex.props(styles.feedBox)}>
                {feed.error ? (
                  <div {...stylex.props(styles.problem)}>
                    <Banner
                      status="error"
                      role="alert"
                      title="This chat did not load."
                      description={feed.error.message}
                      endContent={<Button label="Try again" onClick={() => void feed.refetch()} />}
                    />
                  </div>
                ) : feed.isPending ? (
                  loading ? (
                    <div {...stylex.props(styles.loading)}>
                      <Skeleton height={48} />
                      <Skeleton height={48} />
                    </div>
                  ) : null
                ) : rows.length === 0 && frame?.emptyView ? (
                  <div {...stylex.props(styles.feedColumn, styles.emptyView)}>{frame.emptyView}</div>
                ) : rows.length === 0 ? (
                  <EmptyState
                    title={frame?.empty.title ?? `Talk with ${name} here.`}
                    description={
                      frame?.empty.description ??
                      `Only you and ${name} read this. Once the work is concrete, ${name} makes a ticket and tells you here when it reaches Review.`
                    }
                  />
                ) : (
                  <Virtuoso
                    ref={handle}
                    key={conversationId}
                    data={rows}
                    {...stylex.props(styles.feed)}
                    increaseViewportBy={600}
                    initialTopMostItemIndex={{ index: "LAST", align: "end" }}
                    atBottomThreshold={120}
                    atBottomStateChange={(b) => {
                      atBottom.current = b;
                    }}
                    followOutput={(bottom) => (bottom ? "auto" : false)}
                    // followOutput only follows new rows; an answer that grows while it
                    // streams keeps its end in view too, unless the reader scrolled up.
                    totalListHeightChanged={() => {
                      if (atBottom.current) handle.current?.scrollToIndex({ index: "LAST", align: "end" });
                    }}
                    computeItemKey={(_, row) => (row.kind === "message" ? row.m.id : row.id)}
                    components={FEED_PARTS}
                    itemContent={(_, row) => (
                      <div {...stylex.props(styles.feedColumn)}>
                        {row.kind === "message" ? (
                          <>
                            {row.day ? <DayLine ms={row.m.created_at_ms} /> : null}
                            <div {...(row.grouped ? null : stylex.props(styles.topGap))}>
                              <MessageRow m={row.m} grouped={row.grouped} />
                            </div>
                          </>
                        ) : row.kind === "live" ? (
                          <LiveRow id={row.id} name={name} kind="employee" />
                        ) : approvalMap.get(row.id) ? (
                          <div {...stylex.props(styles.approvalGap)}>
                            <ApprovalCard
                              approval={approvalMap.get(row.id) as NonNullable<ReturnType<typeof approvalMap.get>>}
                            />
                          </div>
                        ) : null}
                      </div>
                    )}
                  />
                )}
              </div>
            </FeedIdsProvider>
            {closed ? (
              <p {...stylex.props(styles.closed)}>
                {closed === "paused"
                  ? `${name} is paused: resume them to go on with this ${noun}.`
                  : `${name} is archived: this ${noun} can only be read.`}
              </p>
            ) : frame?.instead ? (
              frame.instead
            ) : (
              <Composer
                key={conversationId}
                draftKey={`chat:${conversationId}`}
                channel={null}
                threadParent={null}
                waiting={!running && list.at(-1)?.role === "user"}
                placeholder={`Message ${name}`}
                hint={null}
                activeRun={running}
                people={NOBODY}
                archived={NOBODY}
                quick={quick}
                footer={frame?.footer && feed.data ? frame.footer(feed.data) : null}
                // New session's box; files go along where the server takes them.
                prompt={{ files: features.includes(CHAT_FILES_FEATURE) }}
                send={send}
                onSent={() => handle.current?.scrollToIndex({ index: "LAST", align: "end" })}
              />
            )}
          </ChatProvider>
        </div>
        {dock.target ? (
          <DockedArtifact key={targetKey(dock.target)} target={dock.target} ctx={ctx} onClose={dock.close} />
        ) : (
          <DockOpenerProvider value={dock.open}>{frame?.aside}</DockOpenerProvider>
        )}
      </div>
    </section>
  );
}
