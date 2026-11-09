import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { ChatSystemMessage } from "@astryxdesign/core/Chat";
import { Dialog, DialogHeader } from "@astryxdesign/core/Dialog";
import { IconButton } from "@astryxdesign/core/IconButton";
import { LayoutFooter } from "@astryxdesign/core/Layout";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { Text } from "@astryxdesign/core/Text";
import { TextArea } from "@astryxdesign/core/TextArea";
import { expectBody } from "@aurelius/client";
import type { ActivityView, ApprovalView, ContextView, MessageVersionView, MessageView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { createContext, memo, type ReactNode, useContext, useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useRunGuidelines } from "../data";
import { useAction } from "../keys/provider";
import { usePlatform } from "../platform";
import { Markdown } from "../ui/markdown";
import { initial, Mark, markColor } from "../ui/marks";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { useDelayed } from "../ui/use-delayed";
import { ApprovalCard, ReplyCard } from "./approval-card";
import type { LiveStore } from "./chat-live";
import { useLive } from "./chat-live";
import {
  applyReaction,
  authorOf,
  type Caps,
  canChange,
  canMakeTicket,
  clock,
  currentStep,
  dayLabel,
  fold,
  isSystemLine,
  lineMark,
  localDay,
  markMentions,
  mentionsMe,
  planTitle,
  prefill,
  REACTIONS,
  reactionOn,
  reactionTip,
  rowActions,
  runLabel,
  saveOf,
  segments,
  splitBlocks,
  systemText,
  threadLabel,
  threadLine,
  ticketFrom,
  todos,
  webSearchSummary,
} from "./chat-logic";
import { messageLink } from "./links-logic";
import { overlays } from "./overlays-store";
import { RunCard } from "./run-card";
import { waitsOnReply } from "./run-card-logic";
import {
  FileChips,
  KeptFiles,
  MarkdownChips,
  type RetryTarget,
  ScreenshotLink,
  TraceLink,
  useRetryAnswer,
} from "./run-tools";
import { stepLine } from "./tasks-logic";

const OUTPUT_LINES = 400;
const TODO_FOLD = 8;

const styles = stylex.create({
  meta: { fontSize: 12, color: "var(--precision-text3)" },
  metaWarn: { fontSize: 12, fontWeight: 500, color: "var(--color-text-warning)" },
  metaError: { fontSize: 12, color: "var(--color-text-error)" },
  text3: { color: "var(--precision-text3)" },
  text2: { color: "var(--color-text-secondary)" },
  error: { color: "var(--color-text-error)" },
  warn: { color: "var(--color-text-warning)", fontWeight: 600 },
  truncate: { minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  dayLine: { paddingBlockStart: 16, paddingBlockEnd: 4, fontSize: 12, fontWeight: 500 },
  newLine: {
    display: "flex",
    alignItems: "center",
    gap: 12,
    paddingBlock: 4,
    fontSize: 12,
    fontWeight: 500,
    color: "var(--color-text-warning)",
  },
  newRule: { height: 1, flexGrow: 1, backgroundColor: "var(--color-border-warning)" },
  output: {
    display: "flex",
    flexDirection: "column",
    gap: 4,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    paddingInline: 10,
    paddingBlock: 8,
  },
  command: {
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    wordBreak: "break-all",
    color: "var(--color-text-secondary)",
  },
  outputText: {
    margin: 0,
    maxHeight: 320,
    overflow: "auto",
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    whiteSpace: "pre-wrap",
    color: "var(--color-text-secondary)",
  },
  textButton: {
    cursor: "pointer",
    alignSelf: "flex-start",
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    fontSize: 12,
  },
  linkAccent: { color: "var(--color-text-accent)" },
  linkMuted: { color: { default: "var(--precision-text3)", ":hover": "var(--color-text-primary)" } },
  linkPlain: { color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" } },
  stepRun: {
    overflow: "hidden",
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
  },
  stepToggle: {
    display: "flex",
    height: 32,
    width: "100%",
    cursor: "pointer",
    alignItems: "center",
    gap: 10,
    borderWidth: 0,
    backgroundColor: "transparent",
    paddingInline: 10,
    textAlign: "left",
    fontSize: 12,
    color: "var(--color-text-secondary)",
  },
  stepLabel: { flexShrink: 0, fontWeight: 500, color: "var(--color-text-primary)" },
  stepLine: {
    minWidth: 0,
    flexGrow: 1,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontFamily: "var(--font-family-code)",
    color: "var(--precision-text3)",
  },
  checklist: {
    display: "flex",
    flexDirection: "column",
    gap: 4,
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    paddingInline: 12,
    paddingBlock: 8,
  },
  todo: { display: "flex", gap: 8 },
  todoDone: { color: "var(--precision-text3)", textDecoration: "line-through" },
  todoDoing: { fontWeight: 500 },
  todoOpen: { color: "var(--color-text-secondary)" },
  minZero: { minWidth: 0 },
  answer: { display: "flex", flexDirection: "column", gap: 10 },
  liveStatus: { display: "flex", alignItems: "center", gap: 8, fontSize: 12, color: "var(--color-text-accent)" },
  dialogBody: { display: "flex", flexDirection: "column", gap: 12, overflowY: "auto", padding: 16 },
  group: { display: "flex", flexDirection: "column", gap: 4 },
  contextLine: { display: "flex", justifyContent: "space-between", gap: 12 },
  codeMeta: {
    flexShrink: 0,
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  noShrink: { flexShrink: 0 },
  version: {
    display: "flex",
    flexDirection: "column",
    gap: 4,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    paddingTop: 8,
  },
  header: { display: "flex", flexWrap: "wrap", alignItems: "baseline", gap: 8 },
  system: {
    display: "flex",
    flexDirection: "column",
    gap: 2,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    paddingInline: 12,
    paddingBlock: 8,
    fontSize: 13,
  },
  systemNormal: {
    borderColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    color: "var(--color-text-secondary)",
  },
  systemFailed: { borderColor: "var(--color-text-error)", color: "var(--color-text-error)" },
  systemBare: { paddingInline: 40, fontSize: 12, lineHeight: 1.5, color: "var(--precision-text3)" },
  systemTop: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 8 },
  grow: { minWidth: 0, flexGrow: 1, flexBasis: 0 },
  accentLink: { fontSize: 12, color: "var(--color-text-accent)", textDecoration: "none" },
  update: {
    display: "flex",
    flexDirection: "column",
    gap: 6,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    paddingInline: 14,
    paddingBlock: 11,
  },
  updateUrgent: { borderColor: "var(--color-border-warning)", backgroundColor: "var(--color-warning-muted)" },
  updateNormal: { borderColor: "var(--color-border)", backgroundColor: "var(--precision-sunk)" },
  updateTop: { display: "flex", alignItems: "center", gap: 8, fontSize: 11.5, color: "var(--precision-text3)" },
  updateLabel: { fontWeight: 600, letterSpacing: "0.04em" },
  updateAuthor: {
    minWidth: 0,
    flexGrow: 1,
    flexBasis: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: "var(--color-text-secondary)",
  },
  plainButton: { cursor: "pointer", borderWidth: 0, backgroundColor: "transparent", padding: 0, fontSize: "inherit" },
  queued: { display: "flex", alignItems: "center", gap: 12, fontSize: 12, color: "var(--precision-text3)" },
  reactions: { display: "flex", flexWrap: "wrap", gap: 4 },
  reaction: {
    display: "inline-flex",
    height: 22,
    cursor: "pointer",
    alignItems: "center",
    gap: 4,
    borderRadius: 11,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "transparent",
    paddingInline: 7,
    fontSize: 12,
  },
  reactionMine: { borderColor: "var(--color-text-accent)", backgroundColor: "var(--color-overlay-hover)" },
  editBox: { display: "flex", flexDirection: "column", gap: 8 },
  editActions: { display: "flex", gap: 8 },
  // In the rows' group-hover: hidden until the row is hovered or holds focus, and open while its menu is.
  rowBar: {
    position: "absolute",
    top: -12,
    right: 8,
    display: {
      default: "none",
      [stylex.when.ancestor(":hover")]: "flex",
      [stylex.when.ancestor(":focus-within")]: "flex",
      ":has([aria-expanded='true'])": "flex",
    },
    gap: 1,
    borderRadius: 7,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-surface)",
    padding: 2,
    boxShadow: "var(--precision-card-shadow)",
  },
  threadLink: {
    display: "flex",
    height: 28,
    cursor: "pointer",
    alignItems: "center",
    alignSelf: "flex-start",
    gap: 8,
    borderRadius: 6,
    borderWidth: 0,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    paddingInline: 4,
    fontSize: 12,
    color: "var(--color-text-accent)",
  },
  marks: { display: "flex" },
  markOverlap: { marginLeft: { default: -4, ":first-child": 0 } },
  row: {
    position: "relative",
    display: "flex",
    gap: 12,
    borderRadius: 6,
    backgroundColor: {
      default: "transparent",
      ":hover": "color-mix(in srgb, var(--color-overlay-hover) 45%, transparent)",
    },
  },
  rowGrouped: { paddingTop: 2 },
  rowFull: { paddingTop: 8 },
  rowFlash: { backgroundColor: "var(--color-overlay-hover)" },
  markColumn: { width: 28, flexShrink: 0 },
  body: { display: "flex", minWidth: 0, flexGrow: 1, flexBasis: 0, flexDirection: "column", gap: 4 },
  ticketChip: {
    borderRadius: 9,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    paddingInline: 6,
    fontSize: 12,
    color: "var(--color-text-accent)",
    textDecoration: "none",
  },
  answerLinks: { display: "flex", gap: 12 },
  liveRow: { display: "flex", gap: 12, paddingTop: 8 },
  liveName: { display: "flex", alignItems: "baseline", gap: 8 },
  footer: { display: "flex", justifyContent: "flex-end", gap: 8 },
});

/** The small, quiet line used all over a row. */
function Meta({ children }: { children: ReactNode }) {
  return (
    <Text type="supporting" xstyle={styles.meta}>
      {children}
    </Text>
  );
}

export interface ChatContext {
  store: LiveStore;
  caps: Caps;
  features: string[];
  people: string[];
  me: string | null;
  place: string;
  ticketNumber: number | null;
  roleOf: (name: string) => string | null;
  approvals: Map<string, ApprovalView>;
  openThread: (id: string) => void;
  messages: MessageView[];
  lastAnswer: string | null;
  conversationId: string | null;
  channel: string | null;
  /** Where a retried question goes; none: the channel. */
  retryTarget?: RetryTarget;
  /** The message a link led to, marked for a moment. */
  flash: string | null;
}

const Context = createContext<ChatContext | null>(null);
export const ChatProvider = Context.Provider;
function useChat(): ChatContext {
  const value = useContext(Context);
  if (!value) throw new Error("No chat.");
  return value;
}

const offset = () => -new Date().getTimezoneOffset();
const today = () => localDay(Date.now(), offset());

/** Change one message wherever it is cached: the channel, a thread, a discussion. */
export function usePatchMessage() {
  const client = useQueryClient();
  return (id: string, change: (m: MessageView) => MessageView) => {
    const inList = (list: MessageView[]) => list.map((m) => (m.id === id ? change(m) : m));
    client.setQueriesData<Record<string, unknown>>({ queryKey: ["messages"] }, (old) => {
      if (!old || typeof old !== "object" || Array.isArray(old)) return old;
      const next = { ...old };
      if (Array.isArray(old.messages)) next.messages = inList(old.messages as MessageView[]);
      if (Array.isArray(old.replies)) next.replies = inList(old.replies as MessageView[]);
      if (old.root && (old.root as MessageView).id === id) next.root = change(old.root as MessageView);
      return next;
    });
  };
}

export function DayLine({ ms }: { ms: number }) {
  return (
    <ChatSystemMessage variant="divider" xstyle={styles.dayLine}>
      {dayLabel(localDay(ms, offset()), today())}
    </ChatSystemMessage>
  );
}

export function NewLine() {
  return (
    <div {...stylex.props(styles.newLine)}>
      <span {...stylex.props(styles.newRule)} />
      New messages
      <span {...stylex.props(styles.newRule)} />
    </div>
  );
}

export function ActivityOutput({ step, messageId }: { step: ActivityView; messageId: string }) {
  const connection = useConnection();
  const [full, setFull] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const raw = full ?? step.output ?? "";
  const output = step.tool === "WebSearch" ? webSearchSummary(raw) : raw;
  const lines = output.split("\n");
  const code = stepLine(step);
  const load = async () => {
    setBusy(true);
    try {
      const body = expectBody(
        await connection.call("activity.get", { message_id: messageId, call_id: step.call_id }),
        "activity",
      ).value;
      setFull(body.output ?? "");
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <div {...stylex.props(styles.output)}>
      <Meta>{step.tool}</Meta>
      {code ? <code {...stylex.props(styles.command)}>&gt; {code}</code> : null}
      <ScreenshotLink tool={step.tool} output={step.output} />
      {step.tool === "WebSearch" && raw !== output ? (
        <Markdown text={output} />
      ) : output ? (
        <pre {...stylex.props(styles.outputText)}>
          {lines.slice(0, OUTPUT_LINES).join("\n")}
          {lines.length > OUTPUT_LINES ? `\n… ${lines.length - OUTPUT_LINES} more lines` : ""}
        </pre>
      ) : null}
      {step.truncated && full === null ? (
        <button
          type="button"
          disabled={busy}
          onClick={() => void load()}
          {...stylex.props(styles.textButton, styles.linkAccent)}
        >
          {busy ? "Loading…" : "Show full output"}
        </button>
      ) : null}
    </div>
  );
}

function StepRun({ steps, messageId }: { steps: ActivityView[]; messageId: string }) {
  const [open, setOpen] = useState(false);
  const last = steps.at(-1) as ActivityView;
  const failed = steps.some((s) => s.ok === false);
  return (
    <div {...stylex.props(styles.stepRun)}>
      <button type="button" aria-expanded={open} onClick={() => setOpen(!open)} {...stylex.props(styles.stepToggle)}>
        <span aria-hidden="true">{open ? "⌄" : "›"}</span>
        <span {...stylex.props(styles.stepLabel, failed && styles.error)}>{runLabel(steps)}</span>
        {open ? null : <span {...stylex.props(styles.stepLine)}>{stepLine(last)}</span>}
      </button>
      {open ? steps.map((s) => <ActivityOutput key={s.call_id} step={s} messageId={messageId} />) : null}
    </div>
  );
}

function Checklist({ activity }: { activity: ActivityView[] }) {
  const [all, setAll] = useState(false);
  const list = todos(activity);
  if (!list) return null;
  const done = list.filter((t) => t.state === "done").length;
  const folded = list.length > TODO_FOLD && !all;
  const shown = folded ? list.filter((t) => t.state === "doing").slice(0, 1) : list;
  return (
    <div {...stylex.props(styles.checklist)}>
      {shown.map((t) => (
        <div
          key={t.text}
          {...stylex.props(
            styles.todo,
            t.state === "done" ? styles.todoDone : t.state === "doing" ? styles.todoDoing : styles.todoOpen,
          )}
        >
          <span aria-hidden="true">{t.state === "done" ? "✓" : t.state === "doing" ? "▸" : "○"}</span>
          <span {...stylex.props(styles.minZero)}>{t.text}</span>
        </div>
      ))}
      {list.length > TODO_FOLD ? (
        <button type="button" onClick={() => setAll(!all)} {...linkish}>
          {folded ? `${done} of ${list.length} done · show all` : "Show less"}
        </button>
      ) : null}
    </div>
  );
}

/** Long text folds; what was seen while it streamed stays open. */
function Prose({ text, seenLive }: { text: string; seenLive: boolean }) {
  const { people, me } = useChat();
  const [open, setOpen] = useState(false);
  const folded = useMemo(() => (seenLive ? null : fold(text)), [text, seenLive]);
  const shown = folded && !open ? folded.shown : text;
  const marked = useMemo(() => markMentions(shown, people, me), [shown, people, me]);
  return (
    <>
      <Markdown text={marked} />
      {folded ? (
        <button type="button" onClick={() => setOpen(!open)} {...linkish}>
          {open ? "▴ Show less" : `▾ Show more (${folded.hidden} lines)`}
        </button>
      ) : null}
    </>
  );
}

/** While it is written each finished block stays put; only the last one changes. */
function LiveText({ text }: { text: string }) {
  const blocks = splitBlocks(text);
  return (
    <>
      {blocks.map((block, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: blocks only ever grow at the end.
        <Markdown key={i} text={block} live streaming={i === blocks.length - 1} />
      ))}
    </>
  );
}

function statusLine(activity: ActivityView[], text: string, waiting: boolean): string {
  if (waiting) return "Waiting for your approval";
  const step = currentStep(activity);
  if (step) return step;
  return text ? "Writing…" : "Thinking…";
}

function AnswerBody({
  m,
  text,
  activity,
  live,
}: {
  m: MessageView;
  text: string;
  activity: ActivityView[];
  live: boolean;
}) {
  const { store } = useChat();
  const parts = useMemo(() => segments(text, activity), [text, activity]);
  return (
    <div {...stylex.props(styles.answer)}>
      <Checklist activity={activity} />
      {parts.map((part, i) =>
        part.kind === "steps" ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts only ever grow at the end.
          <StepRun key={i} steps={part.steps} messageId={m.id} />
        ) : live ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts only ever grow at the end.
          <LiveText key={i} text={part.text} />
        ) : (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts only ever grow at the end.
          <Prose key={i} text={part.text} seenLive={store.seen.has(m.id)} />
        ),
      )}
    </div>
  );
}

function StopButton({ run }: { run: string }) {
  const connection = useConnection();
  return (
    <IconButton
      size="sm"
      variant="ghost"
      label="Stop"
      tooltip="Stop"
      icon={<span aria-hidden="true">■</span>}
      onClick={() =>
        connection
          .write("run.stop", { run_id: run }, { retry: false })
          .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."))
      }
    />
  );
}

/** Worked out once: most rows have a few of these. */
const linkish = stylex.props(styles.textButton, styles.linkMuted);

function ContextLink({ run }: { run: string }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)} {...linkish}>
        ≡ Context
      </button>
      {open ? <ContextDialog run={run} onClose={() => setOpen(false)} /> : null}
    </>
  );
}

