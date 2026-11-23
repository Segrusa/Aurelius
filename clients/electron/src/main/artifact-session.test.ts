import type { WebContents } from "electron";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ARTIFACT_BYTES_MAX, resultWithin, WORLD_CALL_MS } from "./artifact";
import { ArtifactViews } from "./artifact-session";

// ArtifactViews only reaches Electron's classes in show(); the world calls below need none of them.
vi.mock("electron", () => ({ BrowserWindow: {}, session: {}, WebContentsView: class {} }));

/** A view whose renderer the page hung: every world call stays pending; `calls` counts them. */
function hungViews() {
  const calls = { count: 0 };
  const contents = {
    isDestroyed: () => false,
    isLoadingMainFrame: () => false,
    executeJavaScriptInIsolatedWorld: () => {
      calls.count++;
      return new Promise(() => undefined);
    },
  };
  const views = new ArtifactViews({} as never, new Promise(() => undefined));
  const inner = views as unknown as { shown: Map<string, unknown> };
  inner.shown.set("1/main", {
    view: { webContents: contents },
    stopPick: null,
  });
  return { views, owner: { id: 1 } as unknown as WebContents, calls };
}

/** Whether `promise` settled, without waiting for it. */
function watch<T>(promise: Promise<T>) {
  const state: { done: boolean; value?: T } = { done: false };
  void promise.then((value) => {
    state.done = true;
    state.value = value;
  });
  return state;
}

describe("a world call", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("is limited to five seconds", () => {
    expect(WORLD_CALL_MS).toBe(5_000);
  });

  it("gives null for work that fails or never ends, the result otherwise", async () => {
    await expect(resultWithin(Promise.resolve(7), WORLD_CALL_MS)).resolves.toBe(7);
    await expect(resultWithin(Promise.reject(new Error("gone")), WORLD_CALL_MS)).resolves.toBeNull();
    vi.useFakeTimers();
    const raced = watch(resultWithin(new Promise(() => undefined), WORLD_CALL_MS));
    await vi.advanceTimersByTimeAsync(WORLD_CALL_MS - 1);
    expect(raced.done).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(raced).toEqual({ done: true, value: null });
  });

  it("in a hung view gives up once the limit is up: an update of a live design and a stop", async () => {
    vi.useFakeTimers();
    const { views, owner, calls } = hungViews();
    const update = watch(views.update(owner, "<p>grown</p>"));
    const stop = watch(views.stopPick(owner));
    await vi.advanceTimersByTimeAsync(WORLD_CALL_MS - 1);
    expect([update.done, stop.done]).toEqual([false, false]);
    await vi.advanceTimersByTimeAsync(1);
    expect(update).toEqual({ done: true, value: false });
    expect(stop.done).toBe(true);
    expect(calls.count).toBe(2);
  });

  it("refuses an update that is not HTML text within the limit, without a world call", async () => {
    const { views, owner, calls } = hungViews();
    await expect(views.update(owner, 42)).resolves.toBe(false);
    await expect(views.update(owner, "x".repeat(ARTIFACT_BYTES_MAX + 1))).resolves.toBe(false);
    expect(calls.count).toBe(0);
  });

  it("but a pick waits for the board member, until it is stopped", async () => {
    vi.useFakeTimers();
    const { views, owner } = hungViews();
    const pick = watch(views.pick(owner));
    await vi.advanceTimersByTimeAsync(WORLD_CALL_MS * 10);
    expect(pick.done).toBe(false);
    void views.stopPick(owner);
    await vi.advanceTimersByTimeAsync(0);
    expect(pick).toEqual({ done: true, value: null });
  });
});
