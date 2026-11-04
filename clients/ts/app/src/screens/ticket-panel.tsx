import { expectBody } from "@aurelius/client";
import type { AgentMode, TicketPriority, TicketView } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { type ReactNode, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useBoard, useEmployees, useKeyed, useMembers, useMessages, useTicket, useTicketActivity } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { acceptable, filesFrom } from "../ui/attach";
import { Button, Chip, initial, Mark, markColor } from "../ui/kit";
import { Markdown } from "../ui/markdown";
import { Dialog, Menu } from "../ui/overlay";
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
  priorityLabel,
  runLocked,
  takeOverOffered,
  testingNote,
} from "./board-logic";
import { canChange, completeMention, mentionSuggestions } from "./chat-logic";
import { Held, parseDeadline, parseLabels, useModels, useSend } from "./dialogs";
import { messageLink, ticketLink } from "./links-logic";
import { lastSummaryFailed } from "./run-logic";
import { FileChips } from "./run-tools";
import { foldTree, taskStateLine, treeOrder } from "./tasks-logic";
import { ColumnGlyph, ticketNumber, useTicketNeeds, useTicketWrite } from "./tickets-kit";

const MODES: { key: string; label: string }[] = [
  { key: "default", label: "Owner's default" },
  { key: "ask", label: "Ask" },
  { key: "accept_edits", label: "Accept edits" },
  { key: "auto", label: "Auto" },
  { key: "plan", label: "Plan" },
];

const FIELD =
  "w-full rounded-[6px] border border-btn-line bg-sunk px-2 text-[13px] text-text outline-none focus:border-acc-text";

function time(ms: number): string {
  return new Date(ms).toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" });
}

function Box({ tone = "plain", children }: { tone?: "plain" | "amber" | "red"; children: ReactNode }) {
  const look =
    tone === "amber"
      ? "border-amber-line bg-amber-soft"
      : tone === "red"
        ? "border-red bg-sunk"
        : "border-line bg-sunk";
  return <div className={`flex flex-col gap-[10px] rounded-[8px] border px-[14px] py-3 ${look}`}>{children}</div>;
}

