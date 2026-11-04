import { ProtocolError } from "@aurelius/client";
import type { ApprovalView, GuidelineProposalView, NotificationView, ProposalView, TaskView } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useVirtualizer } from "@tanstack/react-virtual";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useApprovals, useGuidelineProposals, useNotifications, useOpenTasks, useProposals, useTickets } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { StatusGlyph } from "../ui/icons";
import { Button, Kbd, Segments } from "../ui/kit";
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
import { useTicketNeeds } from "./tickets-kit";

const TONES: Record<InboxTone, string> = {
  amber: "var(--amber)",
  violet: "var(--violet)",
  blue: "var(--blue)",
  green: "var(--acc-text)",
  muted: "var(--text3)",
  red: "var(--red)",
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

type ListItem = { kind: "header"; group: InboxGroup } | { kind: "row"; row: InboxRow };

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
  const items = useMemo<ListItem[]>(
    () =>
      groups.flatMap((group) => [
        { kind: "header", group } as ListItem,
        ...group.rows.map((row): ListItem => ({ kind: "row", row })),
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
    <div ref={scroller} className="h-full overflow-y-auto">
      <div className="relative" style={{ height: virtual.getTotalSize() }}>
        {virtual.getVirtualItems().map((v) => {
          const item = items[v.index] as ListItem;
          const place = { transform: `translateY(${v.start}px)`, height: v.size };
          if (item.kind === "header") {
            return (
              <div
                key={item.group.key}
                style={place}
                className="absolute inset-x-0 top-0 flex items-center gap-2 border-y border-line2 bg-sunk px-[18px] text-[12px] font-medium text-text2"
              >
                <span className="size-[7px] rounded-full" style={{ background: TONES[item.group.tone] }} />
                {item.group.label}
                <span className="text-text3">{item.group.rows.length}</span>
              </div>
            );
          }
          const { row } = item;
          const on = row.id === selected;
          return (
            <div key={row.id} style={place} className="absolute inset-x-0 top-0 flex items-center px-[6px]">
              <button
                type="button"
                onClick={() => onSelect(row.id)}
                aria-current={on}
                className={`flex h-[54px] w-full cursor-pointer items-center gap-[10px] rounded-[6px] py-[10px] pr-[14px] pl-3 text-left text-text ${on ? "bg-sel shadow-[inset_2px_0_0_var(--acc-text)]" : "hover:bg-sel"}`}
              >
                <StatusGlyph glyph={row.glyph} color={TONES[row.tone]} />
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-medium">{row.title}</span>
                  <span className="mt-[2px] block truncate text-[12px] text-text3">{row.meta}</span>
                </span>
                <span className="text-[12px] text-text3">{age(row.atMs, now)}</span>
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function ApprovalDetail({ approval }: { approval: ApprovalView }) {
  // A question and a plan need their answer, not Allow or Deny.
  if (approval.tool === QUESTION_TOOL || approval.tool === PLAN_TOOL) {
    return (
      <div className="enter flex max-w-[760px] flex-col gap-5 px-6 py-8 xl:px-10" key={approval.id}>
        <ApprovalCard approval={approval} />
      </div>
    );
  }
  return <ActionDetail approval={approval} />;
}

function ActionDetail({ approval }: { approval: ApprovalView }) {
  const connection = useConnection();
  const client = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const action = approvalAction(approval);
  const eligible = approval.always_allow_eligible ?? false;

  const decide = async (approve: boolean, alwaysAllow = false) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      // Never resent after a reconnect: a decision is only made while the person sees it.
      await connection.write(
        "approval.decide",
        { approval_id: approval.id, action_digest: approval.action_digest, approve, always_allow: alwaysAllow },
        { retry: false },
      );
      await client.invalidateQueries({ queryKey: ["approvals"] });
    } catch (e) {
      setError(e instanceof ProtocolError || e instanceof Error ? e.message : "This did not work.");
      setBusy(false);
    }
  };

  useAction("AllowOnce", () => void decide(true));
  useAction("AlwaysAllow", () => void decide(true, true), eligible);
  useAction("DenyApproval", () => void decide(false));

  return (
    <div className="enter flex max-w-[760px] flex-col gap-6 px-6 py-8 xl:px-10" key={approval.id}>
      <div>
        <div className="inline-flex h-[22px] items-center gap-[6px] rounded-[11px] border border-amber-line bg-amber-soft px-[9px] text-[12px] font-medium text-amber">
          Waiting on you · run paused
        </div>
        <h2 className="mt-[14px] mb-0 text-[22px] leading-[1.3] font-semibold tracking-[-0.015em]">
          {approvalSentence(approval)}
        </h2>
        {action.description ? (
          <p className="mt-2 mb-0 text-[14px] leading-[1.6] text-text2">{action.description}</p>
        ) : null}
      </div>
      <div className="overflow-hidden rounded-[8px] border border-line bg-sunk">
        <div className="flex justify-between border-b border-line2 px-3 py-2 text-[12px] text-text3">
          <span>{action.label}</span>
          <span className="text-amber">{approval.effect}</span>
        </div>
        <pre className="m-0 max-h-[320px] overflow-auto p-[14px] font-mono text-[13px] whitespace-pre-wrap">
          {action.body}
        </pre>
      </div>
      {approval.reasons.length > 0 ? (
        <ul className="m-0 list-none p-0 text-text2">
          {approval.reasons.map((reason) => (
            <li key={reason}>{reason}</li>
          ))}
        </ul>
      ) : null}
      <div className="flex flex-wrap gap-2">
        <Button kind="primary" disabled={busy} onClick={() => void decide(true)}>
          Allow once <Kbd onAccent>↵</Kbd>
        </Button>
        {approval.task_id ? (
          <Link
            to="/tasks/$taskId"
            params={{ taskId: approval.task_id }}
            className="order-last ml-auto self-center text-[12px] text-acc-text no-underline"
          >
            Open run →
          </Link>
        ) : null}
        {eligible ? (
          <Button disabled={busy} onClick={() => void decide(true, true)}>
            Always allow <Kbd>⇧↵</Kbd>
          </Button>
        ) : null}
        <Button kind="danger" disabled={busy} onClick={() => void decide(false)}>
          Deny <Kbd>D</Kbd>
        </Button>
      </div>
      {error ? (
        <p role="alert" className="m-0 text-red">
          {error}
        </p>
      ) : null}
      <dl className="m-0 border-t border-line2 pt-4">
        {[
          ["Agent", approval.requester],
          ["Tool", action.label],
          ["Runs on", approval.account],
          ...approval.resources.slice(0, 3).map((r): [string, string] => ["Resource", r]),
        ].map(([key, value]) => (
          <div key={`${key}${value}`} className="grid min-h-[30px] grid-cols-[110px_minmax(0,1fr)] items-center">
            <dt className="text-text3">{key}</dt>
            <dd className="m-0 truncate">{value}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

function Prose({ title, children }: { title: string; children: string }) {
  return (
    <div className="flex flex-col gap-[6px]">
      <span className="text-[12px] font-medium text-text3">{title}</span>
      <p className="m-0 text-[14px] leading-[1.6] whitespace-pre-wrap">{children}</p>
    </div>
  );
}

function DetailFrame({ row, children }: { row: InboxRow; children: ReactNode }) {
  return (
    <div className="enter flex max-w-[760px] flex-col gap-5 px-6 py-8 xl:px-10" key={row.id}>
      <div className="flex flex-col gap-2">
        <h2 className="m-0 text-[22px] leading-[1.3] font-semibold tracking-[-0.015em]">{row.title}</h2>
        <p className="m-0 text-[14px] leading-[1.6] text-text2">{row.meta}</p>
      </div>
      {children}
    </div>
  );
}

function NotificationDetail({ row, n }: { row: InboxRow; n: NotificationView }) {
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
    if (n.ticket_id) void navigate({ to: "/board", search: { ticket: n.ticket_id } });
    else if (n.task_id) void navigate({ to: "/tasks/$taskId", params: { taskId: n.task_id } });
    else if (n.conversation_id) void navigate({ to: "/chat", search: { channel: n.conversation_id } });
  };
  useAction("OpenSelected", open, Boolean(n.ticket_id || n.task_id || n.conversation_id));
  return (
    <DetailFrame row={row}>
      {n.reason ? <Prose title="Why you see this">{n.reason}</Prose> : null}
      <div className="flex flex-wrap gap-2">
        {n.ticket_id || n.task_id || n.conversation_id ? (
          <Button kind="primary" onClick={open}>
            {n.ticket_id ? "Open ticket" : n.task_id ? "Open task" : "Open chat"} <Kbd onAccent>↵</Kbd>
          </Button>
        ) : null}
        {n.read ? null : <Button onClick={() => void act("notification.mark_read")}>Mark read</Button>}
        <Button onClick={() => void act("notification.dismiss")}>Dismiss</Button>
      </div>
    </DetailFrame>
  );
}

function TaskDetail({ row, task }: { row: InboxRow; task: TaskView }) {
  const navigate = useNavigate();
  const open = () => void navigate({ to: "/tasks/$taskId", params: { taskId: task.id } });
  useAction("OpenSelected", open);
  return (
    <DetailFrame row={row}>
      <Prose title="Goal">{task.goal}</Prose>
      <div>
        <Button kind="primary" onClick={open}>
          Open run <Kbd onAccent>↵</Kbd>
        </Button>
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
  return (
    <DetailFrame row={row}>
      {proposal.reason ? <Prose title="Why">{proposal.reason}</Prose> : null}
      <Prose title="Instructions">{proposal.instructions}</Prose>
      <div className="flex flex-wrap gap-2">
        <Button kind="primary" onClick={() => void decide(true)}>
          Approve
        </Button>
        <Button kind="danger" onClick={() => void decide(false)}>
          Reject
        </Button>
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
  return (
    <DetailFrame row={row}>
      <div className="flex flex-wrap gap-2">
        <Button kind="primary" onClick={() => void decide(true)}>
          Approve
        </Button>
        <Link
          to="/knowledge/guidelines/$id"
          params={{ id: proposal.guideline_id ?? "new" }}
          search={{ proposal: proposal.id }}
          className="inline-flex h-8 items-center rounded-[6px] border border-btn-line bg-btn px-3 text-text no-underline hover:bg-sel"
        >
          Edit and approve…
        </Link>
        <Button kind="danger" onClick={() => void decide(false)}>
          Reject
        </Button>
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

  return (
    <>
      <header className="flex h-11 flex-none items-center gap-[10px] border-b border-line pr-3 pl-[18px]">
        <h1 className="m-0 text-[13px] font-medium">Inbox</h1>
        <div className="ml-2">
          <Segments label="Filter" value={filter} options={FILTERS} onChange={setFilter} />
        </div>
      </header>
      <div className="flex min-h-0 flex-1">
        <section aria-label="Inbox items" className="w-[clamp(280px,36%,400px)] min-w-0 flex-none border-r border-line">
          {error ? (
            <p role="alert" className="m-0 p-[18px] text-red">
              {error.message}
            </p>
          ) : loading ? null : rows.length === 0 ? (
            <p className="m-0 p-[18px] text-text3">Nothing waits for you.</p>
          ) : (
            <InboxList groups={shown} selected={selected?.id ?? null} onSelect={setSelectedId} />
          )}
        </section>
        <section aria-label="Detail" className="flex min-w-0 flex-1 flex-col">
          <div className="flex h-11 flex-none items-center gap-2 border-b border-line px-6 text-text3">
            <span className="truncate text-text">{selected?.title ?? ""}</span>
            {rows.length > 0 ? (
              <span className="ml-auto flex flex-none items-center gap-[6px] text-[12px] whitespace-nowrap">
                {at + 1} / {rows.length} <Kbd>J</Kbd>
                <Kbd>K</Kbd>
              </span>
            ) : null}
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
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
    </>
  );
}
