import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import type { EmployeeChatView, MessagesSnapshot } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { type ReactNode, useEffect, useMemo, useRef } from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { useConnection, useLinkState } from "../connection";
import { useApprovals, useEmployees, useMessages } from "../data";
import { useKeyContext } from "../keys/provider";
import { CHAT_FILES_FEATURE } from "../ui/attach-logic";
import { toast } from "../ui/toast";
import { useDelayed } from "../ui/use-delayed";
import { ApprovalCard } from "./approval-card";
import { Composer } from "./chat";
import { useLiveAll, useLiveStore } from "./chat-live";
import { before, localDay, quickReplies, streamTo } from "./chat-logic";
import { type ChatContext, ChatProvider, DayLine, FeedIdsProvider, LiveRow, MessageRow } from "./chat-message";
import { InboxBell } from "./inbox-bell";
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
  feedCompact: { paddingInline: 12 },
  topGap: { paddingTop: 8 },
  approvalGap: { paddingTop: 12 },
  edge: { height: 14 },
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

/** The acting board member's chat with an employee, made the first time; `go` gets where it is. */
export function useChatOpener(go: (conversationId: string) => void): (employeeId: string) => void {
  const connection = useConnection();
  return (employeeId) => {
    connection
      .write("employee.open_chat", { employee_id: employeeId })
      .then((receipt) => {
        const made = receipt.result;
        if (made.kind === "command" && made.value.type === "employee_chat_opened") go(made.value.conversation_id);
      })
      .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
  };
}

/** Opens that chat on its own screen. */
export function useOpenChat(): (employeeId: string) => void {
  const navigate = useNavigate();
  return useChatOpener((conversationId) => void navigate({ to: "/talk/$conversationId", params: { conversationId } }));
}

/** A board member's lasting chat with one employee, on its own screen. */
export function EmployeeChatScreen({ conversationId }: { conversationId: string }) {
  useKeyContext("TeamChat");
  return <EmployeeChat conversationId={conversationId} />;
}

/** What another kind of conversation with one employee (a work session) shows instead of a chat's words. */
export interface ChatFrame {
  /** The head's title; the employee's name when not given. */
  title?: string;
  /** The head's second line while nobody answers. */
  subtitle?: string;
  empty: { title: string; description: string };
  /** Controls in the composer's bottom row, from what the conversation runs on now. */
  footer?: (snapshot: MessagesSnapshot) => ReactNode;
  /** What stands where the composer is, while nothing can be written (a session nobody took yet). */
  instead?: ReactNode;
  /** Buttons at the right of the head (a session's properties). */
  actions?: ReactNode;
  /** Beside the conversation, below the head: a session's properties panel. */
  aside?: ReactNode;
}

/**
 * The chat itself: only the two of them read it. `compact`: in the popout, which has its own head.
 * `frame`: a work session, which is this same conversation with its own head and empty state.
 */
export function EmployeeChat({
  conversationId,
  compact = false,
  frame,
}: {
  conversationId: string;
  compact?: boolean;
  frame?: ChatFrame;
}) {
  const { user, features } = useLinkState();
  const connection = useConnection();
  const client = useQueryClient();
  const feed = useMessages(conversationId);
  const approvals = useApprovals();
  const employees = useEmployees();
  const store = useLiveStore(conversationId);
  const live = useLiveAll(store);
  const list = useMemo(() => feed.data?.messages ?? [], [feed.data]);
  const handle = useRef<VirtuosoHandle>(null);
  const employee = employees.data?.find((e) => e.id === feed.data?.employee_id);
  const name = employee?.name ?? "Employee";

  // A finished message ends its stream.
  useEffect(() => {
    store.settle(new Set(list.filter((m) => m.status !== "partial").map((m) => m.id)));
  }, [list, store]);

  // Open means read: no dot from a list read just before.
  // biome-ignore lint/correctness/useExhaustiveDependencies: every new message read counts as read too.
  useEffect(() => {
    client.setQueryData<EmployeeChatView[]>(["messages", "employee_chats"], (old) =>
      old?.map((c) => (c.conversation_id === conversationId && c.unread > 0 ? { ...c, unread: 0 } : c)),
    );
  }, [conversationId, client, list.length]);

  const pending = useMemo(
    () => approvalsFor(approvals.data ?? [], { conversationId }),
    [approvals.data, conversationId],
  );
  const approvalMap = useMemo(() => new Map(pending.map((a) => [a.id, a])), [pending]);
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
    }),
    [store, features, user, name, employeesData, approvalMap, list, lastAnswerId, conversationId],
  );

  const running =
    [...list].reverse().find((m) => m.status === "partial" && m.run_id)?.run_id ?? live.at(-1)?.run_id ?? null;
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

  return (
    <section aria-label={frame?.title ?? `Chat with ${name}`} {...stylex.props(styles.screen)}>
      {compact ? null : (
        <header data-titlebar {...stylex.props(styles.top)}>
          <Heading level={1} xstyle={styles.title}>
            {frame?.title || name}
          </Heading>
          <span {...stylex.props(styles.subtitle)}>
            {live.length > 0
              ? `${name} answering`
              : (frame?.subtitle ?? (employee ? (employee.title ?? employee.role) : ""))}
          </span>
          {frame?.actions}
          <InboxBell />
        </header>
      )}
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
                    followOutput={(bottom) => (bottom ? "auto" : false)}
                    computeItemKey={(_, row) => (row.kind === "message" ? row.m.id : row.id)}
                    components={FEED_PARTS}
                    itemContent={(_, row) => (
                      <div {...stylex.props(styles.feedColumn, compact && styles.feedCompact)}>
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
                  ? `${name} is paused: resume them to go on with this ${frame ? "session" : "chat"}.`
                  : `${name} is archived: this ${frame ? "session" : "chat"} can only be read.`}
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
                send={(message) =>
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
                  )
                }
                onSent={() => handle.current?.scrollToIndex({ index: "LAST", align: "end" })}
              />
            )}
          </ChatProvider>
        </div>
        {frame?.aside}
      </div>
    </section>
  );
}
