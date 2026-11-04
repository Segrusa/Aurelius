// What the Vault computes, without a screen. No value is kept here. Port of screens/vault.rs and the
// checks of crates/domain/src/vault.rs: they are said before Save, the server decides.

import type {
  VaultActor,
  VaultAuditAction,
  VaultAuditRow,
  VaultCreateParams,
  VaultItemView,
  VaultKind,
  VaultScopeCount,
  VaultUpdateParams,
} from "@aurelius/protocol";

export const MASK = "••••••••";
export const SHOW_MS = 30_000;
export const PLUGINS_SEE_SECRETS = "Plugins and MCP servers of an employee see the secrets of its runs.";
export const LINUX_KEY_NOTE =
  "On Linux, any process of the daemon's user, also an agent through Bash, can read the key.";
export const ALL_READ =
  "All employees can read this item. Secrets reach runs only as an environment variable, never in a prompt, memory or discussion.";
export const SHOW_TIP = "Shows the value for 30 s. This is logged.";
export const NOT_SECRET = "Not secret: shown to everyone and not redacted. Use Secret for keys and tokens.";
export const BAD_URL = "Enter a full URL, starting with https://";
export const GONE = "This item was deleted or changed elsewhere.";
export const KEY_ABOUT =
  "Restoring a backup on another machine needs this key: without it the secrets cannot be decrypted. Keep it apart from the backups. Every export is logged and every board member is told.";

const MAX_NAME = 80;
const MAX_ENV = 64;
const MAX_VALUE_BYTES = 64 * 1024;
const MIN_SECRET = 8;

const RESERVED = new Set(
  "PATH PATHEXT SYSTEMROOT WINDIR COMSPEC TEMP TMP USERPROFILE HOMEDRIVE HOMEPATH APPDATA LOCALAPPDATA PROGRAMDATA USERNAME USERDOMAIN HOME USER LOGNAME SHELL TMPDIR LANG LC_ALL XDG_CONFIG_HOME XDG_DATA_HOME XDG_CACHE_HOME XDG_RUNTIME_DIR CARGO_BUILD_JOBS PWD OLDPWD TERM IFS PS1 PS4 ENV BASH_ENV ZDOTDIR PROMPT_COMMAND SHELLOPTS BASHOPTS NODE_OPTIONS PYTHONPATH PYTHONSTARTUP REQUESTS_CA_BUNDLE CURL_CA_BUNDLE".split(
    " ",
  ),
);
const RESERVED_PREFIXES =
  "LD_ DYLD_ LC_ XDG_ ANTHROPIC_ CLAUDE_ AURELIUS_ VERTEX_ BEDROCK_ OTEL_ MCP_ NODE_ SSL_CERT_".split(" ");

export function envReserved(name: string): boolean {
  const upper = name.toUpperCase();
  return RESERVED.has(upper) || RESERVED_PREFIXES.some((p) => upper.startsWith(p)) || upper.endsWith("_PROXY");
}

// biome-ignore lint/suspicious/noControlCharactersInRegex: a name is one line of printable text.
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;

export function checkName(name: string): string | null {
  const clean = name.trim();
  if (clean === "") return "Give the item a name.";
  return [...clean].length > MAX_NAME || CONTROL.test(clean)
    ? `A name has at most ${MAX_NAME} characters, on one line.`
    : null;
}

export function checkEnvName(env: string): string | null {
  if (!/^[A-Z][A-Z0-9_]*$/.test(env) || env.length > MAX_ENV) {
    return "A key is A–Z, 0–9 and _, starting with a letter (like GITHUB_TOKEN).";
  }
  return envReserved(env) ? `${env} is kept by Aurelius or Claude Code for every run. Choose another key.` : null;
}

export function checkValue(kind: VaultKind, value: string): string | null {
  if (new TextEncoder().encode(value).length > MAX_VALUE_BYTES) return "A value has at most 64 KB.";
  if (kind === "text") return value === "" ? "Give the item a value." : null;
  if (kind === "url") {
    const rest = value.startsWith("https://") ? value.slice(8) : value.startsWith("http://") ? value.slice(7) : null;
    return rest && !rest.startsWith("/") && !/\s/.test(value) ? null : BAD_URL;
  }
  if ([...value].length < MIN_SECRET) return `A secret has at least ${MIN_SECRET} characters.`;
  return /[\n\r]/.test(value) ? "A secret is one line." : null;
}

/** The key a name suggests: `GitHub token` → `GITHUB_TOKEN`. */
export function envFromName(name: string): string {
  let out = "";
  for (const ch of name.trim()) {
    if (/^[a-zA-Z0-9]$/.test(ch)) out += ch.toUpperCase();
    else if (out !== "" && !out.endsWith("_")) out += "_";
  }
  return out
    .replace(/^[^A-Z]+/, "")
    .slice(0, MAX_ENV)
    .replace(/_+$/, "");
}

