// A design while it is written, as Claude Desktop shows an artifact: the HTML file an answer
// writes gets a card under the answer from its first lines on (the server streams the draft of
// its `Write` call, feature `artifact_drafts`). The card opens the artifact viewer at the right of
// the conversation (artifact-dock), which grows along with the file. After the call, the design comes from the call itself and the edits the
// answer made on it, also when the answer does not keep the file.

import { expectBody } from "@aurelius/client";
import type { ActivityView, ArtifactDraft, FileRef, MessageView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useEffectEvent, useRef, useState } from "react";
import { useConnection } from "../connection";
import { useKeyed } from "../data";
import { usePlatform } from "../platform";
import type { ArtifactTarget } from "./artifact-dock";
import type { ArtifactSend } from "./artifact-feedback";
import { applyEdit, designsOf, designToOpen, isArtifact, type WrittenDesign, writtenContent } from "./artifact-logic";
import { ArtifactViewer } from "./artifact-viewer";

const styles = stylex.create({
  cards: { display: "flex", flexWrap: "wrap", gap: 8, paddingTop: 2 },
  card: {
    display: "flex",
    cursor: "pointer",
    alignItems: "center",
    gap: 10,
    minWidth: 240,
    borderRadius: 7,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: { default: "var(--color-border)", ":hover": "var(--color-border-emphasized)" },
    backgroundColor: "transparent",
    paddingInline: 10,
    paddingBlock: 8,
    fontSize: 12.5,
    color: "var(--color-text-primary)",
    textAlign: "start",
  },
  text: { display: "flex", flexDirection: "column", flexGrow: 1, gap: 2 },
  accent: { color: "var(--color-text-accent)" },
  meta: { fontSize: 11, color: "var(--precision-text3)" },
});

const sizeText = (chars: number) => (chars >= 1024 ? `${Math.round(chars / 1024)} KB` : `${chars} B`);

/** The design's HTML after its call: the whole `Write`, then each edit that fits, in order. */
function useWrittenHtml(messageId: string, design: WrittenDesign) {
  const connection = useConnection();
  const call = async (callId: string) =>
    expectBody(await connection.call("activity.get", { message_id: messageId, call_id: callId }), "activity").value
      .input;
  return useKeyed(
    ["design", messageId, design.callId, design.edits.join(" ")],
    async () => {
      const html = writtenContent(await call(design.callId));
      if (html === null) throw new Error("This design is no longer kept.");
      let out = html;
      for (const edit of design.edits) {
        const next = applyEdit(out, await call(edit));
        // An edit that does not fit: show the file as it was before it.
        if (next === null) break;
        out = next;
      }
      return out;
    },
    design.draft === null,
  );
}

/** The viewer of a written design: its draft while it streams, then the HTML of its calls. */
export function DesignViewer({
  design,
  messageId,
  messages,
  conversationId,
  send,
  docked = false,
  onClose,
}: {
  design: WrittenDesign;
  messageId: string;
  messages: readonly MessageView[];
  conversationId: string | null;
  send: ArtifactSend | null;
  docked?: boolean;
  onClose: () => void;
}) {
  const { artifacts } = usePlatform();
  const loaded = useWrittenHtml(messageId, design);
  // Between the last draft and the whole call, the last draft stays in sight.
  const last = useRef<string | null>(null);
  const html = design.draft ?? loaded.data ?? last.current;
  last.current = html;
  if (!artifacts) return null;
  return (
    <ArtifactViewer
      pages={[]}
      openName={design.name}
      artifacts={artifacts}
      messages={messages}
      conversationId={conversationId}
      send={send}
      docked={docked}
      live={{
        name: design.name,
        html,
        writing: design.writing,
        error: html === null ? (loaded.error?.message ?? null) : null,
      }}
      onClose={onClose}
    />
  );
}

/**
 * The designs an answer writes, as cards that open them live. A design the answer kept as a file
 * shows as that file (KeptFiles); one that is open stays open when that happens. A design opens
 * by itself when the answer starts writing it. `dock`: the conversation's panel at the right, where
 * a design opens; without it (a thread) the viewer is a dialog.
 */
export function WrittenDesigns({
  messageId,
  activity,
  draft,
  files,
  messages,
  conversationId,
  send,
  dock = null,
}: {
  messageId: string;
  activity: readonly ActivityView[];
  draft: ArtifactDraft | null;
  files: readonly FileRef[];
  messages: readonly MessageView[];
  conversationId: string | null;
  send: ArtifactSend | null;
  dock?: ((target: ArtifactTarget) => void) | null;
}) {
  const { artifacts } = usePlatform();
  const [inDialog, setInDialog] = useState<string | null>(null);
  const open = (path: string) => (dock ? dock({ kind: "design", messageId, path }) : setInDialog(path));
  const designs = designsOf(activity, draft);
  const kept = new Set(files.filter(isArtifact).map((f) => f.name));
  const shown = designs.filter((d) => !kept.has(d.name));
  const opened = designs.find((d) => d.path === inDialog) ?? null;
  // As Claude Desktop does: a design opens by itself when the answer starts writing it.
  const autoOpened = useRef(new Set<string>());
  const toOpen = designToOpen(designs, autoOpened.current);
  // In the dock the design being written takes the panel, as Claude Desktop does; a dialog does
  // not replace one that is open.
  const autoOpen = useEffectEvent((path: string) =>
    dock ? dock({ kind: "design", messageId, path }) : setInDialog((current) => current ?? path),
  );
  const openCall = toOpen?.callId ?? null;
  const openPath = toOpen?.path ?? null;
  useEffect(() => {
    if (openCall === null || openPath === null) return;
    autoOpened.current.add(openCall);
    autoOpen(openPath);
  }, [openCall, openPath]);
  if (!artifacts || designs.length === 0) return null;
  return (
    <div {...stylex.props(styles.cards)}>
      {shown.map((d) => (
        <button
          key={d.path}
          type="button"
          aria-label={`Open the design ${d.name}`}
          onClick={() => open(d.path)}
          {...stylex.props(styles.card)}
        >
          <span {...stylex.props(styles.accent)}>◧</span>
          <span {...stylex.props(styles.text)}>
            <span>{d.name}</span>
            <span {...stylex.props(styles.meta)}>
              {d.writing ? `Writing…${d.draft === null ? "" : ` · ${sizeText(d.draft.length)}`}` : "Design"}
            </span>
          </span>
          <span {...stylex.props(styles.accent)}>{d.writing ? "Watch" : "Open"}</span>
        </button>
      ))}
      {opened ? (
        <DesignViewer
          design={opened}
          messageId={messageId}
          messages={messages}
          conversationId={conversationId}
          send={send}
          onClose={() => setInDialog(null)}
        />
      ) : null}
    </div>
  );
}
