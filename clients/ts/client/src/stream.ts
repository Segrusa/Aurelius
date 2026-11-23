// Both transports end in one model: the full text so far and the whole list of tool calls. A pipe
// sends full text (`stream`), the gateway appends (`stream_append`, offsets in UTF-16 units, which
// are JavaScript's own). Either may send only the calls that changed. Port of crates/client/src/stream.rs.

import type {
  ActivityView,
  AgentMode,
  ArtifactDraft,
  Event,
  RunProgress,
  StreamAppend,
  StreamDelta,
} from "@aurelius/protocol";

export interface StreamUpdate {
  run_id: string;
  conversation_id: string;
  message_id: string;
  text: string;
  activity: ActivityView[];
  progress: RunProgress | null;
  mode: AgentMode | null;
  thread_parent: string | null;
  /** A design the answer is writing now (feature `artifact_drafts`), until its call is whole. */
  draft?: ArtifactDraft | null;
}

/** Drafts kept at most: one per answer writing a design, so a stopped run's draft does not stay forever. */
const DRAFTS_KEPT = 16;

function isCut(text: string, at: number): boolean {
  if (at <= 0 || at >= text.length) return false;
  const before = text.charCodeAt(at - 1);
  const after = text.charCodeAt(at);
  return before >= 0xd800 && before <= 0xdbff && after >= 0xdc00 && after <= 0xdfff;
}

export class StreamMerge {
  private readonly texts = new Map<string, string>();
  private readonly calls = new Map<string, ActivityView[]>();
  private readonly drafts = new Map<string, ArtifactDraft>();

  /** The update, or null when an append does not continue what came before: the next full text fixes it. */
  append(append: StreamAppend): StreamUpdate | null {
    const known = this.texts.get(append.message_id) ?? "";
    let text: string;
    if (append.offset === 0) {
      text = append.text;
    } else {
      if (append.offset > known.length || isCut(known, append.offset)) return null;
      text = known.slice(0, append.offset) + append.text;
    }
    this.texts.set(append.message_id, text);
    return this.update(append, text, append.activity ?? null);
  }

  delta(delta: StreamDelta): StreamUpdate {
    return this.update(delta, delta.text, delta.activity);
  }

  /** The server forgets what it sent on these and sends whole texts and lists after. */
  onEvent(event: Event): void {
    if (event.kind.startsWith("message.") || event.kind.startsWith("run.")) {
      this.texts.clear();
      this.calls.clear();
    }
  }

  /** A new connection starts whole again. */
  reset(): void {
    this.texts.clear();
    this.calls.clear();
    this.drafts.clear();
  }

  private update(from: StreamAppend | StreamDelta, text: string, activity: ActivityView[] | null): StreamUpdate {
    const calls = this.merged(from.message_id, activity, from.activity_changes ?? false);
    return {
      run_id: from.run_id,
      conversation_id: from.conversation_id,
      message_id: from.message_id,
      text,
      activity: calls,
      progress: from.progress ?? null,
      mode: from.mode ?? null,
      thread_parent: from.thread_parent ?? null,
      draft: this.drafted(from.message_id, from.draft ?? null, calls),
    };
  }

  /** The design being written: only an update that brings one says so, so it stays until its call is whole. */
  private drafted(
    messageId: string,
    draft: ArtifactDraft | null,
    calls: readonly ActivityView[],
  ): ArtifactDraft | null {
    if (draft) {
      this.drafts.delete(messageId);
      this.drafts.set(messageId, draft);
      for (const old of this.drafts.keys()) {
        if (this.drafts.size <= DRAFTS_KEPT) break;
        this.drafts.delete(old);
      }
    }
    const kept = this.drafts.get(messageId) ?? null;
    if (kept && calls.some((c) => c.call_id === kept.call_id)) {
      this.drafts.delete(messageId);
      return null;
    }
    return kept;
  }

  private merged(messageId: string, activity: ActivityView[] | null, changesOnly: boolean): ActivityView[] {
    const known = this.calls.get(messageId) ?? [];
    if (activity === null) return known;
    if (!changesOnly) {
      this.calls.set(messageId, activity);
      return activity;
    }
    const calls = [...known];
    for (const call of activity) {
      const at = calls.findIndex((c) => c.call_id === call.call_id);
      if (at >= 0) calls[at] = call;
      else calls.push(call);
    }
    this.calls.set(messageId, calls);
    return calls;
  }
}
