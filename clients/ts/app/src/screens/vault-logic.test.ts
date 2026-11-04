import type { VaultItemView } from "@aurelius/protocol";
import { describe, expect, it } from "vitest";
import {
  actionText,
  checkEnvName,
  checkName,
  checkValue,
  createParams,
  deleteTitle,
  envFromName,
  formProblem,
  GONE,
  hidesIn,
  keyEnv,
  keyFileName,
  kindsFor,
  mergeAudit,
  scopes,
  updateParams,
  via,
  who,
} from "./vault-logic";

const item = (over: Partial<VaultItemView> = {}) =>
  ({
    id: "i",
    kind: "secret",
    name: "GitHub token",
    env_name: "GITHUB_TOKEN",
    description: null,
    version: 1,
    ...over,
  }) as VaultItemView;

describe("names", () => {
  it.each([
    ["GitHub token", "GITHUB_TOKEN"],
    ["  2nd staging-url! ", "ND_STAGING_URL"],
    ["Ünïcode", "N_CODE"],
    [`${"A".repeat(63)} b`, "A".repeat(63)],
  ])("%s", (name, env) => {
    expect(envFromName(name)).toBe(env);
  });
  it("a long name still gives a valid key", () => {
    const env = envFromName("Staging database read only token for the nightly export job of the reporting service");
    expect(env.length).toBeLessThanOrEqual(64);
    expect(env.endsWith("_")).toBe(false);
    expect(checkEnvName(env)).toBeNull();
  });
  it.each([
    "GITHUB_TOKEN",
    "STAGING_URL",
    "A",
    "X9_Y",
    "AWS_ACCESS_KEY_ID",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "STRIPE_KEY",
    "CANARY_TOKEN",
    `A${"B".repeat(63)}`,
  ])("allows %s", (env) => {
    expect(checkEnvName(env)).toBeNull();
  });
  it.each(["", "github_token", "1TOKEN", "_X", "MY-KEY", "MY KEY", "TÖKEN", "Path", "A-B", `A${"B".repeat(64)}`])(
    "refuses %s",
    (env) => {
      expect(checkEnvName(env)).toContain("A–Z");
    },
  );
  it.each([
    "PATH",
    "HOME",
    "NODE_OPTIONS",
    "LD_PRELOAD",
    "DYLD_X",
    "CLAUDE_CODE_USE_BEDROCK",
    "AURELIUS_X",
    "HTTPS_PROXY",
    "SSL_CERT_FILE",
    "OTEL_X",
    "MCP_X",
  ])("keeps %s for the runtime", (env) => {
    expect(checkEnvName(env)).toContain("is kept by Aurelius");
  });
  it("says which", () => {
    expect(checkEnvName("ANTHROPIC_API_KEY")).toBe(
      "ANTHROPIC_API_KEY is kept by Aurelius or Claude Code for every run. Choose another key.",
    );
    expect(checkName("GitHub token")).toBeNull();
    expect(checkName("  ")).toBe("Give the item a name.");
    expect(checkName("a\nb")).toContain("one line");
  });
});

describe("values", () => {
  it.each(["https://stg.example.com/x", "http://localhost:8080"])("url %s", (v) => {
    expect(checkValue("url", v)).toBeNull();
  });
  it.each(["stg.example.com", "ftp://x", "https://", "https:///x", "https://a b", "javascript:alert(1)"])(
    "bad url %s",
    (v) => {
      expect(checkValue("url", v)).toContain("full URL");
    },
  );
  it("secrets and text", () => {
    expect(checkValue("secret", "1234567")).toContain("at least 8");
    expect(checkValue("secret", "12345678")).toBeNull();
    expect(checkValue("secret", "sk-test\nFAKE-0000")).toBe("A secret is one line.");
    expect(checkValue("text", "")).toBe("Give the item a value.");
    expect(checkValue("text", "a\nb")).toBeNull();
    expect(checkValue("text", "a".repeat(65537))).toBe("A value has at most 64 KB.");
  });
});

describe("kinds and time", () => {
  it("a secret stays a secret", () => {
    expect(kindsFor(null)).toEqual(["text", "url", "secret"]);
    expect(kindsFor("text")).toEqual(["text", "url"]);
    expect(kindsFor("url")).toEqual(["text", "url"]);
    expect(kindsFor("secret")).toEqual(["secret"]);
  });
  it("counts down", () => {
    expect(hidesIn(30_000, 0)).toBe("Hides in 0:30");
    expect(hidesIn(26_400, 0)).toBe("Hides in 0:27");
    expect(hidesIn(0, 0)).toBeNull();
  });
});

