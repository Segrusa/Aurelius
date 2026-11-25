// A side question about a session or ticket (feature `side_question`), like Claude Code's /btw:
// answered from the conversation without tools, while the work goes on. Nothing is stored and the
// employee never sees it; closing the popover stops what is still being answered and forgets it.

import { Button } from "@astryxdesign/core/Button";
import { IconButton } from "@astryxdesign/core/IconButton";
import { Popover } from "@astryxdesign/core/Popover";
import { TextArea } from "@astryxdesign/core/TextArea";
import { expectBody } from "@aurelius/client";
import type { SideAnswer } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { Icon } from "../ui/icons";
import { Markdown } from "../ui/markdown";
import {
  answered,
  asked,
  canAsk,
  refused,
  type SideAsk,
  started,
  stillAnswering,
  unclaimed,
} from "./side-question-logic";

const styles = stylex.create({
  // The popover opens over the docked properties card; a border and a high shadow keep it apart.
  surface: {
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    boxShadow: "var(--shadow-high)",
  },
  panel: { display: "flex", flexDirection: "column", gap: 12, maxHeight: "min(70vh, 560px)" },
  head: { display: "flex", flexDirection: "column", gap: 2 },
  title: { margin: 0, fontSize: 13, fontWeight: 600, color: "var(--color-text-primary)" },
  note: { margin: 0, fontSize: 12, lineHeight: 1.4, color: "var(--precision-text3)" },
  list: { display: "flex", flexDirection: "column", gap: 14, overflowY: "auto", minHeight: 0 },
  ask: { display: "flex", flexDirection: "column", gap: 6 },
  question: {
    margin: 0,
    alignSelf: "flex-end",
    maxWidth: "90%",
    paddingBlock: 6,
    paddingInline: 10,
    borderRadius: 10,
    fontSize: 13,
    lineHeight: 1.45,
    whiteSpace: "pre-wrap",
    overflowWrap: "anywhere",
    backgroundColor: "var(--precision-sunk)",
    color: "var(--color-text-primary)",
  },
  answer: { fontSize: 13, minWidth: 0 },
  waiting: { margin: 0, fontSize: 12, color: "var(--precision-text3)" },
  error: { display: "flex", alignItems: "center", gap: 8, fontSize: 12, color: "var(--color-text-error)" },
  form: { display: "flex", flexDirection: "column", gap: 6 },
  bar: { display: "flex", alignItems: "center", justifyContent: "space-between", gap: 8 },
  hint: { fontSize: 11, color: "var(--precision-text3)" },
});

/** The head button of a session or ticket that asks a side question about its conversation. */
export function SideQuestionButton({ conversationId }: { conversationId: string }) {
  const { features } = useLinkState();
  const [open, setOpen] = useState(false);
  if (!features.includes("side_question")) return null;
  return (
    <Popover
      label="Side question"
      isOpen={open}
      onOpenChange={setOpen}
      width={420}
      placement="below"
      alignment="end"
      xstyle={styles.surface}
      content={open ? <SidePanel conversationId={conversationId} /> : null}
    >
      <IconButton variant="ghost" size="sm" label="Ask a side question" icon={<Icon name="aside" size={16} />} />
    </Popover>
  );
}

function SidePanel({ conversationId }: { conversationId: string }) {
  const connection = useConnection();
  const [list, setList] = useState<SideAsk[]>([]);
  const [draft, setDraft] = useState("");
  const nextKey = useRef(1);
  // Answers whose id came before `conversation.side_ask` answered.
  const early = useRef(new Map<string, SideAnswer>());
  const open = useRef<string[]>([]);
  open.current = stillAnswering(list);
  // Closed: an id that only arrives now is stopped at once.
  const closed = useRef(false);
  const stop = (sideId: string) => {
    void connection.call("conversation.side_cancel", { side_id: sideId }).catch(() => undefined);
  };

  useEffect(
    () =>
      connection.sideAnswers.on((answer) => {
        setList((current) => {
          if (unclaimed(current, answer)) {
            early.current.set(answer.side_id, answer);
            return current;
          }
          return answered(current, answer);
        });
      }),
    [connection],
  );

  // Closing the popover (or leaving the screen) stops what is still being answered.
  useEffect(
    () => () => {
      closed.current = true;
      for (const sideId of open.current) {
        void connection.call("conversation.side_cancel", { side_id: sideId }).catch(() => undefined);
      }
    },
    [connection],
  );

  const ask = (question: string) => {
    const key = nextKey.current++;
    setList((current) => asked(current, key, question));
    connection
      .call("conversation.side_ask", { conversation_id: conversationId, question })
      .then((body) => {
        const sideId = expectBody(body, "side_asked").value.side_id;
        if (closed.current) return stop(sideId);
        const first = early.current.get(sideId);
        early.current.delete(sideId);
        setList((current) => started(current, key, sideId, first));
      })
      .catch((e: unknown) => {
        setList((current) => refused(current, key, e instanceof Error ? e.message : "The side question failed."));
      });
  };

  const send = () => {
    if (!canAsk(list, draft)) return;
    ask(draft.trim());
    setDraft("");
  };

  return (
    <div {...stylex.props(styles.panel)}>
      <div {...stylex.props(styles.head)}>
        <p {...stylex.props(styles.title)}>Side question</p>
        <p {...stylex.props(styles.note)}>
          Answered from this conversation, without tools. It isn't added to the conversation and the work isn't
          interrupted.
        </p>
      </div>
      {list.length > 0 ? (
        <div {...stylex.props(styles.list)} aria-live="polite">
          {list.map((a) => (
            <Asked key={a.key} ask={a} onRetry={() => ask(a.question)} />
          ))}
        </div>
      ) : null}
      <div {...stylex.props(styles.form)}>
        <TextArea
          hasAutoFocus
          label="Side question"
          isLabelHidden
          placeholder="Ask about this conversation…"
          rows={2}
          value={draft}
          onChange={setDraft}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault();
              send();
            }
          }}
        />
        <div {...stylex.props(styles.bar)}>
          <span {...stylex.props(styles.hint)}>Enter to ask · Shift+Enter for a new line</span>
          <Button size="sm" variant="primary" label="Ask" isDisabled={!canAsk(list, draft)} onClick={send} />
        </div>
      </div>
    </div>
  );
}

function Asked({ ask, onRetry }: { ask: SideAsk; onRetry: () => void }) {
  const busy = ask.status === "asking" || ask.status === "answering";
  return (
    <div {...stylex.props(styles.ask)}>
      <p {...stylex.props(styles.question)}>{ask.question}</p>
      {ask.text ? (
        <div {...stylex.props(styles.answer)}>
          <Markdown text={ask.text} safe streaming={ask.status === "answering"} />
        </div>
      ) : busy ? (
        <p {...stylex.props(styles.waiting)} role="status">
          Thinking…
        </p>
      ) : null}
      {ask.status === "failed" ? (
        <div {...stylex.props(styles.error)} role="alert">
          <span>{ask.error ?? "The side question failed."}</span>
          <Button size="sm" variant="ghost" label="Try again" onClick={onRetry} />
        </div>
      ) : null}
    </div>
  );
}
