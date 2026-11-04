import { ProtocolError } from "@aurelius/client";
import type { ChannelView, EmployeeSummary, MessageView } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useRef, useState } from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { useConnection, useLinkState } from "../connection";
import { useApprovals, useChannelFeed, useChannels, useEmployees, useThread, useTicket } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { SearchIcon } from "../ui/icons";
import { Button, initial, Kbd, Mark, markColor } from "../ui/kit";
import { toast } from "../ui/toast";
import { ApprovalCard } from "./approval-card";
import { useLiveAll, useLiveStore } from "./chat-live";
import {
  bareName,
  before,
  type Caps,
  channelLabel,
  completeMention,
  keepOrder,
  localDay,
  type Mentionable,
  mentionSuggestions,
  newMessagesAt,
  ownerAnswers,
  quickReplies,
  readOnlyBar,
  replies,
  replyLabel,
  runAuthor,
  sections,
  shownCount,
  stepChannel,
  streamTo,
  unknownMentions,
  unknownWarning,
  visibleChannels,
} from "./chat-logic";
import { type ChatContext, ChatProvider, DayLine, FeedIdsProvider, LiveRow, MessageRow, NewLine } from "./chat-message";
import { Held } from "./dialogs";
import { LoadBanner, useRetryAnswer } from "./run-tools";
import { ColumnGlyph } from "./tickets-kit";

const FEED_PARTS = {
  Header: function FeedTop() {
    return <div className="h-[14px]" />;
  },
  Footer: function FeedEnd() {
    return <div className="h-4" />;
  },
};

/** Unsent text per channel, for as long as the app runs. */
const drafts = new Map<string, string>();
const offset = () => -new Date().getTimezoneOffset();

function usePeople(): { people: Mentionable[]; archived: string[]; first: string | null } {
  const employees = useEmployees();
  return useMemo(() => {
    const all: EmployeeSummary[] = employees.data ?? [];
    const active = all.filter((e) => e.status === "active");
    const roots = active.filter((e) => !e.reports_to);
    const rest = active.filter((e) => e.reports_to);
    return {
      people: [...roots, ...rest].map((e) => ({ name: e.name, kind: e.title ?? e.role, employee: true })),
      archived: all.filter((e) => e.status === "archived").map((e) => e.name),
      first: roots[0]?.name ?? null,
    };
  }, [employees.data]);
}

function ChannelRow({ c, on, onOpen }: { c: ChannelView; on: boolean; onOpen: (c: ChannelView) => void }) {
  const unread = !on && c.unread > 0;
  return (
    <button
      type="button"
      aria-current={on ? "true" : undefined}
      aria-label={channelLabel(c)}
      onClick={() => onOpen(c)}
      className={`flex h-7 w-full cursor-pointer items-center gap-2 rounded-[6px] border-0 px-2 text-left ${on ? "bg-sel text-text" : unread ? "bg-transparent text-text" : "bg-transparent text-text2 hover:text-text"} ${unread ? "font-semibold" : ""}`}
    >
      <span className="text-text3">#</span>
      <span className="min-w-0 flex-1 truncate">{bareName(c)}</span>
      {c.archived ? <span className="text-[11px] text-text3">read-only</span> : null}
      {on || c.archived ? null : c.mentions > 0 ? (
        <span className="text-[12px] font-semibold text-amber">@{c.mentions}</span>
      ) : c.unread > 0 ? (
        <span className="text-[12px] text-text2">{c.unread}</span>
      ) : null}
    </button>
  );
}

