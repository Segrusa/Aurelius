// A plan in the dock at the right of a conversation, where it is read and decided: its text with
// comments made from a selection, and the decision in a footer that never scrolls away. Where the
// row is too narrow for the feed beside it, the same parts open in a dialog. A plan is Markdown,
// so unlike a design it needs no sandbox and shows on the web too.

import { Badge } from "@astryxdesign/core/Badge";
import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { IconButton } from "@astryxdesign/core/IconButton";
import { ResizeHandle, useResizable } from "@astryxdesign/core/Resizable";
import { percent, pixel } from "@astryxdesign/core/Resizable/utils";
import { TextArea } from "@astryxdesign/core/TextArea";
import { TextInput } from "@astryxdesign/core/TextInput";
import type { ApprovalView, MessageView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useRouterState } from "@tanstack/react-router";
import {
  type KeyboardEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useAction, useKeyContext } from "../keys/provider";
import { Markdown } from "../ui/markdown";
import { PLAN_MISSING, PlanButtons, type PlanDecisionState, RejectKey, usePlanDecision } from "./approval-card";
import type { ArtifactTarget } from "./artifact-dock";
import { DOCK_DEFAULT_PERCENT, DOCK_MAX_PERCENT, DOCK_MIN_PX, DOCK_SAVE_ID } from "./artifact-viewer";
import { clock, planSummary, planTitle } from "./chat-logic";
import type { ChatContext } from "./chat-message";
import { FormDialog } from "./org-kit";
import {
  decidedLine,
  findQuote,
  latestPlan,
  planDecision,
  planFromSearch,
  planStatus,
  shouldAutoOpenPlan,
  showsPlan,
} from "./plan-logic";
import type { PlanComment } from "./run-logic";

/** The narrowest the feed gets beside the dock; below feed + dock a plan opens in a dialog. */
export const FEED_MIN_PX = 420;
/** Below this panel width the four decisions take two rows. */
const SPLIT_BUTTONS_PX = 480;
/** The width of the dialog a plan opens in when it cannot dock. */
const PLAN_DIALOG_WIDTH = 880;
/** The most lines the feedback box grows to. */
const FEEDBACK_ROWS_MAX = 4;
/** The CSS highlight that marks commented quotes (styled in app.css). */
const MARK_HIGHLIGHT = "plan-comment";

