// Astryx writes two fixed <style> blocks at runtime, and our Content-Security-Policy allows exactly
// those by hash. After an Astryx update this fails with the new hashes, instead of the menus silently
// losing their positioning.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { describe, expect, it, vi } from "vitest";

const repo = resolve(import.meta.dirname, "../../../..");
const core = (file: string) =>
  import(
    /* @vite-ignore */ pathToFileURL(resolve(repo, "clients/ts/app/node_modules/@astryxdesign/core/dist", file)).href
  );
const hash = (css: string) => `'sha256-${createHash("sha256").update(css, "utf8").digest("base64")}'`;

let written: Promise<string[]> | null = null;
const astryxStyleHashes = () => {
  written ??= readHashes();
  return written;
};

async function readHashes(): Promise<string[]> {
  const { slideRulesCss } = await core("Layer/layerSlideRules.js");
  let css = "";
  vi.stubGlobal("document", {
    createElement: () => ({ setAttribute() {}, textContent: "" }),
    head: {
      appendChild(style: { textContent: string }) {
        css = style.textContent;
      },
    },
  });
  const { ensureHighlightStyles } = await core("CodeBlock/highlightStyles.js");
  ensureHighlightStyles();
  vi.unstubAllGlobals();
  expect(css).not.toBe("");
  return [hash(slideRulesCss()), hash(css)];
}

describe("the desktop app's Content-Security-Policy", () => {
  const source = readFileSync(resolve(repo, "clients/electron/src/main/index.ts"), "utf8");
  const policy = /const CSP =\s*"([^"]+)"/.exec(source)?.[1] ?? "";
  const directives = policy.split(";").map((d) => d.trim());

  // Artifacts are not framed in the app's page any more: they run in a view of their own.
  it("does not let the page frame artifacts, and gained nothing for them", () => {
    expect(directives.map((d) => d.split(" ")[0])).toEqual([
      "default-src",
      "style-src",
      "img-src",
      "object-src",
      "base-uri",
      "frame-ancestors",
      "form-action",
    ]);
    expect(policy).not.toContain("artifact");
    expect(directives).toContain("default-src 'self'");
    expect(directives).toContain("img-src 'self' data: blob:");
    expect(directives).toContain("frame-ancestors 'none'");
    expect(directives).toContain("form-action 'none'");
  });
});

describe("the desktop app's switches", () => {
  const source = readFileSync(resolve(repo, "clients/electron/src/main/index.ts"), "utf8");

  it("turns DNS prefetch off before the app is ready", () => {
    const at = source.indexOf('app.commandLine.appendSwitch("blink-settings", "dnsPrefetchingEnabled=false");');
    expect(at).toBeGreaterThan(-1);
    expect(at).toBeLessThan(source.indexOf("app.whenReady()"));
  });

  it("gives the page a clear sentence for a failed export and logs the details", () => {
    const start = source.indexOf('handle("artifact:export"');
    const handler = source.slice(start, source.indexOf("\n  });", start));
    expect(start).toBeGreaterThan(-1);
    const caught = handler.slice(handler.indexOf("} catch (error) {"));
    expect(caught).toContain('"artifacts.log"');
    expect(caught).toContain("throw new Error(exportFailure(error));");
    expect(caught.match(/throw /g)).toHaveLength(1);
    // Rendering and writing the file are both inside the try.
    expect(handler.indexOf("try {")).toBeLessThan(handler.indexOf("renderArtifact("));
    expect(handler.indexOf("writeFileSync(")).toBeLessThan(handler.indexOf("} catch (error) {"));
  });
});

describe("the style hashes in the Content-Security-Policy", () => {
  for (const file of ["clients/electron/src/main/index.ts", "crates/remote-gateway/src/web.rs"]) {
    it(`match what the installed Astryx writes: ${file}`, async () => {
      const policy = readFileSync(resolve(repo, file), "utf8");
      for (const allowed of await astryxStyleHashes()) expect(policy).toContain(allowed);
    });
  }
});