function ChannelList({
  channels,
  open,
  onOpen,
}: {
  channels: ChannelView[];
  open: string | null;
  onOpen: (c: ChannelView) => void;
}) {
  const connection = useConnection();
  const { features } = useLinkState();
  const [query, setQuery] = useState("");
  const [found, setFound] = useState<ChannelView[] | null>(null);
  const [ticketsAll, setTicketsAll] = useState(false);
  const [archivedOpen, setArchivedOpen] = useState(false);
  const [archivedAll, setArchivedAll] = useState(false);
  const [frozen, setFrozen] = useState<string[] | null>(null);
  const s = useMemo(() => sections(channels), [channels]);
  const tickets = frozen ? keepOrder(s.tickets, frozen) : s.tickets;
  const openId = open ?? s.general?.conversation_id ?? null;

  useEffect(() => {
    const text = query.trim();
    if (text === "") return setFound(null);
    const local = () => channels.filter((c) => bareName(c).toLowerCase().includes(text.toLowerCase()));
    if (!features.includes("ticket_channels")) return setFound(local());
    let alive = true;
    const timer = setTimeout(() => {
      connection
        .call("channel.search", { query: text })
        .then((body) => {
          if (alive) setFound(body.type === "channels" ? body.value.channels : local());
        })
        .catch(() => {
          if (alive) setFound(local());
        });
    }, 150);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [query, channels, connection, features]);

  const eyebrow = "m-0 mb-1 px-2 text-[12px] font-medium text-text3";
  const more = "cursor-pointer border-0 bg-transparent px-2 py-1 text-left text-[12px] text-text3 hover:text-text";
  const row = (c: ChannelView) => (
    <ChannelRow key={c.conversation_id} c={c} on={c.conversation_id === openId} onOpen={onOpen} />
  );
  return (
    <section
      aria-label="Channels"
      className="flex w-[220px] flex-none flex-col gap-[14px] overflow-y-auto border-r border-line bg-sunk px-2 py-[10px]"
      onPointerEnter={() => setFrozen(s.tickets.map((c) => c.conversation_id))}
      onPointerLeave={() => setFrozen(null)}
    >
      <label className="flex h-[30px] flex-none items-center gap-[6px] rounded-[6px] border border-line px-2 text-text3">
        <SearchIcon />
        <input
          aria-label="Search channels"
          placeholder="Search channels"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="min-w-0 flex-1 border-0 bg-transparent text-text outline-none placeholder:text-text3"
        />
      </label>
      {found ? (
        <div className="flex flex-col gap-[1px]">
          <p className={eyebrow}>Results</p>
          {found.length === 0 ? <p className="m-0 px-2 text-text3">No channel matches.</p> : found.map(row)}
        </div>
      ) : (
        <>
          <div className="flex flex-col gap-[1px]">
            <p className={eyebrow}>Channels</p>
            {s.general ? row(s.general) : null}
            {s.projects.map(row)}
          </div>
          {tickets.length > 0 ? (
            <div className="flex flex-col gap-[1px]">
              <p className={eyebrow}>Tickets</p>
              {tickets.slice(0, shownCount(tickets.length, ticketsAll)).map(row)}
              {!ticketsAll && tickets.length > 15 ? (
                <button type="button" className={more} onClick={() => setTicketsAll(true)}>
                  Show more ({tickets.length - 15})
                </button>
              ) : null}
            </div>
          ) : null}
          {s.archived.length > 0 ? (
            <div className="flex flex-col gap-[1px]">
              <button
                type="button"
                aria-expanded={archivedOpen}
                className={more}
                onClick={() => setArchivedOpen(!archivedOpen)}
              >
                {archivedOpen ? "▾" : "▸"} Archived ({s.archived.length})
              </button>
              {archivedOpen ? s.archived.slice(0, shownCount(s.archived.length, archivedAll)).map(row) : null}
              {archivedOpen && !archivedAll && s.archived.length > 15 ? (
                <button type="button" className={more} onClick={() => setArchivedAll(true)}>
                  Show more ({s.archived.length - 15})
                </button>
              ) : null}
            </div>
          ) : null}
        </>
      )}
    </section>
  );
}

function Composer({
  draftKey,
  channel,
  threadParent,
  placeholder,
  hint,
  activeRun,
  waiting,
  people,
  archived,
  quick,
  onSent,
}: {
  draftKey: string;
  channel: string | null;
  threadParent: string | null;
  placeholder: string;
  hint: string | null;
  activeRun: string | null;
  waiting: boolean;
  people: Mentionable[];
  archived: string[];
  quick: { question: string | null; options: string[] } | null;
  onSent: () => void;
}) {
  const connection = useConnection();
  const client = useQueryClient();
  const [text, setTextState] = useState(() => drafts.get(draftKey) ?? "");
  const [held, setHeld] = useState<{ names: string[]; text: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [picked, setPicked] = useState(0);
  const [closedOn, setClosedOn] = useState<string | null>(null);
  const [dismissed, setDismissed] = useState(false);
  const area = useRef<HTMLTextAreaElement>(null);
  const setText = (next: string) => {
    if (next.trim() === "") drafts.delete(draftKey);
    else drafts.set(draftKey, next);
    setTextState(next);
    setPicked(0);
    if (closedOn !== next) setClosedOn(null);
    setError(null);
  };
  useEffect(() => {
    const el = area.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
  });
  const go = async (body: string, confirmSecret = false) => {
    const clean = body.trim();
    if (clean === "" || busy) return;
    // Empty at once; a refusal puts the words back.
    setText("");
    setHeld(null);
    setBusy(true);
    try {
      await connection.write(
        "group_chat.send",
        { text: clean, images: [], files: [], channel, thread_parent: threadParent, confirm_secret: confirmSecret },
        { retry: false },
      );
      void client.invalidateQueries({ queryKey: ["messages"] });
      onSent();
    } catch (e) {
      if (e instanceof ProtocolError && e.code === "looks_like_secret")
        setHeld({ names: e.matchedItems ?? [], text: clean });
      else setError(e instanceof Error ? e.message : "This did not work.");
      setTextState((now) => (now.trim() === "" ? clean : now));
      drafts.set(draftKey, clean);
    } finally {
      setBusy(false);
    }
  };
  const sendNow = async () => {
    if (!activeRun || text.trim() === "") return;
    try {
      await connection.write("run.stop", { run_id: activeRun }, { retry: false });
    } catch {
      // It may have just ended: the message goes either way.
    }
    void go(text);
  };
  const suggestions = closedOn === text ? null : mentionSuggestions(text, people);
  const active = suggestions ? Math.min(picked, suggestions.found.length - 1) : 0;
  const warning = unknownWarning(unknownMentions(text, people, archived));
  const pick = (name: string) => {
    if (!suggestions) return;
    setText(completeMention(text, suggestions.at, name));
    area.current?.focus();
  };
  const offerQuick = quick && !dismissed && text.trim() === "" ? quick : null;
  useAction("FocusComposer", () => area.current?.focus());
  useAction("SendNow", () => void sendNow(), Boolean(activeRun));
  useAction("ConfirmPrimary", () => offerQuick && void go(offerQuick.options[0] as string), Boolean(offerQuick));
  return (
    <div className="mx-auto flex w-full max-w-[820px] flex-col gap-[6px] px-6 pb-4">
      {offerQuick ? (
        <section
          aria-label="Suggested replies"
          className="flex flex-col gap-2 rounded-[8px] border border-line bg-sunk px-[14px] py-[10px]"
        >
          <div className="flex items-start gap-2">
            <span className="min-w-0 flex-1">{offerQuick.question}</span>
            <button
              type="button"
              aria-label="Dismiss"
              onClick={() => setDismissed(true)}
              className="cursor-pointer border-0 bg-transparent p-0 text-text3 hover:text-text"
            >
              ✕
            </button>
          </div>
          <div className="flex flex-wrap gap-2">
            {offerQuick.options.map((option, i) => (
              <Button key={option} kind={i === 0 ? "primary" : "secondary"} onClick={() => void go(option)}>
                {replyLabel(option)}
              </Button>
            ))}
          </div>
        </section>
      ) : null}
      <LoadBanner waiting={waiting} />
      {held ? <Held names={held.names} busy={busy} onSend={() => void go(held.text, true)} /> : null}
      {error ? (
        <p role="alert" className="m-0 text-[12px] text-red">
          {error}
        </p>
      ) : null}
      <div className="relative">
        {suggestions ? (
          <div
            role="listbox"
            aria-label="People"
            className="absolute inset-x-0 bottom-full z-10 mb-1 flex max-h-[220px] flex-col overflow-y-auto rounded-[8px] border border-line bg-panel p-1 shadow-(--pop-shadow)"
          >
            {suggestions.found.map((p, i) => (
              <button
                key={p.name}
                type="button"
                role="option"
                aria-selected={i === active}
                aria-label={`${p.name}, ${p.kind}, ${i + 1} of ${suggestions.found.length}`}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => pick(p.name)}
                className={`flex h-8 cursor-pointer items-center gap-2 rounded-[6px] border-0 px-2 text-left text-text ${i === active ? "bg-sel" : "bg-transparent"}`}
              >
                <Mark
                  letter={initial(p.name)}
                  color={p.employee ? markColor(p.name) : "#55595F"}
                  size={18}
                  radius={p.employee ? 5 : 9}
                />
                {p.name}
                <span className="truncate text-[12px] text-text3">{p.kind}</span>
              </button>
            ))}
          </div>
        ) : null}
        <div className="flex items-end gap-2 rounded-[10px] border border-btn-line bg-sunk py-2 pr-2 pl-[14px] focus-within:border-acc-text">
          <textarea
            ref={area}
            rows={1}
            aria-label={placeholder}
            placeholder={placeholder}
            className="min-w-0 flex-1 resize-none border-0 bg-transparent py-1 text-[14px] leading-[1.45] text-text outline-none placeholder:text-text3 focus-visible:outline-none"
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (suggestions) {
                const of = suggestions.found.length;
                if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                  e.preventDefault();
                  e.stopPropagation();
                  setPicked((active + (e.key === "ArrowDown" ? 1 : of - 1)) % of);
                  return;
                }
                if (e.key === "Enter" || e.key === "Tab") {
                  e.preventDefault();
                  e.stopPropagation();
                  pick(suggestions.found[active]?.name as string);
                  return;
                }
                if (e.key === "Escape") {
                  e.preventDefault();
                  e.stopPropagation();
                  setClosedOn(text);
                  return;
                }
              }
              if (e.key === "Escape") {
                e.currentTarget.blur();
                return;
              }
              if (e.key === "Enter" && !e.shiftKey && !e.altKey && !e.ctrlKey && !e.metaKey) {
                e.preventDefault();
                void go(text);
              }
            }}
          />
          {activeRun && text.trim() !== "" ? (
            <Button onClick={() => void sendNow()}>
              Send now <Kbd>⌥↵</Kbd>
            </Button>
          ) : null}
          <Button kind="primary" disabled={busy || text.trim() === ""} onClick={() => void go(text)}>
            Send
          </Button>
        </div>
      </div>
      {warning ? (
        <div className="text-[12px] text-amber">{warning}</div>
      ) : hint ? (
        <div className="text-[12px] text-text3">{hint}</div>
      ) : null}
    </div>
  );
}

