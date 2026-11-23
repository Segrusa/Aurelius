import type { WebContents } from "electron";
import { describe, expect, it, vi } from "vitest";
import { ArtifactViews, VIEW_SWAP_MS } from "./artifact-session";

// Just enough of Electron for show/place/hide: views as children of one window.
const fake = vi.hoisted(() => {
  type Handler = (...args: unknown[]) => void;
  class Emitter {
    private readonly handlers = new Map<string, Handler[]>();
    on(name: string, handler: Handler) {
      this.handlers.set(name, [...(this.handlers.get(name) ?? []), handler]);
      return this;
    }
    once(name: string, handler: Handler) {
      const one: Handler = (...args) => {
        this.off(name, one);
        handler(...args);
      };
      return this.on(name, one);
    }
    off(name: string, handler: Handler) {
      this.handlers.set(
        name,
        (this.handlers.get(name) ?? []).filter((h) => h !== handler),
      );
      return this;
    }
    emit(name: string) {
      for (const h of this.handlers.get(name) ?? []) h();
    }
  }
  class Contents extends Emitter {
    destroyed = false;
    url: string | null = null;
    isDestroyed() {
      return this.destroyed;
    }
    close() {
      this.destroyed = true;
    }
    loadURL(url: string) {
      this.url = url;
      return Promise.resolve();
    }
    setWebRTCIPHandlingPolicy() {}
    setWindowOpenHandler() {}
  }
  class View {
    webContents = new Contents();
    visible = true;
    setBackgroundColor() {}
    setVisible(on: boolean) {
      this.visible = on;
    }
    setBounds() {}
  }
  const children: View[] = [];
  const window = {
    isDestroyed: () => false,
    on() {},
    off() {},
    getContentBounds: () => ({ x: 0, y: 0, width: 1000, height: 800 }),
    contentView: {
      children,
      addChildView(view: View, index?: number) {
        if (index === undefined) children.push(view);
        else children.splice(index, 0, view);
      },
      removeChildView(view: View) {
        const at = children.indexOf(view);
        if (at >= 0) children.splice(at, 1);
      },
    },
  };
  return { Contents, View, window, children };
});

vi.mock("electron", () => ({
  BrowserWindow: { fromWebContents: () => fake.window },
  session: {},
  WebContentsView: fake.View,
}));

const BOUNDS = { x: 0, y: 0, width: 400, height: 300 };

function setUp() {
  fake.children.length = 0;
  const views = new ArtifactViews({ owns: () => true } as never, Promise.resolve({} as never));
  const owner = Object.assign(new fake.Contents(), { id: 7, getZoomFactor: () => 1 }) as unknown as WebContents;
  return { views, owner };
}

const urls = () => fake.children.map((v) => v.webContents.url);

describe("showing another page in a slot", () => {
  it("keeps the page before in sight, above the new one, until the new one has loaded", async () => {
    const { views, owner } = setUp();
    await views.show(owner, "artifact://a", BOUNDS);
    await views.show(owner, "artifact://b", BOUNDS);
    expect(urls()).toEqual(["artifact://b", "artifact://a"]);
    fake.children[0]?.webContents.emit("did-finish-load");
    expect(urls()).toEqual(["artifact://b"]);
    expect(views.size).toBe(1);
  });

  it("lets the page before go after a while when the new one never finishes loading", async () => {
    vi.useFakeTimers();
    try {
      const { views, owner } = setUp();
      await views.show(owner, "artifact://a", BOUNDS);
      const before = fake.children[0];
      await views.show(owner, "artifact://b", BOUNDS);
      vi.advanceTimersByTime(VIEW_SWAP_MS);
      expect(urls()).toEqual(["artifact://b"]);
      expect(before?.webContents.isDestroyed()).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it("takes both away when the page hides the slot during the swap", async () => {
    const { views, owner } = setUp();
    await views.show(owner, "artifact://a", BOUNDS);
    await views.show(owner, "artifact://b", BOUNDS);
    views.hide(7, "main");
    expect(fake.children).toEqual([]);
  });

  it("hides the page before too when the stage is covered during the swap", async () => {
    const { views, owner } = setUp();
    await views.show(owner, "artifact://a", BOUNDS);
    const before = fake.children[0];
    await views.show(owner, "artifact://b", BOUNDS);
    views.place(owner, null);
    expect(before?.visible).toBe(false);
  });
});
