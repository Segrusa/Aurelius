// The artifact viewer docked at the right of a conversation, as Claude Desktop shows one: a kept
// HTML file, or a design an answer writes (live while it streams). The conversation's frame holds
// what is open, so the panel stays while its message scrolls out of the feed.

import type { FileRef } from "@aurelius/protocol";
import { useCallback, useState } from "react";
import { usePlatform } from "../platform";
import { DesignViewer } from "./artifact-live";
import { designPages, designsOf } from "./artifact-logic";
import { ArtifactViewer } from "./artifact-viewer";
import { useLive } from "./chat-live";
import type { ChatContext } from "./chat-message";

/** What the dock shows: a kept file, or the design at `path` that message `messageId` writes. */
export type ArtifactTarget = { kind: "file"; file: FileRef } | { kind: "design"; messageId: string; path: string };

/** One key per target: a new one shows in a fresh viewer. */
export const targetKey = (t: ArtifactTarget) =>
  t.kind === "file" ? `file:${t.file.id}` : `design:${t.messageId}:${t.path}`;

/** The dock's state for a conversation's frame: what is open, and opening or closing it. */
export function useArtifactDock() {
  const [target, setTarget] = useState<ArtifactTarget | null>(null);
  const close = useCallback(() => setTarget(null), []);
  return { target, open: setTarget, close };
}

function DockedDesign({
  ctx,
  messageId,
  path,
  onClose,
}: {
  ctx: ChatContext;
  messageId: string;
  path: string;
  onClose: () => void;
}) {
  const update = useLive(ctx.store, messageId);
  const activity = update?.activity ?? ctx.messages.find((m) => m.id === messageId)?.activity ?? [];
  const design = designsOf(activity, update?.draft ?? null).find((d) => d.path === path) ?? null;
  if (!design) return null;
  return (
    <DesignViewer
      design={design}
      messageId={messageId}
      messages={ctx.messages}
      conversationId={ctx.conversationId}
      send={ctx.artifactSend ?? null}
      docked
      onClose={onClose}
    />
  );
}

/** The open artifact in its panel; nothing where the app cannot show one (the web). */
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
  if (!artifacts) return null;
  if (target.kind === "design")
    return <DockedDesign ctx={ctx} messageId={target.messageId} path={target.path} onClose={onClose} />;
  return (
    <ArtifactViewer
      pages={designPages(ctx.messages, target.file, Date.now())}
      openName={target.file.name}
      artifacts={artifacts}
      messages={ctx.messages}
      conversationId={ctx.conversationId}
      send={ctx.artifactSend ?? null}
      docked
      onClose={onClose}
    />
  );
}
