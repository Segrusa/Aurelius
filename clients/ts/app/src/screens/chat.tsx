import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { ChatComposer } from "@astryxdesign/core/Chat";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { arrived, ProtocolError } from "@aurelius/client";
import type { ChannelView, EmployeeSummary, MessageView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { type ComponentProps, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { useConnection, useLinkState } from "../connection";
import { useApprovals, useChannelFeed, useChannels, useEmployees, useThread, useTicket } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { AttachmentTray, usePromptAttachments } from "../ui/attach";
import { withFolders } from "../ui/attach-logic";
import { KeyHint } from "../ui/key-hint";
import { initial, Mark, markColor } from "../ui/marks";
import { PromptBox, PromptTextarea } from "../ui/prompt-box";
import { toast } from "../ui/toast";
import { useDelayed } from "../ui/use-delayed";
import { ApprovalCard } from "./approval-card";
import { DockedArtifact, targetKey, useArtifactDock, useOpenDesign } from "./artifact-dock";
import { foldUpdates } from "./board-logic";
import { useLiveAll, useLiveStore } from "./chat-live";
import {
  audienceLine,
  bareName,
  before,
  type Caps,
  completeMention,
  EVERYONE,
  localDay,
  type Mentionable,
  mentionSuggestions,
  mentionsEveryone,
  newMessagesAt,
  ownerAnswers,
  quickReplies,
  readOnlyBar,
  replies,
  replyLabel,
  runAuthor,
  streamTo,
  unknownMentions,
  unknownWarning,
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
import { mergeByTime, type WorkRow, workInFilter } from "./ticket-activity-logic";
import { OVERVIEW_KEY, OverviewPanel, ProgressCard, useTicketProgress } from "./ticket-overview";
import { ActivityHead, WorkItem } from "./ticket-page";
import { type ActivityFilter, inFilter } from "./ticket-page-logic";
import { ticketPlace } from "./ticket-session-logic";
import { ColumnGlyph } from "./tickets-kit";

const enter = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(4px)" },
});

const styles = stylex.create({
  feedTop: { height: 14 },
  feedEmpty: { margin: 0, paddingBlock: 16, fontSize: 13, color: "var(--precision-text3)" },
  feedEnd: { height: 16 },
  hash: { color: "var(--precision-text3)" },
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
  pointer: { cursor: "pointer" },
  feedBox: { minHeight: 0, flexGrow: 1, flexBasis: 0 },
  loading: { display: "flex", flexDirection: "column", gap: 8, padding: 24 },
  waiting: { margin: 0, padding: 24, color: "var(--precision-text3)" },
  problem: { padding: 24 },
  feed: { height: "100%", scrollbarGutter: "stable both-edges" },
  feedColumn: { marginInline: "auto", width: "100%", maxWidth: 820, paddingInline: 24 },
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
});

/** Worked out once: every visible row uses these. */
const FEED_COLUMN = stylex.props(styles.feedColumn);
const TOP_GAP = stylex.props(styles.topGap);

/** What a ticket's page puts above its talk; a channel's own chat has none. */
interface FeedContext {
  top: ReactNode | null;
  empty: string | null;
}

const FEED_PARTS = {
  Header: function FeedTop({ context }: { context?: FeedContext }) {
    return context?.top ? <div {...FEED_COLUMN}>{context.top}</div> : <div {...stylex.props(styles.feedTop)} />;
  },
  EmptyPlaceholder: function FeedEmpty({ context }: { context?: FeedContext }) {
    return context?.empty ? (
      <div {...FEED_COLUMN}>
        <p {...stylex.props(styles.feedEmpty)}>{context.empty}</p>
      </div>
    ) : null;
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

/** The chat box's words: its own type, borderless inside Astryx's composer. */
function ChatTextarea(props: Omit<ComponentProps<"textarea">, "className" | "style">) {
  return <textarea {...props} {...stylex.props(styles.textarea)} />;
}

/** The ticket's chat has New session's box; it has taken files of any type since it began. */
const TEAM_PROMPT = { files: true };

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
  everyone = false,
  footer,
  prompt,
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
  /** `@Everyone` is offered (the server has it and the writer is a board member). */
  everyone?: boolean;
  /** Controls in the composer's bottom row, beside Send (a session's model, effort and mode). */
  footer?: ReactNode;
  /**
   * New session's box (`PromptBox`) instead of the chat's: + adds photos, files (where `files`:
   * the place it sends to takes them) and folders. Without it only pasted photos go along.
   */
  prompt?: { files: boolean };
  /** Where the words go; none: the channel. */
  send?: (message: { text: string; images: string[]; files: string[]; confirmSecret: boolean }) => Promise<unknown>;
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
  const attach = usePromptAttachments({
    files: prompt?.files ?? false,
    folders: Boolean(prompt),
    shortcut: Boolean(prompt),
  });
  const ready = text.trim() !== "" || attach.items.length > 0;
  const go = async (body: string, confirmSecret = false) => {
    const clean = body.trim();
    if ((clean === "" && attach.items.length === 0) || busy) return;
    const words = withFolders(clean, attach.folders);
    // Empty at once; a refusal puts the words back.
    setText("");
    setHeld(null);
    setBusy(true);
    try {
      try {
        if (send) await send({ text: words, images: attach.images, files: attach.files, confirmSecret });
        else
          await connection.write(
            "group_chat.send",
            {
              text: words,
              images: attach.images,
              files: attach.files,
              channel,
              thread_parent: threadParent,
              confirm_secret: confirmSecret,
            },
            { retry: false },
          );
      } catch (e) {
        // The server has it; only its answer was lost on the way back.
        if (!arrived(e)) throw e;
      }
      attach.clear();
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
    if (!activeRun || !ready) return;
    try {
      await connection.write("run.stop", { run_id: activeRun }, { retry: false });
    } catch {
      // It may have just ended: the message goes either way.
    }
    void go(text);
  };
  const wantsAudience = everyone && mentionsEveryone(text);
  const [audience, setAudience] = useState<string | null>(null);
  // One read when `@Everyone` shows up in this thread, after a pause; not one per keystroke.
  useEffect(() => {
    if (!wantsAudience) return setAudience(null);
    let alive = true;
    const timer = setTimeout(() => {
      connection
        .call("discussion.audience", { channel })
        .then((body) => {
          if (alive) setAudience(body.type === "discussion_audience" ? audienceLine(body.value) : null);
        })
        .catch(() => {
          if (alive) setAudience(null);
        });
    }, 300);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [wantsAudience, channel, connection]);
  const suggestions = closedOn === text ? null : mentionSuggestions(text, people, everyone);
  const active = suggestions ? Math.min(picked, suggestions.found.length - 1) : 0;
  const warning = people.length > 0 ? unknownWarning(unknownMentions(text, people, archived, everyone)) : null;
  const pick = (name: string) => {
    if (!suggestions) return;
    setText(completeMention(text, suggestions.at, name));
    area.current?.focus();
  };
  const offerQuick = quick && !dismissed && text.trim() === "" ? quick : null;
  const sendNowButton =
    activeRun && ready ? (
      <Button label="Send now" onClick={() => void sendNow()}>
        Send now <KeyHint>⌥↵</KeyHint>
      </Button>
    ) : null;
  const Area = prompt ? PromptTextarea : ChatTextarea;
  const input = (
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
      <Area
        ref={area}
        rows={1}
        aria-label={placeholder}
        placeholder={placeholder}
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
  );
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
      {prompt ? (
        <PromptBox
          attach={attach}
          input={input}
          chips={footer}
          sendActions={sendNowButton}
          send={{ label: "Send", title: "Send (Enter)", disabled: busy || !ready, onClick: () => void go(text) }}
        />
      ) : (
        <ChatComposer
          {...attach.target}
          density="compact"
          elevation="none"
          xstyle={styles.composerBody}
          onSubmit={(value) => void go(value)}
          drawer={<AttachmentTray attach={attach} />}
          {...(footer ? { footerActions: footer } : {})}
          sendActions={sendNowButton}
          sendButton={
            <Button variant="primary" label="Send" isDisabled={busy || !ready} onClick={() => void go(text)} />
          }
          input={input}
        />
      )}
      {warning ? (
        <div {...stylex.props(styles.warning)}>{warning}</div>
      ) : audience ? (
        <div role="status" {...stylex.props(styles.hint)}>
          {audience}
        </div>
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
  everyone,
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
  everyone: boolean;
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
    <aside aria-label="Thread" {...stylex.props(styles.thread)}>
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
            everyone={everyone}
            prompt={TEAM_PROMPT}
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
  | { kind: "work"; id: string; w: WorkRow }
  | { kind: "approval"; id: string };

interface ChannelProps {
  channel: string | null;
  c: ChannelView | null;
  thread: string | null;
  onThread: (id: string | null) => void;
  message?: string | null;
  /** The ticket, when `c` may not be known yet. */
  ticketId?: string | null;
  /** A ticket's session fills these (ticket #70); a channel's own chat leaves them out. */
  session?: SessionSlots;
}

/** What a ticket's session puts around the channel's feed and composer. */
export interface SessionSlots {
  /** In place of the channel's head; a ticket's session puts its own head above the whole body. */
  head?: ReactNode;
  /**
   * Above the talk, scrolling with it (ticket #74): the ticket's description and what its tasks do
   * now. With it the talk gets an Activity heading with a filter.
   */
  top?: ReactNode;
  /** What the ticket's tasks read, edited and ran, merged in time into the Activity (ticket #76). */
  work?: readonly WorkRow[];
  /** In place of the progress card above the composer: what runs this second. */
  live?: ReactNode;
  /** Between the progress and the composer: the decision of the moment. */
  gate: ReactNode;
  /** Beside Send: the next run's mode and model. */
  chips?: ReactNode;
  /** The right panel while no thread is open. */
  aside?: ReactNode;
  /** A kept design file (by id) that opens in the design panel at once. */
  design?: string | null;
  /** The ticket decides, so leaving Done makes it writable before the channel's read catches up. */
  readOnly: boolean;
  readOnlyText: string;
  /** At the end of the read-only bar (Send back on Done). */
  readOnlyEnd?: ReactNode;
  placeholder: string;
  hint: string;
}

/** A ticket's session: its channel's feed and composer, with the session's head, gate and panel around them. */
export function TicketSessionBody({
  conversationId,
  ticketId,
  message,
  thread,
  onThread,
  session,
}: {
  conversationId: string;
  ticketId: string;
  message: string | null;
  thread: string | null;
  onThread: (id: string | null) => void;
  session: SessionSlots;
}) {
  const channels = useChannels();
  const c = channels.data?.find((x) => x.conversation_id === conversationId) ?? null;
  return (
    <ChannelBody
      channel={conversationId}
      c={c}
      ticketId={ticketId}
      thread={thread}
      onThread={onThread}
      message={message}
      session={session}
    />
  );
}

function ChannelBody({ channel, c, thread, onThread, message = null, ticketId = null, session }: ChannelProps) {
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
  // `@Everyone` is for board members, on a server that has it; the desktop's writer is one.
  const everyone = features.includes("everyone_mention") && user !== null && user !== undefined;
  const names = useMemo(() => [...people.map((p) => p.name), ...(everyone ? [EVERYONE] : [])], [people, everyone]);
  const store = useLiveStore(feed.data?.conversation_id);
  // An artifact opens in the right panel, in place of a thread or the ticket's panel.
  const dock = useArtifactDock();
  const live = useLiveAll(store);
  const list = useMemo(() => feed.data?.messages ?? [], [feed.data]);
  useOpenDesign(dock.open, list, session?.design ?? null);
  // What runs and what is still open stays beside the talk, never as posts in it.
  const progress = useTicketProgress(isTicket ? (ticketId ?? c?.ticket_id ?? null) : null, isTicket ? live : []);
  const { storage } = usePlatform();
  const [overviewOpen, setOverviewOpen] = useState(() => storage.get(OVERVIEW_KEY) !== "0");
  const showOverview = (on: boolean) => {
    storage.set(OVERVIEW_KEY, on ? "1" : "0");
    setOverviewOpen(on);
  };
  const talk = useMemo(() => list.filter((m) => !m.thread_parent), [list]);
  // On a ticket a row of "moved to Doing" lines folds, so what people said stays in view.
  const [unfolded, setUnfolded] = useState(message !== null);
  const folded = useMemo(
    () => (isTicket ? foldUpdates(talk, unfolded) : { shown: talk, earlier: new Map<string, number>() }),
    [isTicket, talk, unfolded],
  );
  // The ticket page's Activity filter; a channel's own chat shows everything.
  const [filter, setFilter] = useState<ActivityFilter>("all");
  const filtering = session?.top !== undefined;
  const list2 = useMemo(
    () => (filtering && filter !== "all" ? folded.shown.filter((m) => inFilter(m, filter)) : folded.shown),
    [filtering, filter, folded.shown],
  );
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
    const work = filtering && workInFilter(filter) ? (session?.work ?? []) : [];
    const out: Row[] = mergeByTime(list2, work).map((r, at, all) => {
      if (r.kind === "work") return { kind: "work", id: r.w.id, w: r.w };
      const b = before(list2, r.index, newAt, dayOf);
      // A message right after work starts its own group again.
      const afterWork = all[at - 1]?.kind === "work";
      return {
        kind: "message",
        m: r.m,
        day: b.day,
        isNew: b.new,
        grouped: b.grouped && !afterWork,
        earlier: folded.earlier.get(r.m.id) ?? 0,
      };
    });
    // What streams and what waits on you is talk, not a change.
    if (filter === "changes") return out;
    for (const u of live) {
      if (!inList.has(u.message_id) && streamTo(u.thread_parent, null) === "channel")
        out.push({ kind: "live", id: u.message_id, run: u.run_id });
    }
    for (const a of pending) if (!onCard.has(a.id)) out.push({ kind: "approval", id: a.id });
    return out;
  }, [list2, folded.earlier, newAt, live, inList, pending, onCard, filter, filtering, session?.work]);

  const readOnly = session ? session.readOnly : (c?.archived ?? false) || feed.data?.read_only === true;
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
      openArtifact: (target) => {
        onThread(null);
        dock.open(target);
      },
    }),
    [
      dock.open,
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

  // A thread that opens takes the right panel from an open artifact.
  useEffect(() => {
    if (thread) dock.close();
  }, [thread, dock.close]);

  const running =
    [...list2].reverse().find((m) => m.status === "partial" && m.run_id)?.run_id ?? live.at(-1)?.run_id ?? null;
  const retryAnswer = useRetryAnswer(feed.data?.conversation_id, list2, { channel });
  const [contextOf, setContextOf] = useState<string | null>(null);
  useAction("ShowContext", () => {
    const run = [...list2].reverse().find((m) => m.role === "assistant" && m.run_id)?.run_id;
    if (run) setContextOf(run);
    else toast("Open a conversation with an answer to see the context it used.");
  });
  useAction("RetryAnswer", retryAnswer);
  useAction("StopGeneration", () => {
    if (running)
      connection
        .write("run.stop", { run_id: running }, { retry: false })
        .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
    else toast("No answer is running here.");
  });

  const lastAnswer = [...list2].reverse().find((m) => !m.update);
  const quick =
    features.includes("quick_replies") &&
    lastAnswer?.role === "assistant" &&
    lastAnswer.status === "complete" &&
    !pending.some((a) => a.run_id === lastAnswer.run_id)
      ? quickReplies(lastAnswer.activity)
      : null;
  const owner = t && t.column !== "done" ? (t.owner_name ?? null) : null;
  const placeholder = session
    ? session.placeholder
    : isTicket && owner
      ? `Message ${owner} (owner)… · @ to mention`
      : `Message #${name} · @ to mention`;
  const hint = session
    ? session.hint
    : isTicket
      ? t
        ? `@Name asks that employee${ownerAnswers(owner)}.`
        : "@Name asks that employee about this ticket."
      : `@Name asks that employee; without a name, ${first ?? "the top of the org chart"} answers.`;
  const answering = [...new Set(live.flatMap((u) => runAuthor(employeesData ?? [], u.run_id) ?? []))];
  const loading = useDelayed(feed.isPending);
  const feedContext = useMemo<FeedContext>(
    () =>
      filtering
        ? {
            top: (
              <>
                {session?.top}
                <ActivityHead filter={filter} onFilter={setFilter} />
              </>
            ),
            empty:
              filter === "all"
                ? `Nothing said yet. Write @Name to ask an employee${t ? ownerAnswers(owner) : " about this ticket"}.`
                : filter === "changes"
                  ? "No changes yet."
                  : "No messages yet.",
          }
        : { top: null, empty: null },
    [filtering, session?.top, filter, t, owner],
  );

  return (
    <section aria-label={`#${name}`} {...stylex.props(styles.channel)}>
      <div {...stylex.props(styles.channelMain)}>
        {session ? (
          (session.head ?? null)
        ) : (
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
              <Link {...ticketPlace({ id: c.ticket_id, number: c.ticket_number })} {...stylex.props(styles.openTicket)}>
                Open ticket ↗
              </Link>
            ) : null}
            {isTicket && !overviewOpen ? (
              <button
                type="button"
                onClick={() => showOverview(true)}
                {...stylex.props(styles.openTicket, styles.pointer)}
              >
                Overview
              </button>
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
                    title="The ticket chat did not load."
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
              ) : rows.length === 0 && !filtering ? (
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
                  context={feedContext}
                  itemContent={(_, row) => (
                    <div {...FEED_COLUMN}>
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
                      ) : row.kind === "work" ? (
                        <WorkItem row={row.w} />
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
          {isTicket && !readOnly ? session?.live !== undefined ? session.live : <ProgressCard p={progress} /> : null}
          {session?.gate}
          {readOnly ? (
            <div {...stylex.props(styles.readOnlyRow)}>
              <div {...stylex.props(styles.readOnlyBar)}>
                <span {...stylex.props(styles.readOnlyText)}>
                  {session
                    ? session.readOnlyText
                    : c
                      ? readOnlyBar(c, c.ticket_column === "done")
                      : "This channel is read-only."}
                </span>
                {session?.readOnlyEnd}
                {c?.ticket_id && !session ? (
                  <Link
                    {...ticketPlace({ id: c.ticket_id, number: c.ticket_number })}
                    {...stylex.props(styles.readOnlyLink)}
                  >
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
              everyone={everyone}
              prompt={TEAM_PROMPT}
              {...(session?.chips ? { footer: session.chips } : {})}
              onSent={() => {
                atBottom.current = true;
                handle.current?.scrollToIndex({ index: "LAST", align: "end" });
              }}
            />
          )}
        </ChatProvider>
      </div>
      {dock.target ? (
        <DockedArtifact key={targetKey(dock.target)} target={dock.target} ctx={ctx} onClose={dock.close} />
      ) : thread ? (
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
            everyone={everyone}
            onClose={() => onThread(null)}
          />
        </ChatProvider>
      ) : session ? (
        (session.aside ?? null)
      ) : isTicket && overviewOpen ? (
        <OverviewPanel p={progress} onClose={() => showOverview(false)} />
      ) : null}
    </section>
  );
}
