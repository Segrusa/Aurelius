import { Button } from "@astryxdesign/core/Button";
import { Icon } from "@astryxdesign/core/Icon";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Link } from "@astryxdesign/core/Link";
import { TextArea } from "@astryxdesign/core/TextArea";
import { TextInput } from "@astryxdesign/core/TextInput";
import { expectBody } from "@aurelius/client";
import type { TicketView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { type ReactNode, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useAllTasks, useBoard, useCompany, useEmployees, useKeyed, useMembers, useMessages } from "../data";
import { showChord } from "../keys/keymap";
import { isMac, useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { KeyHint } from "../ui/key-hint";
import { Markdown } from "../ui/markdown";
import { initial, Mark, markColor } from "../ui/marks";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { decideToo, dropEffect, loginWait, suggestedReplies, takeOverOffered, testingNote } from "./board-logic";
import { Held, useSend } from "./dialogs";
import { ticketLink } from "./links-logic";
import { FormDialog, Loading, Problem } from "./org-kit";
import {
  acceptLabel,
  cancelConfirmation,
  cancelNeedsConfirm,
  doneConfirmation,
  doneLine,
  isQuick,
} from "./quick-logic";
import { lastFailure, lastSummaryFailed } from "./run-logic";
import { useRetryAnswer } from "./run-tools";
import { gateKeyAllowed } from "./ticket-session-logic";
import { NoConnectionNote, ticketNumber, useNoConnection, useTicketNeeds, useTicketWrite } from "./tickets-kit";

/** The modes of a ticket's run; no choice is "Your default" (on an old server "Owner's default"). */
export const MODES: { key: string; label: string }[] = [
  { key: "ask", label: "Ask" },
  { key: "accept_edits", label: "Accept edits" },
  { key: "auto", label: "Auto" },
  { key: "plan", label: "Plan" },
];

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
  gateTitle: { flexGrow: 1 },
  clamped: { maxHeight: "calc(8 * 1.5em)", overflow: "hidden" },
  dot: { width: 7, height: 7, borderRadius: "50%", backgroundColor: "var(--color-text-warning)" },
  small: { fontSize: 12, color: "var(--color-text-secondary)" },
  secondary: { color: "var(--color-text-secondary)" },
  third: { color: "var(--precision-text3)" },
  thirdSmall: { fontSize: 12, color: "var(--precision-text3)" },
  medium: { fontWeight: 500 },
  failed: { fontWeight: 500, color: "var(--color-text-error)" },
  red: { color: "var(--color-text-error)" },
  error: { fontSize: 12, color: "var(--color-text-error)" },
  linkEnd: { marginInlineStart: "auto", alignSelf: "center", fontSize: 12 },
  followers: { display: "inline-flex", alignItems: "center", gap: 4 },
  spaced: { marginTop: 8 },
  summaryLine: { marginTop: 8, display: "flex", alignItems: "center", gap: 8 },
});

export function time(ms: number): string {
  return new Date(ms).toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" });
}

export function Box({ tone = "plain", children }: { tone?: "plain" | "amber" | "red"; children: ReactNode }) {
  return (
    <div {...stylex.props(styles.box, tone === "amber" && styles.boxAmber, tone === "red" && styles.boxRed)}>
      {children}
    </div>
  );
}

export function Row({ name, children }: { name: string; children: ReactNode }) {
  return (
    <div {...stylex.props(styles.row)}>
      <span {...stylex.props(styles.third)}>{name}</span>
      <div {...stylex.props(styles.rowValue)}>{children}</div>
    </div>
  );
}

