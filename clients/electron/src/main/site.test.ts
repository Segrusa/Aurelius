import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { siteFile } from "./site";

const root = mkdtempSync(join(tmpdir(), "aurelius-site-"));
const site = join(root, "site");
mkdirSync(join(site, "assets"), { recursive: true });
writeFileSync(join(site, "index.html"), "");
writeFileSync(join(site, "assets/a.js"), "");
writeFileSync(join(root, "secret.txt"), "");
const index = join(site, "index.html");

afterAll(() => rmSync(root, { recursive: true, force: true }));

describe("the site", () => {
  it("serves its files", () => {
    expect(siteFile(site, "/assets/a.js")).toBe(join(site, "assets/a.js"));
    expect(siteFile(site, "/")).toBe(index);
  });

  it("hands every other path to the router", () => {
    expect(siteFile(site, "/inbox")).toBe(index);
    expect(siteFile(site, "/assets")).toBe(index);
    expect(siteFile(site, "/%E0%A4%A")).toBe(index);
  });

  it("never serves what lies outside it", () => {
    expect(siteFile(site, "/../secret.txt")).toBe(index);
    expect(siteFile(site, "/%2e%2e/secret.txt")).toBe(index);
    expect(siteFile(site, "/..%2Fsecret.txt")).toBe(index);
  });
});