const styles = stylex.create({
  dock: {
    display: "flex",
    flexDirection: "column",
    flexShrink: 0,
    minHeight: 0,
    backgroundColor: "var(--color-background-surface)",
  },
  head: {
    display: "flex",
    height: 44,
    flexShrink: 0,
    alignItems: "center",
    gap: 8,
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
    paddingInlineStart: 14,
    paddingInlineEnd: 8,
  },
  title: {
    margin: 0,
    minWidth: 0,
    flexGrow: 1,
    flexBasis: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontSize: 13,
    fontWeight: 600,
    outline: { default: "none", ":focus-visible": "2px solid var(--color-accent)" },
  },
  meta: {
    display: "flex",
    height: 32,
    flexShrink: 0,
    alignItems: "center",
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
    paddingInline: 14,
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  body: { flexGrow: 1, minHeight: 0, overflow: "auto" },
  text: { position: "relative", maxWidth: "72ch", padding: 20, lineHeight: 1.6, userSelect: "text" },
  floating: { position: "absolute", zIndex: 2 },
  composer: {
    position: "absolute",
    zIndex: 2,
    insetInline: 20,
    display: "flex",
    flexDirection: "column",
    gap: 8,
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-popover)",
    boxShadow: "var(--shadow-med, 0 4px 16px rgba(0, 0, 0, 0.18))",
    padding: 8,
  },
  composerQuote: {
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  composerActions: { display: "flex", justifyContent: "flex-end", gap: 8 },
  markNumber: {
    position: "absolute",
    zIndex: 1,
    minWidth: 16,
    height: 16,
    borderWidth: 0,
    borderRadius: 8,
    backgroundColor: "var(--color-warning)",
    paddingInline: 4,
    fontSize: 10,
    fontWeight: 600,
    lineHeight: "16px",
    color: "var(--color-text-on-warning, var(--color-text-primary))",
    cursor: "pointer",
    transform: "translateY(-45%)",
    outline: { default: "none", ":focus-visible": "2px solid var(--color-accent)" },
  },
  strip: {
    display: "flex",
    flexDirection: "column",
    flexShrink: 0,
    maxHeight: "30%",
    overflow: "auto",
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    paddingBlock: 8,
    paddingInline: 12,
  },
  stripTitle: { margin: 0, marginBottom: 4, fontSize: 12, fontWeight: 600 },
  stripList: { display: "flex", flexDirection: "column", gap: 4, margin: 0, padding: 0, listStyle: "none" },
  stripRow: { display: "flex", alignItems: "flex-start", gap: 8, borderRadius: 6, padding: 4, fontSize: 12 },
  stripRowFlash: { backgroundColor: "var(--color-overlay-hover)" },
  stripNumber: {
    flexShrink: 0,
    minWidth: 28,
    minHeight: 28,
    borderWidth: 0,
    borderRadius: 6,
    backgroundColor: "var(--color-warning-muted)",
    fontSize: 12,
    fontWeight: 600,
    color: "var(--color-text-primary)",
    cursor: "pointer",
    outline: { default: "none", ":focus-visible": "2px solid var(--color-accent)" },
  },
  stripText: { display: "flex", flexDirection: "column", minWidth: 0, flexGrow: 1, flexBasis: 0, paddingTop: 5 },
  stripQuote: {
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--precision-text3)",
  },
  footer: {
    display: "flex",
    flexDirection: "column",
    flexShrink: 0,
    gap: 8,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    paddingBlock: 10,
    paddingInline: 12,
  },
  problem: { fontSize: 12, color: "var(--color-text-error)" },
  decided: { fontSize: 12.5, color: "var(--color-text-secondary)" },
  sent: { fontSize: 12.5 },
  sentSummary: { cursor: "pointer", color: "var(--precision-text3)" },
  dialogBody: { display: "flex", flexDirection: "column", gap: 0, minHeight: 0 },
  probe: { display: "none" },
  srOnly: {
    position: "absolute",
    width: 1,
    height: 1,
    overflow: "hidden",
    clipPath: "inset(50%)",
    whiteSpace: "nowrap",
  },
});

const offset = () => -new Date().getTimezoneOffset();

const TYPING = "input, textarea, select, [contenteditable=true]";
const BLOCK = "p, li, h1, h2, h3, h4, h5, h6, pre, blockquote, table, tr";

/** Moves focus back to a plan's Open plan button in the feed after the panel closes. */
function focusPlanCard(messageId: string) {
  requestAnimationFrame(() => {
    document.querySelector<HTMLElement>(`[data-plan-card="${CSS.escape(messageId)}"] button:not([disabled])`)?.focus();
  });
}

/** The element's width, followed as it changes; null before it is measured. */
function useWidth(): [(el: HTMLElement | null) => void, number | null] {
  const [width, setWidth] = useState<number | null>(null);
  const observer = useRef<ResizeObserver | null>(null);
  const ref = useCallback((el: HTMLElement | null) => {
    observer.current?.disconnect();
    observer.current = null;
    if (!el) return;
    const o = new ResizeObserver(([entry]) => {
      if (entry) setWidth(entry.contentRect.width);
    });
    o.observe(el);
    observer.current = o;
  }, []);
  return [ref, width];
}

// ---- Where comments sit in the rendered text ------------------------------------------------

function textNodes(root: Node): Text[] {
  const out: Text[] = [];
  const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let n = walk.nextNode(); n; n = walk.nextNode()) out.push(n as Text);
  return out;
}

