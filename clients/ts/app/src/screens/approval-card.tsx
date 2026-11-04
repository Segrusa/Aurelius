// An approval where it is asked: in a run, in a channel, in the Inbox. A question gets its form, a plan
// its decision, the rest Allow or Deny.

import type { ApprovalView } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useConnection } from "../connection";
import { Button } from "../ui/kit";
import { Markdown } from "../ui/markdown";
import { toast } from "../ui/toast";
import { approvalAction, approvalSentence } from "./inbox-logic";
import { composePlanFeedback, type PlanComment } from "./run-logic";

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
    <div className="overflow-hidden rounded-[8px] border border-amber-line">
      <div className="flex items-center gap-2 bg-amber-soft px-3 py-[10px] font-medium">
        <span className="size-[7px] flex-none rounded-full bg-amber" />
        {title}
      </div>
      {children}
    </div>
  );
}

function QuestionCard({ approval }: { approval: ApprovalView }) {
  const { busy, run } = useApprovalWrite();
  const questions = questionsOf(approval);
  const [chosen, setChosen] = useState<Record<number, number[]>>({});
  const [own, setOwn] = useState<Record<number, string>>({});
  const [missing, setMissing] = useState(false);
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
      <div className="flex flex-col gap-3 p-3">
        {questions.map((q, i) => (
          <fieldset key={q.text} className="m-0 flex flex-col gap-1 border-0 p-0">
            <legend className="mb-1 p-0 text-[14px] leading-[1.45] font-medium">
              {q.header ? (
                <span className="mr-2 rounded-[4px] bg-sel px-[6px] py-[1px] text-[12px] text-text2">{q.header}</span>
              ) : null}
              {q.text}
              {q.multi ? <span className="ml-2 text-[12px] font-normal text-text3">choose one or more</span> : null}
            </legend>
            {q.options.map((o, n) => (
              <label
                key={o.label}
                className="flex cursor-pointer items-start gap-[10px] rounded-[6px] px-2 py-[6px] hover:bg-sel"
              >
                <input
                  type={q.multi ? "checkbox" : "radio"}
                  name={`${approval.id}-${i}`}
                  className="mt-[3px] accent-(--acc-text)"
                  checked={own[i] === undefined && (chosen[i] ?? []).includes(n)}
                  onChange={() => pick(i, n, q.multi)}
                />
                <span className="min-w-0">
                  {o.label}
                  {o.description ? <span className="block text-[12px] text-text2">{o.description}</span> : null}
                </span>
              </label>
            ))}
            <input
              aria-label={`Your own answer: ${q.text}`}
              placeholder="Or in your own words…"
              className="mt-1 h-8 rounded-[6px] border border-btn-line bg-sunk px-2 text-text outline-none placeholder:text-text3 focus:border-acc-text"
              value={own[i] ?? ""}
              onChange={(e) => {
                const { [i]: _, ...rest } = own;
                setOwn(e.target.value === "" ? rest : { ...own, [i]: e.target.value });
                setMissing(false);
              }}
            />
          </fieldset>
        ))}
        {missing ? <span className="text-[12px] text-red">Answer every question first, or skip.</span> : null}
      </div>
      <div className="flex gap-2 border-t border-line2 px-3 py-[10px]">
        <Button kind="primary" disabled={busy} onClick={submit}>
          Send answer
        </Button>
        <Button
          disabled={busy}
          onClick={() =>
            void run((c) =>
              c.write(
                "approval.decide",
                {
                  approval_id: approval.id,
                  action_digest: approval.action_digest,
                  approve: false,
                  always_allow: false,
                },
                { retry: false },
              ),
            )
          }
        >
          Skip
        </Button>
      </div>
    </Shell>
  );
}