function ThreadPanel({
  root,
  channelName,
  readOnly,
  ctx,
  channel,
  conversationId,
  people,
  archived,
  onClose,
}: {
  root: string;
  channelName: string;
  readOnly: boolean;
  ctx: ChatContext;
  channel: string | null;
  conversationId: string | null;
  people: Mentionable[];
  archived: string[];
  onClose: () => void;
}) {
  useKeyContext("ThreadPanel");
  useAction("Back", onClose);
  const thread = useThread(root);
  const employees = useEmployees();
  const live = useLiveAll(ctx.store);
  const end = useRef<HTMLDivElement>(null);
  const list = thread.data?.replies ?? [];
  const known = new Set(list.map((m) => m.id));
  const streams = live.filter((u) => streamTo(u.thread_parent, root) === "thread" && !known.has(u.message_id));
  const size = list.length + streams.reduce((n, u) => n + u.text.length, 0);
  useEffect(() => {
    if (size >= 0) end.current?.scrollIntoView({ block: "end" });
  }, [size]);
  const closed = readOnly || thread.data?.read_only === true;
  return (
    <aside aria-label="Thread" className="enter flex w-[360px] flex-none flex-col border-l border-line bg-panel">
      <div className="flex h-11 flex-none items-center gap-2 border-b border-line2 pr-[10px] pl-[14px]">
        <h2 className="m-0 min-w-0 flex-1 truncate text-[13px] font-semibold">Thread · #{channelName}</h2>
        <button
          type="button"
          aria-label="Close thread"
          onClick={onClose}
          className="grid size-7 cursor-pointer place-items-center rounded-[6px] border-0 bg-transparent text-text2 hover:bg-sel hover:text-text"
        >
          ✕
        </button>
      </div>
      <FeedIdsProvider value={{ conversationId }}>
        <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-3 py-3">
          {thread.error ? <span className="text-red">The thread did not load.</span> : null}
          {thread.isPending ? <span className="text-text3">Loading the thread…</span> : null}
          {thread.data ? <MessageRow m={thread.data.root} grouped={false} inThread /> : null}
          {list.length > 0 ? (
            <div className="flex items-center gap-3 py-1 text-[12px] text-text3">
              {replies(list.length)}
              <span className="h-px flex-1 bg-line2" />
            </div>
          ) : null}
          {list.map((m) => (
            <MessageRow key={m.id} m={m} grouped={false} inThread />
          ))}
          {streams.map((u) => (
            <LiveRow
              key={u.message_id}
              id={u.message_id}
              name={runAuthor(employees.data ?? [], u.run_id) ?? "An employee"}
              kind="employee"
            />
          ))}
          <div ref={end} />
        </div>
      </FeedIdsProvider>
      {closed ? (
        <div className="border-t border-line2 p-3 text-[12px] text-text3">
          This channel is closed · the thread is read-only.
        </div>
      ) : (
        <div className="pt-2">
          <Composer
            key={root}
            draftKey={`thread:${root}`}
            channel={channel}
            threadParent={root}
            waiting={false}
            placeholder="Reply… · @ to mention"
            hint={null}
            activeRun={null}
            people={people}
            archived={archived}
            quick={null}
            onSent={() => undefined}
          />
        </div>
      )}
    </aside>
  );
}