/** The DOM range of `quote` in `root`'s text (ignoring whitespace), or null. */
function rangeOf(root: HTMLElement, quote: string): Range | null {
  const nodes = textNodes(root);
  const starts: number[] = [];
  let text = "";
  for (const n of nodes) {
    starts.push(text.length);
    text += n.data;
  }
  const hit = findQuote(text, quote);
  if (!hit) return null;
  const locate = (at: number): [Text, number] | null => {
    for (let i = nodes.length - 1; i >= 0; i--) {
      const start = starts[i] ?? 0;
      const node = nodes[i];
      if (node && at >= start) return [node, at - start];
    }
    return null;
  };
  const from = locate(hit[0]);
  const to = locate(hit[1] - 1);
  if (!from || !to) return null;
  const range = document.createRange();
  range.setStart(from[0], from[1]);
  range.setEnd(to[0], to[1] + 1);
  return range;
}

function caretAt(x: number, y: number): { node: Node; offset: number } | null {
  const doc = document as Document & {
    caretPositionFromPoint?: (x: number, y: number) => { offsetNode: Node; offset: number } | null;
    caretRangeFromPoint?: (x: number, y: number) => Range | null;
  };
  const pos = doc.caretPositionFromPoint?.(x, y);
  if (pos) return { node: pos.offsetNode, offset: pos.offset };
  const range = doc.caretRangeFromPoint?.(x, y);
  return range ? { node: range.startContainer, offset: range.startOffset } : null;
}

interface Selected {
  quote: string;
  top: number;
  left: number;
  /** Under the paragraph the selection ends in: where the comment box goes. */
  below: number;
}

/**
 * The plan's text. While the plan waits, a selection gets a Comment button above it, and each
 * comment marks its quote (a highlight plus a numbered button, so not by colour alone).
 */
