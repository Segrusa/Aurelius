import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ARTIFACT_BYTES_MAX,
  ARTIFACT_CSP,
  ARTIFACT_PROXY,
  ARTIFACTS_MAX,
  ArtifactStore,
  artifactMayNavigate,
  artifactNonce,
  artifactRequestAllowed,
  artifactResponse,
  artifactToken,
  captureHeight,
  EXPORT_DEVICE_HEIGHT_MAX,
  EXPORT_FAILED_MESSAGE,
  EXPORT_PIXELS_MAX,
  EXPORT_TIMEOUT_MESSAGE,
  EXPORT_TIMEOUT_MS,
  exportFailure,
  exportName,
  exportWidth,
  INLINE_ONLY_CSP,
  viewBounds,
  withinTime,
} from "./artifact";

const counting = () => {
  let n = 0;
  return () => (n++).toString(16).padStart(32, "0");
};

describe("the artifact policy", () => {
  const nonce = "0123456789abcdef0123456789abcdef";
  const parse = (csp: string) =>
    new Map(
      csp.split(";").map((d) => {
        const [name = "", ...values] = d.trim().split(/\s+/);
        return [name, values.join(" ")] as const;
      }),
    );
  const page =
    '<!doctype html><SCRIPT>a()</SCRIPT><body><script type="module">b()</script>' +
    '<script\nsrc="x.js"></script><button onclick="c()">c</button><scripts-list></scripts-list>';
  const served = artifactResponse(page, nonce);
  const policies = served.csp.split(", ");
  const directives = parse(policies[0] ?? "");

  it("runs only scripts with this response's nonce and on…= attributes, never javascript: URLs", () => {
    expect(directives.get("script-src")).toBe(`'nonce-${nonce}'`);
    expect(directives.get("script-src")).not.toContain("unsafe-inline");
    expect(directives.get("script-src-attr")).toBe("'unsafe-inline'");
    expect(policies[0]?.match(/script-src /g)).toHaveLength(1);
  });

  it("adds a second policy that allows only inline scripts, no script from a URL", () => {
    expect(policies).toHaveLength(2);
    expect(policies[1]).toBe(INLINE_ONLY_CSP);
    expect(parse(policies[1] ?? "").get("script-src")).toBe("'unsafe-inline'");
    expect(policies[1]).not.toMatch(/https?:|data:|\*|nonce/);
  });

  it("gives every <script element the nonce and leaves the rest of the page alone", () => {
    const tags = served.body.match(/<script\b[^>]*>/gi) ?? [];
    expect(tags).toHaveLength(3);
    for (const tag of tags) expect(tag).toMatch(new RegExp(`^<script nonce=${nonce}[\\s>]`, "i"));
    expect(served.body.replaceAll(` nonce=${nonce}`, "")).toBe(page.replace("<SCRIPT", "<script"));
    expect(served.body).toContain("<scripts-list>");
  });

  it("keeps the rest of the policy and accepts only a hex nonce", () => {
    expect(policies[0]?.startsWith(`${ARTIFACT_CSP}; `)).toBe(true);
    expect(parse(ARTIFACT_CSP).has("script-src")).toBe(false);
    expect(() => artifactResponse(page, "x' 'unsafe-inline")).toThrow();
    expect(() => artifactResponse(page, "")).toThrow();
  });

  it("makes a fresh nonce for every response", () => {
    const nonces = new Set(Array.from({ length: 50 }, () => artifactNonce()));
    expect(nonces.size).toBe(50);
    for (const n of nonces) expect(n).toMatch(/^[0-9a-f]{32}$/);
  });

  it("blocks every way out", () => {
    expect(directives.get("sandbox")).toBe("allow-scripts");
    expect(directives.get("default-src")).toBe("'none'");
    expect(directives.get("style-src")).toBe("'unsafe-inline'");
    expect(directives.get("connect-src")).toBe("'none'");
    expect(directives.get("form-action")).toBe("'none'");
    expect(directives.get("frame-src")).toBe("'none'");
    expect(directives.get("object-src")).toBe("'none'");
    expect(directives.get("base-uri")).toBe("'none'");
  });

  it("loads images, fonts and media only from data: or blob:", () => {
    expect(directives.get("img-src")).toBe("data: blob:");
    expect(directives.get("font-src")).toBe("data:");
    expect(directives.get("media-src")).toBe("data: blob:");
    expect(ARTIFACT_CSP).not.toMatch(/https?:|'self'|\*/);
  });
});