type Row =
  | { kind: "message"; m: MessageView; day: boolean; isNew: boolean; grouped: boolean }
  | { kind: "live"; id: string; run: string }
  | { kind: "approval"; id: string };

function Channel({
  channel,
  c,
  thread,
  onThread,
  general,
}: {
  channel: string | null;
  c: ChannelView | null;
  thread: string | null;
  onThread: (id: string | null) => void;
  general: boolean;
}) {
  useKeyContext("TeamChat");
  const { user, features, status } = useLinkState();
  const connection = useConnection();
  const client = useQueryClient();
  const feed = useChannelFeed(channel);
  const approvals = useApprovals();
  const employees = useEmployees();
  const ticket = useTicket(c?.ticket_id ?? null);
  const { people: staff, archived, first } = usePeople();
  const people = useMemo(
    () => [
      ...staff,
      ...(user && !staff.some((p) => p.name === user.username)
        ? [{ name: user.username, kind: "board member", employee: false }]
        : []),
    ],
    [staff, user],
  );
  const names = useMemo(() => people.map((p) => p.name), [people]);
  const store = useLiveStore(feed.data?.conversation_id);
  const live = useLiveAll(store);
  const list = useMemo(() => feed.data?.messages ?? [], [feed.data]);
  const list2 = useMemo(() => list.filter((m) => !m.thread_parent), [list]);
  const handle = useRef<VirtuosoHandle>(null);
  const atBottom = useRef(true);

  // The line "New messages" is decided once, from what was unread before this read.
  const [firstUnread] = useState(() => ({ read: c?.read_through, unread: c?.unread ?? 0 }));
  const newAt = useMemo(() => {
    const at = newMessagesAt(
      list2.map((m) => m.ordinal),
      firstUnread.read,
      firstUnread.unread,
    );
    return at === null ? null : (list2[at]?.id ?? null);
  }, [list2, firstUnread]);

  // The open channel counts as read: no badge from a list read just before.
  const openId = feed.data?.conversation_id;
  // biome-ignore lint/correctness/useExhaustiveDependencies: every new message read counts as read too.
  useEffect(() => {
    if (!openId) return;
    client.setQueryData<ChannelView[]>(["messages", "channels"], (old) =>
      old?.map((x) =>
        x.conversation_id === openId && (x.unread > 0 || x.mentions > 0) ? { ...x, unread: 0, mentions: 0 } : x,
      ),
    );
  }, [openId, client, list2.length]);

  // A finished message ends its stream; one that is still partial goes on in its own row.
  useEffect(() => {
    store.settle(new Set(list.filter((m) => m.status !== "partial").map((m) => m.id)));
  }, [list, store]);

  const pending = useMemo(
    () =>
      (approvals.data ?? []).filter((a) => a.status === "pending" && a.conversation_id && a.conversation_id === openId),
    [approvals.data, openId],
  );
  const approvalMap = useMemo(() => new Map(pending.map((a) => [a.id, a])), [pending]);
  const onCard = useMemo(() => new Set(list.flatMap((m) => m.approval_id ?? [])), [list]);
  const inList = useMemo(() => new Set(list.map((m) => m.id)), [list]);

  const rows = useMemo<Row[]>(() => {
    const dayOf = (ms: number) => localDay(ms, offset());
    const out: Row[] = list2.map((m, i) => {
      const b = before(list2, i, newAt, dayOf);
      return { kind: "message", m, day: b.day, isNew: b.new, grouped: b.grouped };
    });
    for (const u of live) {
      if (!inList.has(u.message_id) && streamTo(u.thread_parent, null) === "channel")
        out.push({ kind: "live", id: u.message_id, run: u.run_id });
    }
    for (const a of pending) if (!onCard.has(a.id)) out.push({ kind: "approval", id: a.id });
    return out;
  }, [list2, newAt, live, inList, pending, onCard]);

  const readOnly = (c?.archived ?? false) || feed.data?.read_only === true;
  const caps = useMemo<Caps>(
    () => ({ threads: features.includes("chat_threads"), reactions: features.includes("chat_reactions"), readOnly }),
    [features, readOnly],
  );
  const employeesData = employees.data;
  const name = c ? bareName(c) : "general";
  const lastAnswerId = useMemo(
    () => [...list2].reverse().find((m) => !m.update && m.role === "assistant")?.id ?? null,
    [list2],
  );
  const ctx = useMemo<ChatContext>(
    () => ({
      store,
      caps,
      features,
      people: names,
      me: user?.username ?? null,
      place: `#${name}`,
      ticketNumber: c?.ticket_number ?? null,
      roleOf: (who) => {
        const e = employeesData?.find((x) => x.name === who);
        return e ? (e.title ?? e.role) : null;
      },
      approvals: approvalMap,
      openThread: onThread,
      messages: list,
      lastAnswer: lastAnswerId,
      conversationId: feed.data?.conversation_id ?? null,
    }),
    [
      store,
      caps,
      features,
      names,
      user,
      name,
      c?.ticket_number,
      employeesData,
      approvalMap,
      onThread,
      list,
      lastAnswerId,
      feed.data?.conversation_id,
    ],
  );

  const running =
    [...list2].reverse().find((m) => m.status === "partial" && m.run_id)?.run_id ?? live.at(-1)?.run_id ?? null;
  const retryAnswer = useRetryAnswer(feed.data?.conversation_id, list2);
  useAction("RetryAnswer", retryAnswer);
  useAction(
    "StopGeneration",
    () => {
      if (running)
        connection
          .write("run.stop", { run_id: running }, { retry: false })
          .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
      else toast("No answer is running here.");
    },
    true,
  );

  const lastAnswer = [...list2].reverse().find((m) => !m.update);
  const quick =
    features.includes("quick_replies") &&
    lastAnswer?.role === "assistant" &&
    lastAnswer.status === "complete" &&
    !pending.some((a) => a.run_id === lastAnswer.run_id)
      ? quickReplies(lastAnswer.activity)
      : null;
  const t = ticket.data?.ticket;
  const owner = t && t.column !== "done" ? (t.owner_name ?? null) : null;
  const isTicket = c?.kind === "ticket";
  const placeholder =
    isTicket && owner
      ? `Message ${owner} (owner)… · @ to mention`
      : general
        ? "Message the team · @ to mention"
        : `Message #${name} · @ to mention`;
  const hint = isTicket
    ? t
      ? `@Name asks that employee${ownerAnswers(owner)}.`
      : "@Name asks that employee about this ticket."
    : `@Name asks that employee; without a name, ${first ?? "the top of the org chart"} answers.`;
  const answering = [...new Set(live.flatMap((u) => runAuthor(employeesData ?? [], u.run_id) ?? []))];

  return (
    <section aria-label={`#${name}`} className="flex min-w-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex min-h-11 flex-none flex-wrap items-center gap-[10px] border-b border-line py-[6px] pr-3 pl-[18px]">
          <h1 className="m-0 text-[13px] font-semibold">
            <span className="text-text3">#</span> {name}
          </h1>
          {t ? (
            <span className="inline-flex h-[22px] items-center gap-[6px] rounded-[11px] border border-line px-2 text-[12px] text-text2">
              <ColumnGlyph kind={t.column} />
              {t.column.replace("_", " ")}
            </span>
          ) : null}
          <span className="min-w-0 flex-1 truncate text-[12px] text-text3">
            {answering.length > 0
              ? `${answering.join(", ")} answering`
              : t
                ? [
                    t.title,
                    t.owner_name ? `${t.owner_name} owner` : null,
                    t.responsible_user_id === user?.id
                      ? "You responsible"
                      : t.responsible_username
                        ? `${t.responsible_username} responsible`
                        : null,
                  ]
                    .filter(Boolean)
                    .join(" · ")
                : c?.kind === "project"
                  ? c.archived
                    ? "Archived project · read-only"
                    : `Project channel · you and ${staff.length} ${staff.length === 1 ? "employee" : "employees"}`
                  : `You and ${staff.length} ${staff.length === 1 ? "employee" : "employees"}`}
          </span>
          {c?.ticket_id ? (
            <Link
              to="/board"
              search={{ ticket: c.ticket_id }}
              className="inline-flex h-7 flex-none items-center rounded-[6px] border border-btn-line bg-btn px-[10px] text-[12px] font-medium text-text no-underline"
            >
              Open ticket ↗
            </Link>
          ) : null}
        </header>
        <ChatProvider value={ctx}>
          <FeedIdsProvider value={{ conversationId: openId ?? null }}>
            <div className="min-h-0 flex-1">
              {feed.error ? (
                <div role="alert" className="flex flex-col items-start gap-2 p-6">
                  <p className="m-0 font-medium">The Team chat did not load.</p>
                  <p className="m-0 text-text2">{feed.error.message}</p>
                  <Button onClick={() => void feed.refetch()}>Try again</Button>
                </div>
              ) : feed.isPending ? (
                <p className="m-0 p-6 text-text3">{status === "online" ? "Loading…" : "Waiting for the server…"}</p>
              ) : rows.length === 0 ? (
                <div className="mx-auto flex max-w-[520px] flex-col gap-2 p-10 text-center">
                  <p className="m-0 text-[15px] font-semibold">
                    {isTicket ? "Discuss the ticket here." : "Talk with your whole team here."}
                  </p>
                  <p className="m-0 text-text2">
                    {isTicket
                      ? `Write @Name to ask an employee${t ? ownerAnswers(owner) : " about this ticket"}.`
                      : `Write @Name to ask an employee; without a name, ${first ?? "the top of the org chart"} answers and brings in whoever is needed.`}
                  </p>
                </div>
              ) : (
                <Virtuoso
                  ref={handle}
                  key={openId}
                  data={rows}
                  className="h-full [scrollbar-gutter:stable_both-edges]"
                  increaseViewportBy={600}
                  initialTopMostItemIndex={{ index: "LAST", align: "end" }}
                  atBottomThreshold={120}
                  atBottomStateChange={(b) => {
                    atBottom.current = b;
                  }}
                  followOutput={(bottom) => (bottom ? "auto" : false)}
                  computeItemKey={(_, row) => (row.kind === "message" ? row.m.id : row.id)}
                  components={FEED_PARTS}
                  itemContent={(_, row) => (
                    <div className="mx-auto w-full max-w-[820px] px-6">
                      {row.kind === "message" ? (
                        <>
                          {row.day ? <DayLine ms={row.m.created_at_ms} /> : null}
                          {row.isNew ? <NewLine /> : null}
                          <div className={row.grouped ? "" : "pt-2"}>
                            <MessageRow m={row.m} grouped={row.grouped} />
                          </div>
                        </>
                      ) : row.kind === "live" ? (
                        <LiveRow
                          id={row.id}
                          name={runAuthor(employeesData ?? [], row.run) ?? "An employee"}
                          kind="employee"
                        />
                      ) : (
                        <div className="pt-3">
                          {approvalMap.get(row.id) ? (
                            <ApprovalCard
                              approval={approvalMap.get(row.id) as NonNullable<ReturnType<typeof approvalMap.get>>}
                            />
                          ) : null}
                        </div>
                      )}
                    </div>
                  )}
                />
              )}
            </div>
          </FeedIdsProvider>
          {readOnly ? (
            <div className="mx-auto flex w-full max-w-[820px] items-center gap-3 px-6 pb-4">
              <div className="flex flex-1 items-center gap-3 rounded-[10px] border border-line bg-sunk px-[14px] py-[10px] text-text2">
                <span className="flex-1">
                  {c ? readOnlyBar(c, c.ticket_column === "done") : "This channel is read-only."}
                </span>
                {c?.ticket_id ? (
                  <Link to="/board" search={{ ticket: c.ticket_id }} className="text-[12px] text-acc-text no-underline">
                    Open ticket
                  </Link>
                ) : null}
              </div>
            </div>
          ) : (
            <Composer
              key={channel ?? "general"}
              draftKey={channel ?? "general"}
              channel={channel}
              threadParent={null}
              waiting={!running && list2.at(-1)?.role === "user"}
              placeholder={placeholder}
              hint={hint}
              activeRun={running}
              people={people}
              archived={archived}
              quick={quick}
              onSent={() => {
                atBottom.current = true;
                handle.current?.scrollToIndex({ index: "LAST", align: "end" });
              }}
            />
          )}
        </ChatProvider>
      </div>
      {thread ? (
        <ChatProvider value={ctx}>
          <ThreadPanel
            key={thread}
            root={thread}
            channelName={name}
            readOnly={readOnly}
            ctx={ctx}
            channel={channel}
            conversationId={openId ?? null}
            people={people}
            archived={archived}
            onClose={() => onThread(null)}
          />
        </ChatProvider>
      ) : null}
    </section>
  );
}