/** What an answer was given: its full list, with why each part did or didn't go in. */
export function ContextDialog({ run, onClose }: { run: string; onClose: () => void }) {
  const connection = useConnection();
  const guidelines = useRunGuidelines(run);
  const list = guidelines.data?.guidelines ?? [];
  const [view, setView] = useState<ContextView | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    connection
      .call("run.get_context", { run_id: run })
      .then((body) => setView(expectBody(body, "context").value))
      .catch((e: unknown) => setProblem(e instanceof Error ? e.message : "This did not load."));
  }, [connection, run]);
  const sections = view ? [...new Set(view.items.map((i) => i.section))] : [];
  const loading = useDelayed(!view && !problem);
  const close = (open: boolean) => {
    if (!open) onClose();
  };
  return (
    <Dialog isOpen onOpenChange={close} width={720} maxHeight="calc(100vh - 48px)" purpose="form" padding={0}>
      <DialogHeader title="Context of this answer" onOpenChange={close} />
      <div {...stylex.props(styles.dialogBody)}>
        {view ? (
          <Text as="p" type="supporting" xstyle={styles.meta}>
            {view.runtime_managed
              ? "The runtime manages its own context."
              : `${view.used_tokens} of ${view.budget_tokens} tokens`}
            {view.scopes.length > 0 ? ` · ${view.scopes.join(", ")}` : ""}
          </Text>
        ) : null}
        {problem ? <Banner status="error" role="alert" title={problem} /> : null}
        {loading ? <Skeleton height={48} /> : null}
        {sections.map((section) => (
          <section key={section} {...stylex.props(styles.group)}>
            <Meta>{section}</Meta>
            {view?.items
              .filter((i) => i.section === section)
              .map((i) => (
                <div key={i.id} {...stylex.props(styles.contextLine, !i.included && styles.text3)}>
                  <span {...stylex.props(styles.minZero)}>
                    {i.included ? "" : "Left out: "}
                    {i.label}
                    {i.reason ? <span {...stylex.props(styles.text3)}> · {i.reason}</span> : null}
                  </span>
                  <span {...stylex.props(styles.codeMeta)}>
                    {i.tokens} tok{typeof i.score === "number" ? ` · ${i.score.toFixed(2)}` : ""}
                  </span>
                </div>
              ))}
          </section>
        ))}
        {list.length > 0 ? (
          <section {...stylex.props(styles.group)}>
            <Meta>Guidelines</Meta>
            {list.map((g) => (
              <div key={g.guideline_id} {...stylex.props(styles.contextLine)}>
                <span {...stylex.props(styles.truncate)}>{g.title}</span>
                <span {...stylex.props(styles.noShrink)}>
                  <Meta>
                    v{g.version} · {g.chars} chars
                  </Meta>
                </span>
              </div>
            ))}
          </section>
        ) : null}
        <Meta>Only permitted scopes are searched, before any text is loaded. Memory is data, not instructions.</Meta>
      </div>
    </Dialog>
  );
}

