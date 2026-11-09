import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { ChatComposer } from "@astryxdesign/core/Chat";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { TextInput } from "@astryxdesign/core/TextInput";
import { ProtocolError } from "@aurelius/client";
import type { ChannelView, EmployeeSummary, MessageView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useRef, useState } from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { useConnection, useLinkState } from "../connection";
import { useApprovals, useChannelFeed, useChannels, useEmployees, useThread, useTicket } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { AttachedImages, useImagePaste } from "../ui/attach";
import { KeyHint } from "../ui/key-hint";
import { initial, Mark, markColor } from "../ui/marks";
import { toast } from "../ui/toast";
import { useDelayed } from "../ui/use-delayed";
import { useWorkspace } from "../workspace";
import { ApprovalCard } from "./approval-card";
import { foldUpdates } from "./board-logic";
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
import {
  type ChatContext,
  ChatProvider,
  ContextDialog,
  DayLine,
  FeedIdsProvider,
  LiveRow,
  MessageRow,
  NewLine,
} from "./chat-message";
import { Held } from "./dialogs";
import { approvalsFor } from "./run-card-logic";
import { LoadBanner, useRetryAnswer } from "./run-tools";
import { ThreadWaiting } from "./scheduled";
import { ColumnGlyph } from "./tickets-kit";
import { homeChannel, narrow } from "./workspace-logic";

const enter = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(4px)" },
});