function PlanText({
  text,
  comments,
  onComment,
  onMark,
  goTo,
}: {
  text: string;
  /** Null: read only (the plan was decided). */
  comments: PlanComment[] | null;
  onComment: (quote: string, text: string) => void;
  /** A mark was clicked: show its comment. */
  onMark: (index: number) => void;
  /** Set to an index to scroll that comment's mark into view. */
  goTo: { index: number; at: number } | null;
}) {
  const wrap = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<Selected | null>(null);
  const [composing, setComposing] = useState<Selected | null>(null);
  const [draft, setDraft] = useState("");
  const [marks, setMarks] = useState<{ index: number; top: number; left: number; range: Range }[]>([]);
  const [layout, setLayout] = useState(0);

  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const o = new ResizeObserver(() => setLayout((n) => n + 1));
    o.observe(el);
    return () => o.disconnect();
  }, []);

  // Marks follow the text: recomputed when the comments or the layout change.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `layout` and `text` re-run it on purpose.
  useLayoutEffect(() => {
    const root = wrap.current;
    if (!root || !comments) return setMarks([]);
    const box = root.getBoundingClientRect();
    const next = comments.flatMap((c, index) => {
      const range = rangeOf(root, c.quote);
      const rect = range ? [...range.getClientRects()].at(-1) : undefined;
      if (!range || !rect) return [];
      return [{ index, range, top: rect.top - box.top, left: Math.min(rect.right - box.left, box.width - 18) }];
    });
    setMarks(next);
    const highlights = typeof CSS !== "undefined" ? CSS.highlights : undefined;
    if (!highlights || typeof Highlight === "undefined") return;
    highlights.set(MARK_HIGHLIGHT, new Highlight(...next.map((m) => m.range)));
    return () => {
      highlights.delete(MARK_HIGHLIGHT);
    };
  }, [comments, layout, text]);

  useEffect(() => {
    if (!goTo) return;
    const mark = marks.find((m) => m.index === goTo.index);
    mark?.range.startContainer.parentElement?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [goTo, marks]);

  const readSelection = (): Selected | null => {
    const root = wrap.current;
    const s = window.getSelection();
    if (!root || !s || s.isCollapsed || s.rangeCount === 0) return null;
    const range = s.getRangeAt(0);
    if (!root.contains(range.commonAncestorContainer)) return null;
    const quote = s.toString().trim();
    if (quote === "") return null;
    const box = root.getBoundingClientRect();
    const rect = range.getBoundingClientRect();
    const end = range.endContainer instanceof Element ? range.endContainer : range.endContainer.parentElement;
    const block = end?.closest(BLOCK);
    const blockBottom = block && root.contains(block) ? block.getBoundingClientRect().bottom : rect.bottom;
    return {
      quote,
      top: Math.max(0, rect.top - box.top - 34),
      left: Math.max(0, Math.min(rect.left - box.left, box.width - 110)),
      below: blockBottom - box.top + 4,
    };
  };

  const startComment = (from: Selected | null = selected ?? readSelection()) => {
    if (!from) return;
    setComposing(from);
    setSelected(null);
    setDraft("");
  };
  useAction("CommentOnSelection", () => startComment(readSelection()), comments !== null);

  const add = () => {
    if (!composing || draft.trim() === "") return;
    onComment(composing.quote, draft);
    setComposing(null);
    setDraft("");
    window.getSelection()?.removeAllRanges();
  };

  const onPick = () => {
    if (comments === null || composing) return;
    setSelected(readSelection());
  };

  const onClick = (e: React.MouseEvent) => {
    if (!window.getSelection()?.isCollapsed) return;
    const at = caretAt(e.clientX, e.clientY);
    if (!at) return;
    const hit = marks.find((m) => m.range.isPointInRange(at.node, at.offset));
    if (hit) onMark(hit.index);
  };

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: a selection and the marks' text, not a control; the marks have their own buttons.
    <div
      ref={wrap}
      data-plan-text
      onMouseUp={onPick}
      onKeyUp={(e) => {
        if (e.shiftKey) onPick();
      }}
      onClick={onClick}
      {...stylex.props(styles.text)}
    >
      <Markdown text={text} />
      {marks.map((m) => (
        <button
          key={m.index}
          type="button"
          aria-label={`Comment ${m.index + 1}`}
          onClick={() => onMark(m.index)}
          style={{ top: m.top, left: m.left }}
          {...stylex.props(styles.markNumber)}
        >
          {m.index + 1}
        </button>
      ))}
      {selected && !composing ? (
        <span style={{ top: selected.top, left: selected.left }} {...stylex.props(styles.floating)}>
          <Button size="sm" variant="primary" label="Comment" onClick={() => startComment(selected)} />
        </span>
      ) : null}
      {composing ? (
        // biome-ignore lint/a11y/noStaticElementInteractions: Esc of the field inside, which handles its own keys.
        <div
          style={{ top: composing.below }}
          onKeyDown={(e) => {
            if (e.key !== "Escape") return;
            e.preventDefault();
            e.stopPropagation();
            setComposing(null);
          }}
          {...stylex.props(styles.composer)}
        >
          <span {...stylex.props(styles.composerQuote)}>Comment on: “{composing.quote}”</span>
          <TextInput
            hasAutoFocus
            label="Your comment"
            isLabelHidden
            placeholder="Your comment"
            value={draft}
            onChange={setDraft}
            onEnter={add}
          />
          <span {...stylex.props(styles.composerActions)}>
            <Button size="sm" label="Cancel" onClick={() => setComposing(null)} />
            <Button size="sm" variant="primary" label="Add comment" isDisabled={draft.trim() === ""} onClick={add} />
          </span>
        </div>
      ) : null}
    </div>
  );
}