function EditedLink({ id }: { id: string }) {
  const connection = useConnection();
  const [versions, setVersions] = useState<MessageVersionView[] | null>(null);
  const load = async () => {
    try {
      setVersions(
        expectBody(await connection.call("message.history", { message_id: id }), "message_history").value.versions,
      );
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  return (
    <>
      <button type="button" onClick={() => void load()} {...linkish}>
        edited
      </button>
      {versions ? (
        <Dialog
          isOpen
          onOpenChange={(open) => {
            if (!open) setVersions(null);
          }}
          width={560}
          maxHeight="calc(100vh - 48px)"
          purpose="form"
          padding={0}
        >
          <DialogHeader
            title="Earlier versions"
            onOpenChange={(open) => {
              if (!open) setVersions(null);
            }}
          />
          <div {...stylex.props(styles.dialogBody)}>
            {versions.length === 0 ? <Text color="secondary">The earlier text is gone.</Text> : null}
            {versions.map((v) => (
              <div key={v.replaced_at_ms} {...stylex.props(styles.version)}>
                <Meta>{clock(v.replaced_at_ms, offset())}</Meta>
                <Markdown text={v.text} />
              </div>
            ))}
          </div>
        </Dialog>
      ) : null}
    </>
  );
}

/** An answer in its own row while it streams: only this re-renders. */
function LiveAnswer({ m, waiting }: { m: MessageView; waiting: boolean }) {
  const { store } = useChat();
  const update = useLive(store, m.id);
  const text = update?.text ?? m.text;
  const activity = update?.activity ?? m.activity;
  return (
    <>
      <AnswerBody m={m} text={text} activity={activity} live />
      <div {...stylex.props(styles.liveStatus)}>
        {m.run_id ? <StopButton run={m.run_id} /> : null}

        {update?.progress?.compacting ? "Context comprimeren…" : statusLine(activity, text, waiting)}
      </div>
    </>
  );
}

function Header({
  name,
  kind,
  human,
  at,
  children,
}: {
  name: string;
  kind: string;
  human: boolean;
  at: number;
  children?: React.ReactNode;
}) {
  return (
    <div {...stylex.props(styles.header)}>
      <Text weight="semibold">{name}</Text>
      <Meta>{human ? "· board member" : `— ${kind}`}</Meta>
      <Meta>{clock(at, offset())}</Meta>
      {children}
    </div>
  );
}

function SystemRow({ m }: { m: MessageView }) {
  const { ticketNumber } = useChat();
  const card = m.run ? <RunCard m={m} Output={ActivityOutput} /> : null;
  const line = m.system;
  if (!line) return <div {...stylex.props(styles.systemBare)}>{m.text}</div>;
  const text = systemText(line, m.text, ticketNumber);
  const { mark, failed } = lineMark(line, text);
  const [first, reason] = text.split(" Reason:");
  return (
    <section
      {...stylex.props(styles.system, failed ? styles.systemFailed : styles.systemNormal)}
      aria-label={`System: ${text} at ${clock(m.created_at_ms, offset())}`}
    >
      <div {...stylex.props(styles.systemTop)}>
        <span {...stylex.props(styles.grow)}>
          {mark}
          {first}
        </span>
        {line.task_id ? (
          <Link to="/tasks/$taskId" params={{ taskId: line.task_id }} {...stylex.props(styles.accentLink)}>
            Open task →
          </Link>
        ) : line.ticket_id ? (
          <Link to="/board" search={{ ticket: line.ticket_id }} {...stylex.props(styles.accentLink)}>
            Open ticket →
          </Link>
        ) : null}
      </div>
      {reason ? (
        <Text type="supporting" maxLines={2} hasTruncateTooltip={false} xstyle={styles.meta}>
          Reason:{reason}
        </Text>
      ) : null}
      {card}
    </section>
  );
}

function UpdateCard({ m }: { m: MessageView }) {
  const { approvals } = useChat();
  const pending = m.approval_id ? approvals.get(m.approval_id) : undefined;
  const isPlan = m.update === "plan";
  const [choice, setChoice] = useState<boolean | null>(null);
  const open = !isPlan || (choice ?? Boolean(pending));
  useAction("TogglePlanFold", () => setChoice(!open), isPlan);
  const label = ((): [string, boolean] => {
    switch (m.update) {
      case "brief":
        return ["TASK", true];
      case "finding":
        return ["FINDING", false];
      case "question":
        return ["NEEDS A DECISION", true];
      case "approval":
        return pending ? ["WAITING FOR YOU", true] : ["ASKED", false];
      case "plan":
        return pending ? ["PLAN · WAITING FOR YOU", true] : ["PLAN", false];
      case "message":
        return ["MESSAGE FROM", false];
      case "mode_chosen":
        return ["MODE CHOSEN", false];
      case "ticket_note":
        return ["FROM A TICKET", false];
      case "scheduled":
        return ["SCHEDULED", false];
      default:
        return ["UPDATE", false];
    }
  })();
  return (
    <div {...stylex.props(styles.update, label[1] ? styles.updateUrgent : styles.updateNormal)}>
      <div {...stylex.props(styles.updateTop)}>
        <span {...stylex.props(styles.updateLabel)}>{label[0]}</span>
        <span {...stylex.props(styles.updateAuthor)}>{m.author ?? "Aurelius"}</span>
        {isPlan ? (
          <button
            type="button"
            aria-expanded={open}
            onClick={() => setChoice(!open)}
            {...stylex.props(styles.plainButton, styles.linkPlain)}
          >
            {open ? "▾ Hide plan" : "▸ Show plan"}
          </button>
        ) : null}
      </div>
      {open ? (
        <Prose text={m.text} seenLive={false} />
      ) : (
        <span {...stylex.props(styles.truncate, styles.text2)}>{planTitle(m.text)}</span>
      )}
      {pending ? <ApprovalCard approval={pending} /> : null}
    </div>
  );
}

function Queued({ m }: { m: MessageView }) {
  const connection = useConnection();
  const client = useQueryClient();
  const act = async (method: "conversation.delete_queued" | "conversation.send_queued_now", conversationId: string) => {
    try {
      await connection.write(method, { conversation_id: conversationId, message_id: m.id }, { retry: false });
      await client.invalidateQueries({ queryKey: ["messages"] });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  const { conversationId } = useFeedIds();
  return (
    <div {...stylex.props(styles.queued)}>
      Waiting · sends after the current answer
      {conversationId ? (
        <>
          <button
            type="button"
            onClick={() => void act("conversation.delete_queued", conversationId)}
            {...stylex.props(styles.plainButton, styles.linkPlain)}
          >
            Delete
          </button>
          <button
            type="button"
            onClick={() => void act("conversation.send_queued_now", conversationId)}
            {...stylex.props(styles.plainButton, styles.linkAccent)}
          >
            Send now
          </button>
        </>
      ) : null}
    </div>
  );
}

const FeedIds = createContext<{ conversationId: string | null }>({ conversationId: null });
export const FeedIdsProvider = FeedIds.Provider;
const useFeedIds = () => useContext(FeedIds);

function Reactions({ m, can }: { m: MessageView; can: boolean }) {
  const list = m.reactions ?? [];
  const react = useReact(m);
  if (list.length === 0) return null;
  return (
    <div {...stylex.props(styles.reactions)}>
      {list.map((r) => (
        <button
          key={r.emoji}
          type="button"
          aria-pressed={r.mine}
          title={reactionTip(r)}
          disabled={!can}
          onClick={() => react(r.emoji)}
          {...stylex.props(styles.reaction, r.mine && styles.reactionMine)}
        >
          {r.emoji} <span {...stylex.props(styles.text2)}>{r.count}</span>
        </button>
      ))}
    </div>
  );
}

function useReact(m: MessageView) {
  const connection = useConnection();
  const client = useQueryClient();
  const patch = usePatchMessage();
  const { me } = useChat();
  return (emoji: string) => {
    const on = reactionOn(m.reactions ?? [], emoji);
    patch(m.id, (x) => ({ ...x, reactions: applyReaction(x.reactions ?? [], emoji, on, me) }));
    connection.write("message.react", { message_id: m.id, emoji, on }, { retry: false }).catch((e: unknown) => {
      void client.invalidateQueries({ queryKey: ["messages"] });
      toast(e instanceof Error ? e.message : "This did not work.");
    });
  };
}

function EditBox({ m, onDone }: { m: MessageView; onDone: () => void }) {
  const connection = useConnection();
  const client = useQueryClient();
  const [text, setText] = useState(m.text);
  const [gone, setGone] = useState<string | null>(null);
  const platform = usePlatform();
  const save = saveOf(m.text, text);
  const go = async () => {
    if (save.kind === "close") return onDone();
    if (save.kind !== "send") return;
    try {
      await connection.write("message.edit", { message_id: m.id, text: save.text }, { retry: false });
      await client.invalidateQueries({ queryKey: ["messages"] });
      onDone();
    } catch (e) {
      setGone(e instanceof Error ? e.message : "This did not work.");
    }
  };
  return (
    <div {...stylex.props(styles.editBox)}>
      {gone ? (
        <Banner
          status="error"
          role="alert"
          title={gone}
          endContent={
            <Button
              label="Copy my text"
              onClick={() => {
                void platform.copy(text).then(() => toast("Copied your text."));
                onDone();
              }}
            />
          }
        />
      ) : null}
      <TextArea
        label="Edit message"
        isLabelHidden
        ref={(el) => el?.focus({ preventScroll: true })}
        rows={Math.min(10, text.split("\n").length + 1)}
        width="100%"
        value={text}
        onChange={setText}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            void go();
          }
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            onDone();
          }
        }}
      />
      {save.kind === "off" ? <Meta>To remove it, delete the message.</Meta> : null}
      <div {...stylex.props(styles.editActions)}>
        <Button variant="primary" label="Save" isDisabled={save.kind === "off"} onClick={() => void go()} />
        <Button label="Cancel" onClick={onDone} />
      </div>
    </div>
  );
}