describe("artifactToken", () => {
  it("reads the token of an artifact URL only", () => {
    const token = "0123456789abcdef0123456789abcdef";
    expect(artifactToken(`artifact://view/${token}`)).toBe(token);
    expect(artifactToken(`artifact://view/${token}/`)).toBe(token);
    expect(artifactToken(`artifact://view/${token}#top`)).toBe(token);
    expect(artifactToken(`artifact://other/${token}`)).toBeNull();
    expect(artifactToken(`app://aurelius/${token}`)).toBeNull();
    expect(artifactToken("artifact://view/../index.html")).toBeNull();
    expect(artifactToken(`artifact://view/${token}/x`)).toBeNull();
  });
});

describe("ArtifactStore", () => {
  it("serves what a window handed over until it is released", () => {
    const store = new ArtifactStore(counting());
    const url = store.serve(1, "<p>hi</p>");
    expect(url).toMatch(/^artifact:\/\/view\/[0-9a-f]{32}$/);
    expect(store.html(url)).toBe("<p>hi</p>");
    store.release(1, url);
    expect(store.html(url)).toBeNull();
  });

  it("does not let another window release an artifact", () => {
    const store = new ArtifactStore(counting());
    const url = store.serve(1, "a");
    store.release(2, url);
    expect(store.has(url)).toBe(true);
  });

  it("knows which window an artifact belongs to", () => {
    const store = new ArtifactStore(counting());
    const url = store.serve(1, "a");
    expect(store.owns(1, url)).toBe(true);
    expect(store.owns(2, url)).toBe(false);
    expect(store.owns(1, "artifact://view/ffffffffffffffffffffffffffffffff")).toBe(false);
  });

  it("drops everything of a window that closed and keeps the others", () => {
    const store = new ArtifactStore(counting());
    const mine = store.serve(1, "a");
    const theirs = store.serve(2, "b");
    store.releaseAll(1);
    expect(store.has(mine)).toBe(false);
    expect(store.has(theirs)).toBe(true);
  });

  it("holds at most ARTIFACTS_MAX and lets the oldest go first", () => {
    const store = new ArtifactStore(counting());
    const urls = Array.from({ length: ARTIFACTS_MAX + 2 }, (_, i) => store.serve(1, `v${i}`));
    expect(store.size).toBe(ARTIFACTS_MAX);
    expect(store.has(urls[0] ?? "")).toBe(false);
    expect(store.has(urls[1] ?? "")).toBe(false);
    expect(store.has(urls.at(-1) ?? "")).toBe(true);
  });

  it("counts ARTIFACTS_MAX per window: another window's artifacts never push out mine", () => {
    const store = new ArtifactStore(counting());
    const mine = store.serve(1, "a");
    const theirs = Array.from({ length: ARTIFACTS_MAX + 4 }, (_, i) => store.serve(2, `b${i}`));
    expect(store.has(mine)).toBe(true);
    expect(store.has(theirs[0] ?? "")).toBe(false);
    expect(store.has(theirs.at(-1) ?? "")).toBe(true);
    expect(store.size).toBe(ARTIFACTS_MAX + 1);
  });

  it("makes room in bytes from the window's own artifacts, and refuses what only another's could free", () => {
    const store = new ArtifactStore(counting());
    const big = "x".repeat(ARTIFACT_BYTES_MAX);
    const first = store.serve(1, big);
    const second = store.serve(1, big);
    expect(() => store.serve(2, big)).toThrow(/Other windows hold too many large artifacts/);
    expect(store.has(first)).toBe(true);
    expect(store.has(second)).toBe(true);
    store.serve(1, big);
    expect(store.has(first)).toBe(false);
  });

  it("refuses an artifact that is too large", () => {
    const store = new ArtifactStore(counting());
    expect(() => store.serve(1, "x".repeat(ARTIFACT_BYTES_MAX + 1))).toThrow(/larger than 25 MB/);
    expect(store.size).toBe(0);
  });

  it("keeps the total within its limit by letting old ones go", () => {
    const store = new ArtifactStore(counting());
    const big = "x".repeat(ARTIFACT_BYTES_MAX);
    const first = store.serve(1, big);
    store.serve(1, big);
    const third = store.serve(1, big);
    expect(store.has(first)).toBe(false);
    expect(store.has(third)).toBe(true);
  });
});

