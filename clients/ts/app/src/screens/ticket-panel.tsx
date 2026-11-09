import { Button } from "@astryxdesign/core/Button";
import { TicketActivityLine } from "./activity-bar";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { IconButton } from "@astryxdesign/core/IconButton";
import { LayoutPanel } from "@astryxdesign/core/Layout";
import { Link } from "@astryxdesign/core/Link";
import { ResizeHandle, useResizable } from "@astryxdesign/core/Resizable";
import { Tab, TabList } from "@astryxdesign/core/TabList";
import { TextArea } from "@astryxdesign/core/TextArea";
import { TextInput } from "@astryxdesign/core/TextInput";
import { expectBody } from "@aurelius/client";
import type { AgentMode, TicketPriority, TicketView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useNavigate } from "@tanstack/react-router";
import { type ReactNode, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import {
  useAllTasks,
  useBoard,
  useCompany,
  useEmployees,
  useKeyed,
  useMembers,
  useMessages,
  useTicket,
  useTicketActivity,
} from "../data";
import { showChord } from "../keys/keymap";
import { isMac, useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { acceptable, filesFrom } from "../ui/attach";
import { Chip } from "../ui/chip";
import { KeyHint } from "../ui/key-hint";
import { Markdown } from "../ui/markdown";
import { initial, Mark, markColor } from "../ui/marks";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import {
  activityDetail,
  activityText,
  deadlineInput,
  decideToo,
  doneCheck,
  dropEffect,
  editable,
  feedbackLabel,
  latestPlan,
  loginWait,
  ownerChoosable,
  PRIORITIES,
  peopleCount,
  priorityLabel,
  runLocked,
  suggestedReplies,
  takeOverOffered,
  testingNote,
} from "./board-logic";
import { TicketChat } from "./chat";
import { ARTIFACT_HINT, ARTIFACT_PLACEHOLDER, artifactProblem, DESIGN_FEATURE } from "./design-logic";
import { Held, parseDeadline, parseLabels, useModels, useSend } from "./dialogs";
import { ticketLink } from "./links-logic";
import { FormDialog, Loading, Problem } from "./org-kit";
import { TicketOdometer } from "./run-card";
import { lastFailure, lastSummaryFailed } from "./run-logic";
import { FileChips, useRetryAnswer } from "./run-tools";
import { ThreadWaiting } from "./scheduled";
import { foldTree, taskStateLine, treeOrder } from "./tasks-logic";
import {
  ColumnGlyph,
  NoConnectionNote,
  ticketNumber,
  useNoConnection,
  useTicketNeeds,
  useTicketWrite,
} from "./tickets-kit";

const MODES: { key: string; label: string }[] = [
  { key: "default", label: "Owner's default" },
  { key: "ask", label: "Ask" },
  { key: "accept_edits", label: "Accept edits" },
  { key: "auto", label: "Auto" },
  { key: "plan", label: "Plan" },
];

const enter = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(4px)" },
});

const styles = stylex.create({
  box: {
    display: "flex",
    flexDirection: "column",
    gap: 10,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    paddingInline: 14,
    paddingBlock: 12,
  },
  boxAmber: { borderColor: "var(--color-border-warning)", backgroundColor: "var(--color-warning-muted)" },
  boxRed: { borderColor: "var(--color-text-error)" },
  row: { display: "grid", minHeight: 30, gridTemplateColumns: "100px minmax(0,1fr)", alignItems: "center", gap: 8 },
  rowValue: { minWidth: 0 },
  editable: {
    display: "block",
    width: "100%",
    cursor: "text",
    borderRadius: 5,
    borderWidth: 0,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    padding: 0,
    textAlign: "left",
    color: "inherit",
  },
  stack: { display: "flex", flexDirection: "column", gap: 8 },
  designLink: { color: "var(--color-text-accent)", textDecoration: "none" },
  designChange: {
    cursor: "pointer",
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    fontSize: 12,
    color: { default: "var(--precision-text3)", ":hover": "var(--color-text-primary)" },
  },
  buttons: { display: "flex", flexWrap: "wrap", gap: 8 },
  buttonsCentered: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 8 },
  gateHead: { display: "flex", alignItems: "center", gap: 8, fontWeight: 500 },
  dot: { width: 7, height: 7, borderRadius: "50%", backgroundColor: "var(--color-text-warning)" },
  small: { fontSize: 12, color: "var(--color-text-secondary)" },
  secondary: { color: "var(--color-text-secondary)" },
  third: { color: "var(--precision-text3)" },
  thirdSmall: { fontSize: 12, color: "var(--precision-text3)" },
  sectionLabel: { fontSize: 12, fontWeight: 500, color: "var(--precision-text3)" },
  medium: { fontWeight: 500 },
  failed: { fontWeight: 500, color: "var(--color-text-error)" },
  red: { color: "var(--color-text-error)" },
  error: { fontSize: 12, color: "var(--color-text-error)" },
  linkEnd: { marginInlineStart: "auto", alignSelf: "center", fontSize: 12 },
  followers: { display: "inline-flex", alignItems: "center", gap: 4 },
  spaced: { marginTop: 8 },
  summaryLine: { marginTop: 8, display: "flex", alignItems: "center", gap: 8 },
  // A card clips its content, so in this column it would be squeezed instead of scrolled.
  // Negative margin: the button may not make a one-line message head taller than its text.
  activity: { margin: 0, display: "flex", listStyleType: "none", flexDirection: "column", gap: 12, padding: 0 },
  activityItem: { display: "flex", flexDirection: "column", gap: 2 },
  pending: { display: "flex", flexDirection: "column", gap: 12, padding: 20 },
  panel: {
    display: "flex",
    height: "100%",
    minWidth: 0,
    flexDirection: "column",
    backgroundColor: "var(--color-background-surface)",
    animationName: enter,
    animationDuration: "160ms",
    animationTimingFunction: "cubic-bezier(0.2, 0, 0, 1)",
  },
  panelHead: {
    display: "flex",
    height: 44,
    flexShrink: 0,
    alignItems: "center",
    gap: 8,
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
    paddingInlineStart: 20,
    paddingInlineEnd: 10,
    color: "var(--precision-text3)",
  },
  number: { fontFamily: "var(--font-family-code)", fontSize: 12 },
  panelTools: { marginInlineStart: "auto", display: "flex", gap: 2 },
  top: { display: "flex", flexShrink: 0, flexDirection: "column", gap: 12, paddingInline: 22, paddingTop: 20 },
  topGate: { display: "flex", flexShrink: 0, flexDirection: "column", gap: 12, paddingInline: 22, paddingTop: 12 },
  title: { margin: 0, fontSize: 20, fontWeight: 600, letterSpacing: "-0.015em" },
  chips: { display: "flex", flexWrap: "wrap", gap: 6 },
  tabPanel: { display: "flex", minHeight: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%", flexDirection: "column" },
  section: {
    display: "flex",
    minHeight: 0,
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: "0%",
    flexDirection: "column",
    gap: 18,
    overflowY: "auto",
    paddingInline: 22,
    paddingBlock: 16,
  },
  rows: { display: "flex", flexDirection: "column" },
  files: { display: "flex", flexDirection: "column", alignItems: "flex-start", gap: 6 },
  tasks: { display: "flex", flexDirection: "column", gap: 4 },
  task: {
    display: "flex",
    height: 32,
    cursor: "pointer",
    alignItems: "center",
    gap: 8,
    borderRadius: 6,
    borderWidth: 0,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    paddingInlineEnd: 4,
    textAlign: "left",
    color: "var(--color-text-primary)",
  },
  taskTitle: {
    minWidth: 0,
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: "0%",
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
  },
  taskState: { flexShrink: 0, fontSize: 12 },
});