function RowBar({
  m,
  onEdit,
  onDelete,
  name,
}: {
  m: MessageView;
  onEdit: () => void;
  onDelete: () => void;
  name: string;
}) {
  const platform = usePlatform();
  const { caps, openThread, features, place } = useChat();
  const actions = rowActions(m, caps);
  const react = useReact(m);
  const editable = canChange(m, features.includes("message_edits"));
  const makeTicket = features.includes("ticket_discussion") && canMakeTicket(m);
  const { hostId } = useLinkState();
  const linkable = hostId !== null && features.includes("message_locate") && actions.react && !m.deleted_at_ms;
  const more = [
    { key: "copy", label: "Copy" },
    ...(linkable ? [{ key: "link", label: "Copy link" }] : []),
    ...(makeTicket ? [{ key: "ticket", label: "Create ticket…" }] : []),
    ...(editable
      ? [
          { key: "edit", label: "Edit" },
          { key: "delete", label: "Delete" },
        ]
      : []),
  ];
  const pick = (key: string) => {
    if (key === "copy") void platform.copy(m.text).catch(() => toast("This did not copy."));
    if (key === "link") void platform.copy(messageLink(hostId ?? "", m.id)).then(() => toast("Link copied."));
    if (key === "edit") onEdit();
    if (key === "delete") onDelete();
    if (key === "ticket")
      overlays.open("ticket", { ...prefill(m.text, name, place, clock(m.created_at_ms, offset())), fromMessage: m.id });
  };
  const glyph = (text: string) => <span aria-hidden="true">{text}</span>;
  return (
    <div {...stylex.props(styles.rowBar)}>
      {actions.react ? (
        <Menu
          label="React"
          options={REACTIONS.map((e) => ({ key: e, label: reactionOn(m.reactions ?? [], e) ? e : `${e}  Take back` }))}
          onPick={react}
          trigger={<IconButton size="sm" variant="ghost" label="React" icon={glyph("☺+")} />}
        />
      ) : null}
      {actions.reply ? (
        <IconButton
          size="sm"
          variant="ghost"
          label="Reply in thread"
          icon={glyph("↳")}
          onClick={() => openThread(m.id)}
        />
      ) : null}
      <Menu
        label="More actions"
        options={more}
        onPick={pick}
        trigger={<IconButton size="sm" variant="ghost" label="More actions" icon={glyph("⋯")} />}
      />
    </div>
  );
}