describe("the artifact session", () => {
  const token = "0123456789abcdef0123456789abcdef";

  it("lets only an artifact document through", () => {
    expect(artifactRequestAllowed(`artifact://view/${token}`)).toBe(true);
    expect(artifactRequestAllowed(`artifact://view/${token}#top`)).toBe(true);
    expect(artifactRequestAllowed("http://127.0.0.1:3478/leak?d=x")).toBe(false);
    expect(artifactRequestAllowed("https://exfil.example/")).toBe(false);
    expect(artifactRequestAllowed("ws://127.0.0.1/")).toBe(false);
    expect(artifactRequestAllowed(`artifact://exfil.example/?d=${token}`)).toBe(false);
    expect(artifactRequestAllowed("artifact://view/favicon.ico")).toBe(false);
    expect(artifactRequestAllowed("app://aurelius/index.html")).toBe(false);
  });

  it("sends whatever gets past the filter to a proxy that is not there, loopback too", () => {
    for (const scheme of ["http", "https", "socks"])
      expect(ARTIFACT_PROXY.proxyRules).toContain(`${scheme}=127.0.0.1:9`);
    expect(ARTIFACT_PROXY.proxyBypassRules).toBe("<-loopback>");
  });
});

describe("artifactMayNavigate", () => {
  const own = "artifact://view/0123456789abcdef0123456789abcdef";

  it("keeps an artifact on its own document", () => {
    expect(artifactMayNavigate(own, own)).toBe(true);
    expect(artifactMayNavigate(own, `${own}#pricing`)).toBe(true);
    expect(artifactMayNavigate(`${own}#a`, `${own}#b`)).toBe(true);
  });

  it("refuses another artifact and anything that is not one", () => {
    expect(artifactMayNavigate(own, "artifact://view/ffffffffffffffffffffffffffffffff")).toBe(false);
    expect(artifactMayNavigate(own, `${own}?leak=1`)).toBe(false);
    expect(artifactMayNavigate(own, "https://example.com/")).toBe(false);
    expect(artifactMayNavigate(own, "app://aurelius/settings")).toBe(false);
    expect(artifactMayNavigate(own, "about:srcdoc")).toBe(false);
    expect(artifactMayNavigate("https://example.com/", "https://example.com/")).toBe(false);
  });
});

describe("viewBounds", () => {
  const content = { width: 1440, height: 900 };

  it("places the view where the page's stage is, scaled by the page's zoom", () => {
    expect(viewBounds({ x: 50.4, y: 120, width: 1280, height: 600 }, 1, content)).toEqual({
      x: 50,
      y: 120,
      width: 1280,
      height: 600,
    });
    expect(viewBounds({ x: 10, y: 20, width: 100, height: 50 }, 1.5, content)).toEqual({
      x: 15,
      y: 30,
      width: 150,
      height: 75,
    });
  });

  it("keeps the view inside the window", () => {
    expect(viewBounds({ x: -100, y: 800, width: 2000, height: 400 }, 1, content)).toEqual({
      x: 0,
      y: 800,
      width: 1440,
      height: 100,
    });
  });

  it("shows nothing for what is off the window or not a rectangle", () => {
    expect(viewBounds({ x: 1500, y: 0, width: 100, height: 100 }, 1, content)).toBeNull();
    expect(viewBounds({ x: 0, y: 0, width: 0, height: 100 }, 1, content)).toBeNull();
    expect(viewBounds({ x: 0, y: 0, width: Number.NaN, height: 100 }, 1, content)).toBeNull();
    expect(viewBounds({ x: "0", y: 0, width: 10, height: 10 }, 1, content)).toBeNull();
    expect(viewBounds(null, 1, content)).toBeNull();
    expect(viewBounds("everywhere", 1, content)).toBeNull();
  });
});