function time(ms: number): string {
  return new Date(ms).toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" });
}

function Box({ tone = "plain", children }: { tone?: "plain" | "amber" | "red"; children: ReactNode }) {
  return (
    <div {...stylex.props(styles.box, tone === "amber" && styles.boxAmber, tone === "red" && styles.boxRed)}>
      {children}
    </div>
  );
}

function Row({ name, children }: { name: string; children: ReactNode }) {
  return (
    <div {...stylex.props(styles.row)}>
      <span {...stylex.props(styles.third)}>{name}</span>
      <div {...stylex.props(styles.rowValue)}>{children}</div>
    </div>
  );
}

/** Text that turns into a field on a click. Saved with Enter or the button, left with Escape. */
function Editable({
  value,
  enabled,
  multiline = false,
  display,
  onSave,
  label,
  placeholder,
  opened = false,
  onClose,
}: {
  value: string;
  enabled: boolean;
  multiline?: boolean;
  display: ReactNode;
  onSave: (next: string) => Promise<boolean>;
  label: string;
  placeholder?: string;
  /** Opens as an editor (a link field has no display worth clicking while it has a value). */
  opened?: boolean;
  onClose?: () => void;
}) {
  const [draft, setDraftRaw] = useState<string | null>(opened ? value : null);
  const setDraft = (next: string | null) => {
    setDraftRaw(next);
    if (next === null) onClose?.();
  };
  if (draft === null) {
    return enabled ? (
      <button
        type="button"
        aria-label={`Edit ${label}`}
        onClick={() => setDraft(value)}
        {...stylex.props(styles.editable)}
      >
        {display}
      </button>
    ) : (
      <div>{display}</div>
    );
  }
  const save = async () => {
    if (draft === value || (await onSave(draft))) setDraft(null);
  };
  const keys = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      setDraft(null);
    }
    if (event.key === "Enter" && (!multiline || event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void save();
    }
  };
  return (
    <div {...stylex.props(styles.stack)}>
      {multiline ? (
        <TextArea
          label={label}
          isLabelHidden
          rows={6}
          value={draft}
          onChange={setDraft}
          onKeyDown={keys}
          ref={(el) => el?.focus({ preventScroll: true })}
        />
      ) : (
        <TextInput
          label={label}
          isLabelHidden
          value={draft}
          {...(placeholder ? { placeholder } : {})}
          onChange={setDraft}
          onKeyDown={keys}
          ref={(el) => el?.focus({ preventScroll: true })}
        />
      )}
      <div {...stylex.props(styles.buttons)}>
        <Button variant="primary" label="Save" onClick={() => void save()} />
        <Button label="Cancel" onClick={() => setDraft(null)} />
      </div>
    </div>
  );
}

/** A ticket's design: only an Open link when it has one; "None" (click to edit) otherwise. */
function DesignRow({
  url,
  enabled,
  onSave,
}: {
  url: string | null;
  enabled: boolean;
  onSave: (text: string) => Promise<boolean>;
}) {
  const [editing, setEditing] = useState(false);
  const field =
    url && !editing ? (
      <span {...stylex.props(styles.buttonsCentered)}>
        <a href={url} target="_blank" rel="noreferrer" {...stylex.props(styles.designLink)}>
          Open
        </a>
        {enabled ? (
          <button type="button" onClick={() => setEditing(true)} {...stylex.props(styles.designChange)}>
            Change
          </button>
        ) : null}
      </span>
    ) : (
      <Editable
        value={url ?? ""}
        enabled={enabled}
        opened={editing}
        onClose={() => setEditing(false)}
        label="Design"
        placeholder={ARTIFACT_PLACEHOLDER}
        display={<span {...stylex.props(styles.third)}>None</span>}
        onSave={onSave}
      />
    );
  return (
    <Row name="Design">
      <div {...stylex.props(styles.stack)}>
        {field}
        <span {...stylex.props(styles.thirdSmall)}>{ARTIFACT_HINT}</span>
      </div>
    </Row>
  );
}

