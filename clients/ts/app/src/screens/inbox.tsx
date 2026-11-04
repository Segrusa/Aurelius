import { ProtocolError } from "@aurelius/client";
import type { ApprovalView } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import { useConnection } from "../connection";
import { useApprovals, useGuidelineProposals, useNotifications, useOpenTasks, useProposals } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { StatusGlyph } from "../ui/icons";
import { Button, Kbd, Segments } from "../ui/kit";
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
  const groups = useMemo(
    () =>
      buildInbox({
        approvals: approvals.data ?? [],
        proposals: proposals.data ?? [],
        guidelineProposals: guidelineProposals.data ?? [],
        tasks: tasks.data ?? [],
        notifications: notifications.data ?? [],
      }),
    [approvals.data, proposals.data, guidelineProposals.data, tasks.data, notifications.data],
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

function PlainDetail({ row }: { row: InboxRow }) {
  return (
    <div className="enter flex max-w-[760px] flex-col gap-2 px-6 py-8 xl:px-10" key={row.id}>
      <h2 className="m-0 text-[22px] leading-[1.3] font-semibold tracking-[-0.015em]">{row.title}</h2>
      <p className="m-0 text-[14px] leading-[1.6] text-text2">{row.meta}</p>
    </div>
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
            ) : selected ? (
              <PlainDetail row={selected} />
            ) : null}
          </div>
        </section>
      </div>
    </>
  );
}