/** "Your comments": number (jumps to the mark), quote, comment, remove. */
function CommentStrip({
  comments,
  flash,
  onGo,
  onRemove,
}: {
  comments: PlanComment[];
  flash: number | null;
  onGo: (index: number) => void;
  onRemove: (index: number) => void;
}) {
  const list = useRef<HTMLOListElement>(null);
  useEffect(() => {
    if (flash === null) return;
    list.current?.children[flash]?.scrollIntoView({ block: "nearest" });
  }, [flash]);
  if (comments.length === 0) return null;
  return (
    <section aria-label="Your comments" {...stylex.props(styles.strip)}>
      <h3 {...stylex.props(styles.stripTitle)}>Your comments</h3>
      <ol ref={list} {...stylex.props(styles.stripList)}>
        {comments.map((c, i) => (
          <li key={`${c.quote}|${c.text}`} {...stylex.props(styles.stripRow, flash === i && styles.stripRowFlash)}>
            <button
              type="button"
              aria-label={`Show comment ${i + 1} in the plan`}
              onClick={() => onGo(i)}
              {...stylex.props(styles.stripNumber)}
            >
              {i + 1}
            </button>
            <span {...stylex.props(styles.stripText)}>
              <span {...stylex.props(styles.stripQuote)}>“{c.quote}”</span>
              <span>{c.text}</span>
            </span>
            <IconButton
              variant="ghost"
              size="sm"
              label={`Remove comment ${i + 1}`}
              icon={<Icon icon="close" />}
              onClick={() => onRemove(i)}
            />
          </li>
        ))}
      </ol>
    </section>
  );
}

/** The footer while the plan waits: what should change, and the four decisions. */
function PendingFooter({ plan, split }: { plan: PlanDecisionState; split: boolean }) {
  const lines = plan.feedback.split("\n").length;
  return (
    <>
      <TextArea
        label="What should change?"
        isLabelHidden
        placeholder="What should change?"
        rows={Math.min(FEEDBACK_ROWS_MAX, Math.max(1, lines))}
        value={plan.feedback}
        onChange={plan.setFeedback}
      />
      {plan.missing ? <span {...stylex.props(styles.problem)}>{PLAN_MISSING}</span> : null}
      <PlanButtons plan={plan} split={split} />
    </>
  );
}

/** The footer after the decision: one line, and what was sent, folded. */
function DecidedFooter({ decided }: { decided: ReturnType<typeof planDecision> }) {
  if (!decided) return <span {...stylex.props(styles.decided)}>This plan no longer waits for a decision.</span>;
  const line = decidedLine(decided.decision, clock(decided.at, offset()));
  return (
    <>
      <span {...stylex.props(styles.decided)}>{line}</span>
      {decided.decision.feedback ? (
        <details {...stylex.props(styles.sent)}>
          <summary {...stylex.props(styles.sentSummary)}>What you sent</summary>
          <Markdown text={decided.decision.feedback} />
        </details>
      ) : null}
    </>
  );
}

/** Keys of an open plan: comment on a selection, and `x` / Ctrl+Backspace reject from inside it. */
function PlanKeys({ onReject }: { onReject: (() => void) | null }) {
  useKeyContext("PlanPanel");
  return onReject ? <RejectKey onReject={onReject} /> : null;
}

const inPanel = () => Boolean(document.activeElement?.closest("[data-plan-panel]"));

/** The plan of message `messageId`, docked (or in a dialog where it cannot dock). */
export function PlanDock({
  ctx,
  messageId,
  focus = false,
  inDialog = false,
  notice,
  onClose,
}: {
  ctx: ChatContext;
  messageId: string;
  /** Opened by the board member: focus goes to the heading. Opened by itself: focus stays. */
  focus?: boolean;
  /** Always in a dialog: where there is no dock at all. */
  inDialog?: boolean;
  notice: ReactNode;
  onClose: () => void;
}) {
  const m = ctx.messages.find((x) => x.id === messageId);
  const approval = m?.approval_id ? ctx.approvals.get(m.approval_id) : undefined;
  if (!m) return null;
  const frame = { m, ctx, focus, inDialog, notice, onClose };
  return approval ? (
    <PendingPlan approval={approval} {...frame} />
  ) : (
    <PlanFrame {...frame} pending={false}>
      {{
        body: () => <PlanText text={m.text} comments={null} onComment={noop} onMark={noop} goTo={null} />,
        strip: null,
        footer: () => <DecidedFooter decided={planDecision(ctx.messages, m.id)} />,
        onReject: null,
      }}
    </PlanFrame>
  );
}