const styles = stylex.create({
  feedTop: { height: 14 },
  feedEnd: { height: 16 },
  channelRow: {
    display: "flex",
    height: 28,
    width: "100%",
    cursor: "pointer",
    alignItems: "center",
    gap: 8,
    borderRadius: 6,
    borderWidth: 0,
    paddingInline: 8,
    textAlign: "left",
    backgroundColor: "transparent",
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
  },
  channelOn: { backgroundColor: "var(--color-overlay-hover)", color: "var(--color-text-primary)" },
  channelUnread: { color: "var(--color-text-primary)", fontWeight: 600 },
  hash: { color: "var(--precision-text3)" },
  channelName: {
    minWidth: 0,
    flexGrow: 1,
    flexBasis: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  readOnlyTag: { fontSize: 11, color: "var(--precision-text3)" },
  badgeMention: { fontSize: 12, fontWeight: 600, color: "var(--color-text-warning)" },
  badgeUnread: { fontSize: 12, color: "var(--color-text-secondary)" },
  list: {
    display: "flex",
    width: 220,
    flexShrink: 0,
    flexDirection: "column",
    gap: 14,
    overflowY: "auto",
    borderRightWidth: 1,
    borderRightStyle: "solid",
    borderRightColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    paddingInline: 8,
    paddingBlock: 10,
  },
  listGroup: { display: "flex", flexDirection: "column", gap: 1 },
  eyebrow: {
    margin: 0,
    marginBottom: 4,
    paddingInline: 8,
    fontSize: 12,
    fontWeight: 500,
    color: "var(--precision-text3)",
  },
  none: { margin: 0, paddingInline: 8, color: "var(--precision-text3)" },
  more: {
    cursor: "pointer",
    borderWidth: 0,
    backgroundColor: "transparent",
    paddingInline: 8,
    paddingBlock: 4,
    textAlign: "left",
    fontSize: 12,
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-primary)" },
  },
  composer: {
    marginInline: "auto",
    display: "flex",
    width: "100%",
    maxWidth: 820,
    flexDirection: "column",
    gap: 6,
    paddingInline: 24,
    paddingBottom: 16,
  },
  quick: {
    display: "flex",
    flexDirection: "column",
    gap: 8,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    paddingInline: 14,
    paddingBlock: 10,
  },
  quickTop: { display: "flex", alignItems: "flex-start", gap: 8 },
  quickText: { minWidth: 0, flexGrow: 1, flexBasis: 0 },
  quickOptions: { display: "flex", flexWrap: "wrap", gap: 8 },
  composerBody: { borderRadius: 10, backgroundColor: "var(--precision-sunk)" },
  composerInput: { position: "relative" },
  people: {
    position: "absolute",
    insetInline: 0,
    bottom: "100%",
    zIndex: 10,
    marginBottom: 12,
    display: "flex",
    maxHeight: 220,
    flexDirection: "column",
    overflowY: "auto",
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-surface)",
    padding: 4,
    boxShadow: "var(--shadow-high)",
  },
  person: {
    display: "flex",
    height: 32,
    flexShrink: 0,
    cursor: "pointer",
    alignItems: "center",
    gap: 8,
    borderRadius: 6,
    borderWidth: 0,
    paddingInline: 8,
    textAlign: "left",
    backgroundColor: "transparent",
    color: "var(--color-text-primary)",
  },
  personOn: { backgroundColor: "var(--color-overlay-hover)" },
  personKind: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  textarea: {
    display: "block",
    width: "100%",
    resize: "none",
    borderWidth: 0,
    backgroundColor: "transparent",
    paddingBlock: 4,
    paddingInline: 6,
    fontSize: 14,
    lineHeight: 1.45,
    color: "var(--color-text-primary)",
    outlineStyle: { default: "none", ":focus-visible": "none" },
    "::placeholder": { color: "var(--precision-text3)" },
  },
  warning: { fontSize: 12, color: "var(--color-text-warning)" },
  hint: { fontSize: 12, color: "var(--precision-text3)" },
  thread: {
    display: "flex",
    width: 360,
    flexShrink: 0,
    flexDirection: "column",
    borderLeftWidth: 1,
    borderLeftStyle: "solid",
    borderLeftColor: "var(--color-border)",
    backgroundColor: "var(--color-background-surface)",
    animationName: enter,
    animationDuration: "160ms",
    animationTimingFunction: "var(--ease-standard)",
  },
  threadTop: {
    display: "flex",
    height: 44,
    flexShrink: 0,
    alignItems: "center",
    gap: 8,
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
    paddingInlineStart: 14,
    paddingInlineEnd: 10,
  },
  threadTitle: {
    margin: 0,
    minWidth: 0,
    flexGrow: 1,
    flexBasis: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontSize: 13,
    fontWeight: 600,
  },
  threadFeed: {
    display: "flex",
    minHeight: 0,
    flexGrow: 1,
    flexBasis: 0,
    flexDirection: "column",
    gap: 8,
    overflowY: "auto",
    padding: 12,
  },
  error: { color: "var(--color-text-error)" },
  replies: {
    display: "flex",
    alignItems: "center",
    gap: 12,
    paddingBlock: 4,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  rule: { height: 1, flexGrow: 1, backgroundColor: "var(--precision-line2)" },
  threadClosed: {
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    padding: 12,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  threadComposer: { paddingTop: 8 },
  channel: { display: "flex", minWidth: 0, minHeight: 0, flexGrow: 1, flexBasis: 0 },
  threadFull: { width: "100%", flexShrink: 1, borderLeftWidth: 0 },
  channelMain: { display: "flex", minWidth: 0, flexGrow: 1, flexBasis: 0, flexDirection: "column" },
  top: {
    display: "flex",
    minHeight: 44,
    flexShrink: 0,
    flexWrap: "wrap",
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
  column: {
    display: "inline-flex",
    height: 22,
    alignItems: "center",
    gap: 6,
    borderRadius: 11,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    paddingInline: 8,
    fontSize: 12,
    color: "var(--color-text-secondary)",
  },
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
  openTicket: {
    display: "inline-flex",
    height: 28,
    flexShrink: 0,
    alignItems: "center",
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    backgroundColor: "var(--color-neutral)",
    paddingInline: 10,
    fontSize: 12,
    fontWeight: 500,
    color: "var(--color-text-primary)",
    textDecoration: "none",
  },
  feedBox: { minHeight: 0, flexGrow: 1, flexBasis: 0 },
  loading: { display: "flex", flexDirection: "column", gap: 8, padding: 24 },
  waiting: { margin: 0, padding: 24, color: "var(--precision-text3)" },
  problem: { padding: 24 },
  feed: { height: "100%", scrollbarGutter: "stable both-edges" },
  feedColumn: { marginInline: "auto", width: "100%", maxWidth: 820, paddingInline: 24 },
  feedNarrow: { paddingInline: 12 },
  earlier: {
    display: "block",
    borderWidth: 0,
    padding: 0,
    paddingTop: 8,
    cursor: "pointer",
    fontSize: 12,
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-primary)" },
    backgroundColor: "transparent",
  },
  hiddenMain: { display: "none" },
  topGap: { paddingTop: 8 },
  approvalGap: { paddingTop: 12 },
  readOnlyRow: {
    marginInline: "auto",
    display: "flex",
    width: "100%",
    maxWidth: 820,
    alignItems: "center",
    gap: 12,
    paddingInline: 24,
    paddingBottom: 16,
  },
  readOnlyBar: {
    display: "flex",
    flexGrow: 1,
    flexBasis: 0,
    alignItems: "center",
    gap: 12,
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    paddingInline: 14,
    paddingBlock: 10,
    color: "var(--color-text-secondary)",
  },
  readOnlyText: { flexGrow: 1, flexBasis: 0 },
  readOnlyLink: { fontSize: 12, color: "var(--color-text-accent)", textDecoration: "none" },
  screen: { display: "flex", minHeight: 0, flexGrow: 1, flexBasis: 0 },
});

/** Worked out once: every visible row uses these. */
const FEED_COLUMN = stylex.props(styles.feedColumn);
const FEED_NARROW = stylex.props(styles.feedColumn, styles.feedNarrow);
const TOP_GAP = stylex.props(styles.topGap);

const FEED_PARTS = {
  Header: function FeedTop() {
    return <div {...stylex.props(styles.feedTop)} />;
  },
  Footer: function FeedEnd() {
    return <div {...stylex.props(styles.feedEnd)} />;
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
      {...stylex.props(styles.channelRow, on && styles.channelOn, unread && styles.channelUnread)}
    >
      <span {...stylex.props(styles.hash)}>#</span>
      <span {...stylex.props(styles.channelName)}>{bareName(c)}</span>
      {c.archived ? <span {...stylex.props(styles.readOnlyTag)}>read-only</span> : null}
      {on || c.archived ? null : c.mentions > 0 ? (
        <span {...stylex.props(styles.badgeMention)}>@{c.mentions}</span>
      ) : c.unread > 0 ? (
        <span {...stylex.props(styles.badgeUnread)}>{c.unread}</span>
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
  const workspace = useWorkspace().id;
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
          if (alive) setFound(body.type === "channels" ? narrow(body.value.channels, workspace) : local());
        })
        .catch(() => {
          if (alive) setFound(local());
        });
    }, 150);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [query, channels, connection, features, workspace]);

  const eyebrow = stylex.props(styles.eyebrow);
  const more = stylex.props(styles.more);
  const group = stylex.props(styles.listGroup);
  const row = (c: ChannelView) => (
    <ChannelRow key={c.conversation_id} c={c} on={c.conversation_id === openId} onOpen={onOpen} />
  );
  return (
    <section
      aria-label="Channels"
      {...stylex.props(styles.list)}
      onPointerEnter={() => setFrozen(s.tickets.map((c) => c.conversation_id))}
      onPointerLeave={() => setFrozen(null)}
    >
      <TextInput
        label="Search channels"
        isLabelHidden
        placeholder="Search channels"
        size="sm"
        startIcon="search"
        width="100%"
        value={query}
        onChange={setQuery}
      />
      {found ? (
        <div {...group}>
          <p {...eyebrow}>Results</p>
          {found.length === 0 ? <p {...stylex.props(styles.none)}>No channel matches.</p> : found.map(row)}
        </div>
      ) : (
        <>
          <div {...group}>
            <p {...eyebrow}>Channels</p>
            {s.general ? row(s.general) : null}
            {s.projects.map(row)}
          </div>
          {tickets.length > 0 ? (
            <div {...group}>
              <p {...eyebrow}>Tickets</p>
              {tickets.slice(0, shownCount(tickets.length, ticketsAll)).map(row)}
              {!ticketsAll && tickets.length > 15 ? (
                <button type="button" {...more} onClick={() => setTicketsAll(true)}>
                  Show more ({tickets.length - 15})
                </button>
              ) : null}
            </div>
          ) : null}
          {s.archived.length > 0 ? (
            <div {...group}>
              <button
                type="button"
                aria-expanded={archivedOpen}
                {...more}
                onClick={() => setArchivedOpen(!archivedOpen)}
              >
                {archivedOpen ? "▾" : "▸"} Archived ({s.archived.length})
              </button>
              {archivedOpen ? s.archived.slice(0, shownCount(s.archived.length, archivedAll)).map(row) : null}
              {archivedOpen && !archivedAll && s.archived.length > 15 ? (
                <button type="button" {...more} onClick={() => setArchivedAll(true)}>
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

export function Composer({
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
  send,
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
  /** Where the words go; none: the channel. */
  send?: (message: { text: string; images: string[]; confirmSecret: boolean }) => Promise<unknown>;
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
  const images = useImagePaste();
  const go = async (body: string, confirmSecret = false) => {
    const clean = body.trim();
    if (clean === "" || busy) return;
    // Empty at once; a refusal puts the words back.
    setText("");
    setHeld(null);
    setBusy(true);
    try {
      if (send) await send({ text: clean, images: images.paths, confirmSecret });
      else
        await connection.write(
          "group_chat.send",
          {
            text: clean,
            images: images.paths,
            files: [],
            channel,
            thread_parent: threadParent,
            confirm_secret: confirmSecret,
          },
          { retry: false },
        );
      images.clear();
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
  const warning = people.length > 0 ? unknownWarning(unknownMentions(text, people, archived)) : null;
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
    <div {...stylex.props(styles.composer)}>
      {offerQuick ? (
        <section aria-label="Suggested replies" {...stylex.props(styles.quick)}>
          <div {...stylex.props(styles.quickTop)}>
            <span {...stylex.props(styles.quickText)}>{offerQuick.question}</span>
            <IconButton
              size="sm"
              variant="ghost"
              label="Dismiss"
              icon={<Icon icon="close" />}
              onClick={() => setDismissed(true)}
            />
          </div>
          <div {...stylex.props(styles.quickOptions)}>
            {offerQuick.options.map((option, i) => (
              <Button
                key={option}
                variant={i === 0 ? "primary" : "secondary"}
                label={replyLabel(option)}
                onClick={() => void go(option)}
              />
            ))}
          </div>
        </section>
      ) : null}
      <LoadBanner waiting={waiting} />
      {held ? <Held names={held.names} busy={busy} onSend={() => void go(held.text, true)} /> : null}
      {error ? <Banner status="error" role="alert" title={error} /> : null}
      {/* The text stays this component's own: Enter and Send call `go`, which empties the field at once and puts
          refused words back. The composer's own submit, which would clear it, is never reached. */}
      <ChatComposer
        {...images.target}
        density="compact"
        elevation="none"
        xstyle={styles.composerBody}
        onSubmit={(value) => void go(value)}
        drawer={<AttachedImages images={images} />}
        sendActions={
          activeRun && text.trim() !== "" ? (
            <Button label="Send now" onClick={() => void sendNow()}>
              Send now <KeyHint>⌥↵</KeyHint>
            </Button>
          ) : null
        }
        sendButton={
          <Button
            variant="primary"
            label="Send"
            isDisabled={busy || text.trim() === ""}
            onClick={() => void go(text)}
          />
        }
        input={
          <div {...stylex.props(styles.composerInput)}>
            {suggestions ? (
              <div role="listbox" aria-label="People" {...stylex.props(styles.people)}>
                {suggestions.found.map((p, i) => (
                  <button
                    key={p.name}
                    type="button"
                    role="option"
                    aria-selected={i === active}
                    aria-label={`${p.name}, ${p.kind}, ${i + 1} of ${suggestions.found.length}`}
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => pick(p.name)}
                    {...stylex.props(styles.person, i === active && styles.personOn)}
                  >
                    <Mark
                      letter={initial(p.name)}
                      color={p.employee ? markColor(p.name) : "#55595F"}
                      size={18}
                      radius={p.employee ? 5 : 9}
                    />
                    {p.name}
                    <span {...stylex.props(styles.personKind)}>{p.kind}</span>
                  </button>
                ))}
              </div>
            ) : null}
            <textarea
              ref={area}
              rows={1}
              aria-label={placeholder}
              placeholder={placeholder}
              {...stylex.props(styles.textarea)}
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
          </div>
        }
      />
      {warning ? (
        <div {...stylex.props(styles.warning)}>{warning}</div>
      ) : hint ? (
        <div {...stylex.props(styles.hint)}>{hint}</div>
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
  full = false,
}: {
  full?: boolean;
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
  const loading = useDelayed(thread.isPending);
  return (
    <aside aria-label="Thread" {...stylex.props(styles.thread, full && styles.threadFull)}>
      <div {...stylex.props(styles.threadTop)}>
        <Heading level={2} xstyle={styles.threadTitle}>
          Thread · #{channelName}
        </Heading>
        <IconButton size="sm" variant="ghost" label="Close thread" icon={<Icon icon="close" />} onClick={onClose} />
      </div>
      <FeedIdsProvider value={{ conversationId }}>
        <div {...stylex.props(styles.threadFeed)}>
          {thread.error ? <span {...stylex.props(styles.error)}>The thread did not load.</span> : null}
          {loading ? <Skeleton height={48} /> : null}
          {thread.data ? <MessageRow m={thread.data.root} grouped={false} inThread /> : null}
          {list.length > 0 ? (
            <div {...stylex.props(styles.replies)}>
              {replies(list.length)}
              <span {...stylex.props(styles.rule)} />
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
        <div {...stylex.props(styles.threadClosed)}>This channel is closed · the thread is read-only.</div>
      ) : (
        <div {...stylex.props(styles.threadComposer)}>
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
  | { kind: "message"; m: MessageView; day: boolean; isNew: boolean; grouped: boolean; earlier: number }
  | { kind: "live"; id: string; run: string }
  | { kind: "approval"; id: string };

interface ChannelProps {
  channel: string | null;
  c: ChannelView | null;
  thread: string | null;
  onThread: (id: string | null) => void;
  general: boolean;
  message?: string | null;
  /** Inside a ticket's panel or the popout: no head of its own, narrow, and a thread takes the whole width. */
  embedded?: boolean;
  /** The ticket, when `c` may not be known yet. */
  ticketId?: string | null;
}

function Channel(props: ChannelProps) {
  useKeyContext("TeamChat");
  return <ChannelBody {...props} />;
}

/** A ticket's discussion, as a chat: in the ticket's panel and in the popout. */
export function TicketChat({
  conversationId,
  ticketId,
  message = null,
}: {
  conversationId: string;
  ticketId: string;
  message?: string | null;
}) {
  const channels = useChannels();
  const c = channels.data?.find((x) => x.conversation_id === conversationId) ?? null;
  const [thread, setThread] = useState<string | null>(null);
  return (
    <ChannelBody
      channel={conversationId}
      c={c}
      ticketId={ticketId}
      thread={thread}
      onThread={setThread}
      general={false}
      message={message}
      embedded
    />
  );
}

function ChannelBody({
  channel,
  c,
  thread,
  onThread,
  general,
  message = null,
  embedded = false,
  ticketId = null,
}: ChannelProps) {
  const { user, features, status } = useLinkState();
  const connection = useConnection();
  const client = useQueryClient();
  const feed = useChannelFeed(channel);
  const approvals = useApprovals();
  const employees = useEmployees();
  const ticket = useTicket(ticketId ?? c?.ticket_id ?? null);
  const t = ticket.data?.ticket;
  const isTicket = ticketId !== null || c?.kind === "ticket";
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
  const talk = useMemo(() => list.filter((m) => !m.thread_parent), [list]);
  // On a ticket a row of "moved to Doing" lines folds, so what people said stays in view.
  const [unfolded, setUnfolded] = useState(message !== null);
  const folded = useMemo(
    () => (isTicket ? foldUpdates(talk, unfolded) : { shown: talk, earlier: new Map<string, number>() }),
    [isTicket, talk, unfolded],
  );
  const list2 = folded.shown;
  const handle = useRef<VirtuosoHandle>(null);
  // The message a link led to: into view, and marked for a moment.
  const [flash, setFlash] = useState<string | null>(null);
  useEffect(() => {
    if (message === null) return;
    const at = list2.findIndex((m) => m.id === message);
    if (at < 0) return;
    setFlash(message);
    handle.current?.scrollToIndex({ index: at, align: "center" });
    const timer = setTimeout(() => setFlash(null), 2000);
    return () => clearTimeout(timer);
  }, [message, list2]);
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
    () => approvalsFor(approvals.data ?? [], { conversationId: openId, ticketId: ticketId ?? c?.ticket_id }),
    [approvals.data, openId, ticketId, c?.ticket_id],
  );
  const approvalMap = useMemo(() => new Map(pending.map((a) => [a.id, a])), [pending]);
  const onCard = useMemo(() => new Set(list.flatMap((m) => m.approval_id ?? [])), [list]);
  const inList = useMemo(() => new Set(list.map((m) => m.id)), [list]);

  const rows = useMemo<Row[]>(() => {
    const dayOf = (ms: number) => localDay(ms, offset());
    const out: Row[] = list2.map((m, i) => {
      const b = before(list2, i, newAt, dayOf);
      return {
        kind: "message",
        m,
        day: b.day,
        isNew: b.new,
        grouped: b.grouped,
        earlier: folded.earlier.get(m.id) ?? 0,
      };
    });
    for (const u of live) {
      if (!inList.has(u.message_id) && streamTo(u.thread_parent, null) === "channel")
        out.push({ kind: "live", id: u.message_id, run: u.run_id });
    }
    for (const a of pending) if (!onCard.has(a.id)) out.push({ kind: "approval", id: a.id });
    return out;
  }, [list2, folded.earlier, newAt, live, inList, pending, onCard]);

  const readOnly = (c?.archived ?? false) || feed.data?.read_only === true;
  const caps = useMemo<Caps>(
    () => ({ threads: features.includes("chat_threads"), reactions: features.includes("chat_reactions"), readOnly }),
    [features, readOnly],
  );
  const employeesData = employees.data;
  const name = c ? bareName(c) : t ? String(t.number) : "general";
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
      ticketNumber: c?.ticket_number ?? t?.number ?? null,
      roleOf: (who) => {
        const e = employeesData?.find((x) => x.name === who);
        return e ? (e.title ?? e.role) : null;
      },
      approvals: approvalMap,
      openThread: onThread,
      messages: list,
      lastAnswer: lastAnswerId,
      conversationId: feed.data?.conversation_id ?? null,
      channel,
      flash,
    }),
    [
      channel,
      flash,
      store,
      caps,
      features,
      names,
      user,
      name,
      c?.ticket_number,
      t?.number,
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
  const retryAnswer = useRetryAnswer(feed.data?.conversation_id, list2, { channel });
  const [contextOf, setContextOf] = useState<string | null>(null);
  useAction(
    "ShowContext",
    () => {
      const run = [...list2].reverse().find((m) => m.role === "assistant" && m.run_id)?.run_id;
      if (run) setContextOf(run);
      else toast("Open a conversation with an answer to see the context it used.");
    },
    !embedded,
  );
  useAction("RetryAnswer", retryAnswer, !embedded);
  useAction(
    "StopGeneration",
    () => {
      if (running)
        connection
          .write("run.stop", { run_id: running }, { retry: false })
          .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
      else toast("No answer is running here.");
    },
    !embedded,
  );

  const lastAnswer = [...list2].reverse().find((m) => !m.update);
  const quick =
    features.includes("quick_replies") &&
    lastAnswer?.role === "assistant" &&
    lastAnswer.status === "complete" &&
    !pending.some((a) => a.run_id === lastAnswer.run_id)
      ? quickReplies(lastAnswer.activity)
      : null;
  const owner = t && t.column !== "done" ? (t.owner_name ?? null) : null;
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
  const loading = useDelayed(feed.isPending);

  return (
    <section aria-label={`#${name}`} {...stylex.props(styles.channel)}>
      <div {...stylex.props(styles.channelMain, embedded && thread !== null && styles.hiddenMain)}>
        {embedded ? null : (
          <header data-titlebar {...stylex.props(styles.top)}>
            <Heading level={1} xstyle={styles.title}>
              <span {...stylex.props(styles.hash)}>#</span> {name}
            </Heading>
            {t ? (
              <span {...stylex.props(styles.column)}>
                <ColumnGlyph kind={t.column} />
                {t.column.replace("_", " ")}
              </span>
            ) : null}
            <span {...stylex.props(styles.subtitle)}>
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
              <Link to="/board" search={{ ticket: c.ticket_id }} {...stylex.props(styles.openTicket)}>
                Open ticket ↗
              </Link>
            ) : null}
          </header>
        )}
        <ThreadWaiting conversationId={openId} />
        {contextOf ? <ContextDialog run={contextOf} onClose={() => setContextOf(null)} /> : null}
        <ChatProvider value={ctx}>
          <FeedIdsProvider value={{ conversationId: openId ?? null }}>
            <div {...stylex.props(styles.feedBox)}>
              {feed.error ? (
                <div {...stylex.props(styles.problem)}>
                  <Banner
                    status="error"
                    role="alert"
                    title="The Team chat did not load."
                    description={feed.error.message}
                    endContent={<Button label="Try again" onClick={() => void feed.refetch()} />}
                  />
                </div>
              ) : feed.isPending ? (
                status === "online" ? (
                  loading ? (
                    <div {...stylex.props(styles.loading)}>
                      <Skeleton height={48} />
                      <Skeleton height={48} />
                      <Skeleton height={48} />
                    </div>
                  ) : null
                ) : (
                  <p {...stylex.props(styles.waiting)}>Waiting for the server…</p>
                )
              ) : rows.length === 0 ? (
                <EmptyState
                  title={isTicket ? "Discuss the ticket here." : "Talk with your whole team here."}
                  description={
                    isTicket
                      ? `Write @Name to ask an employee${t ? ownerAnswers(owner) : " about this ticket"}.`
                      : `Write @Name to ask an employee; without a name, ${first ?? "the top of the org chart"} answers and brings in whoever is needed.`
                  }
                />
              ) : (
                <Virtuoso
                  ref={handle}
                  key={openId}
                  data={rows}
                  {...stylex.props(styles.feed)}
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
                    <div {...(embedded ? FEED_NARROW : FEED_COLUMN)}>
                      {row.kind === "message" ? (
                        <>
                          {row.earlier > 0 ? (
                            <button type="button" onClick={() => setUnfolded(true)} {...stylex.props(styles.earlier)}>
                              {row.earlier === 1 ? "1 earlier update" : `${row.earlier} earlier updates`}
                            </button>
                          ) : null}
                          {row.day ? <DayLine ms={row.m.created_at_ms} /> : null}
                          {row.isNew ? <NewLine /> : null}
                          <div {...(row.grouped ? null : TOP_GAP)}>
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
                        <div {...stylex.props(styles.approvalGap)}>
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
            <div {...stylex.props(styles.readOnlyRow)}>
              <div {...stylex.props(styles.readOnlyBar)}>
                <span {...stylex.props(styles.readOnlyText)}>
                  {c ? readOnlyBar(c, c.ticket_column === "done") : "This channel is read-only."}
                </span>
                {c?.ticket_id && !embedded ? (
                  <Link to="/board" search={{ ticket: c.ticket_id }} {...stylex.props(styles.readOnlyLink)}>
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
              hint={embedded ? null : hint}
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
            full={embedded}
          />
        </ChatProvider>
      ) : null}
    </section>
  );
}

export function ChatScreen({
  channel,
  thread,
  message = null,
}: {
  channel: string | null;
  thread: string | null;
  message?: string | null;
}) {
  const navigate = useNavigate();
  const channels = useChannels();
  const workspace = useWorkspace().id;
  const all = useMemo(() => channels.data ?? [], [channels.data]);
  const list = useMemo(() => narrow(all, workspace), [all, workspace]);
  const s = useMemo(() => sections(list), [list]);
  const general = s.general;
  // general reads as "no channel": the one older servers have. A workspace opens on its project's.
  const home = workspace === null ? general : homeChannel(list, workspace);
  const shownChannel = channel ?? (workspace === null ? null : (home?.conversation_id ?? null));
  // A linked channel of another project shows while the workspace moves to it.
  const openC = channel ? (all.find((c) => c.conversation_id === channel) ?? null) : home;
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
  const gone = channel !== null && channels.data !== undefined && !all.some((c) => c.conversation_id === channel);
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
    <div {...stylex.props(styles.screen)}>
      {list.length > 0 ? <ChannelList channels={list} open={shownChannel} onOpen={open} /> : null}
      {workspace !== null && shownChannel === null ? (
        // Without this the feed would be the general channel's.
        channels.data ? (
          <EmptyState isCompact title="No channel in this workspace yet." />
        ) : null
      ) : (
        <Channel
          key={shownChannel ?? "general"}
          channel={shownChannel}
          c={openC}
          thread={thread}
          message={message}
          onThread={onThread}
          general={shownChannel === null}
        />
      )}
    </div>
  );
}