export function ChatScreen({ channel, thread }: { channel: string | null; thread: string | null }) {
  const navigate = useNavigate();
  const channels = useChannels();
  const list = useMemo(() => channels.data ?? [], [channels.data]);
  const s = useMemo(() => sections(list), [list]);
  const general = s.general;
  // general reads as "no channel": the one older servers have.
  const openC = channel ? (list.find((c) => c.conversation_id === channel) ?? null) : general;
  const asParam = (c: ChannelView) => (c === general || c.kind === "general" ? undefined : c.conversation_id);
  const open = (c: ChannelView) => {
    const param = asParam(c);
    void navigate({ to: "/chat", search: param ? { channel: param } : {} });
  };
  const onThread = useMemo(
    () => (id: string | null) =>
      void navigate({
        to: "/chat",
        search: (old: { channel?: string; thread?: string }) => ({
          ...(old.channel ? { channel: old.channel } : {}),
          ...(id ? { thread: id } : {}),
        }),
      }),
    [navigate],
  );
  // A channel that went away: back to general.
  const gone = channel !== null && channels.data !== undefined && !list.some((c) => c.conversation_id === channel);
  useEffect(() => {
    if (gone) void navigate({ to: "/chat", search: {} });
  }, [gone, navigate]);

  const visible = useMemo(() => visibleChannels(s, true, false, false), [s]);
  const step = (down: boolean) => {
    const next = stepChannel(
      visible.map((c) => c.conversation_id),
      openC?.conversation_id ?? null,
      down,
    );
    const target = list.find((c) => c.conversation_id === next);
    if (target) open(target);
  };
  useAction("NextChannel", () => step(true));
  useAction("PrevChannel", () => step(false));

  return (
    <div className="flex min-h-0 flex-1">
      {list.length > 0 ? <ChannelList channels={list} open={channel} onOpen={open} /> : null}
      <Channel
        key={channel ?? "general"}
        channel={channel}
        c={openC}
        thread={thread}
        onThread={onThread}
        general={channel === null}
      />
    </div>
  );
}
