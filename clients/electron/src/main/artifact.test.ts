import { describe, expect, it } from "vitest";
import {
  ARTIFACT_BYTES_MAX,
  ARTIFACT_CSP,
  ARTIFACT_PROXY,
  ARTIFACTS_MAX,
  ArtifactStore,
  artifactMayNavigate,
  artifactRequestAllowed,
  artifactToken,
  captureHeight,
  EXPORT_HEIGHT_MAX,
  exportName,
  exportWidth,
  viewBounds,
} from "./artifact";

const counting = () => {
  let n = 0;
  return () => (n++).toString(16).padStart(32, "0");
};

describe("the artifact policy", () => {
  const directives = new Map(
    ARTIFACT_CSP.split(";").map((d) => {
      const [name = "", ...values] = d.trim().split(/\s+/);
      return [name, values.join(" ")] as const;
    }),
  );

  it("lets inline code run and blocks every way out", () => {
    expect(directives.get("sandbox")).toBe("allow-scripts");
    expect(directives.get("default-src")).toBe("'none'");
    expect(directives.get("script-src")).toBe("'unsafe-inline'");
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

  it("keeps the width and height within what can be made", () => {
    expect(exportWidth(390)).toBe(390);
    expect(exportWidth(10)).toBe(320);
    expect(exportWidth(1e9)).toBe(1920);
    expect(exportWidth("wide")).toBe(1280);
    expect(captureHeight(2000.4, 800)).toBe(2001);
    expect(captureHeight(1e9, 800)).toBe(EXPORT_HEIGHT_MAX);
    expect(captureHeight("tall", 800)).toBe(800);
    expect(captureHeight(Number.NaN, 800)).toBe(800);
  });
});
