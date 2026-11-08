import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { Link } from "@astryxdesign/core/Link";
import { List, ListItem } from "@astryxdesign/core/List";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import type { ApprovalView, GuidelineProposalView, NotificationView, ProposalView, TaskView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useVirtualizer } from "@tanstack/react-virtual";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import {
  useApprovals,
  useGuidelineProposals,
  useMessages,
  useNotifications,
  useOpenTasks,
  useProposals,
  useTickets,
} from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { StatusGlyph } from "../ui/icons";
import { KeyHint } from "../ui/key-hint";
import { toast } from "../ui/toast";
import { ApprovalCard, PLAN_TOOL, QUESTION_TOOL } from "./approval-card";
import { taskCounts } from "./board-logic";
import {
  age,
  approvalAction,
  approvalSentence,
  buildInbox,
  filterInbox,
  type InboxFilter,
  type InboxGroup,
  type InboxRow,
  type InboxTone,
} from "./inbox-logic";
import { useFollowLink } from "./links";
import { messageLink } from "./links-logic";
import { Confirm, Loading, Page, PageHead, Problem } from "./org-kit";
import { lastFailure } from "./run-logic";
import { useRetryAnswer } from "./run-tools";
import { TicketDecision } from "./ticket-panel";
import { NoConnectionNote, useNoConnection, useTicketNeeds } from "./tickets-kit";

const enter = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(4px)" },
});

const styles = stylex.create({
  actions: { display: "flex", flexShrink: 0, gap: 6 },
  body: { display: "flex", minHeight: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%" },
  items: {
    width: "clamp(280px, 36%, 400px)",
    minWidth: 0,
    flexShrink: 0,
    borderRightWidth: 1,
    borderRightStyle: "solid",
    borderRightColor: "var(--color-border)",
  },
  message: { padding: 18 },
  detail: { display: "flex", minWidth: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%", flexDirection: "column" },
  detailHead: {
    display: "flex",
    height: 44,
    flexShrink: 0,
    alignItems: "center",
    gap: 8,
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--color-border)",
    paddingInline: 24,
    color: "var(--precision-text3)",
  },
  detailTitle: {
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--color-text-primary)",
  },
  position: {
    marginInlineStart: "auto",
    display: "flex",
    flexShrink: 0,
    alignItems: "center",
    gap: 6,
    fontSize: 12,
    whiteSpace: "nowrap",
  },
  detailScroll: { minHeight: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%", overflowY: "auto" },
  scroller: { height: "100%", overflowY: "auto" },
  track: { position: "relative" },
  groupHead: {
    position: "absolute",
    insetInline: 0,
    top: 0,
    display: "flex",
    alignItems: "center",
    gap: 8,
    borderBlockWidth: 1,
    borderBlockStyle: "solid",
    borderBlockColor: "var(--precision-line2)",
    backgroundColor: "var(--precision-sunk)",
    paddingInline: 18,
    fontSize: 12,
    fontWeight: 500,
    color: "var(--color-text-secondary)",
    listStyleType: "none",
  },
  groupDot: { width: 7, height: 7, borderRadius: "50%" },
  groupCount: { color: "var(--precision-text3)" },
  row: { position: "absolute", insetInline: 6, top: 2, boxSizing: "border-box", height: 54, borderRadius: 6 },
  age: { fontSize: 12, color: "var(--precision-text3)" },
  page: {
    display: "flex",
    maxWidth: 760,
    flexDirection: "column",
    gap: 20,
    paddingInline: { default: 24, "@media (min-width: 1280px)": 40 },
    paddingBlock: 32,
    animationName: enter,
    animationDuration: "160ms",
    animationTimingFunction: "cubic-bezier(0.2, 0, 0, 1)",
  },
  pageWide: { gap: 24 },
  waitingTag: {
    display: "inline-flex",
    height: 22,
    alignItems: "center",
    gap: 6,
    borderRadius: 11,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-warning)",
    backgroundColor: "var(--color-warning-muted)",
    paddingInline: 9,
    fontSize: 12,
    fontWeight: 500,
    color: "var(--color-text-warning)",
  },
  sentence: {
    marginTop: 14,
    marginBottom: 0,
    fontSize: 22,
    lineHeight: 1.3,
    fontWeight: 600,
    letterSpacing: "-0.015em",
  },
  rowTitle: { margin: 0, fontSize: 22, lineHeight: 1.3, fontWeight: 600, letterSpacing: "-0.015em" },
  description: {
    marginTop: 8,
    marginBottom: 0,
    fontSize: 14,
    lineHeight: 1.6,
    color: "var(--color-text-secondary)",
  },
  meta: { margin: 0, fontSize: 14, lineHeight: 1.6, color: "var(--color-text-secondary)" },
  reasons: { margin: 0, listStyleType: "none", padding: 0, color: "var(--color-text-secondary)" },
  buttons: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 8 },
  openRun: { alignSelf: "flex-start", fontSize: 12 },
  facts: {
    margin: 0,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    paddingTop: 16,
  },
  fact: { display: "grid", minHeight: 30, gridTemplateColumns: "110px minmax(0,1fr)", alignItems: "center" },
  factKey: { color: "var(--precision-text3)" },
  factValue: { margin: 0, overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis" },
  prose: { display: "flex", flexDirection: "column", gap: 6 },
  proseTitle: { fontSize: 12, fontWeight: 500, color: "var(--precision-text3)" },
  proseText: { margin: 0, fontSize: 14, lineHeight: 1.6, whiteSpace: "pre-wrap" },
  frameHead: { display: "flex", flexDirection: "column", gap: 8 },
});