function Row({ name, children }: { name: string; children: ReactNode }) {
  return (
    <div className="grid min-h-[30px] grid-cols-[100px_minmax(0,1fr)] items-center gap-2">
      <span className="text-text3">{name}</span>
      <div className="min-w-0">{children}</div>
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
}: {
  value: string;
  enabled: boolean;
  multiline?: boolean;
  display: ReactNode;
  onSave: (next: string) => Promise<boolean>;
  label: string;
  placeholder?: string;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  if (draft === null) {
    return enabled ? (
      <button
        type="button"
        aria-label={`Edit ${label}`}
        onClick={() => setDraft(value)}
        className="block w-full cursor-text rounded-[5px] border-0 bg-transparent p-0 text-left text-inherit hover:bg-sel"
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
    <div className="flex flex-col gap-2">
      {multiline ? (
        <textarea
          aria-label={label}
          className={`${FIELD} min-h-[120px] resize-y py-2`}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={keys}
          ref={(el) => el?.focus({ preventScroll: true })}
        />
      ) : (
        <input
          aria-label={label}
          className={`${FIELD} h-8`}
          value={draft}
          placeholder={placeholder}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={keys}
          ref={(el) => el?.focus({ preventScroll: true })}
        />
      )}
      <div className="flex gap-2">
        <Button kind="primary" onClick={() => void save()}>
          Save
        </Button>
        <Button onClick={() => setDraft(null)}>Cancel</Button>
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
      <textarea
        aria-label={placeholder}
        className={`${FIELD} min-h-[72px] resize-y py-2`}
        placeholder={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
      {missing ? <span className="text-[12px] text-red">Write what needs to change.</span> : null}
    </>
  );
}

function ReviewGate({
  t,
  report,
  ready,
  whyNot,
}: {
  t: TicketView;
  report: string | null;
  ready: boolean;
  whyNot: string | null;
}) {
  const connection = useConnection();
  const write = useTicketWrite();
  const [feedback, setFeedback] = useState("");
  const [open, setOpen] = useState(false);
  const [missing, setMissing] = useState(false);
  const { busy, held, error, send, sendAnyway } = useSend((text) => {
    toast(text);
    setOpen(false);
    setFeedback("");
  });
  const too = decideToo(t);
  return (
    <Box tone="amber">
      <div className="flex items-center gap-2 font-medium">
        <span className="size-[7px] rounded-full bg-amber" />
        In review · your decision
      </div>
      {too ? <span className="text-[12px] text-text2">{too}</span> : null}
      {report ? <Markdown text={report} /> : <span className="text-text2">No report text. See the conversation.</span>}
      {open ? (
        <Feedback placeholder="What needs to change?" value={feedback} onChange={setFeedback} missing={missing} />
      ) : null}
      {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
      {error ? (
        <p role="alert" className="m-0 text-red">
          {error}
        </p>
      ) : null}
      {whyNot ? <span className="text-[12px] text-text2">{whyNot}</span> : null}
      <div className="flex flex-wrap gap-2">
        <Button
          kind="primary"
          disabled={!ready}
          onClick={() => void write("ticket.accept", { ticket_id: t.id }, { done: `${ticketNumber(t)} is Done.` })}
        >
          Mark done
        </Button>
        <Button
          disabled={busy}
          onClick={() => {
            if (!open) return setOpen(true);
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
        >
          Send back to Doing
        </Button>
        {t.root_task_id ? (
          <Link
            to="/tasks/$taskId"
            params={{ taskId: t.root_task_id }}
            className="ml-auto self-center text-[12px] text-acc-text no-underline"
          >
            Open run →
          </Link>
        ) : null}
      </div>
    </Box>
  );
}

function PlanGate({ t, plan }: { t: TicketView; plan: string | null | undefined }) {
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
      <div className="flex items-center gap-2 font-medium">
        <span className="size-[7px] rounded-full bg-amber" />
        Plan review · your decision
      </div>
      {too ? <span className="text-[12px] text-text2">{too}</span> : null}
      {plan === undefined ? (
        <span className="text-text2">Loading the plan…</span>
      ) : plan === null ? (
        <span className="text-text2">The plan is in the conversation.</span>
      ) : (
        <Markdown text={plan} />
      )}
      <Feedback placeholder="Feedback (needed for Pas aan)" value={feedback} onChange={setFeedback} missing={missing} />
      {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
      {error ? (
        <p role="alert" className="m-0 text-red">
          {error}
        </p>
      ) : null}
      <div className="flex flex-wrap gap-2">
        <Button kind="primary" onClick={() => void approve("approve")}>
          Approve
        </Button>
        <Button onClick={() => void approve("approve_edits")}>Approve, accept edits</Button>
        <Button
          disabled={busy}
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
        >
          Pas aan
        </Button>
        <Button kind="danger" onClick={() => setRejecting(true)}>
          Afwijzen
        </Button>
      </div>
      <ConfirmDialog
        open={rejecting}
        onClose={() => setRejecting(false)}
        title={`Reject the plan of ${ticketNumber(t)}?`}
        body="The ticket goes back to Backlog."
        confirm="Afwijzen"
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
    <Dialog open={open} onClose={onClose} title={title} width={440}>
      <div className="flex flex-col gap-2 p-4">
        <p className="m-0 text-[15px] font-semibold">{title}</p>
        <p className="m-0 text-text2">{body}</p>
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Cancel</Button>
        <Button
          data-autofocus
          kind={danger ? "danger" : "primary"}
          onClick={() => {
            onClose();
            void onConfirm();
          }}
        >
          {confirm}
        </Button>
      </div>
    </Dialog>
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
      <span title={list.map((f) => f.username).join(", ")} className="inline-flex items-center gap-1">
        {shown.map((f) => (
          <Mark key={f.user_id} letter={initial(f.username)} color={markColor(f.user_id)} size={18} radius={5} />
        ))}
        {list.length > shown.length ? <span className="text-text3">+{list.length - shown.length}</span> : null}
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
  const { user } = useLinkState();
  const write = useTicketWrite();
  const employees = useEmployees();
  const discussion = useMessages(t.column === "done" ? t.conversation_id : null);
  const summaryFailed = lastSummaryFailed(discussion.data?.messages ?? []);
  const [leaving, setLeaving] = useState<"backlog" | "delete" | null>(null);
  const me = user?.id ?? null;
  const wait = loginWait(t, me, false);
  const active = (employees.data ?? []).filter((e) => e.status === "active").map((e) => ({ key: e.id, label: e.name }));
  const start = (employee: string | null = null) =>
    write("ticket.start", { ticket_id: t.id, employee_id: employee }, { done: `${ticketNumber(t)} started.` });
  const back = () => write("ticket.cancel", { ticket_id: t.id }, { done: `${ticketNumber(t)} is back in Backlog.` });
  const runLink = t.root_task_id ? (
    <Link
      to="/tasks/$taskId"
      params={{ taskId: t.root_task_id }}
      className="ml-auto self-center text-[12px] text-acc-text no-underline"
    >
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
        <span className="font-medium">{wait}</span>
        {takeOverOffered(t, me) ? (
          <div>
            <Button onClick={() => void write("ticket.use_my_login", { ticket_id: t.id })}>Run on my login</Button>
          </div>
        ) : null}
      </Box>
    );
  }
  if ((t.column === "todo" || t.column === "doing") && t.root_task_status === "failed") {
    return (
      <Box tone="red">
        <span className="font-medium text-red">✕ The main task failed</span>
        <div className="flex flex-wrap gap-2">
          <Button kind="primary" onClick={() => void start()}>
            Start again
          </Button>
          <Button onClick={() => setLeaving("backlog")}>Back to Backlog</Button>
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
          <div className="flex flex-wrap gap-2">
            <Button kind="primary" onClick={() => void start()}>
              Start
            </Button>
            <Button kind="danger" onClick={() => setLeaving("delete")}>
              Delete
            </Button>
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
          <span className={choosing ? "text-text2" : "font-medium"}>{dispatcherLine}</span>
          <div className="flex flex-wrap gap-2">
            <Menu
              label={choosing ? "Choose yourself" : "Choose employee"}
              options={active}
              onPick={(id) => void write("ticket.assign", { ticket_id: t.id, employee_id: id })}
              trigger={<Button>{choosing ? "Choose yourself" : "Choose employee"}</Button>}
            />
            <Button onClick={() => (choosing ? setLeaving("backlog") : void back())}>Back to Backlog</Button>
          </div>
          {dialogs}
        </Box>
      );
    case "doing":
      return (
        <Box>
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-text2">{t.owner_name ? `${t.owner_name} is working on it.` : "In progress."}</span>
            {runLink}
          </div>
          <div>
            <Button onClick={() => setLeaving("backlog")}>Back to Backlog</Button>
          </div>
          {dialogs}
        </Box>
      );
    case "testing":
      return <Box>{testingNote(null)}</Box>;
    case "done":
      return (
        <Box>
          {`Done${t.done_at_ms ? ` · ${time(t.done_at_ms)}` : ""}. Nothing was merged, pushed or deployed by closing it.`}
          {summaryFailed ? (
            <div className="mt-2 flex items-center gap-2">
              <span className="text-red">✕ The summary could not be saved to project memory.</span>
              <Button onClick={() => void write("ticket.retry_summary", { ticket_id: t.id })}>Try again</Button>
            </div>
          ) : null}
        </Box>
      );
    default:
      return null;
  }
}

function Discussion({ t, conversationId }: { t: TicketView; conversationId: string | null | undefined }) {
  const connection = useConnection();
  const messages = useMessages(conversationId);
  const [text, setText] = useState("");
  const { busy, held, error, send, sendAnyway } = useSend(() => setText(""));
  const post = () => {
    if (text.trim() === "" || busy) return;
    send(async (confirmSecret) => {
      await connection.write("ticket.post", { ticket_id: t.id, text: text.trim(), confirm_secret: confirmSecret });
      return "";
    });
  };
  const { features, hostId } = useLinkState();
  const platform = usePlatform();
  const client = useQueryClient();
  const employees = useEmployees();
  const [editing, setEditing] = useState<{ id: string; text: string } | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [closedOn, setClosedOn] = useState<string | null>(null);
  const area = useRef<HTMLInputElement>(null);
  const people = (employees.data ?? [])
    .filter((e) => e.status === "active")
    .map((e) => ({ name: e.name, kind: "employee", employee: true }));
  const suggestions = closedOn === text ? null : mentionSuggestions(text, people);
  const change = (run: () => Promise<unknown>) =>
    run()
      .then(() => client.invalidateQueries({ queryKey: ["messages", conversationId] }))
      .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
  const list = (messages.data?.messages ?? []).filter((m) => !m.deleted_at_ms && !m.thread_parent);
  return (
    <>
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-[22px] py-4">
        {messages.isPending && conversationId ? <span className="text-text3">Loading the discussion…</span> : null}
        {list.length === 0 && !messages.isPending ? <span className="text-text3">Nothing said yet.</span> : null}
        {list.map((m) => (
          <div key={m.id} className="flex flex-col gap-1">
            <div className="flex items-baseline gap-2">
              <span className="font-medium">
                {m.author_user?.username ?? m.author ?? (m.role === "user" ? "You" : "Aurelius")}
              </span>
              <span className="text-[12px] text-text3">{time(m.created_at_ms)}</span>
              {m.system ? null : (
                <Menu
                  label="Message actions"
                  options={[
                    { key: "copy", label: "Copy" },
                    ...(hostId && features.includes("message_locate") ? [{ key: "link", label: "Copy link" }] : []),
                    ...(canChange(m, features.includes("message_edits"))
                      ? [
                          { key: "edit", label: "Edit" },
                          { key: "delete", label: "Delete", danger: true },
                        ]
                      : []),
                  ]}
                  onPick={(key) => {
                    if (key === "copy") void platform.copy(m.text);
                    if (key === "link")
                      void platform.copy(messageLink(hostId ?? "", m.id)).then(() => toast("Link copied."));
                    if (key === "edit") setEditing({ id: m.id, text: m.text });
                    if (key === "delete") setDeleting(m.id);
                  }}
                  trigger={
                    <button
                      type="button"
                      aria-label="Message actions"
                      className="ml-auto cursor-pointer border-0 bg-transparent p-0 text-text3 hover:text-text"
                    >
                      ⋯
                    </button>
                  }
                />
              )}
            </div>
            {editing?.id === m.id ? (
              <div className="flex flex-col gap-2">
                <textarea
                  aria-label="Edit message"
                  rows={3}
                  value={editing.text}
                  onChange={(e) => setEditing({ id: m.id, text: e.target.value })}
                  className="resize-y rounded-[6px] border border-btn-line bg-sunk p-2 text-[13px] text-text"
                />
                <div className="flex gap-2">
                  <Button
                    kind="primary"
                    disabled={editing.text.trim() === ""}
                    onClick={() => {
                      const { text: next } = editing;
                      setEditing(null);
                      void change(() =>
                        connection.write("message.edit", { message_id: m.id, text: next }, { retry: false }),
                      );
                    }}
                  >
                    Save
                  </Button>
                  <Button onClick={() => setEditing(null)}>Cancel</Button>
                </div>
              </div>
            ) : m.system ? (
              <span className="text-text2">{m.text}</span>
            ) : (
              <Markdown text={m.text} />
            )}
          </div>
        ))}
      </div>
      {t.column === "done" ? null : (
        <div className="flex flex-col gap-2 border-t border-line2 px-4 pt-3 pb-[14px]">
          {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
          {error ? (
            <p role="alert" className="m-0 text-red">
              {error}
            </p>
          ) : null}
          {suggestions && suggestions.found.length > 0 ? (
            <div
              role="listbox"
              aria-label="Mention"
              className="flex flex-col rounded-[8px] border border-line bg-panel py-1"
            >
              {suggestions.found.map((p) => (
                <button
                  key={p.name}
                  type="button"
                  role="option"
                  aria-selected={false}
                  onClick={() => {
                    setText(completeMention(text, suggestions.at, p.name));
                    setClosedOn(null);
                    area.current?.focus();
                  }}
                  className="cursor-pointer border-0 bg-transparent px-3 py-1 text-left text-text hover:bg-sel"
                >
                  {p.name}
                </button>
              ))}
            </div>
          ) : null}
          <div className="flex items-center gap-2 rounded-[8px] border border-btn-line bg-sunk py-[6px] pr-[6px] pl-3">
            <input
              ref={area}
              aria-label="Reply in discussion"
              placeholder="Reply in discussion · @ to mention"
              className="min-w-0 flex-1 border-0 bg-transparent text-text outline-none placeholder:text-text3"
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape" && suggestions) {
                  e.stopPropagation();
                  setClosedOn(text);
                } else if (e.key === "Enter") {
                  e.preventDefault();
                  const first = suggestions?.found[0];
                  if (suggestions && first) setText(completeMention(text, suggestions.at, first.name));
                  else post();
                }
              }}
            />
            <Button disabled={busy || text.trim() === ""} onClick={post}>
              Send
            </Button>
          </div>
        </div>
      )}
      {deleting ? (
        <ConfirmDialog
          open
          danger
          title="Delete this message?"
          body="It reads as deleted for everyone; earlier versions stay with the board."
          confirm="Delete"
          onClose={() => setDeleting(null)}
          onConfirm={() => {
            const id = deleting;
            setDeleting(null);
            void change(() => connection.write("message.delete", { message_id: id }, { retry: false }));
          }}
        />
      ) : null}
    </>
  );
}

function Activity({ id }: { id: string }) {
  const activity = useTicketActivity(id);
  if (activity.isPending) return <span className="text-text3">Loading the activity…</span>;
  if (activity.error) return <span className="text-red">{activity.error.message}</span>;
  if ((activity.data ?? []).length === 0) return <span className="text-text3">Nothing happened yet.</span>;
  return (
    <ul className="m-0 flex list-none flex-col gap-3 p-0">
      {activity.data?.map((card) => {
        const detail = activityDetail(card);
        return (
          <li key={card.seq} className="flex flex-col gap-[2px]">
            <span>{activityText(card)}</span>
            {detail ? <span className="text-[12px] text-text2">{detail}</span> : null}
            <span className="text-[12px] text-text3">{time(card.at_ms)}</span>
          </li>
        );
      })}
    </ul>
  );
}

type Tab = "ticket" | "discussion" | "activity";

export function TicketPanel({ id, onClose }: { id: string; onClose: () => void }) {
  const platform = usePlatform();
  const { hostId } = useLinkState();
  useKeyContext("TicketPanel");
  useAction("Back", onClose);
  const navigate = useNavigate();
  const { features, status } = useLinkState();
  const detail = useTicket(id);
  const board = useBoard(null);
  const employees = useEmployees();
  const models = useModels();
  const write = useTicketWrite();
  const members = useMembers();
  const [tab, setTab] = useState<Tab>("ticket");
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
      <aside
        aria-label="Ticket"
        className="flex w-[clamp(360px,38%,560px)] flex-none flex-col gap-3 border-l border-line p-5"
      >
        <span className="text-text2">
          {gone ? "This ticket is not on this server." : (detail.error?.message ?? "Loading the ticket…")}
        </span>
        <div>
          <Button onClick={onClose}>Close</Button>
        </div>
      </aside>
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
  const count = (discussion.data?.messages ?? []).filter((m) => !m.deleted_at_ms && !m.thread_parent).length;
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
    <aside
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
      aria-label={`${ticketNumber(t)} ${t.title}`}
      className="enter flex w-[clamp(360px,38%,560px)] min-w-0 flex-none flex-col border-l border-line bg-panel"
    >
      <div className="flex h-11 flex-none items-center gap-2 border-b border-line2 pr-[10px] pl-5 text-text3">
        <span className="font-mono text-[12px]">{ticketNumber(t)}</span>
        <div className="ml-auto flex gap-[2px]">
          <Menu
            label="More"
            options={more}
            onPick={pick}
            trigger={
              <button
                type="button"
                className="grid size-7 cursor-pointer place-items-center rounded-[6px] border-0 bg-transparent text-text2 hover:bg-sel hover:text-text"
              >
                ⋯
              </button>
            }
          />
          <button
            type="button"
            aria-label="Close"
            onClick={onClose}
            className="grid size-7 cursor-pointer place-items-center rounded-[6px] border-0 bg-transparent text-text2 hover:bg-sel hover:text-text"
          >
            ✕
          </button>
        </div>
      </div>
      <div className="flex flex-none flex-col gap-3 px-[22px] pt-5">
        <Editable
          label="Title"
          value={t.title}
          enabled={open}
          display={<h2 className="m-0 text-[20px] font-semibold tracking-[-0.015em]">{t.title}</h2>}
          onSave={(title) => update({ title: title.trim() })}
        />
        <div className="flex flex-wrap gap-[6px]">
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
                <Chip name="Model">{models.find((m) => m.key === (t.run.model ?? "default"))?.label ?? "Default"}</Chip>
              }
            />
          ) : null}
        </div>
      </div>
      <div className="flex flex-none flex-col gap-3 px-[22px] pt-3">
        {t.column === "review" ? (
          <ReviewGate
            key={t.version}
            t={t}
            report={detail.data?.root_task?.result ?? null}
            ready={check.kind === "ready" && can}
            whyNot={check.kind === "working" ? "Still at work: Done waits until that has finished." : null}
          />
        ) : t.column === "plan_review" ? (
          <PlanGate t={t} plan={rootMessages.isPending ? undefined : latestPlan(rootMessages.data?.messages ?? [])} />
        ) : (
          <StatusBox t={t} choosing={choosing} dispatcherLine={dispatcherLine} />
        )}
        <div role="tablist" aria-label="Ticket sections" className="flex gap-[18px] border-b border-line2">
          {(
            [
              ["ticket", "Ticket"],
              ...(features.includes("ticket_discussion")
                ? ([["discussion", count > 0 ? `Discussion ${count}` : "Discussion"]] as [Tab, string][])
                : []),
              ["activity", "Activity"],
            ] as [Tab, string][]
          ).map(([key, label]) => (
            <button
              key={key}
              type="button"
              role="tab"
              aria-selected={tab === key}
              onClick={() => setTab(key)}
              className={`h-8 cursor-pointer border-0 border-b-2 border-solid bg-transparent p-0 text-[13px] font-medium ${tab === key ? "border-text text-text" : "border-transparent text-text3 hover:text-text2"}`}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
      {tab === "discussion" ? (
        <Discussion t={t} conversationId={t.conversation_id} />
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-[18px] overflow-y-auto px-[22px] py-4">
          {tab === "activity" ? (
            <Activity id={t.id} />
          ) : (
            <>
              {t.review_feedback && t.column === "doing" ? (
                <Box>
                  <span className="text-[12px] font-medium text-text3">{feedbackLabel(activity.data)}</span>
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
                    <span className="text-text3">No description.</span>
                  )
                }
                onSave={(description) => update({ description })}
              />
              <div className="flex flex-col">
                <Row name="Project">{t.project_name}</Row>
                <Row name="Labels">
                  <Editable
                    label="Labels"
                    value={t.labels.join(", ")}
                    enabled={open}
                    placeholder="backend, ui"
                    display={t.labels.length > 0 ? t.labels.join(", ") : <span className="text-text3">None</span>}
                    onSave={(text) => update({ labels: parseLabels(text) })}
                  />
                </Row>
                <Row name="Deadline">
                  <Editable
                    label="Deadline"
                    value={t.deadline_ms ? deadlineInput(t.deadline_ms, -new Date().getTimezoneOffset()) : ""}
                    enabled={open}
                    placeholder="2026-11-04 or 2026-11-04 17:00"
                    display={t.deadline_ms ? time(t.deadline_ms) : <span className="text-text3">None</span>}
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
                {features.includes("ticket_responsible") ? (
                  <Row name="Responsible">{t.responsible_username ?? <span className="text-text3">No one</span>}</Row>
                ) : null}
                <Row name="Runs on">
                  {t.login_user === need.me ? "Your Claude login" : `${t.login_username}'s Claude login`}
                </Row>
                <Row name="Created by">{`${t.created_by_username} · ${time(t.created_at_ms)}`}</Row>
                {features.includes("ticket_follow") ? <Followers id={t.id} /> : null}
                <Row name="Files">
                  <span className="flex flex-col items-start gap-[6px]">
                    <FileChips files={t.attachments} />
                    {t.column === "done" ? null : (
                      <Button onClick={() => platform.pickFiles().then(addFiles, addFailed)}>Add files…</Button>
                    )}
                  </span>
                </Row>
              </div>
              {tasks.length > 0 ? (
                <div className="flex flex-col gap-1">
                  <span className="text-[12px] font-medium text-text3">Tasks ({detail.data?.tasks.length})</span>
                  {tasks.map(({ task, depth }) => {
                    const state = taskStateLine(task);
                    return (
                      <button
                        key={task.id}
                        type="button"
                        onClick={() => void navigate({ to: "/tasks/$taskId", params: { taskId: task.id } })}
                        style={{ paddingLeft: 4 + 16 * depth }}
                        className="flex h-8 cursor-pointer items-center gap-2 rounded-[6px] border-0 bg-transparent pr-1 text-left text-text hover:bg-sel"
                      >
                        <span className="min-w-0 flex-1 truncate">{task.title}</span>
                        <span className="flex-none text-[12px]" style={{ color: state.color }}>
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
    </aside>
  );
}