function PlanCard({ approval }: { approval: ApprovalView }) {
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
      <div className="flex flex-col gap-3 p-3">
        {/* biome-ignore lint/a11y/noStaticElementInteractions: a selection, not a control. */}
        <div onMouseUp={() => setSelected(window.getSelection()?.toString().trim() ?? "")}>
          {plan ? <Markdown text={plan} /> : <span className="text-text2">The plan is in the conversation.</span>}
        </div>
        {selected && quote === null ? (
          <div className="flex items-center gap-2 text-[12px] text-text3">
            <span className="min-w-0 flex-1 truncate">Selected: “{selected}”</span>
            <Button onClick={() => setQuote(selected)}>Comment</Button>
          </div>
        ) : null}
        {quote !== null ? (
          <div className="flex flex-col gap-2 rounded-[6px] border border-line p-2">
            <span className="truncate text-[12px] text-text3">Comment on: “{quote}”</span>
            <input
              // biome-ignore lint/a11y/noAutofocus: it opens because of this input.
              autoFocus
              aria-label="Your comment"
              className="h-8 rounded-[6px] border border-btn-line bg-sunk px-2 text-text outline-none"
              value={comment}
              onChange={(e) => setComment(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  addComment();
                }
              }}
            />
            <span className="flex justify-end gap-2">
              <Button onClick={() => setQuote(null)}>Cancel</Button>
              <Button kind="primary" disabled={comment.trim() === ""} onClick={addComment}>
                Add comment
              </Button>
            </span>
          </div>
        ) : null}
        {comments.map((c, i) => (
          <div key={`${c.quote}|${c.text}`} className="flex items-start gap-2 rounded-[6px] bg-sunk p-2 text-[12px]">
            <span className="min-w-0 flex-1">
              <span className="block truncate text-text3">“{c.quote}”</span>
              {c.text}
            </span>
            <button
              type="button"
              aria-label="Remove comment"
              className="cursor-pointer border-0 bg-transparent p-0 text-text3 hover:text-text"
              onClick={() => setComments(comments.filter((_, at) => at !== i))}
            >
              ×
            </button>
          </div>
        ))}
        <input
          aria-label="What should change?"
          placeholder="What should change?"
          className="h-8 rounded-[6px] border border-btn-line bg-sunk px-2 text-text outline-none placeholder:text-text3 focus:border-acc-text"
          value={feedback}
          onChange={(e) => {
            setFeedback(e.target.value);
            setMissing(false);
          }}
        />
        {missing ? <span className="text-[12px] text-red">Say what should change, then ask for changes.</span> : null}
      </div>
      <div className="flex flex-wrap gap-2 border-t border-line2 px-3 py-[10px]">
        <Button
          kind="primary"
          disabled={busy}
          onClick={() => decide("approve", "Plan approved. The agent goes on in auto mode.")}
        >
          Goedkeuren + Auto
        </Button>
        <Button
          disabled={busy}
          onClick={() => decide("approve_edits", "Plan approved. Edits go through; the agent asks for the rest.")}
        >
          Goedkeuren, edits
        </Button>
        <Button disabled={busy} onClick={() => decide("revise", "Changes requested. The agent revises the plan.")}>
          Pas aan
        </Button>
        <Button kind="danger" disabled={busy} onClick={() => decide("reject", "Plan rejected. The agent stops.")}>
          Afwijzen
        </Button>
      </div>
    </Shell>
  );
}

function ActionCard({ approval }: { approval: ApprovalView }) {
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
      <pre className="m-0 max-h-[240px] overflow-auto border-t border-line2 bg-sunk p-3 font-mono text-[13px] whitespace-pre-wrap">
        {action.body}
      </pre>
      <div className="flex flex-wrap gap-2 border-t border-line2 px-3 py-[10px]">
        <Button kind="primary" disabled={busy} onClick={() => void decide(true)}>
          Allow once
        </Button>
        {eligible ? (
          <Button disabled={busy} onClick={() => void decide(true, true)}>
            Always allow
          </Button>
        ) : null}
        <Button kind="danger" disabled={busy} onClick={() => void decide(false)}>
          Deny
        </Button>
      </div>
    </Shell>
  );
}

export function ApprovalCard({ approval }: { approval: ApprovalView }) {
  if (approval.tool === QUESTION_TOOL) return <QuestionCard approval={approval} />;
  if (approval.tool === PLAN_TOOL) return <PlanCard approval={approval} />;
  return <ActionCard approval={approval} />;
}