const TONES: Record<InboxTone, string> = {
  amber: "var(--color-text-warning)",
  violet: "var(--precision-violet)",
  blue: "var(--color-text-info)",
  green: "var(--color-text-accent)",
  muted: "var(--precision-text3)",
  red: "var(--color-text-error)",
};

const FILTERS: { key: InboxFilter; label: string }[] = [
  { key: "all", label: "All" },
  { key: "decisions", label: "Decisions" },
  { key: "reviews", label: "Reviews" },
  { key: "mentions", label: "Mentions" },
];

export function useInbox() {
  const approvals = useApprovals();
  const proposals = useProposals();
  const guidelineProposals = useGuidelineProposals();
  const tasks = useOpenTasks();
  const notifications = useNotifications();
  const { features } = useLinkState();
  const responsible = features.includes("ticket_responsible");
  const tickets = useTickets(responsible);
  const need = useTicketNeeds(tickets.data ?? []);
  const groups = useMemo(
    () =>
      buildInbox({
        approvals: approvals.data ?? [],
        proposals: proposals.data ?? [],
        guidelineProposals: guidelineProposals.data ?? [],
        // Freeze §5: a ticket's main task only when its ticket needs this board member.
        tasks: (tasks.data ?? []).filter((t) =>
          taskCounts(
            t,
            responsible,
            tickets.data?.find((x) => x.id === t.ticket_id),
            need.mine,
          ),
        ),
        notifications: notifications.data ?? [],
      }),
    [
      approvals.data,
      proposals.data,
      guidelineProposals.data,
      tasks.data,
      notifications.data,
      responsible,
      tickets.data,
      need,
    ],
  );
  const loading = approvals.isPending || notifications.isPending || tasks.isPending;
  const error = approvals.error ?? notifications.error ?? tasks.error ?? null;
  return { groups, loading, error };
}

type Entry = { kind: "header"; group: InboxGroup } | { kind: "row"; row: InboxRow };

const HEADER_HEIGHT = 32;
const ROW_HEIGHT = 58;

