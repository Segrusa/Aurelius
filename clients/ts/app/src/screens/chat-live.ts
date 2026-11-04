// What is streaming in the open channel, outside React state: only the row that shows an answer
// re-renders when it grows, at most every 40 ms.

import type { StreamUpdate } from "@aurelius/client";
import { useEffect, useMemo, useSyncExternalStore } from "react";
import { useConnection } from "../connection";

const REPAINT_MS = 40;

export class LiveStore {
  private readonly entries = new Map<string, StreamUpdate>();
  private readonly seenAt = new Map<string, number>();
  private readonly listeners = new Set<() => void>();
  private snapshot: StreamUpdate[] = [];
  private timer: ReturnType<typeof setTimeout> | null = null;

  /** Answers seen while they were written: they stay unfolded, as read. */
  readonly seen = new Set<string>();

  put(update: StreamUpdate) {
    this.seen.add(update.message_id);
    this.entries.set(update.message_id, update);
    this.seenAt.set(update.message_id, Date.now());
    this.schedule();
  }

  /** The list has these: their stream is over, or goes on in their own row. */
  settle(finished: Set<string>) {
    let changed = false;
    for (const id of this.entries.keys()) {
      if (finished.has(id)) {
        this.entries.delete(id);
        this.seenAt.delete(id);
        changed = true;
      }
    }
    if (changed) this.schedule();
  }

  /** The connection went: nothing can grow any more, and what the list says after it comes back holds. */
  clear() {
    if (this.entries.size === 0) return;
    this.entries.clear();
    this.seenAt.clear();
    this.schedule();
  }

  /** An empty answer no one writes to any more goes away. */
  sweep(now: number, afterMs: number) {
    let changed = false;
    for (const [id, update] of this.entries) {
      if (update.text === "" && now - (this.seenAt.get(id) ?? now) > afterMs) {
        this.entries.delete(id);
        changed = true;
      }
    }
    if (changed) this.schedule();
  }

  private schedule() {
    if (this.timer !== null) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      this.snapshot = [...this.entries.values()];
      for (const listener of this.listeners) listener();
    }, REPAINT_MS);
  }

  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  all = () => this.snapshot;
  get = (id: string) => this.snapshot.find((u) => u.message_id === id) ?? null;

  dispose() {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
}

/** The store of one conversation, fed from the connection for as long as it is shown. */
export function useLiveStore(conversationId: string | null | undefined): LiveStore {
  const connection = useConnection();
  const store = useMemo(() => new LiveStore(), []);
  useEffect(() => {
    if (!conversationId) return;
    const off = connection.streams.on((u) => {
      if (u.conversation_id === conversationId) store.put(u);
    });
    const sweep = setInterval(() => store.sweep(Date.now(), 3000), 1000);
    const offState = connection.subscribe(() => {
      if (connection.getState().status !== "online") store.clear();
    });
    return () => {
      off();
      offState();
      clearInterval(sweep);
      store.dispose();
    };
  }, [connection, conversationId, store]);
  return store;
}

export const useLiveAll = (store: LiveStore) => useSyncExternalStore(store.subscribe, store.all);
export const useLive = (store: LiveStore, id: string) => useSyncExternalStore(store.subscribe, () => store.get(id));