/** What may be chosen: a secret stays a secret, and nothing becomes one. */
export function kindsFor(current: VaultKind | null): VaultKind[] {
  if (current === null) return ["text", "url", "secret"];
  return current === "secret" ? ["secret"] : ["text", "url"];
}

export const KIND_LABEL: Record<VaultKind, string> = { text: "Text", url: "URL", secret: "Secret" };
export const KIND_TAG: Record<VaultKind, string> = { text: "¶ Text", url: "🔗 URL", secret: "🔒 Secret" };

export function hidesIn(until: number, now: number): string | null {
  const left = Math.ceil((until - now) / 1000);
  if (left <= 0) return null;
  return `Hides in ${Math.floor(left / 60)}:${String(left % 60).padStart(2, "0")}`;
}

export interface Form {
  name: string;
  env: string;
  kind: VaultKind;
  value: string;
  note: string;
}

export function formProblem(form: Form, item: VaultItemView | null, editing: boolean): string | null {
  if (editing && !item) return GONE;
  const env = form.env.trim() || envFromName(form.name);
  const keeping = item !== null && item.kind === "secret" && form.kind === "secret";
  const value = form.value.trim();
  return checkName(form.name) ?? checkEnvName(env) ?? (keeping && value === "" ? null : checkValue(form.kind, value));
}

export function createParams(form: Form, projectId: string | null): VaultCreateParams {
  return {
    ...(projectId ? { project_id: projectId } : {}),
    kind: form.kind,
    name: form.name.trim(),
    env_name: form.env.trim() || envFromName(form.name),
    description: form.note.trim() || null,
    value: form.value.trim(),
  };
}

/** Only what changed; null when nothing did. An empty value keeps a secret. */
export function updateParams(item: VaultItemView, form: Form): VaultUpdateParams | null {
  const out: VaultUpdateParams = { item_id: item.id };
  const [name, env, note, value] = [form.name.trim(), form.env.trim(), form.note.trim(), form.value.trim()];
  if (name !== item.name) out.name = name;
  if (env !== "" && env !== item.env_name) out.env_name = env;
  if (note !== (item.description ?? "")) out.description = note;
  if (form.kind !== item.kind) out.kind = form.kind;
  if (item.kind === "secret" ? value !== "" : value !== (item.value ?? "")) out.value = value;
  return Object.keys(out).length > 1 ? out : null;
}

const ACTIONS: Record<VaultAuditAction, string> = {
  create: "Created",
  edit: "Edited",
  replace: "Replaced",
  archive: "Deleted",
  reveal: "Revealed",
  copy: "Copied",
  deliver: "Read by run",
  tool_read: "Read by tool",
  key_export: "Exported the key",
};

const FIELD: Record<string, string> = { name: "name", env_name: "key", description: "note", kind: "type" };

export function actionText(row: Pick<VaultAuditRow, "action" | "changed_fields">): string {
  const fields = (row.changed_fields ?? []).flatMap((f) => FIELD[f] ?? []);
  return fields.length > 0 ? `${ACTIONS[row.action]} · ${fields.join(", ")}` : ACTIONS[row.action];
}

export function who(actor: VaultActor, me: string | null): string {
  if (actor.type === "employee") return `${actor.name} (employee)`;
  return actor.user.id === me ? "You" : actor.user.username;
}

export function via(row: VaultAuditRow): string {
  if (row.via) return row.via;
  if (row.actor.type !== "employee") return "";
  if (row.actor.ticket_id) return "Ticket · run";
  return row.actor.run_id ? "Run" : "";
}

export function mergeAudit(rows: VaultAuditRow[], page: VaultAuditRow[], before: number | null): VaultAuditRow[] {
  if (before === null) return page;
  const seen = new Set(rows.map((r) => r.id));
  return [...rows, ...page.filter((r) => !seen.has(r.id))];
}

export interface Scope {
  id: string | null;
  name: string;
  count: number;
}

/** Company first, then active projects, then archived projects that still hold something. */
export function scopes(projects: { id: string; name: string; status: string }[], counts: VaultScopeCount[]): Scope[] {
  const of = (id: string | null) => counts.find((c) => (c.project_id ?? null) === id)?.count ?? 0;
  return [
    { id: null, name: "Company", count: of(null) },
    ...projects
      .filter((p) => p.status !== "archived" || of(p.id) > 0)
      .map((p) => ({ id: p.id, name: p.name, count: of(p.id) })),
  ];
}

export const deleteTitle = (item: VaultItemView | null) =>
  item ? `Delete ${item.name} (${item.env_name})?` : "Delete this item?";

export function keyFileName(keyId: string, key: string): string {
  const id = keyId.replace(/[^a-zA-Z0-9]/g, "").slice(0, 16);
  const env = keyEnv(key);
  return env ? `aurelius-vault-key-${env}-${id}.txt` : `aurelius-vault-key-${id}.txt`;
}

export function keyEnv(key: string): string | null {
  const parts = key.split(":");
  return parts[0] === "aurelius-vault-key" && parts[1] === "v1" && (parts[2] === "prod" || parts[2] === "dev")
    ? parts[2]
    : null;
}