function Feedback({
  placeholder,
  value,
  onChange,
  missing,
}: {
  placeholder: string;
  value: string;
  onChange: (v: string) => void;
  missing: boolean;
}) {
  return (
    <>
      <TextArea
        label={placeholder}
        isLabelHidden
        placeholder={placeholder}
        rows={3}
        value={value}
        onChange={onChange}
      />
      {missing ? <span {...stylex.props(styles.error)}>Write what needs to change.</span> : null}
    </>
  );
}

/** Enter stays "open"; deciding takes the stronger chord. */
function GateKeys({ onYes, onNo }: { onYes: () => void; onNo: () => void }) {
  useKeyContext("Decision");
  useAction("DecideYes", onYes);
  useAction("RejectApproval", onNo);
  return null;
}

/** A ticket's plan or report, to decide on where it is read: the Inbox. */
export function TicketDecision({ id }: { id: string }) {
  const { status } = useLinkState();
  const detail = useTicket(id);
  const t = detail.data?.ticket;
  const conversation = detail.data?.root_task?.conversation_id;
  const messages = useMessages(t?.column === "plan_review" ? conversation : null);
  if (!t) return null;
  if (t.column === "review") {
    const check = doneCheck(detail.data);
    return (
      <ReviewGate
        key={t.version}
        keyed
        t={t}
        report={detail.data?.root_task?.result ?? null}
        ready={check.kind === "ready" && status === "online"}
        whyNot={check.kind === "working" ? "Still at work: Done waits until that has finished." : null}
      />
    );
  }
  if (t.column === "plan_review") {
    return (
      <PlanGate
        key={t.version}
        keyed
        t={t}
        plan={messages.isPending ? undefined : latestPlan(messages.data?.messages ?? [])}
      />
    );
  }
  return null;
}

