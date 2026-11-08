// The web build under the policy the gateway really sends: Astryx puts a few <style> elements in the
// page, and only those with a hash in the policy may stay. A refused one is silent without this check.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { REPO, serveStatic, WEB_DIST } from "./daemon";
import { channel, message, messages, scriptServer } from "./fake-server";

const SOURCE = readFileSync(join(REPO, "crates/remote-gateway/src/web.rs"), "utf8");
const POLICY = /"content-security-policy",\s*"([^"]+)"/.exec(SOURCE)?.[1] as string;

test("nothing the app draws is refused by the gateway's policy", async ({ page }) => {
  expect(POLICY).toContain("style-src 'self' 'sha256-");
  const site = await serveStatic(WEB_DIST, { "content-security-policy": POLICY });
  try {
    await page.addInitScript(() => {
      const seen: string[] = [];
      (globalThis as unknown as { violations: string[] }).violations = seen;
      document.addEventListener("securitypolicyviolation", (e) =>
        seen.push(`${e.violatedDirective} ${e.blockedURI} ${e.sample}`),
      );
    });
    await scriptServer(page, {
      employees: [{ id: "e1", name: "Flux", role: "engineer", title: "Engineer", status: "active" }],
      channels: [channel("g", "general", { kind: "general" })],
      feeds: { general: messages("g", [message("m1", "assistant", "```ts\nconst answer: number = 42;\n```")]) },
    });
    await page.goto(`${site.origin}/chat`);

    // A code block brings its highlight rules.
    await expect(page.locator(".astryx-code-block code")).toBeVisible();
    await expect(page.locator("style:not([data-astryx-highlight-dynamic])").first()).toBeAttached();

    // A dialog and a tooltip bring the slide rules of a layer.
    await page.keyboard.press("Alt+t");
    await expect(page.getByLabel("Title")).toBeVisible();
    await page.keyboard.press("Escape");
    await page.getByRole("navigation", { name: "Main" }).getByRole("link").first().hover();
    await page.waitForTimeout(600);

    expect(await page.evaluate(() => (globalThis as unknown as { violations: string[] }).violations)).toEqual([]);
  } finally {
    site.close();
  }
});