const noop = () => undefined;

interface FrameProps {
  m: MessageView;
  ctx: ChatContext;
  focus: boolean;
  inDialog: boolean;
  notice: ReactNode;
  onClose: () => void;
}

function PendingPlan({ approval, ...frame }: FrameProps & { approval: ApprovalView }) {
  const { m } = frame;
  const plan = usePlanDecision(approval);
  const [flash, setFlash] = useState<number | null>(null);
  const [goTo, setGoTo] = useState<{ index: number; at: number } | null>(null);
  return (
    <PlanFrame {...frame} pending>
      {{
        body: () => (
          <PlanText text={m.text} comments={plan.comments} onComment={plan.addComment} onMark={setFlash} goTo={goTo} />
        ),
        strip: (
          <CommentStrip
            comments={plan.comments}
            flash={flash}
            onGo={(index) => {
              setFlash(index);
              setGoTo({ index, at: Date.now() });
            }}
            onRemove={(index) => {
              setFlash(null);
              plan.removeComment(index);
            }}
          />
        ),
        footer: (split: boolean) => <PendingFooter plan={plan} split={split} />,
        onReject: () => {
          if (inPanel()) plan.decide("reject");
        },
      }}
    </PlanFrame>
  );
}

/** The panel's frame: head, meta, body, comments and footer, docked or in a dialog. */
function PlanFrame({
  m,
  ctx,
  pending,
  focus,
  inDialog,
  notice,
  onClose,
  children,
}: FrameProps & {
  pending: boolean;
  children: {
    body: () => ReactNode;
    strip: ReactNode;
    footer: (split: boolean) => ReactNode;
    onReject: (() => void) | null;
  };
}) {
  const title = planTitle(m.text);
  const size = planSummary(m.text);
  const decided = useMemo(() => planDecision(ctx.messages, m.id), [ctx.messages, m.id]);
  const status = planStatus(pending, decided?.decision ?? null);
  const meta = [m.author ?? "Aurelius", clock(m.created_at_ms, offset()), size.line].filter(Boolean).join(" · ");

  // The dock's width is the design dock's: same handle, limits and remembered width.
  const row = useRef<HTMLElement | null>(null);
  const dockSize = useResizable({
    containerRef: row,
    defaultSize: percent(DOCK_DEFAULT_PERCENT, { min: pixel(DOCK_MIN_PX) }),
    minSize: DOCK_MIN_PX,
    maxSize: `${DOCK_MAX_PERCENT}%`,
    autoSaveId: DOCK_SAVE_ID,
  });
  const [measureRow, rowWidth] = useWidth();
  const [measurePanel, panelWidth] = useWidth();
  const probe = useCallback(
    (el: HTMLElement | null) => {
      const parent = el?.parentElement ?? null;
      if (el) row.current = parent;
      measureRow(parent);
    },
    [measureRow],
  );
  const narrow = inDialog || (rowWidth !== null && rowWidth < FEED_MIN_PX + DOCK_MIN_PX);
  const split = panelWidth !== null && panelWidth < SPLIT_BUTTONS_PX;

  // Another screen closes the plan, as it closes a design.
  const path = useRouterState({ select: (s) => s.location.pathname });
  const openedAt = useRef(path);
  useEffect(() => {
    if (path !== openedAt.current) onClose();
  }, [path, onClose]);

  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    const el = heading.current;
    if (!focus || !el) return;
    el.tabIndex = -1;
    el.focus();
  }, [focus]);

  // Announced politely: a plan that opened by itself, and its status when it changes.
  const [said, setSaid] = useState("");
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      if (pending && !focus) setSaid("Plan ready for review");
      return;
    }
    setSaid(`Plan: ${status.label}`);
  }, [status.label, pending, focus]);

  const close = () => {
    onClose();
    focusPlanCard(m.id);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== "Escape" || e.defaultPrevented) return;
    if (e.target instanceof Element && e.target.closest(TYPING)) return;
    e.preventDefault();
    close();
  };

  const live = (
    <span role="status" {...stylex.props(styles.srOnly)}>
      {said}
    </span>
  );
  const statusBadge = <Badge variant={status.tone} label={status.label} />;
  const keys = <PlanKeys onReject={children.onReject} />;

  if (narrow)
    return (
      <>
        <span ref={probe} aria-hidden {...stylex.props(styles.probe)} />
        <FormDialog
          title={title}
          width={PLAN_DIALOG_WIDTH}
          onClose={onClose}
          start={statusBadge}
          footer={
            <div data-plan-panel ref={measurePanel} {...stylex.props(styles.footer)}>
              {children.footer(split)}
            </div>
          }
        >
          {keys}
          {live}
          <div data-plan-panel {...stylex.props(styles.dialogBody)}>
            <span {...stylex.props(styles.meta)}>{meta}</span>
            {children.body()}
            {children.strip}
          </div>
        </FormDialog>
      </>
    );

  return (
    <>
      <ResizeHandle direction="horizontal" isReversed hasDivider label="Resize plan panel" resizable={dockSize.props} />
      <aside
        ref={(el) => {
          probe(el);
          measurePanel(el);
        }}
        data-plan-panel
        aria-label={`Plan: ${title}`}
        onKeyDown={onKeyDown}
        style={{ width: dockSize.size }}
        {...stylex.props(styles.dock)}
      >
        {keys}
        {live}
        <div {...stylex.props(styles.head)}>
          <Heading level={2} xstyle={styles.title} ref={heading}>
            {title}
          </Heading>
          {statusBadge}
          {notice}
          <IconButton size="sm" variant="ghost" label="Close plan" icon={<Icon icon="close" />} onClick={close} />
        </div>
        <div {...stylex.props(styles.meta)}>{meta}</div>
        <div {...stylex.props(styles.body)}>{children.body()}</div>
        {children.strip}
        <div {...stylex.props(styles.footer)}>{children.footer(split)}</div>
      </aside>
    </>
  );
}