function ThreadLink({ m }: { m: MessageView }) {
  const { openThread } = useChat();
  const line = threadLine(m, offset(), today());
  if (!line) return null;
  return (
    <button
      type="button"
      aria-label={threadLabel(m, offset(), today())}
      onClick={() => openThread(m.id)}
      {...stylex.props(styles.threadLink)}
    >
      <span {...stylex.props(styles.marks)}>
        {(m.thread_repliers ?? []).slice(0, 3).map((name) => (
          <span key={name} {...stylex.props(styles.markOverlap)}>
            <Mark letter={initial(name)} color={markColor(name)} size={18} radius={5} />
          </span>
        ))}
      </span>
      {line}
      {(m.thread_unread ?? 0) > 0 ? <span {...stylex.props(styles.warn)}>{m.thread_unread} new</span> : null}
    </button>
  );
}

export const MessageRow = memo(function MessageRow({
  m,
  grouped,
  inThread = false,
}: {
  m: MessageView;
  grouped: boolean;
  inThread?: boolean;
}) {
  const { roleOf, caps, approvals, me, messages, lastAnswer, conversationId, channel, retryTarget } = useChat();
  const made = useMemo(() => ticketFrom(messages, m.id), [messages, m.id]);
  const { flash } = useChat();
  const connection = useConnection();
  const client = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  // A row's menus are only built once it is pointed at or focused: with 2,000 messages, scrolling mounts many rows.
  const [touched, setTouched] = useState(false);
  const touch = () => setTouched(true);
  const retry = useRetryAnswer(
    conversationId,
    messages,
    retryTarget ?? { channel, threadParent: m.thread_parent ?? null },
  );
  if (m.system || isSystemLine(m, true)) return <SystemRow m={m} />;
  const who = authorOf(m, roleOf);
  const body = m.deleted_at_ms ? (
    <span {...stylex.props(styles.text3)}>⊘ This message was deleted.</span>
  ) : editing ? (
    <EditBox m={m} onDone={() => setEditing(false)} />
  ) : m.update && m.update !== "group" ? (
    <UpdateCard m={m} />
  ) : m.status === "partial" ? (
    <LiveAnswer m={m} waiting={[...approvals.values()].some((a) => a.run_id === m.run_id)} />
  ) : (
    <>
      <AnswerBody m={m} text={m.text} activity={m.activity} live={false} />
      <MarkdownChips
        text={m.text}
        scope={conversationId ? { kind: "conversation", conversation_id: conversationId } : null}
      />
      <FileChips files={m.uploads} />
      <KeptFiles files={m.files ?? []} />
      {m.id === lastAnswer && m.status === "complete" && m.run_id ? (
        <span {...stylex.props(styles.answerLinks)}>
          <ContextLink run={m.run_id} />
          <TraceLink run={m.run_id} />
        </span>
      ) : null}
      {m.status === "interrupted" ? <Meta>Stopped; text up to that point is kept.</Meta> : null}
      {m.status === "failed" ? (
        <span {...stylex.props(styles.metaError)}>Failed; this doesn't count as an answer.</span>
      ) : null}
      {m.id === lastAnswer && (m.status === "failed" || m.status === "interrupted") ? (
        <button type="button" onClick={retry} {...linkish}>
          ↻ Retry answer
        </button>
      ) : null}
    </>
  );
  const remove = async () => {
    try {
      await connection.write("message.delete", { message_id: m.id }, { retry: false });
      await client.invalidateQueries({ queryKey: ["messages"] });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  return (
    <div
      onPointerEnter={touch}
      onFocusCapture={touch}
      {...stylex.props(
        styles.row,
        grouped ? styles.rowGrouped : styles.rowFull,
        flash === m.id && styles.rowFlash,
        stylex.defaultMarker(),
      )}
    >
      <div {...stylex.props(styles.markColumn)}>
        {grouped ? null : (
          <Mark
            letter={initial(who.name)}
            color={who.human ? "#55595F" : markColor(who.name)}
            size={28}
            radius={who.human ? 14 : 7}
          />
        )}
      </div>
      <div data-message-body {...stylex.props(styles.body)}>
        {grouped ? null : (
          <Header name={who.name} kind={who.kind} human={who.human} at={m.created_at_ms}>
            {mentionsMe(m.text, me) && m.role !== "user" ? <span {...stylex.props(styles.metaWarn)}>@ you</span> : null}
            {m.edited_at_ms && !m.deleted_at_ms ? <EditedLink id={m.id} /> : null}
            {made ? (
              made.ticket ? (
                <Link to="/board" search={{ ticket: made.ticket }} {...stylex.props(styles.ticketChip)}>
                  → {made.label}
                </Link>
              ) : (
                <Meta>→ {made.label}</Meta>
              )
            ) : null}
          </Header>
        )}
        {body}
        {m.queued ? <Queued m={m} /> : null}
        {m.run ? <RunCard m={m} Output={ActivityOutput} /> : null}
        {!caps.readOnly && waitsOnReply(m, [...approvals.values()]) ? (
          <ReplyCard
            who={who.name}
            onSend={(text) =>
              connection.write(
                "group_chat.send",
                { text, images: [], files: [], channel, thread_parent: m.thread_parent ?? null, confirm_secret: false },
                { retry: false },
              )
            }
          />
        ) : null}
        <Reactions m={m} can={!caps.readOnly} />
        {inThread ? null : <ThreadLink m={m} />}
      </div>
      {!touched || editing || m.deleted_at_ms || m.status === "partial" ? null : (
        <RowBar m={m} name={who.name} onEdit={() => setEditing(true)} onDelete={() => setDeleting(true)} />
      )}
      {deleting ? (
        <Dialog
          isOpen
          onOpenChange={(open) => {
            if (!open) setDeleting(false);
          }}
          width={440}
          maxHeight="calc(100vh - 48px)"
          purpose="form"
          padding={0}
        >
          <DialogHeader title="Delete this message?" />
          <div {...stylex.props(styles.dialogBody)}>
            <Text color="secondary">The channel keeps a line saying a message was deleted.</Text>
          </div>
          <LayoutFooter hasDivider>
            <div {...stylex.props(styles.footer)}>
              <Button label="Cancel" onClick={() => setDeleting(false)} />
              <Button
                data-autofocus
                variant="destructive"
                label="Delete"
                onClick={() => {
                  setDeleting(false);
                  void remove();
                }}
              />
            </div>
          </LayoutFooter>
        </Dialog>
      ) : null}
    </div>
  );
});

/** An answer the list does not hold yet. */
export function LiveRow({ id, name, kind }: { id: string; name: string; kind: string }) {
  const { store } = useChat();
  const update = useLive(store, id);
  if (!update) return null;
  const stub: MessageView = {
    id,
    activity: [],
    created_at_ms: Date.now(),
    files: [],
    ordinal: 0,
    queued: false,
    role: "assistant",
    sources: [],
    status: "partial",
    text: "",
    uploads: [],
  };
  return (
    <div {...stylex.props(styles.liveRow)}>
      <div {...stylex.props(styles.markColumn)}>
        <Mark letter={initial(name)} color={markColor(name)} size={28} radius={7} />
      </div>
      <div data-message-body {...stylex.props(styles.body)}>
        <div {...stylex.props(styles.liveName)}>
          <Text weight="semibold">{name}</Text>
          <Meta>— {kind}</Meta>
        </div>
        <LiveAnswer m={stub} waiting={false} />
      </div>
    </div>
  );
}
