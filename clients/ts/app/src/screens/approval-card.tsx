// An approval where it is asked: in a run, in a channel, in the Inbox. A question gets its form, a plan
// its decision, the rest Allow or Deny.

import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { ButtonGroup } from "@astryxdesign/core/ButtonGroup";
import { Card } from "@astryxdesign/core/Card";
import { CheckboxInput } from "@astryxdesign/core/CheckboxInput";
import { Icon } from "@astryxdesign/core/Icon";
import { IconButton } from "@astryxdesign/core/IconButton";
import { RadioList, RadioListItem } from "@astryxdesign/core/RadioList";
import { TextInput } from "@astryxdesign/core/TextInput";
import type { ApprovalView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { useConnection } from "../connection";
import { useAction, useKeyContext } from "../keys/provider";
import { KeyHint } from "../ui/key-hint";
import { Markdown } from "../ui/markdown";
import { toast } from "../ui/toast";
import { approvalAction, approvalSentence } from "./inbox-logic";
import { composePlanFeedback, type PlanComment } from "./run-logic";

const styles = stylex.create({
  shell: { overflow: "hidden", borderColor: "var(--color-border-warning)" },
  body: { display: "flex", flexDirection: "column", gap: 12, padding: 12 },
  question: {
    display: "flex",
    flexDirection: "column",
    gap: 4,
    margin: 0,
    borderWidth: 0,
    padding: 0,
  },
  current: { borderRadius: 6, backgroundColor: "var(--color-overlay-hover)" },
  legend: { marginBottom: 4, padding: 0, fontSize: 14, lineHeight: 1.45, fontWeight: 500 },
  header: {
    marginInlineEnd: 8,
    borderRadius: 4,
    backgroundColor: "var(--color-overlay-hover)",
    paddingInline: 6,
    paddingBlock: 1,
    fontSize: 12,
    color: "var(--color-text-secondary)",
  },
  several: { marginInlineStart: 8, fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
  problem: { fontSize: 12, color: "var(--color-text-error)" },
  footer: {
    display: "flex",
    flexWrap: "wrap",
    gap: 8,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    paddingInline: 12,
    paddingBlock: 10,
  },
  muted: { color: "var(--color-text-secondary)" },
  selected: { display: "flex", alignItems: "center", gap: 8, fontSize: 12, color: "var(--precision-text3)" },
  selectedText: {
    minWidth: 0,
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: "0%",
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
  },
  commentBox: {
    display: "flex",
    flexDirection: "column",
    gap: 8,
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    padding: 8,
  },
  quote: {
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  quoteBlock: { display: "block" },
  commentActions: { display: "flex", justifyContent: "flex-end", gap: 8 },
  comment: {
    display: "flex",
    alignItems: "flex-start",
    gap: 8,
    borderRadius: 6,
    backgroundColor: "var(--precision-sunk)",
    padding: 8,
    fontSize: 12,
  },
  commentText: { minWidth: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%" },
  // The command must stay selectable, also where a parent turned selection off, and stays one block.
  command: {
    margin: 0,
    maxHeight: 240,
    overflow: "auto",
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    backgroundColor: "var(--precision-sunk)",
    padding: 12,
    fontFamily: "var(--font-family-code)",
    fontSize: 13,
    whiteSpace: "pre-wrap",
    userSelect: "text",
  },
});

export const QUESTION_TOOL = "claude.AskUserQuestion";
export const PLAN_TOOL = "claude.ExitPlanMode";

export interface Question {
  text: string;
  header: string;
  multi: boolean;
  options: { label: string; description: string }[];
}

export function questionsOf(a: ApprovalView): Question[] {
  const args = a.arguments as { questions?: Record<string, unknown>[] } | null;
  return (args?.questions ?? [])
    .map((q) => ({
      text: String(q.question ?? ""),
      header: String(q.header ?? ""),
      multi: q.multiSelect === true,
      options: ((q.options as Record<string, unknown>[] | undefined) ?? []).map((o) => ({
        label: String(o.label ?? ""),
        description: String(o.description ?? ""),
      })),
    }))
    .filter((q) => q.text !== "");
}

/** The answers by question text, or null while one is missing. */
export function answersOf(
  questions: Question[],
  chosen: Record<number, number[]>,
  own: Record<number, string>,
): Record<string, string> | null {
  const answers: Record<string, string> = {};
  for (const [i, q] of questions.entries()) {
    const answer = own[i] ?? (chosen[i] ?? []).flatMap((n) => q.options[n]?.label ?? []).join(", ");
    if (answer.trim() === "") return null;
    answers[q.text] = answer;
  }
  return answers;
}

function useApprovalWrite() {
  const connection = useConnection();
  const client = useQueryClient();
  const [busy, setBusy] = useState(false);
  // Never resent after a reconnect: a decision is only made while the person sees it.
  const run = async (write: (c: typeof connection) => Promise<unknown>, done?: string) => {
    if (busy) return;
    setBusy(true);
    try {
      await write(connection);
      await client.invalidateQueries({ queryKey: ["approvals"] });
      if (done) toast(done);
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
      setBusy(false);
    }
  };
  return { busy, run };
}

function Shell({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <Card padding={0} xstyle={styles.shell}>
      <Banner status="warning" container="section" title={title} />
      {children}
    </Card>
  );
}

/** The keys of a decision in the Inbox. */
function DecisionKeys({
  onOption,
  onNext,
  onOwn,
  onSkip,
  onSend,
  several,
}: {
  onOption: (n: number) => void;
  onNext: () => void;
  onOwn: () => void;
  onSkip: () => void;
  onSend: () => void;
  several: boolean;
}) {
  useKeyContext("Decision");
  useAction("QuestionOption1", () => onOption(0));
  useAction("QuestionOption2", () => onOption(1));
  useAction("QuestionOption3", () => onOption(2));
  useAction("QuestionOption4", () => onOption(3));
  useAction("QuestionNext", onNext, several);
  useAction("QuestionOwnAnswer", onOwn);
  useAction("RejectApproval", onSkip);
  useAction("AllowOnce", onSend);
  return null;
}

function QuestionCard({ approval, keyed }: { approval: ApprovalView; keyed: boolean }) {
  const { busy, run } = useApprovalWrite();
  const questions = questionsOf(approval);
  const [chosen, setChosen] = useState<Record<number, number[]>>({});
  const [own, setOwn] = useState<Record<number, string>>({});
  const [missing, setMissing] = useState(false);
  const [at, setAt] = useState(0);
  const card = useRef<HTMLDivElement>(null);
  const skip = () =>
    void run((c) =>
      c.write(
        "approval.decide",
        { approval_id: approval.id, action_digest: approval.action_digest, approve: false, always_allow: false },
        { retry: false },
      ),
    );
  const pick = (i: number, n: number, multi: boolean) => {
    const now = chosen[i] ?? [];
    setChosen({ ...chosen, [i]: multi ? (now.includes(n) ? now.filter((x) => x !== n) : [...now, n]) : [n] });
    const { [i]: _, ...rest } = own;
    setOwn(rest);
    setMissing(false);
  };
  const submit = () => {
    const answers = answersOf(questions, chosen, own);
    if (!answers) return setMissing(true);
    void run((c) => c.write("approval.answer", { approval_id: approval.id, answers }, { retry: false }));
  };
  return (
    <Shell title={`Question · ${approval.requester} waits for your answer`}>
      {keyed ? (
        <DecisionKeys
          several={questions.length > 1}
          onOption={(n) => {
            const q = questions[at];
            if (q?.options[n]) pick(at, n, q.multi);
          }}
          onNext={() => setAt((at + 1) % questions.length)}
          onOwn={() => card.current?.querySelectorAll<HTMLInputElement>("[data-own]")[at]?.focus()}
          onSkip={skip}
          onSend={submit}
        />
      ) : null}
      <div ref={card} {...stylex.props(styles.body)}>
        {questions.map((q, i) => (
          <fieldset
            key={q.text}
            {...stylex.props(styles.question, keyed && questions.length > 1 && i === at && styles.current)}
          >
            <legend {...stylex.props(styles.legend)}>
              {q.header ? <span {...stylex.props(styles.header)}>{q.header}</span> : null}
              {q.text}
              {q.multi ? <span {...stylex.props(styles.several)}>choose one or more</span> : null}
            </legend>
            {q.multi ? (
              q.options.map((o, n) => (
                <CheckboxInput
                  key={o.label}
                  label={o.label}
                  {...(o.description ? { description: o.description } : {})}
                  value={own[i] === undefined && (chosen[i] ?? []).includes(n)}
                  onChange={() => pick(i, n, true)}
                />
              ))
            ) : (
              <RadioList
                label={q.text}
                isLabelHidden
                value={own[i] === undefined && chosen[i]?.[0] !== undefined ? String(chosen[i]?.[0]) : ""}
                onChange={(next) => pick(i, Number(next), false)}
              >
                {q.options.map((o, n) => (
                  <RadioListItem
                    key={o.label}
                    value={String(n)}
                    label={o.label}
                    {...(o.description ? { description: o.description } : {})}
                  />
                ))}
              </RadioList>
            )}
            <TextInput
              data-own
              label={`Your own answer: ${q.text}`}
              isLabelHidden
              placeholder="Or in your own words…"
              value={own[i] ?? ""}
              onChange={(value) => {
                const { [i]: _, ...rest } = own;
                setOwn(value === "" ? rest : { ...own, [i]: value });
                setMissing(false);
              }}
            />
          </fieldset>
        ))}
        {missing ? <span {...stylex.props(styles.problem)}>Answer every question first, or skip.</span> : null}
      </div>
      <div {...stylex.props(styles.footer)}>
        <ButtonGroup label="Answer">
          <Button variant="primary" label="Send answer" isDisabled={busy} onClick={submit} />
          <Button label="Skip" isDisabled={busy} onClick={skip} />
        </ButtonGroup>
      </div>
    </Shell>
  );
}

function RejectKey({ onReject }: { onReject: () => void }) {
  useKeyContext("Decision");
  useAction("RejectApproval", onReject);
  return null;
}

function PlanCard({ approval, keyed }: { approval: ApprovalView; keyed: boolean }) {
  const { busy, run } = useApprovalWrite();
  const [feedback, setFeedback] = useState("");
  const [missing, setMissing] = useState(false);
  const [selected, setSelected] = useState("");
  const [quote, setQuote] = useState<string | null>(null);
  const [comment, setComment] = useState("");
  const [comments, setComments] = useState<PlanComment[]>([]);
  const plan = (approval.arguments as { plan?: string } | null)?.plan ?? "";
  const addComment = () => {
    if (quote === null || comment.trim() === "") return;
    setComments([...comments, { quote, text: comment.trim() }]);
    setQuote(null);
    setComment("");
    setSelected("");
    setMissing(false);
  };
  const decide = (decision: "approve" | "approve_edits" | "revise" | "reject", done: string) => {
    const text = composePlanFeedback(comments, feedback);
    if (decision === "revise" && text === "") return setMissing(true);
    void run(
      (c) =>
        c.write(
          "approval.decide_plan",
          { approval_id: approval.id, decision, feedback: text || null },
          { retry: false },
        ),
      done,
    );
  };
  return (
    <Shell title="Plan · waiting for you">
      {keyed ? <RejectKey onReject={() => decide("reject", "Plan rejected. The agent stops.")} /> : null}
      <div {...stylex.props(styles.body)}>
        {/* biome-ignore lint/a11y/noStaticElementInteractions: a selection, not a control. */}
        <div onMouseUp={() => setSelected(window.getSelection()?.toString().trim() ?? "")}>
          {plan ? (
            <Markdown text={plan} />
          ) : (
            <span {...stylex.props(styles.muted)}>The plan is in the conversation.</span>
          )}
        </div>
        {selected && quote === null ? (
          <div {...stylex.props(styles.selected)}>
            <span {...stylex.props(styles.selectedText)}>Selected: “{selected}”</span>
            <Button label="Comment" onClick={() => setQuote(selected)} />
          </div>
        ) : null}
        {quote !== null ? (
          <div {...stylex.props(styles.commentBox)}>
            <span {...stylex.props(styles.quote)}>Comment on: “{quote}”</span>
            <TextInput
              hasAutoFocus
              label="Your comment"
              isLabelHidden
              value={comment}
              onChange={setComment}
              onEnter={addComment}
            />
            <span {...stylex.props(styles.commentActions)}>
              <Button label="Cancel" onClick={() => setQuote(null)} />
              <Button variant="primary" label="Add comment" isDisabled={comment.trim() === ""} onClick={addComment} />
            </span>
          </div>
        ) : null}
        {comments.map((c, i) => (
          <div key={`${c.quote}|${c.text}`} {...stylex.props(styles.comment)}>
            <span {...stylex.props(styles.commentText)}>
              <span {...stylex.props(styles.quote, styles.quoteBlock)}>“{c.quote}”</span>
              {c.text}
            </span>
            <IconButton
              variant="ghost"
              size="sm"
              label="Remove comment"
              icon={<Icon icon="close" />}
              onClick={() => setComments(comments.filter((_, at) => at !== i))}
            />
          </div>
        ))}
        <TextInput
          label="What should change?"
          isLabelHidden
          placeholder="What should change?"
          value={feedback}
          onChange={(value) => {
            setFeedback(value);
            setMissing(false);
          }}
        />
        {missing ? <span {...stylex.props(styles.problem)}>Say what should change, then ask for changes.</span> : null}
      </div>
      <div {...stylex.props(styles.footer)}>
        <ButtonGroup label="Decide on the plan">
          <Button
            variant="primary"
            label="Approve + Auto"
            isDisabled={busy}
            onClick={() => decide("approve", "Plan approved. The agent goes on in auto mode.")}
          />
          <Button
            label="Approve, edits"
            isDisabled={busy}
            onClick={() => decide("approve_edits", "Plan approved. Edits go through; the agent asks for the rest.")}
          />
          <Button
            label="Request changes"
            isDisabled={busy}
            onClick={() => decide("revise", "Changes requested. The agent revises the plan.")}
          />
          <Button
            variant="destructive"
            label="Reject"
            isDisabled={busy}
            onClick={() => decide("reject", "Plan rejected. The agent stops.")}
          />
        </ButtonGroup>
      </div>
    </Shell>
  );
}

function ActionKeys({
  eligible,
  onAllow,
  onAlways,
  onDeny,
}: {
  eligible: boolean;
  onAllow: () => void;
  onAlways: () => void;
  onDeny: () => void;
}) {
  useKeyContext("Decision");
  useAction("AllowOnce", onAllow);
  useAction("AlwaysAllow", onAlways, eligible);
  useAction("DenyApproval", onDeny);
  useAction("RejectApproval", onDeny);
  return null;
}

function ActionCard({ approval, keyed }: { approval: ApprovalView; keyed: boolean }) {
  const { busy, run } = useApprovalWrite();
  const action = approvalAction(approval);
  const eligible = approval.always_allow_eligible ?? false;
  const decide = (approve: boolean, alwaysAllow = false) =>
    run((c) =>
      c.write(
        "approval.decide",
        { approval_id: approval.id, action_digest: approval.action_digest, approve, always_allow: alwaysAllow },
        { retry: false },
      ),
    );
  return (
    <Shell title={`Approval needed · ${approvalSentence(approval)}`}>
      {keyed ? (
        <ActionKeys
          eligible={eligible}
          onAllow={() => void decide(true)}
          onAlways={() => void decide(true, true)}
          onDeny={() => void decide(false)}
        />
      ) : null}
      <pre {...stylex.props(styles.command)}>{action.body}</pre>
      <div {...stylex.props(styles.footer)}>
        <ButtonGroup label="Decide">
          <Button variant="primary" label="Allow once" isDisabled={busy} onClick={() => void decide(true)}>
            Allow once{keyed ? <KeyHint onAccent>↵</KeyHint> : null}
          </Button>
          {eligible ? (
            <Button label="Always allow" isDisabled={busy} onClick={() => void decide(true, true)}>
              Always allow{keyed ? <KeyHint>⇧↵</KeyHint> : null}
            </Button>
          ) : null}
          <Button variant="destructive" label="Deny" isDisabled={busy} onClick={() => void decide(false)}>
            Deny{keyed ? <KeyHint>D</KeyHint> : null}
          </Button>
        </ButtonGroup>
      </div>
    </Shell>
  );
}

/** An employee's post that waits for the board member, without a question form: answered with one line. */
export function ReplyCard({ who, onSend }: { who: string; onSend: (text: string) => Promise<unknown> }) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    const clean = text.trim();
    if (clean === "" || busy) return;
    setBusy(true);
    try {
      await onSend(clean);
      setText("");
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <Shell title={`${who} is waiting for you`}>
      <div {...stylex.props(styles.body)}>
        <TextInput
          label={`Your answer to ${who}`}
          isLabelHidden
          placeholder="Your answer…"
          value={text}
          onChange={setText}
          onEnter={() => void submit()}
        />
      </div>
      <div {...stylex.props(styles.footer)}>
        <Button
          variant="primary"
          label="Send answer"
          isDisabled={busy || text.trim() === ""}
          onClick={() => void submit()}
        />
      </div>
    </Shell>
  );
}

export function ApprovalCard({ approval, keyed = false }: { approval: ApprovalView; keyed?: boolean }) {
  if (approval.tool === QUESTION_TOOL) return <QuestionCard approval={approval} keyed={keyed} />;
  if (approval.tool === PLAN_TOOL) return <PlanCard approval={approval} keyed={keyed} />;
  return <ActionCard approval={approval} keyed={keyed} />;
}