// ---- The host's side: opening a plan by itself, Alt+Shift+P and ?plan= ----------------------

/**
 * A conversation's plan behaviour around its dock: a waiting plan opens by itself in an empty
 * dock (once), Alt+Shift+P opens or closes the latest plan, and `?plan=<messageId>` opens that one.
 * `busy`: the right panel shows something that is not in the dock (a thread), so nothing opens
 * by itself.
 */
export function usePlanDock(
  dock: { target: ArtifactTarget | null; open: (t: ArtifactTarget) => void; close: () => void },
  messages: readonly MessageView[],
  approvals: ReadonlyMap<string, ApprovalView>,
  busy = false,
) {
  const pending = useMemo(() => new Set(approvals.keys()), [approvals]);
  const latest = useMemo(() => latestPlan(messages, pending), [messages, pending]);
  const handled = useRef(new Set<string>());
  const { target, open, close } = dock;

  useEffect(() => {
    if (!latest) return;
    if (!busy && shouldAutoOpenPlan(target, latest, handled.current)) open({ kind: "plan", messageId: latest.id });
    // A waiting plan seen while the dock was busy gets the pill, not a later take-over.
    if (latest.pending) handled.current.add(latest.id);
  }, [latest, target, open, busy]);

  const search = useRouterState({ select: (s) => s.location.searchStr });
  const linked = useMemo(() => planFromSearch(search, messages), [search, messages]);
  const opened = useRef<string | null>(null);
  useEffect(() => {
    if (!linked || opened.current === linked) return;
    opened.current = linked;
    handled.current.add(linked);
    open({ kind: "plan", messageId: linked, focus: true });
  }, [linked, open]);

  useAction(
    "TogglePlanFold",
    () => {
      if (!latest) return;
      if (showsPlan(target, latest.id)) close();
      else open({ kind: "plan", messageId: latest.id, focus: true });
    },
    latest !== null,
  );
}