function ReviewGate({
  keyed = false,
  t,
  report,
  ready,
  whyNot,
}: {
  keyed?: boolean;
  t: TicketView;
  report: string | null;
  ready: boolean;
  whyNot: string | null;
}) {
  const connection = useConnection();
  const write = useTicketWrite();
  const [feedback, setFeedback] = useState("");
  // "back" asks for changes; "reply" tells the owner what to do next and leaves the ticket in review.
  const [mode, setMode] = useState<"back" | "reply" | null>(null);
  const [missing, setMissing] = useState(false);
  const { busy, held, error, send, sendAnyway } = useSend((text) => {
    if (text !== "") toast(text);
    setMode(null);
    setFeedback("");
  });
  const navigate = useNavigate();
  const discussion = useMessages(t.conversation_id);
  const lastMessage = (discussion.data?.messages ?? []).filter((m) => !m.deleted_at_ms).at(-1);
  const reply = (text: string) =>
    send(async (confirmSecret) => {
      await connection.write("ticket.post", { ticket_id: t.id, text, confirm_secret: confirmSecret });
      return "";
    });
  const too = decideToo(t);
  const done = () => void write("ticket.accept", { ticket_id: t.id }, { done: `${ticketNumber(t)} is Done.` });
  return (
    <Box tone="amber">
      {keyed ? (
        <GateKeys
          onYes={() => {
            if (ready) done();
          }}
          onNo={() => setMode("back")}
        />
      ) : null}
      <div {...stylex.props(styles.gateHead)}>
        <span {...stylex.props(styles.dot)} />
        In review · your decision
      </div>
      {too ? <span {...stylex.props(styles.small)}>{too}</span> : null}
      {report ? (
        <Markdown text={report} />
      ) : (
        <span {...stylex.props(styles.secondary)}>No report text. See the conversation.</span>
      )}
      {mode ? (
        <Feedback
          placeholder={mode === "reply" ? "What should happen next?" : "What needs to change?"}
          value={feedback}
          onChange={setFeedback}
          missing={missing}
        />
      ) : null}
      {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
      {error ? <Problem>{error}</Problem> : null}
      {whyNot ? <span {...stylex.props(styles.small)}>{whyNot}</span> : null}
      <div {...stylex.props(styles.buttonsCentered)}>
        <span {...stylex.props(styles.thirdSmall)}>Tell {t.owner_name ?? "the owner"}:</span>
        {suggestedReplies(report).map((r) => (
          <Button label={r.label} key={r.label} isDisabled={busy} onClick={() => reply(r.text)} />
        ))}
      </div>
      <div {...stylex.props(styles.buttons)}>
        <Button variant="primary" label="Mark done" isDisabled={!ready} onClick={done}>
          {keyed ? (
            <>
              Mark done <KeyHint onAccent>{showChord("secondary-enter", isMac)}</KeyHint>
            </>
          ) : null}
        </Button>
        <Button
          label={`Reply to ${t.owner_name ?? "the owner"}`}
          isDisabled={busy}
          onClick={() => {
            if (mode !== "reply") return setMode("reply");
            if (feedback.trim() === "") return setMissing(true);
            setMissing(false);
            send(async (confirmSecret) => {
              await connection.write("ticket.post", {
                ticket_id: t.id,
                text: feedback.trim(),
                confirm_secret: confirmSecret,
              });
              return "";
            });
          }}
        />
        <Button
          label="Send back to Doing"
          isDisabled={busy}
          onClick={() => {
            if (mode !== "back") return setMode("back");
            if (feedback.trim() === "") return setMissing(true);
            setMissing(false);
            send(async (confirmSecret) => {
              await connection.write(
                "ticket.send_back",
                { ticket_id: t.id, feedback: feedback.trim(), confirm_secret: confirmSecret },
                { retry: false },
              );
              return `${ticketNumber(t)} went back to Doing.`;
            });
          }}
        />
        {lastMessage ? (
          <Button
            label="Open discussion"
            onClick={() => void navigate({ to: "/board", search: { ticket: t.id, message: lastMessage.id } })}
          />
        ) : null}
        {t.root_task_id ? (
          <Link href={`/tasks/${t.root_task_id}`} xstyle={styles.linkEnd}>
            Open run →
          </Link>
        ) : null}
      </div>
    </Box>
  );
}

function PlanGate({ t, plan, keyed = false }: { t: TicketView; plan: string | null | undefined; keyed?: boolean }) {
  const connection = useConnection();
  const write = useTicketWrite();
  const [feedback, setFeedback] = useState("");
  const [missing, setMissing] = useState(false);
  const [rejecting, setRejecting] = useState(false);
  const { busy, held, error, send, sendAnyway } = useSend((text) => {
    toast(text);
    setFeedback("");
  });
  const approve = (decision: "approve" | "approve_edits") =>
    write(
      "ticket.approve_plan",
      { ticket_id: t.id, decision, feedback: feedback.trim() || null },
      { done: "The plan is approved." },
    );
  const too = decideToo(t);
  return (
    <Box tone="amber">
      {keyed ? <GateKeys onYes={() => void approve("approve")} onNo={() => setRejecting(true)} /> : null}
      <div {...stylex.props(styles.gateHead)}>
        <span {...stylex.props(styles.dot)} />
        Plan review · your decision
      </div>
      {too ? <span {...stylex.props(styles.small)}>{too}</span> : null}
      {plan === undefined ? (
        <Loading />
      ) : plan === null ? (
        <span {...stylex.props(styles.secondary)}>
          This agent did not write a plan of its own.{" "}
          {t.root_task_id ? <Link href={`/tasks/${t.root_task_id}`}>Read what it said →</Link> : null}
        </span>
      ) : (
        <Markdown text={plan} />
      )}
      <Feedback
        placeholder="Feedback (needed for Request changes)"
        value={feedback}
        onChange={setFeedback}
        missing={missing}
      />
      {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
      {error ? <Problem>{error}</Problem> : null}
      <div {...stylex.props(styles.buttons)}>
        <Button variant="primary" label="Approve" onClick={() => void approve("approve")}>
          {keyed ? (
            <>
              Approve <KeyHint onAccent>{showChord("secondary-enter", isMac)}</KeyHint>
            </>
          ) : null}
        </Button>
        <Button label="Approve, accept edits" onClick={() => void approve("approve_edits")} />
        <Button
          label="Request changes"
          isDisabled={busy}
          onClick={() => {
            if (feedback.trim() === "") return setMissing(true);
            setMissing(false);
            send(async (confirmSecret) => {
              await connection.write(
                "ticket.revise_plan",
                { ticket_id: t.id, feedback: feedback.trim(), confirm_secret: confirmSecret },
                { retry: false },
              );
              return "A new plan is asked.";
            });
          }}
        />
        <Button label="Reject" variant="destructive" onClick={() => setRejecting(true)} />
      </div>
      <ConfirmDialog
        open={rejecting}
        onClose={() => setRejecting(false)}
        title={`Reject the plan of ${ticketNumber(t)}?`}
        body="The ticket goes back to Backlog."
        confirm="Reject"
        danger
        onConfirm={() => write("ticket.cancel", { ticket_id: t.id })}
      />
    </Box>
  );
}

export function ConfirmDialog({
  open,
  onClose,
  title,
  body,
  confirm,
  danger = false,
  onConfirm,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  body: string;
  confirm: string;
  danger?: boolean;
  onConfirm: () => unknown;
}) {
  return (
    <>
      {open ? (
        <FormDialog
          title={title}
          width={440}
          onClose={onClose}
          footer={
            <>
              <Button label="Cancel" onClick={onClose} />
              <Button
                data-autofocus
                variant={danger ? "destructive" : "primary"}
                label={confirm}
                onClick={() => {
                  onClose();
                  void onConfirm();
                }}
              />
            </>
          }
        >
          <span {...stylex.props(styles.secondary)}>{body}</span>
        </FormDialog>
      ) : null}
    </>
  );
}

const FOLLOWERS_SHOWN = 3;

function Followers({ id }: { id: string }) {
  const connection = useConnection();
  const followers = useKeyed(
    ["tickets", "followers", id],
    async () =>
      expectBody(await connection.call("ticket.followers", { ticket_id: id }), "ticket_followers").value.followers,
  );
  const list = followers.data ?? [];
  if (list.length === 0) return null;
  const shown = list.slice(0, FOLLOWERS_SHOWN);
  return (
    <Row name="Followers">
      <span title={list.map((f) => f.username).join(", ")} {...stylex.props(styles.followers)}>
        {shown.map((f) => (
          <Mark key={f.user_id} letter={initial(f.username)} color={markColor(f.user_id)} size={18} radius={5} />
        ))}
        {list.length > shown.length ? <span {...stylex.props(styles.third)}>+{list.length - shown.length}</span> : null}
      </span>
    </Row>
  );
}

function StatusBox({
  t,
  choosing,
  dispatcherLine,
}: {
  t: TicketView;
  choosing: boolean;
  dispatcherLine: string | null;
}) {
  const { user, claudeLogin } = useLinkState();
  const write = useTicketWrite();
  const employees = useEmployees();
  const discussion = useMessages(t.column === "done" ? t.conversation_id : null);
  const summaryFailed = lastSummaryFailed(discussion.data?.messages ?? []);
  const [leaving, setLeaving] = useState<"backlog" | "delete" | null>(null);
  const me = user?.id ?? null;
  const wait = loginWait(t, me, false);
  const company = useCompany();
  const allTasks = useAllTasks();
  const root = allTasks.data?.find((x) => x.id === t.root_task_id);
  const stopped = t.column === "doing" && root?.status === "waiting" && root.waiting_reason === "input";
  const rootChat = useMessages(stopped ? root?.conversation_id : null);
  const rootList = rootChat.data?.messages ?? [];
  const failure = lastFailure(rootList);
  const retry = useRetryAnswer(root?.conversation_id, rootList, { employeeId: root?.employee_id ?? null });
  const noConnection = useNoConnection();
  const testerId = company.data?.company.tester_employee_id ?? null;
  const tester = employees.data?.find((e) => e.id === testerId)?.name ?? null;
  const tests = (allTasks.data ?? []).filter((x) => x.ticket_id === t.id && x.employee_id === testerId);
  const testTask = testerId ? [...tests].sort((a, b) => b.updated_at_ms - a.updated_at_ms)[0] : undefined;
  const active = (employees.data ?? []).filter((e) => e.status === "active").map((e) => ({ key: e.id, label: e.name }));
  const start = (employee: string | null = null) =>
    write("ticket.start", { ticket_id: t.id, employee_id: employee }, { done: `${ticketNumber(t)} started.` });
  const back = () => write("ticket.cancel", { ticket_id: t.id }, { done: `${ticketNumber(t)} is back in Backlog.` });
  const runLink = t.root_task_id ? (
    <Link href={`/tasks/${t.root_task_id}`} xstyle={styles.linkEnd}>
      Open run →
    </Link>
  ) : null;
  const dialogs = (
    <>
      <ConfirmDialog
        open={leaving === "backlog"}
        onClose={() => setLeaving(null)}
        title={`Back to Backlog: ${ticketNumber(t)}?`}
        body="This stops the work."
        confirm="Back to Backlog"
        onConfirm={back}
      />
      <ConfirmDialog
        open={leaving === "delete"}
        onClose={() => setLeaving(null)}
        title={`Delete ${ticketNumber(t)}?`}
        body="It leaves the board and stays readable for the record."
        confirm="Delete"
        danger
        onConfirm={() => write("ticket.archive", { ticket_id: t.id }, { done: `${ticketNumber(t)} is deleted.` })}
      />
    </>
  );
  if (wait) {
    return (
      <Box tone="amber">
        <span {...stylex.props(styles.medium)}>{wait}</span>
        {t.login_user === me && !wait.startsWith("⏸") ? (
          <div>
            <Button
              href="/settings/account"
              label={claudeLogin?.status === "expired" ? "Reconnect Claude login" : "Connect Claude login"}
            />
          </div>
        ) : null}
        {takeOverOffered(t, me) ? (
          <div>
            <Button label="Run on my login" onClick={() => void write("ticket.use_my_login", { ticket_id: t.id })} />
          </div>
        ) : null}
      </Box>
    );
  }
  if ((t.column === "todo" || t.column === "doing") && t.root_task_status === "failed") {
    return (
      <Box tone="red">
        <span {...stylex.props(styles.failed)}>✕ The main task failed</span>
        <div {...stylex.props(styles.buttons)}>
          <Button label="Start again" variant="primary" onClick={() => void start()} />
          <Button label="Back to Backlog" onClick={() => setLeaving("backlog")} />
          {runLink}
        </div>
        {dialogs}
      </Box>
    );
  }
  switch (t.column) {
    case "backlog":
      return (
        <Box>
          <div {...stylex.props(styles.buttons)}>
            <Button label="Start" variant="primary" onClick={() => void start()} />
            <Button label="Delete" variant="destructive" onClick={() => setLeaving("delete")} />
          </div>
          {dialogs}
        </Box>
      );
    case "todo":
      if (t.owner_employee_id) {
        return <Box>Queued for {t.owner_name}.</Box>;
      }
      return (
        <Box tone={choosing ? "plain" : "amber"}>
          <span {...stylex.props(choosing ? styles.secondary : styles.medium)}>{dispatcherLine}</span>
          <div {...stylex.props(styles.buttons)}>
            <Menu
              label={choosing ? "Choose yourself" : "Choose employee"}
              options={active}
              onPick={(id) => void write("ticket.assign", { ticket_id: t.id, employee_id: id })}
              trigger={<Button label={choosing ? "Choose yourself" : "Choose employee"} />}
            />
            <Button label="Back to Backlog" onClick={() => (choosing ? setLeaving("backlog") : void back())} />
          </div>
          {dialogs}
        </Box>
      );
    case "doing":
      if (stopped) {
        return (
          <Box tone="amber">
            <div {...stylex.props(styles.buttonsCentered)}>
              <span {...stylex.props(styles.medium)}>{t.owner_name ?? "The employee"} stopped and waits for you.</span>
              {runLink}
            </div>
            {failure ? <span {...stylex.props(styles.secondary)}>{failure}</span> : null}
            {failure && noConnection ? <NoConnectionNote /> : null}
            <div {...stylex.props(styles.buttons)}>
              {failure ? <Button label="Try again" variant="primary" onClick={retry} /> : null}
              <Button label="Back to Backlog" onClick={() => setLeaving("backlog")} />
            </div>
            {dialogs}
          </Box>
        );
      }
      return (
        <Box>
          <div {...stylex.props(styles.buttonsCentered)}>
            <span {...stylex.props(styles.secondary)}>
              {t.owner_name ? `${t.owner_name} is working on it.` : "In progress."}
            </span>
            {runLink}
          </div>
          <div>
            <Button label="Back to Backlog" onClick={() => setLeaving("backlog")} />
          </div>
          {dialogs}
        </Box>
      );
    case "testing":
      return (
        <Box>
          {testingNote(testTask?.employee_name ?? tester)}
          {testTask ? (
            <div {...stylex.props(styles.spaced)}>
              <Button href={`/tasks/${testTask.id}`} label="Open test task" />
            </div>
          ) : null}
        </Box>
      );
    case "done":
      return (
        <Box>
          {`Done${t.done_at_ms ? ` · ${time(t.done_at_ms)}` : ""}. Nothing was merged, pushed or deployed by closing it.`}
          {summaryFailed ? (
            <div {...stylex.props(styles.summaryLine)}>
              <span {...stylex.props(styles.red)}>✕ The summary could not be saved to project memory.</span>
              <Button label="Try again" onClick={() => void write("ticket.retry_summary", { ticket_id: t.id })} />
            </div>
          ) : null}
        </Box>
      );
    default:
      return null;
  }
}

function Activity({ id }: { id: string }) {
  const activity = useTicketActivity(id);
  if (activity.isPending) return <Loading />;
  if (activity.error) return <Problem>{activity.error.message}</Problem>;
  if ((activity.data ?? []).length === 0) return <EmptyState isCompact title="Nothing happened yet." />;
  return (
    <ul {...stylex.props(styles.activity)}>
      {activity.data?.map((card) => {
        const detail = activityDetail(card);
        return (
          <li key={card.seq} {...stylex.props(styles.activityItem)}>
            <span>{activityText(card)}</span>
            {detail ? <span {...stylex.props(styles.small)}>{detail}</span> : null}
            <span {...stylex.props(styles.thirdSmall)}>{time(card.at_ms)}</span>
          </li>
        );
      })}
    </ul>
  );
}

type Section = "ticket" | "discussion" | "activity";

export function TicketPanel({ id, onClose, message }: { id: string; onClose: () => void; message?: string }) {
  const platform = usePlatform();
  const { hostId } = useLinkState();
  useKeyContext("TicketPanel");
  useAction("Back", onClose);
  const resizable = useResizable({ defaultSize: 460, minSize: 360, maxSize: 720, autoSaveId: "ticket-panel" });
  const navigate = useNavigate();
  const { features, status } = useLinkState();
  const detail = useTicket(id);
  const board = useBoard(null);
  const employees = useEmployees();
  const models = useModels();
  const write = useTicketWrite();
  const members = useMembers();
  const [tab, setTab] = useState<Section>(message ? "discussion" : "ticket");
  const t = detail.data?.ticket;
  const list = useMemo(() => (t ? [t] : []), [t]);
  const need = useTicketNeeds(list);
  const rootConversation = detail.data?.root_task?.conversation_id;
  const rootMessages = useMessages(t?.column === "plan_review" ? rootConversation : null);
  const discussion = useMessages(t?.conversation_id);
  const activity = useTicketActivity(t?.review_feedback ? id : null);
  const tasks = useMemo(() => foldTree(treeOrder(detail.data?.tasks ?? []), {}), [detail.data?.tasks]);

  if (!t) {
    const gone = detail.error && /not.?found/i.test(detail.error.message);
    return (
      <>
        <ResizeHandle resizable={resizable} isReversed hasDivider />
        <LayoutPanel resizable={resizable} role="complementary" label="Ticket" isScrollable={false} padding={0}>
          <div {...stylex.props(styles.pending)}>
            {gone ? (
              <span {...stylex.props(styles.secondary)}>This ticket is not on this server.</span>
            ) : detail.error ? (
              <Problem>{detail.error.message}</Problem>
            ) : (
              <Loading />
            )}
            <div>
              <Button label="Close" onClick={onClose} />
            </div>
          </div>
        </LayoutPanel>
      </>
    );
  }

  const can = status === "online";
  const open = editable(t) && can;
  const locked = runLocked(t);
  const choosable = ownerChoosable(t) && can;
  const check = doneCheck(detail.data);
  const dispatcher = need.dispatcher(t.project_id);
  const choosing = need.choosing(t);
  const dispatcherName = dispatcher.kind === "project" || dispatcher.kind === "company" ? dispatcher.name : null;
  const dispatcherLine = choosing
    ? `${dispatcherName ?? "The dispatcher"} is choosing an employee…`
    : dispatcher.kind === "none"
      ? "No dispatcher is set. Choose an employee."
      : dispatcher.kind === "inactive"
        ? `${dispatcher.name} is ${dispatcher.state}. Choose an employee.`
        : `${dispatcher.name} could not choose. Choose an employee.`;
  const count = peopleCount((discussion.data?.messages ?? []).filter((m) => !m.deleted_at_ms && !m.thread_parent));
  const update = (params: Record<string, unknown>) =>
    write("ticket.update", { ticket_id: t.id, ...params } as never, { expectedVersion: t.version });
  const columns = (board.data?.columns ?? []).filter((c) => c.column.kind !== t.column);
  const moves = columns.flatMap((c) => {
    const effect = dropEffect(t.column, c.column.kind, choosing);
    return effect.kind === "refused" ? [] : [{ key: c.column.kind, label: `Move to ${c.column.name}` }];
  });
  const move = (to: string) => {
    const effect = dropEffect(t.column, to as TicketView["column"], choosing);
    if (effect.kind === "start") void write("ticket.start", { ticket_id: t.id });
    else if (effect.kind === "back_to_backlog" || effect.kind === "reject")
      void write("ticket.cancel", { ticket_id: t.id });
    else if (effect.kind === "mark_done") void write("ticket.accept", { ticket_id: t.id });
    else setTab("ticket");
  };
  const more = [
    ...moves,
    ...(features.includes("ticket_follow")
      ? [
          { key: "follow", label: "Follow" },
          { key: "unfollow", label: "Unfollow" },
        ]
      : []),
    ...(features.includes("ticket_responsible") && t.column !== "done"
      ? [
          t.responsible_user_id === need.me
            ? { key: "responsible:none", label: "Remove me as responsible" }
            : { key: "responsible", label: "Make me responsible" },
          ...(members.data ?? [])
            .filter((u) => !u.disabled_at_ms && u.id !== need.me && u.id !== t.responsible_user_id)
            .map((u) => ({ key: `responsible:${u.id}`, label: `Responsible: ${u.username}` })),
          ...(t.responsible_user_id && t.responsible_user_id !== need.me
            ? [{ key: "responsible:none", label: "Responsible: no one" }]
            : []),
        ]
      : []),
    { key: "copy", label: "Copy link" },
  ];
  const pick = (key: string) => {
    if (key === "follow" || key === "unfollow")
      void write(
        "ticket.follow",
        { ticket_id: t.id, on: key === "follow" },
        { done: key === "follow" ? "You follow this ticket." : "You don't follow this ticket any more." },
      );
    else if (key.startsWith("responsible:")) {
      const who = key.slice("responsible:".length);
      void write("ticket.set_responsible", { ticket_id: t.id, responsible_user_id: who === "none" ? null : who });
    } else if (key === "responsible")
      void write("ticket.set_responsible", { ticket_id: t.id, responsible_user_id: need.me });
    else if (key === "copy") void platform.copy(ticketLink(hostId ?? "", t.id)).then(() => toast("Link copied."));
    else move(key);
  };

  const addFiles = async (files: { path: string }[]) => {
    if (files.length === 0) return;
    await write(
      "ticket.update",
      { ticket_id: t.id, add_files: files.map((f) => f.path) },
      { expectedVersion: t.version, done: "Files added." },
    );
  };
  const addFailed = (e: unknown) => toast(e instanceof Error ? e.message : "This did not work.");
  // A file copied or dropped on the panel of an open ticket goes on the ticket.
  const takeFiles = (given: File[]) => {
    const { fit, problems } = acceptable(given);
    for (const problem of problems) toast(problem);
    if (fit.length > 0 && t.column !== "done") platform.keepFiles(fit).then(addFiles).catch(addFailed);
  };
  return (
    <>
      <ResizeHandle resizable={resizable} isReversed hasDivider />
      <LayoutPanel
        resizable={resizable}
        role="complementary"
        label={`${ticketNumber(t)} ${t.title}`}
        isScrollable={false}
        padding={0}
        onPaste={(e) => {
          const given = filesFrom(e.clipboardData);
          if (given.length === 0) return;
          e.preventDefault();
          takeFiles(given);
        }}
        onDragOver={(e) => {
          if (e.dataTransfer.types.includes("Files")) e.preventDefault();
        }}
        onDrop={(e) => {
          const given = filesFrom(e.dataTransfer);
          if (given.length === 0) return;
          e.preventDefault();
          takeFiles(given);
        }}
      >
        <div {...stylex.props(styles.panel)}>
          <div {...stylex.props(styles.panelHead)}>
            <span {...stylex.props(styles.number)}>{ticketNumber(t)}</span>
            <div {...stylex.props(styles.panelTools)}>
              <Menu
                label="More"
                options={more}
                onPick={pick}
                trigger={<IconButton variant="ghost" size="sm" label="More" icon={<Icon icon="moreHorizontal" />} />}
              />
              <IconButton variant="ghost" size="sm" label="Close" icon={<Icon icon="close" />} onClick={onClose} />
            </div>
          </div>
          <div {...stylex.props(styles.top)}>
            <Editable
              label="Title"
              value={t.title}
              enabled={open}
              display={
                <Heading level={2} xstyle={styles.title}>
                  {t.title}
                </Heading>
              }
              onSave={(title) => update({ title: title.trim() })}
            />
            <TicketActivityLine ticketId={t.id} />
            <div {...stylex.props(styles.chips)}>
              <Chip>
                <ColumnGlyph kind={t.column} />
                {columns.length >= 0
                  ? (board.data?.columns.find((c) => c.column.kind === t.column)?.column.name ?? t.column)
                  : null}
              </Chip>
              {open ? (
                <Menu
                  label="Priority"
                  value={t.priority}
                  options={PRIORITIES.map((p) => ({ key: p, label: priorityLabel(p) }))}
                  onPick={(priority: TicketPriority) => void update({ priority })}
                  trigger={<Chip>{priorityLabel(t.priority)}</Chip>}
                />
              ) : (
                <Chip disabled>{priorityLabel(t.priority)}</Chip>
              )}
              {choosable ? (
                <Menu
                  label="Owner"
                  {...(t.owner_employee_id ? { value: t.owner_employee_id } : {})}
                  options={(employees.data ?? [])
                    .filter((e) => e.status === "active")
                    .map((e) => ({ key: e.id, label: e.name }))}
                  onPick={(id) => void write("ticket.assign", { ticket_id: t.id, employee_id: id })}
                  trigger={
                    <Chip>
                      {t.owner_name ? (
                        <Mark
                          letter={initial(t.owner_name)}
                          color={markColor(t.owner_employee_id ?? "")}
                          size={14}
                          radius={4}
                        />
                      ) : null}
                      {t.owner_name ?? "No owner"}
                    </Chip>
                  }
                />
              ) : (
                <Chip disabled>{t.owner_name ?? "No owner"}</Chip>
              )}
              {open && !locked ? (
                <Menu
                  label="Mode"
                  value={t.run.mode ?? "default"}
                  options={MODES}
                  onPick={(mode) =>
                    void update({ run: { ...t.run, mode: mode === "default" ? null : (mode as AgentMode) } })
                  }
                  trigger={<Chip name="Mode">{MODES.find((m) => m.key === (t.run.mode ?? "default"))?.label}</Chip>}
                />
              ) : null}
              {open && !locked ? (
                <Menu
                  label="Model"
                  value={t.run.model ?? "default"}
                  options={models}
                  onPick={(model) => void update({ run: { ...t.run, model: model === "default" ? null : model } })}
                  trigger={
                    <Chip name="Model">
                      {models.find((m) => m.key === (t.run.model ?? "default"))?.label ?? "Default"}
                    </Chip>
                  }
                />
              ) : null}
            </div>
            <TicketOdometer ticketId={t.id} />
            <ThreadWaiting ticketId={t.id} />
          </div>
          <div {...stylex.props(styles.topGate)}>
            {t.column === "review" ? (
              <ReviewGate
                key={t.version}
                t={t}
                report={detail.data?.root_task?.result ?? null}
                ready={check.kind === "ready" && can}
                whyNot={check.kind === "working" ? "Still at work: Done waits until that has finished." : null}
              />
            ) : t.column === "plan_review" ? (
              <PlanGate
                t={t}
                plan={rootMessages.isPending ? undefined : latestPlan(rootMessages.data?.messages ?? [])}
              />
            ) : (
              <StatusBox t={t} choosing={choosing} dispatcherLine={dispatcherLine} />
            )}
            <TabList
              role="tablist"
              aria-label="Ticket sections"
              hasDivider
              value={tab}
              onChange={(next) => setTab(next as Section)}
            >
              {(
                [
                  ["ticket", "Ticket"],
                  ...(features.includes("ticket_discussion")
                    ? ([["discussion", count > 0 ? `Discussion ${count}` : "Discussion"]] as [Section, string][])
                    : []),
                  ["activity", "Activity"],
                ] as [Section, string][]
              ).map(([key, label]) => (
                <Tab key={key} value={key} label={label} panelId="ticket-section" />
              ))}
            </TabList>
          </div>
          <div id="ticket-section" role="tabpanel" aria-label="Ticket section" {...stylex.props(styles.tabPanel)}>
            {tab === "discussion" ? (
              t.conversation_id ? (
                <TicketChat
                  key={t.conversation_id}
                  conversationId={t.conversation_id}
                  ticketId={t.id}
                  {...(message ? { message } : {})}
                />
              ) : null
            ) : (
              <div {...stylex.props(styles.section)}>
                {tab === "activity" ? (
                  <Activity id={t.id} />
                ) : (
                  <>
                    {t.review_feedback && t.column === "doing" ? (
                      <Box>
                        <span {...stylex.props(styles.sectionLabel)}>{feedbackLabel(activity.data)}</span>
                        <span>{t.review_feedback}</span>
                      </Box>
                    ) : null}
                    <Editable
                      label="Description"
                      value={t.description ?? ""}
                      enabled={open}
                      multiline
                      display={
                        t.description?.trim() ? (
                          <Markdown text={t.description} />
                        ) : (
                          <span {...stylex.props(styles.third)}>No description.</span>
                        )
                      }
                      onSave={(description) => update({ description })}
                    />
                    <div {...stylex.props(styles.rows)}>
                      <Row name="Project">{t.project_name}</Row>
                      <Row name="Labels">
                        <Editable
                          label="Labels"
                          value={t.labels.join(", ")}
                          enabled={open}
                          placeholder="backend, ui"
                          display={
                            t.labels.length > 0 ? (
                              t.labels.join(", ")
                            ) : (
                              <span {...stylex.props(styles.third)}>None</span>
                            )
                          }
                          onSave={(text) => update({ labels: parseLabels(text) })}
                        />
                      </Row>
                      <Row name="Deadline">
                        <Editable
                          label="Deadline"
                          value={t.deadline_ms ? deadlineInput(t.deadline_ms, -new Date().getTimezoneOffset()) : ""}
                          enabled={open}
                          placeholder="2026-11-04 or 2026-11-04 17:00"
                          display={
                            t.deadline_ms ? time(t.deadline_ms) : <span {...stylex.props(styles.third)}>None</span>
                          }
                          onSave={async (text) => {
                            try {
                              const ms = parseDeadline(text, -new Date().getTimezoneOffset());
                              return await update(ms === null ? { clear_deadline: true } : { deadline_ms: ms });
                            } catch (e) {
                              toast((e as Error).message);
                              return false;
                            }
                          }}
                        />
                      </Row>
                      {features.includes(DESIGN_FEATURE) ? (
                        <DesignRow
                          url={t.design_url ?? null}
                          enabled={open}
                          onSave={async (text) => {
                            const problem = artifactProblem(text);
                            if (problem) {
                              toast(problem);
                              return false;
                            }
                            return update({ design_url: text.trim() });
                          }}
                        />
                      ) : null}
                      {features.includes("ticket_responsible") ? (
                        <Row name="Responsible">
                          {t.responsible_username ?? <span {...stylex.props(styles.third)}>No one</span>}
                        </Row>
                      ) : null}
                      <Row name="Runs on">
                        {t.login_user === need.me ? "Your Claude login" : `${t.login_username}'s Claude login`}
                      </Row>
                      <Row name="Created by">{`${t.created_by_username} · ${time(t.created_at_ms)}`}</Row>
                      {features.includes("ticket_follow") ? <Followers id={t.id} /> : null}
                      <Row name="Files">
                        <span {...stylex.props(styles.files)}>
                          <FileChips files={t.attachments} />
                          {t.column === "done" ? null : (
                            <Button label="Add files…" onClick={() => platform.pickFiles().then(addFiles, addFailed)} />
                          )}
                        </span>
                      </Row>
                    </div>
                    {tasks.length > 0 ? (
                      <div {...stylex.props(styles.tasks)}>
                        <span {...stylex.props(styles.sectionLabel)}>Tasks ({detail.data?.tasks.length})</span>
                        {tasks.map(({ task, depth }) => {
                          const state = taskStateLine(task);
                          return (
                            <button
                              key={task.id}
                              type="button"
                              onClick={() => void navigate({ to: "/tasks/$taskId", params: { taskId: task.id } })}
                              {...stylex.props(styles.task)}
                              style={{ paddingLeft: 4 + 16 * depth }}
                            >
                              <span {...stylex.props(styles.taskTitle)}>{task.title}</span>
                              <span {...stylex.props(styles.taskState)} style={{ color: state.color }}>
                                {state.text}
                              </span>
                            </button>
                          );
                        })}
                      </div>
                    ) : null}
                  </>
                )}
              </div>
            )}
          </div>
        </div>
      </LayoutPanel>
    </>
  );
}