/** Text that turns into a field on a click. Saved with Enter or the button, left with Escape. */
export function Editable({
  value,
  enabled,
  multiline = false,
  display,
  onSave,
  label,
  placeholder,
  opened = false,
  onClose,
  footer,
}: {
  value: string;
  enabled: boolean;
  multiline?: boolean;
  display: ReactNode;
  /** Under the display while it is not edited, outside its click target (a "Show more"). */
  footer?: ReactNode;
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
    const shown = enabled ? (
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
    // Always the same shape, so a footer that comes or goes does not remount the display.
    return (
      <>
        {shown}
        {footer}
      </>
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

/** Enter stays "open"; deciding takes the stronger chord, never while it would send a filled field's words. */
export function GateKeys({ onYes, onNo }: { onYes: () => void; onNo: () => void }) {
  useKeyContext("Decision");
  useAction("DecideYes", () => {
    if (gateKeyAllowed(document.activeElement as HTMLInputElement | null)) onYes();
  });
  useAction("RejectApproval", onNo);
  return null;
}

export function ReviewGate({
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
  // "back" asks for changes; a reply to the owner goes through the session's composer.
  const [mode, setMode] = useState<"back" | null>(null);
  const [missing, setMissing] = useState(false);
  const { busy, held, error, send, sendAnyway } = useSend((text) => {
    if (text !== "") toast(text);
    setMode(null);
    setFeedback("");
  });
  const reply = (text: string) =>
    send(async (confirmSecret) => {
      await connection.write("ticket.post", { ticket_id: t.id, text, confirm_secret: confirmSecret });
      return "";
    });
  const too = decideToo(t);
  const quick = isQuick(t);
  // A quick task stays in Review while the daemon merges it; its events move the card when it worked.
  const done = () =>
    void write(
      "ticket.accept",
      { ticket_id: t.id },
      { done: quick ? `Merging ${ticketNumber(t)} into main…` : `${ticketNumber(t)} is Done.` },
    );
  const accept = acceptLabel(t);
  const [open, toggle] = useGateOpen();
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
      <GateHead open={open} onToggle={toggle}>
        In review · your decision
      </GateHead>
      {open && too ? <span {...stylex.props(styles.small)}>{too}</span> : null}
      {!open ? null : report ? (
        <Clamped text={report} more="Show the whole report" />
      ) : (
        <span {...stylex.props(styles.secondary)}>No report text. See the conversation.</span>
      )}
      {mode ? (
        <Feedback placeholder="What needs to change?" value={feedback} onChange={setFeedback} missing={missing} />
      ) : null}
      {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
      {error ? <Problem>{error}</Problem> : null}
      {whyNot ? <span {...stylex.props(styles.small)}>{whyNot}</span> : null}
      <div {...stylex.props(styles.buttonsCentered)}>
        <span {...stylex.props(styles.thirdSmall)}>Tell {t.owner_name ?? "the owner"}:</span>
        {suggestedReplies(report, quick).map((r) => (
          <Button label={r.label} key={r.label} isDisabled={busy} onClick={() => reply(r.text)} />
        ))}
      </div>
      <div {...stylex.props(styles.buttons)}>
        <Button variant="primary" label={accept} isDisabled={!ready} onClick={done}>
          {keyed ? (
            <>
              {accept} <KeyHint onAccent>{showChord("secondary-enter", isMac)}</KeyHint>
            </>
          ) : null}
        </Button>
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
        {t.root_task_id ? (
          <Link href={`/tasks/${t.root_task_id}`} xstyle={styles.linkEnd}>
            Open run →
          </Link>
        ) : null}
      </div>
    </Box>
  );
}

export function PlanGate({
  t,
  plan,
  keyed = false,
}: {
  t: TicketView;
  plan: string | null | undefined;
  keyed?: boolean;
}) {
  const connection = useConnection();
  const write = useTicketWrite();
  const [feedback, setFeedback] = useState("");
  // The note opens only when it is wanted.
  const [noting, setNoting] = useState(false);
  const [open, toggle] = useGateOpen();
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
      <GateHead open={open} onToggle={toggle}>
        Plan review · your decision
      </GateHead>
      {open && too ? <span {...stylex.props(styles.small)}>{too}</span> : null}
      {!open ? null : plan === undefined ? (
        <Loading />
      ) : plan === null ? (
        <span {...stylex.props(styles.secondary)}>
          This agent did not write a plan of its own.{" "}
          {t.root_task_id ? <Link href={`/tasks/${t.root_task_id}`}>Read what it said →</Link> : null}
        </span>
      ) : (
        <Clamped text={plan} more="Show the whole plan" />
      )}
      {noting ? (
        <Feedback
          placeholder="Feedback (needed for Request changes)"
          value={feedback}
          onChange={setFeedback}
          missing={missing}
        />
      ) : null}
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
            if (!noting) return setNoting(true);
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
        {noting ? null : <Button variant="ghost" label="Add a note" onClick={() => setNoting(true)} />}
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

/** Whether a gate shows its body; it folds to its head and buttons, remembered. */
const GATE_KEY = "ticket-session.gate.open";

function useGateOpen(): [boolean, () => void] {
  const { storage } = usePlatform();
  const [open, setOpen] = useState(() => storage.get(GATE_KEY) !== "0");
  return [
    open,
    () => {
      storage.set(GATE_KEY, open ? "0" : "1");
      setOpen(!open);
    },
  ];
}

function GateHead({ open, onToggle, children }: { open: boolean; onToggle: () => void; children: ReactNode }) {
  return (
    <div {...stylex.props(styles.gateHead)}>
      <span {...stylex.props(styles.dot)} />
      <span {...stylex.props(styles.gateTitle)}>{children}</span>
      <IconButton
        variant="ghost"
        size="sm"
        label={open ? "Fold the decision" : "Unfold the decision"}
        aria-expanded={open}
        icon={<Icon icon={open ? "chevronDown" : "chevronRight"} />}
        onClick={onToggle}
      />
    </div>
  );
}

/** A long report or plan starts at eight lines; the rest on request. */
export function Clamped({ text, more }: { text: string; more: string }) {
  const [all, setAll] = useState(false);
  const long = text.split("\n").length > 8 || text.length > 640;
  return (
    <div {...stylex.props(styles.stack)}>
      <div {...stylex.props(long && !all && styles.clamped)}>
        <Markdown text={text} />
      </div>
      {long ? (
        <button type="button" onClick={() => setAll(!all)} {...stylex.props(styles.designChange)}>
          {all ? "Show less" : more}
        </button>
      ) : null}
    </div>
  );
}

/** What the dispatcher does for a ticket in Todo without an owner, in words, and whether it is choosing. */
export function useDispatcherLine(t: TicketView | undefined): { choosing: boolean; line: string | null } {
  const list = useMemo(() => (t ? [t] : []), [t]);
  const need = useTicketNeeds(list);
  if (!t) return { choosing: false, line: null };
  const dispatcher = need.dispatcher(t.project_id);
  const choosing = need.choosing(t);
  const name = dispatcher.kind === "project" || dispatcher.kind === "company" ? dispatcher.name : null;
  const line = choosing
    ? `${name ?? "The dispatcher"} is choosing an employee…`
    : dispatcher.kind === "none"
      ? "No dispatcher is set. Choose an employee."
      : dispatcher.kind === "inactive"
        ? `${dispatcher.name} is ${dispatcher.state}. Choose an employee.`
        : `${dispatcher.name} could not choose. Choose an employee.`;
  return { choosing, line };
}

/**
 * A ticket's More menu: Move to, Follow, Responsible and Copy link, with the same writes and the
 * quick task's confirmations as the board. `dialogs` must be rendered for those confirmations.
 */
export function useTicketMenu(
  t: TicketView | undefined,
  choosing: boolean,
): { options: { key: string; label: string }[]; pick: (key: string) => void; dialogs: ReactNode } {
  const platform = usePlatform();
  const { hostId, features } = useLinkState();
  const board = useBoard(null);
  const members = useMembers();
  const write = useTicketWrite();
  const need = useTicketNeeds(useMemo(() => (t ? [t] : []), [t]));
  // Done or Backlog of a quick task: merging and losing its branch are asked first.
  const [asking, setAsking] = useState<"done" | "backlog" | null>(null);
  if (!t) return { options: [], pick: () => undefined, dialogs: null };
  const quick = isQuick(t);
  const columns = (board.data?.columns ?? []).filter((c) => c.column.kind !== t.column);
  const moves = columns.flatMap((c) => {
    const effect = dropEffect(t.column, c.column.kind, choosing);
    return effect.kind === "refused" ? [] : [{ key: c.column.kind, label: `Move to ${c.column.name}` }];
  });
  const move = (to: string) => {
    const effect = dropEffect(t.column, to as TicketView["column"], choosing);
    if (effect.kind === "start") void write("ticket.start", { ticket_id: t.id });
    else if (quick && effect.kind === "back_to_backlog") setAsking("backlog");
    else if (quick && effect.kind === "mark_done") setAsking("done");
    else if (effect.kind === "back_to_backlog" || effect.kind === "reject")
      void write("ticket.cancel", { ticket_id: t.id });
    else if (effect.kind === "mark_done") void write("ticket.accept", { ticket_id: t.id });
  };
  const options = [
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
    else if (key === "copy")
      void platform.copy(ticketLink(hostId ?? "", String(t.number))).then(() => toast("Link copied."));
    else move(key);
  };
  const asked = asking === "done" ? doneConfirmation(t) : cancelConfirmation(t);
  const dialogs = (
    <ConfirmDialog
      open={asking !== null}
      onClose={() => setAsking(null)}
      title={asked.title}
      body={asked.body}
      confirm={asked.confirm}
      danger={asking === "backlog"}
      onConfirm={() =>
        asking === "done"
          ? write("ticket.accept", { ticket_id: t.id }, { done: `Merging ${ticketNumber(t)} into main…` })
          : write("ticket.cancel", { ticket_id: t.id }, { done: `${ticketNumber(t)} is back in Backlog.` })
      }
    />
  );
  return { options, pick, dialogs };
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

export function Followers({ id }: { id: string }) {
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

export function StatusBox({
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
  const leave = cancelConfirmation(t);
  const dialogs = (
    <>
      <ConfirmDialog
        open={leaving === "backlog"}
        onClose={() => setLeaving(null)}
        title={leave.title}
        body={leave.body}
        confirm={leave.confirm}
        danger={isQuick(t)}
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
            <Button
              label="Back to Backlog"
              onClick={() => (cancelNeedsConfirm(t, choosing) ? setLeaving("backlog") : void back())}
            />
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
      // Nothing while the owner simply works: the progress says what runs.
      return null;
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
      // On Done only what there is to do or know: a summary to retry, a quick task's merge.
      if (!summaryFailed && !isQuick(t)) return null;
      return (
        <Box>
          {doneLine(t, t.done_at_ms ? time(t.done_at_ms) : null)}
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