describe("exports", () => {
  it("names the file after the artifact with the format's extension", () => {
    expect(exportName("design/landing-page.html", "pdf")).toBe("landing-page.pdf");
    expect(exportName("C:\\work\\Plan.HTM", "png")).toBe("Plan.png");
    expect(exportName(".html", "pdf")).toBe("artifact.pdf");
  });

  it("tells the page only that an export ran out of time, or that it did not work", () => {
    expect(exportFailure(new Error(EXPORT_TIMEOUT_MESSAGE))).toBe(EXPORT_TIMEOUT_MESSAGE);
    expect(exportFailure(new Error("UnknownVizError"))).toBe(EXPORT_FAILED_MESSAGE);
    expect(exportFailure(new Error("EACCES: permission denied, open '/home/u/x.png'"))).toBe(EXPORT_FAILED_MESSAGE);
    expect(exportFailure("printToPDF failed")).toBe(EXPORT_FAILED_MESSAGE);
    expect(exportFailure(undefined)).toBe("The export did not work.");
  });

  it("keeps the width and height within what can be made", () => {
    expect(exportWidth(390)).toBe(390);
    expect(exportWidth(10)).toBe(320);
    expect(exportWidth(1e9)).toBe(1920);
    expect(exportWidth("wide")).toBe(1280);
    expect(captureHeight(2000.4, 800, 1280)).toBe(2001);
    expect(captureHeight(1e9, 800, 1920)).toBe(EXPORT_DEVICE_HEIGHT_MAX);
    expect(captureHeight("tall", 800, 1280)).toBe(800);
    expect(captureHeight(Number.NaN, 800, 1280)).toBe(800);
  });

  it("keeps a PNG within EXPORT_PIXELS_MAX device pixels, also on a HiDPI display", () => {
    expect(captureHeight(1e9, 800, 1920, 1) * 1920).toBeLessThanOrEqual(EXPORT_PIXELS_MAX);
    const atTwo = captureHeight(1e9, 800, 1920, 2);
    expect(atTwo).toBe(Math.min(EXPORT_DEVICE_HEIGHT_MAX / 2, Math.floor(EXPORT_PIXELS_MAX / (1920 * 4))));
    expect(atTwo * 1920 * 2 * 2).toBeLessThanOrEqual(EXPORT_PIXELS_MAX);
    expect(captureHeight(1e9, 800, 1920, 3) * 1920 * 9).toBeLessThanOrEqual(EXPORT_PIXELS_MAX);
    expect(captureHeight(2000, 800, 390, 2)).toBe(2000);
    expect(captureHeight(1e9, 800, 1920, Number.NaN)).toBe(EXPORT_DEVICE_HEIGHT_MAX);
  });

  it("keeps a PNG within EXPORT_DEVICE_HEIGHT_MAX device pixels tall at every scale", () => {
    // Chromium failed with UnknownVizError at 390 px wide on a 2x display (Sentinel, SEC2 N2).
    expect(captureHeight(1e9, 800, 390, 2)).toBe(8192);
    expect(captureHeight(1e9, 800, 1280, 2)).toBe(8192);
    expect(captureHeight(1e9, 800, 390, 1.5)).toBe(10_922);
    for (const width of [320, 390, 768, 1024, 1280, 1920])
      for (const scale of [1, 1.25, 1.5, 2, 2.5, 3, 4]) {
        const tall = captureHeight(1e9, 800, width, scale);
        expect(tall * scale).toBeLessThanOrEqual(EXPORT_DEVICE_HEIGHT_MAX);
        expect(width * scale * tall * scale).toBeLessThanOrEqual(EXPORT_PIXELS_MAX);
        expect(tall).toBeGreaterThan(0);
        expect(captureHeight(1e9, 1e9, width, scale)).toBe(tall);
        expect(captureHeight(Number.NaN, 1e9, width, scale)).toBe(tall);
      }
  });
});

describe("the export's time limit", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("is twenty seconds", () => {
    expect(EXPORT_TIMEOUT_MS).toBe(20_000);
    expect(EXPORT_TIMEOUT_MESSAGE).toBe("This artifact did not finish rendering.");
  });

  it("gives the result of work that finishes in time", async () => {
    await expect(withinTime(Promise.resolve(42), EXPORT_TIMEOUT_MS, EXPORT_TIMEOUT_MESSAGE)).resolves.toBe(42);
  });

  it("fails work that never finishes once the time is up", async () => {
    vi.useFakeTimers();
    const never = new Promise<number>(() => undefined);
    const raced = withinTime(never, EXPORT_TIMEOUT_MS, EXPORT_TIMEOUT_MESSAGE);
    const seen = expect(raced).rejects.toThrow(EXPORT_TIMEOUT_MESSAGE);
    await vi.advanceTimersByTimeAsync(EXPORT_TIMEOUT_MS);
    await seen;
  });

  it("passes on the work's own failure, and drops a failure that comes after the limit", async () => {
    await expect(withinTime(Promise.reject(new Error("no")), 1_000, EXPORT_TIMEOUT_MESSAGE)).rejects.toThrow("no");
    vi.useFakeTimers();
    let fail: (error: Error) => void = () => undefined;
    const late = new Promise<number>((_resolve, reject) => {
      fail = reject;
    });
    const unhandled = vi.fn();
    process.on("unhandledRejection", unhandled);
    const raced = withinTime(late, 1_000, EXPORT_TIMEOUT_MESSAGE);
    const seen = expect(raced).rejects.toThrow(EXPORT_TIMEOUT_MESSAGE);
    await vi.advanceTimersByTimeAsync(1_000);
    await seen;
    fail(new Error("too late"));
    await vi.advanceTimersByTimeAsync(0);
    process.off("unhandledRejection", unhandled);
    expect(unhandled).not.toHaveBeenCalled();
  });
});
