// The dock at the right of a conversation, as Claude Desktop shows an artifact: a kept HTML file,
// a design an answer writes (live while it streams), or a plan waiting for a decision. The
// conversation's frame holds what is open, so the panel stays while its message scrolls out of
// the feed. One thing at a time: opening one replaces the other.

import { Button } from "@astryxdesign/core/Button";
import type { FileRef, MessageView } from "@aurelius/protocol";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { usePlatform } from "../platform";
import { DesignViewer } from "./artifact-live";
import { designFile, designPages, designsOf, keptDesign } from "./artifact-logic";
import { ArtifactViewer } from "./artifact-viewer";
import { useLive } from "./chat-live";
import type { ChatContext } from "./chat-message";
import { PlanDock } from "./plan-dock";
import { latestPlan, newPlanWaiting } from "./plan-logic";

/**
 * What the dock shows: a kept file, the design at `path` that message `messageId` writes, or the
 * plan of plan message `messageId` (`focus`: the board member opened it, so focus goes to it).
 */
export type ArtifactTarget =
  | { kind: "file"; file: FileRef }
  | { kind: "design"; messageId: string; path: string }
  | { kind: "plan"; messageId: string; focus?: boolean };

/** One key per target: a new one shows in a fresh viewer. */
export const targetKey = (t: ArtifactTarget) =>
  t.kind === "file"
    ? `file:${t.file.id}`
    : t.kind === "design"
      ? `design:${t.messageId}:${t.path}`
      : `plan:${t.messageId}`;

/** The dock's state for a conversation's frame: what is open, and opening or closing it. */
export function useArtifactDock() {
  const [target, setTarget] = useState<ArtifactTarget | null>(null);
  const close = useCallback(() => setTarget(null), []);
  return { target, open: setTarget, close };
}

/**
 * Opens the kept design file `fileId` in the dock once, as soon as the conversation's messages
 * hold it: a design opened from the Designs page shows beside its conversation at once.
 */
export function useOpenDesign(
  open: (target: ArtifactTarget) => void,
  messages: readonly MessageView[],
  fileId: string | null,
) {
  const done = useRef(false);
  const file = fileId ? designFile(messages, fileId) : null;
  useEffect(() => {
    if (file === null || done.current) return;
    done.current = true;
    open({ kind: "file", file });
  }, [file, open]);
}

function DockedDesign({
  ctx,
  messageId,
  path,
  notice,
  onClose,
}: {
  ctx: ChatContext;
  messageId: string;
  path: string;
  notice: ReactNode;
  onClose: () => void;
}) {
  const { artifacts } = usePlatform();
  const update = useLive(ctx.store, messageId);
  const message = ctx.messages.find((m) => m.id === messageId);
  const activity = update?.activity ?? message?.activity ?? [];
  const design = designsOf(activity, update?.draft ?? null).find((d) => d.path === path) ?? null;
  // Once the answer kept the page, the panel shows the design's kept pages (with comments).
  const kept = message ? keptDesign(message, path) : null;
  if (kept && artifacts)
    return (
      <ArtifactViewer
        pages={designPages(ctx.messages, kept, Date.now())}
        openName={kept.name}
        artifacts={artifacts}
        messages={ctx.messages}
        conversationId={ctx.conversationId}
        send={ctx.artifactSend ?? null}
        docked
        notice={notice}
        onClose={onClose}
      />
    );
  if (!design) return null;
  return (
    <DesignViewer
      design={design}
      messageId={messageId}
      messages={ctx.messages}
      conversationId={ctx.conversationId}
      send={ctx.artifactSend ?? null}
      docked
      notice={notice}
      onClose={onClose}
    />
  );
}

/**
 * The open artifact in its panel. A plan is Markdown and shows everywhere; a file or a design
 * needs the app's sandbox, so it shows nothing where there is none (the web).
 */
export function DockedArtifact({
  target,
  ctx,
  onClose,
}: {
  target: ArtifactTarget;
  ctx: ChatContext;
  onClose: () => void;
}) {
  const { artifacts } = usePlatform();
  const waiting = useMemo(
    () => newPlanWaiting(target, latestPlan(ctx.messages, new Set(ctx.approvals.keys()))),
    [target, ctx.messages, ctx.approvals],
  );
  const notice = waiting ? (
    <Button
      size="sm"
      variant="secondary"
      label="New plan waiting"
      onClick={() => ctx.openArtifact?.({ kind: "plan", messageId: waiting })}
    />
  ) : null;
  if (target.kind === "plan")
    return (
      <PlanDock
        key={target.messageId}
        ctx={ctx}
        messageId={target.messageId}
        focus={target.focus ?? false}
        notice={notice}
        onClose={onClose}
      />
    );
  if (!artifacts) return null;
  if (target.kind === "design")
    return <DockedDesign ctx={ctx} messageId={target.messageId} path={target.path} notice={notice} onClose={onClose} />;
  return (
    <ArtifactViewer
      pages={designPages(ctx.messages, target.file, Date.now())}
      openName={target.file.name}
      artifacts={artifacts}
      messages={ctx.messages}
      conversationId={ctx.conversationId}
      send={ctx.artifactSend ?? null}
      docked
      notice={notice}
      onClose={onClose}
    />
  );
}