function InboxList({
  groups,
  selected,
  onSelect,
}: {
  groups: InboxGroup[];
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const items = useMemo<Entry[]>(
    () =>
      groups.flatMap((group) => [
        { kind: "header", group } as Entry,
        ...group.rows.map((row): Entry => ({ kind: "row", row })),
      ]),
    [groups],
  );
  const virtual = useVirtualizer({
    count: items.length,
    getScrollElement: () => scroller.current,
    estimateSize: (i) => (items[i]?.kind === "header" ? HEADER_HEIGHT : ROW_HEIGHT),
    overscan: 12,
    initialRect: { width: 400, height: 800 },
  });
  // J and K, and a row handed over after a decision, stay in view.
  const selectedIndex = items.findIndex((item) => item.kind === "row" && item.row.id === selected);
  // biome-ignore lint/correctness/useExhaustiveDependencies: only a new selection scrolls.
  useEffect(() => {
    if (selectedIndex >= 0) virtual.scrollToIndex(selectedIndex, { align: "auto" });
  }, [selectedIndex]);
  const now = Date.now();
  return (
    <div ref={scroller} {...stylex.props(styles.scroller)}>
      <List density="compact" xstyle={styles.track} style={{ height: virtual.getTotalSize() }}>
        {virtual.getVirtualItems().map((v) => {
          const item = items[v.index] as Entry;
          const place = { transform: `translateY(${v.start}px)`, height: v.size };
          if (item.kind === "header") {
            return (
              <li key={item.group.key} style={place} {...stylex.props(styles.groupHead)}>
                <span {...stylex.props(styles.groupDot)} style={{ background: TONES[item.group.tone] }} />
                {item.group.label}
                <span {...stylex.props(styles.groupCount)}>{item.group.rows.length}</span>
              </li>
            );
          }
          const { row } = item;
          return (
            <ListItem
              key={row.id}
              xstyle={styles.row}
              style={{ transform: `translateY(${v.start}px)` }}
              label={row.title}
              description={row.meta}
              startContent={<StatusGlyph glyph={row.glyph} color={TONES[row.tone]} />}
              endContent={<span {...stylex.props(styles.age)}>{age(row.atMs, now)}</span>}
              isSelected={row.id === selected}
              onClick={() => onSelect(row.id)}
            />
          );
        })}
      </List>
    </div>
  );
}

function ApprovalDetail({ approval }: { approval: ApprovalView }) {
  // A question and a plan need their answer, not Allow or Deny.
  if (approval.tool === QUESTION_TOOL || approval.tool === PLAN_TOOL) {
    return (
      <div {...stylex.props(styles.page)} key={approval.id}>
        <ApprovalCard approval={approval} keyed />
      </div>
    );
  }
  return <ActionDetail approval={approval} />;
}

function ActionDetail({ approval }: { approval: ApprovalView }) {
  const action = approvalAction(approval);
  return (
    <div {...stylex.props(styles.page, styles.pageWide)} key={approval.id}>
      <div>
        <div {...stylex.props(styles.waitingTag)}>Waiting on you · run paused</div>
        <Heading level={2} xstyle={styles.sentence}>
          {approvalSentence(approval)}
        </Heading>
        {action.description ? <p {...stylex.props(styles.description)}>{action.description}</p> : null}
      </div>
      {approval.reasons.length > 0 ? (
        <ul {...stylex.props(styles.reasons)}>
          {approval.reasons.map((reason) => (
            <li key={reason}>{reason}</li>
          ))}
        </ul>
      ) : null}
      <ApprovalCard approval={approval} keyed />
      {approval.task_id ? (
        <Link href={`/tasks/${approval.task_id}`} xstyle={styles.openRun}>
          Open run →
        </Link>
      ) : null}
      <dl {...stylex.props(styles.facts)}>
        {[
          ["Agent", approval.requester],
          ["Tool", action.label],
          ["Effect", approval.effect],
          ["Runs on", approval.account],
          ...approval.resources.slice(0, 3).map((r): [string, string] => ["Resource", r]),
        ].map(([key, value]) => (
          <div key={`${key}${value}`} {...stylex.props(styles.fact)}>
            <dt {...stylex.props(styles.factKey)}>{key}</dt>
            <dd {...stylex.props(styles.factValue)}>{value}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

function Prose({ title, children }: { title: string; children: string }) {
  return (
    <div {...stylex.props(styles.prose)}>
      <span {...stylex.props(styles.proseTitle)}>{title}</span>
      <p {...stylex.props(styles.proseText)}>{children}</p>
    </div>
  );
}

function DetailFrame({ row, children }: { row: InboxRow; children: ReactNode }) {
  return (
    <div {...stylex.props(styles.page)} key={row.id}>
      <div {...stylex.props(styles.frameHead)}>
        <Heading level={2} xstyle={styles.rowTitle}>
          {row.title}
        </Heading>
        <p {...stylex.props(styles.meta)}>{row.meta}</p>
      </div>
      {children}
    </div>
  );
}

function NotificationDetail({ row, n }: { row: InboxRow; n: NotificationView }) {
  const { hostId, features } = useLinkState();
  const follow = useFollowLink();
  const connection = useConnection();
  const client = useQueryClient();
  const navigate = useNavigate();
  const act = async (method: "notification.mark_read" | "notification.dismiss") => {
    try {
      await connection.write(method, { id: n.id });
      await client.invalidateQueries({ queryKey: ["notifications"] });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  const open = () => {
    // A mention or a message outside a ticket: right to that message.
    const inDiscussion = n.reason === "mention" || n.reason === "sent_back";
    if (n.message_id && hostId && follow && (!n.ticket_id || inDiscussion) && features.includes("message_locate")) {
      follow(messageLink(hostId, n.message_id));
    } else if (n.ticket_id) void navigate({ to: "/board", search: { ticket: n.ticket_id } });
    else if (n.task_id) void navigate({ to: "/tasks/$taskId", params: { taskId: n.task_id } });
    else if (n.conversation_id) void navigate({ to: "/chat", search: { channel: n.conversation_id } });
  };
  useAction("OpenSelected", open, Boolean(n.ticket_id || n.task_id || n.conversation_id));
  return (
    <DetailFrame row={row}>
      {n.ticket_id && (n.reason === "plan_review" || n.reason === "review") ? (
        <TicketDecision id={n.ticket_id} />
      ) : n.reason ? (
        <Prose title="Why you see this">{reasonText(n.reason)}</Prose>
      ) : null}
      <div {...stylex.props(styles.buttons)}>
        {n.ticket_id || n.task_id || n.conversation_id ? (
          <Button
            variant="primary"
            label={n.ticket_id ? "Open ticket" : n.task_id ? "Open task" : "Open chat"}
            onClick={open}
          >
            {n.ticket_id ? "Open ticket" : n.task_id ? "Open task" : "Open chat"} <KeyHint onAccent>↵</KeyHint>
          </Button>
        ) : null}
        {n.read ? null : <Button label="Mark read" onClick={() => void act("notification.mark_read")} />}
        <Button label="Dismiss" onClick={() => void act("notification.dismiss")} />
      </div>
    </DetailFrame>
  );
}

function reasonText(reason: string): string {
  const known: Record<string, string> = {
    responsible: "You are responsible for this ticket.",
    mention: "Someone mentioned you.",
    sent_back: "This was sent back.",
    failed: "The main task failed.",
    login: "It waits for your Claude login.",
  };
  return known[reason] ?? reason;
}

function TaskDetail({ row, task }: { row: InboxRow; task: TaskView }) {
  const navigate = useNavigate();
  const open = () => void navigate({ to: "/tasks/$taskId", params: { taskId: task.id } });
  useAction("OpenSelected", open);
  const stopped = task.waiting_reason === "input";
  const chat = useMessages(stopped ? task.conversation_id : null);
  const list = chat.data?.messages ?? [];
  const failure = lastFailure(list);
  const retry = useRetryAnswer(task.conversation_id, list, { employeeId: task.employee_id ?? null });
  const noConnection = useNoConnection();
  return (
    <DetailFrame row={row}>
      {task.ticket_id && task.waiting_reason === "plan_review" ? <TicketDecision id={task.ticket_id} /> : null}
      {failure ? <Prose title="Why it stopped">{failure}</Prose> : null}
      {failure && noConnection ? <NoConnectionNote /> : null}
      <Prose title="Goal">{task.goal}</Prose>
      <div {...stylex.props(styles.buttons)}>
        <Button variant="primary" label="Open run" onClick={open}>
          Open run <KeyHint onAccent>↵</KeyHint>
        </Button>
        {failure ? <Button label="Try again" onClick={retry} /> : null}
      </div>
    </DetailFrame>
  );
}

function ProposalDetail({ row, proposal }: { row: InboxRow; proposal: ProposalView }) {
  const connection = useConnection();
  const client = useQueryClient();
  const decide = async (approve: boolean) => {
    try {
      await connection.write("proposal.decide", { proposal_id: proposal.id, approve }, { retry: false });
      await client.invalidateQueries({ queryKey: ["proposals"] });
      toast(approve ? "The proposal is approved." : "The proposal is rejected.");
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  useKeyContext("Decision");
  useAction("AllowOnce", () => void decide(true));
  useAction("RejectApproval", () => void decide(false));
  return (
    <DetailFrame row={row}>
      {proposal.reason ? <Prose title="Why">{proposal.reason}</Prose> : null}
      <Prose title="Instructions">{proposal.instructions}</Prose>
      <div {...stylex.props(styles.buttons)}>
        <Button variant="primary" label="Approve" onClick={() => void decide(true)} />
        <Button variant="destructive" label="Reject" onClick={() => void decide(false)} />
      </div>
    </DetailFrame>
  );
}

function GuidelineDetail({ row, proposal }: { row: InboxRow; proposal: GuidelineProposalView }) {
  const connection = useConnection();
  const client = useQueryClient();
  const decide = async (approve: boolean) => {
    try {
      // Approving a change holds only if the guideline is still the version it was proposed on.
      await connection.write(
        "guideline.decide_proposal",
        { proposal_id: proposal.id, approve },
        {
          retry: false,
          ...(approve && proposal.base_version != null ? { expectedVersion: proposal.base_version } : {}),
        },
      );
      await client.invalidateQueries({ queryKey: ["guideline_proposals"] });
      toast(approve ? "The guideline proposal is approved." : "The guideline proposal is rejected.");
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  useKeyContext("Decision");
  useAction("AllowOnce", () => void decide(true));
  useAction("RejectApproval", () => void decide(false));
  return (
    <DetailFrame row={row}>
      <div {...stylex.props(styles.buttons)}>
        <Button variant="primary" label="Approve" onClick={() => void decide(true)} />
        <Button
          label="Edit and approve…"
          href={`/knowledge/guidelines/${proposal.guideline_id ?? "new"}?proposal=${proposal.id}`}
        />
        <Button variant="destructive" label="Reject" onClick={() => void decide(false)} />
      </div>
    </DetailFrame>
  );
}

export function InboxScreen() {
  const { groups, loading, error } = useInbox();
  const [filter, setFilter] = useState<InboxFilter>("all");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const shown = useMemo(() => filterInbox(groups, filter), [groups, filter]);
  const rows = useMemo(() => shown.flatMap((g) => g.rows), [shown]);
  // A row that went away (decided) hands over to the first one.
  const selected = rows.find((r) => r.id === selectedId) ?? rows[0] ?? null;
  const at = selected ? rows.indexOf(selected) : -1;

  useKeyContext("Inbox");
  const move = (by: number) => {
    const next = rows[at + by];
    if (next) setSelectedId(next.id);
  };
  useAction("SelectNext", () => move(1));
  useAction("SelectPrev", () => move(-1));
  useAction("Filter1", () => setFilter("all"));
  useAction("Filter2", () => setFilter("decisions"));
  useAction("Filter3", () => setFilter("reviews"));
  useAction("Filter4", () => setFilter("mentions"));
  const connection = useConnection();
  const client = useQueryClient();
  const notifications = useNotifications().data ?? [];
  const unread = notifications.some((n) => !n.read);
  const clearable = notifications.some((n) => !n.needs_you);
  const [clearing, setClearing] = useState(false);
  const all = async (method: "notification.mark_all_read" | "notification.dismiss_all") => {
    try {
      await connection.write(method, undefined, { retry: false });
      await client.invalidateQueries({ queryKey: ["notifications"] });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };

  return (
    <Page>
      <PageHead
        title="Inbox"
        tabs={
          <SegmentedControl label="Filter" size="sm" value={filter} onChange={(next) => setFilter(next as InboxFilter)}>
            {FILTERS.map((f) => (
              <SegmentedControlItem key={f.key} value={f.key} label={f.label} />
            ))}
          </SegmentedControl>
        }
      >
        <span {...stylex.props(styles.actions)}>
          {unread ? <Button label="Mark all read" onClick={() => void all("notification.mark_all_read")} /> : null}
          {clearable ? <Button label="Clear all" onClick={() => setClearing(true)} /> : null}
        </span>
      </PageHead>
      {clearing ? (
        <Confirm
          title="Clear all results?"
          body="Every result in your notifications is removed. Approvals waiting for you are not touched."
          action="Clear all"
          danger
          onClose={() => setClearing(false)}
          onConfirm={() => {
            setClearing(false);
            void all("notification.dismiss_all");
          }}
        />
      ) : null}
      <div {...stylex.props(styles.body)}>
        <section aria-label="Inbox items" {...stylex.props(styles.items)}>
          {error ? (
            <div {...stylex.props(styles.message)}>
              <Problem>{error.message}</Problem>
            </div>
          ) : loading ? (
            <Loading rows={6} />
          ) : rows.length === 0 ? (
            <div {...stylex.props(styles.message)}>
              <EmptyState isCompact title="Nothing waits for you." />
            </div>
          ) : (
            <InboxList groups={shown} selected={selected?.id ?? null} onSelect={setSelectedId} />
          )}
        </section>
        <section aria-label="Detail" {...stylex.props(styles.detail)}>
          <div {...stylex.props(styles.detailHead)}>
            <span {...stylex.props(styles.detailTitle)}>{selected?.title ?? ""}</span>
            {rows.length > 0 ? (
              <span {...stylex.props(styles.position)}>
                {at + 1} / {rows.length} <KeyHint>J</KeyHint>
                <KeyHint>K</KeyHint>
              </span>
            ) : null}
          </div>
          <div {...stylex.props(styles.detailScroll)}>
            {selected?.source.kind === "approval" ? (
              <ApprovalDetail key={selected.id} approval={selected.source.approval} />
            ) : selected?.source.kind === "notification" ? (
              <NotificationDetail key={selected.id} row={selected} n={selected.source.notification} />
            ) : selected?.source.kind === "task" ? (
              <TaskDetail key={selected.id} row={selected} task={selected.source.task} />
            ) : selected?.source.kind === "proposal" ? (
              <ProposalDetail key={selected.id} row={selected} proposal={selected.source.proposal} />
            ) : selected?.source.kind === "guideline" ? (
              <GuidelineDetail key={selected.id} row={selected} proposal={selected.source.proposal} />
            ) : null}
          </div>
        </section>
      </div>
    </Page>
  );
}