describe("saving", () => {
  const form = { name: "GitHub token", env: "GITHUB_TOKEN", kind: "secret" as const, value: "", note: "" };
  it("an empty value keeps a secret", () => {
    expect(updateParams(item(), form)).toBeNull();
    expect(updateParams(item(), { ...form, env: "GH_TOKEN", note: "Fine-grained" })).toEqual({
      item_id: "i",
      env_name: "GH_TOKEN",
      description: "Fine-grained",
    });
    expect(updateParams(item(), { ...form, value: "new-value-1" })).toEqual({ item_id: "i", value: "new-value-1" });
  });
  it("a URL becomes text with only its kind", () => {
    const url = item({ kind: "url", value: "https://x" } as Partial<VaultItemView>);
    expect(updateParams(url, { ...form, kind: "text", value: "https://x" })).toEqual({ item_id: "i", kind: "text" });
  });
  it("creates from what was typed", () => {
    expect(createParams({ ...form, env: "", value: " FAKE\n", note: " " }, "p")).toEqual({
      project_id: "p",
      kind: "secret",
      name: "GitHub token",
      env_name: "GITHUB_TOKEN",
      description: null,
      value: "FAKE",
    });
  });
  it("names the problem", () => {
    expect(formProblem(form, null, true)).toBe(GONE);
    expect(formProblem({ ...form, name: "" }, null, false)).toBe("Give the item a name.");
    expect(formProblem(form, item(), true)).toBeNull();
    expect(deleteTitle(item())).toBe("Delete GitHub token (GITHUB_TOKEN)?");
    expect(deleteTitle(null)).toBe("Delete this item?");
  });
});

describe("audit", () => {
  it("words a row", () => {
    expect(actionText({ action: "edit", changed_fields: ["name", "env_name", "description"] })).toBe(
      "Edited · name, key, note",
    );
    expect(actionText({ action: "edit", changed_fields: ["kind"] })).toBe("Edited · type");
    expect(actionText({ action: "replace", changed_fields: ["kind", "value"] })).toBe("Replaced · type");
    expect(actionText({ action: "replace", changed_fields: ["value"] })).toBe("Replaced");
    expect(actionText({ action: "key_export" })).toBe("Exported the key");
  });
  it("says who and through what", () => {
    expect(who({ type: "board_member", user: { id: "u", username: "Sam" } }, "u")).toBe("You");
    expect(who({ type: "board_member", user: { id: "v", username: "Sam" } }, "u")).toBe("Sam");
    expect(who({ type: "employee", employee_id: "e", name: "Forge" }, "u")).toBe("Forge (employee)");
    const row = { id: 1, action: "reveal", at_ms: 0 } as const;
    expect(
      via({
        ...row,
        via: "Desktop · dev-laptop (Linux)",
        actor: { type: "board_member", user: { id: "u", username: "S" } },
      }),
    ).toBe("Desktop · dev-laptop (Linux)");
    expect(via({ ...row, actor: { type: "employee", employee_id: "e", name: "F", ticket_id: "t" } })).toBe(
      "Ticket · run",
    );
    expect(via({ ...row, actor: { type: "employee", employee_id: "e", name: "F", run_id: "r" } })).toBe("Run");
  });
  it("merges a page", () => {
    const r = (id: number) =>
      ({
        id,
        action: "reveal",
        at_ms: 0,
        actor: { type: "board_member", user: { id: "u", username: "S" } },
      }) as const;
    expect(mergeAudit([r(9)], [r(9), r(8)], 9).map((x) => x.id)).toEqual([9, 8]);
    expect(mergeAudit([r(9)], [r(10)], null).map((x) => x.id)).toEqual([10]);
  });
});

describe("scopes and keys", () => {
  it("lists scopes", () => {
    const list = [
      { id: "web", name: "web", status: "active" },
      { id: "old", name: "old", status: "archived" },
      { id: "gone", name: "gone", status: "archived" },
    ];
    expect(scopes(list, [{ count: 6 }, { project_id: "old", count: 2 }]).map((s) => [s.id, s.count])).toEqual([
      [null, 6],
      ["web", 0],
      ["old", 2],
    ]);
    expect(scopes([], [])[0]).toEqual({ id: null, name: "Company", count: 0 });
  });
  it("names a key file", () => {
    expect(keyEnv("aurelius-vault-key:v1:prod:ab12cd34:x")).toBe("prod");
    expect(keyEnv("aurelius-vault-key:v2:prod:x:y")).toBeNull();
    expect(keyEnv("aurelius-vault-key:v1:qa:x:y")).toBeNull();
    expect(keyFileName("ab12-cd34/../x", "c2st")).toBe("aurelius-vault-key-ab12cd34x.txt");
    expect(keyFileName("ab12-cd34/../x", "aurelius-vault-key:v1:prod:a:b")).toBe(
      "aurelius-vault-key-prod-ab12cd34x.txt",
    );
  });
});
