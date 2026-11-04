// The Vault. A secret's value lives only in `shown` below, for at most 30 seconds, and in the editor's
// field while it is open. It goes through no query cache, no log and no retry.

import { expectBody, ProtocolError } from "@aurelius/client";
import type { VaultAuditRow, VaultItemView, VaultKind } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useProjects, useVault, useVaultAudit } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { Button, initial, Mark, markColor } from "../ui/kit";
import { Dialog } from "../ui/overlay";
import { toast } from "../ui/toast";
import { AREA, Confirm, EYEBROW, FIELD, hhmm, KnowledgeTabs, PageHead, SMALL, shortDay } from "./org-kit";
import {
  ALL_READ,
  actionText,
  createParams,
  deleteTitle,
  envFromName,
  type Form,
  formProblem,
  hidesIn,
  KEY_ABOUT,
  KIND_LABEL,
  KIND_TAG,
  keyEnv,
  keyFileName,
  kindsFor,
  LINUX_KEY_NOTE,
  MASK,
  mergeAudit,
  NOT_SECRET,
  PLUGINS_SEE_SECRETS,
  SHOW_MS,
  SHOW_TIP,
  scopes,
  updateParams,
  via,
  who,
} from "./vault-logic";

/** Never holds a value: only what the server answered in general terms. */
function failureText(e: unknown): string {
  if (e instanceof ProtocolError) {
    switch (e.code) {
      case "vault_kind_locked":
        return "A secret stays a secret, and nothing becomes one. Make a new item instead.";
      case "vault_undecryptable":
        return "Cannot be decrypted: the server's vault key is missing or does not fit this value.";
      case "version_conflict":
        return "Changed on another device. Showing the latest.";
      case "wrong_password":
        return "That password is wrong.";
      default:
        return e.message;
    }
  }
  return "Not connected to the server. Try again when it is back.";
}

function usePlatformCopyNote() {
  const platform = usePlatform();
  return useCallback(
    async (value: string) => {
      const { sure } = await platform.copySecret(value);
      toast(sure ? "Copied. Clears in 60 s." : "Copied. A browser may keep it on the clipboard: clear it yourself.");
    },
    [platform],
  );
}

function Editor({
  item,
  projectId,
  scopeName,
  onClose,
}: {
  item: VaultItemView | null;
  projectId: string | null;
  scopeName: string;
  onClose: () => void;
}) {
  const connection = useConnection();
  const client = useQueryClient();
  const [form, setForm] = useState<Form>({
    name: item?.name ?? "",
    env: item?.env_name ?? "",
    kind: item?.kind ?? "text",
    value: item && item.kind !== "secret" ? (item.value ?? "") : "",
    note: item?.description ?? "",
  });
  const [reveal, setReveal] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const change = (patch: Partial<Form>) => {
    setForm({ ...form, ...patch });
    setError(null);
  };
  const close = () => {
    setForm({ name: "", env: "", kind: "text", value: "", note: "" });
    onClose();
  };
  const refresh = () => client.invalidateQueries({ queryKey: ["vault"] });
  const save = async () => {
    if (busy) return;
    const problem = formProblem(form, item, item !== null);
    if (problem) return setError(problem);
    setBusy(true);
    try {
      // Never kept for a reconnect: the parameters hold the value.
      if (item) {
        const params = updateParams(item, form);
        if (params) await connection.write("vault.update", params, { retry: false });
      } else {
        await connection.write("vault.create", createParams(form, projectId), { retry: false });
      }
      await refresh();
      toast(`Saved ${form.name.trim()}.`);
      close();
    } catch (e) {
      setError(failureText(e));
      setBusy(false);
    }
  };
  useKeyContext("AureliusDialog", true);
  useAction("ConfirmPrimary", () => void save());
  const remove = async () => {
    if (!item) return;
    try {
      await connection.write("vault.archive", { item_id: item.id }, { retry: false });
      await refresh();
      toast(`Deleted ${item.name}. Runs no longer get ${item.env_name}.`);
      close();
    } catch (e) {
      setDeleting(false);
      setError(failureText(e));
    }
  };
  const envHint = form.env === "" && form.name ? `${envFromName(form.name)} · ` : "";
  const isSecret = form.kind === "secret";
  const title = item ? `${item.name} · ${scopeName}` : `New item · ${scopeName}`;
  const kinds = kindsFor(item?.kind ?? null);
  const label = "grid grid-cols-[70px_1fr] items-start gap-2 text-[12px] text-text3";
  return (
    <Dialog open onClose={close} title={title} width={560}>
      <div className="flex max-h-[70vh] flex-col gap-3 overflow-y-auto p-4">
        <p className="m-0 text-[17px] font-semibold">{title}</p>
        <div className={label}>
          <span className="pt-1">Type</span>
          <div className="flex flex-col gap-1">
            <div className="flex gap-1">
              {kinds.map((k: VaultKind) => (
                <button
                  key={k}
                  type="button"
                  aria-pressed={form.kind === k}
                  onClick={() => change({ kind: k })}
                  className={`h-[26px] cursor-pointer rounded-[6px] border px-2 text-[12px] ${form.kind === k ? "border-btn-line bg-sel text-text" : "border-line bg-transparent text-text2"}`}
                >
                  {KIND_LABEL[k]}
                </button>
              ))}
            </div>
            {item?.kind === "secret" ? (
              <span>A secret stays a secret. Make a new item to change its type.</span>
            ) : item ? (
              <span>Text and URL trade places; a secret needs a new item.</span>
            ) : null}
          </div>
        </div>
        <label className={label}>
          <span className="pt-2">Name *</span>
          <input
            data-autofocus
            placeholder="GitHub token"
            className={FIELD}
            value={form.name}
            onChange={(e) => change({ name: e.target.value })}
          />
        </label>
        <label className={label}>
          <span className="pt-2">Key *</span>
          <span className="flex flex-col gap-1">
            <input
              placeholder="From the name"
              className={`${FIELD} font-mono`}
              value={form.env}
              onChange={(e) => change({ env: e.target.value })}
            />
            <span>{envHint}A–Z, 0–9 and _ · unique in this scope</span>
            {item && form.env.trim() !== "" && form.env.trim() !== item.env_name ? (
              <span className="text-amber">Runs that use {item.env_name} will no longer find it.</span>
            ) : null}
          </span>
        </label>
        <div className={label}>
          <span className="pt-2">Value *</span>
          <span className="flex flex-col gap-1">
            {isSecret ? (
              <span className="flex gap-1">
                <input
                  autoComplete="off"
                  spellCheck={false}
                  aria-label="Value"
                  type={reveal ? "text" : "password"}
                  placeholder={item ? "Leave empty to keep the current secret" : ""}
                  className={`${FIELD} font-mono`}
                  value={form.value}
                  onChange={(e) => change({ value: e.target.value })}
                />
                <Button
                  aria-label={reveal ? "Hide what you type" : "Show what you type"}
                  onClick={() => setReveal(!reveal)}
                >
                  {reveal ? "Hide" : "Show"}
                </Button>
              </span>
            ) : form.kind === "url" ? (
              <input
                className={`${FIELD} font-mono`}
                value={form.value}
                onChange={(e) => change({ value: e.target.value })}
              />
            ) : (
              <textarea
                rows={3}
                className={AREA}
                value={form.value}
                onChange={(e) => change({ value: e.target.value })}
              />
            )}
            {isSecret ? (
              <span>At least 8 characters. It is never shown again here.</span>
            ) : form.kind === "url" ? (
              <span>https://… · {NOT_SECRET}</span>
            ) : (
              <span>{NOT_SECRET}</span>
            )}
          </span>
        </div>
        <label className={label}>
          <span className="pt-2">Note</span>
          <input
            placeholder="Employees see this; never a secret"
            className={FIELD}
            value={form.note}
            onChange={(e) => change({ note: e.target.value })}
          />
        </label>
        <p className="m-0 text-[12px] text-text3">{ALL_READ}</p>
        {error ? (
          <p role="alert" className="m-0 text-red">
            {error}
          </p>
        ) : null}
      </div>
      <div className="flex gap-2 border-t border-line2 px-4 py-3">
        {item ? (
          <Button kind="danger" onClick={() => setDeleting(true)}>
            Delete…
          </Button>
        ) : null}
        <Button className="ml-auto" onClick={close}>
          Cancel
        </Button>
        <Button kind="primary" disabled={busy} onClick={() => void save()}>
          {busy ? "Saving…" : "Save"}
        </Button>
      </div>
      {deleting ? (
        <Confirm
          danger
          title={deleteTitle(item)}
          body="Runs and employees lose access, and runs that start from now on no longer get it. A run that already got it keeps it until it ends. This is logged."
          action="Delete"
          onClose={() => setDeleting(false)}
          onConfirm={() => void remove()}
        />
      ) : null}
    </Dialog>
  );
}

function KeyExport({ onClose }: { onClose: () => void }) {
  const connection = useConnection();
  const platform = usePlatform();
  const copy = usePlatformCopyNote();
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [key, setKey] = useState<{ id: string; value: string } | null>(null);
  const [until, setUntil] = useState(0);
  const [now, setNow] = useState(() => Date.now());
  const saving = useRef(false);
  const hide = useCallback(() => {
    setKey(null);
    onClose();
  }, [onClose]);
  useEffect(() => {
    if (!key) return;
    const tick = setInterval(() => {
      if (saving.current) {
        // A native dialog is up: the countdown waits for it.
        setUntil((u) => u + 500);
        return;
      }
      setNow(Date.now());
      if (Date.now() >= until) hide();
    }, 500);
    const onBlur = () => {
      if (!saving.current) hide();
    };
    window.addEventListener("blur", onBlur);
    return () => {
      clearInterval(tick);
      window.removeEventListener("blur", onBlur);
    };
  }, [key, until, hide]);
  const run = async () => {
    if (busy || password === "") return;
    setBusy(true);
    const typed = password;
    // Gone on any answer, right or wrong.
    setPassword("");
    try {
      const body = expectBody(await connection.call("vault.key_export", { password: typed }), "vault_key").value;
      setKey({ id: body.key_id, value: body.key });
      setUntil(Date.now() + SHOW_MS);
      setNow(Date.now());
    } catch (e) {
      setError(failureText(e));
    } finally {
      setBusy(false);
    }
  };
  const save = async () => {
    if (!key) return;
    saving.current = true;
    try {
      const saved = await platform.saveSecretFile(keyFileName(key.id, key.value), key.value);
      if (saved)
        toast(
          saved.private
            ? `Key saved to ${saved.path} (only you can read it).`
            : "Key downloaded. A download can be read by others on this machine: move it somewhere safe.",
        );
    } catch (e) {
      toast(`Could not save the key: ${e instanceof Error ? e.message : "it failed"}`);
    } finally {
      saving.current = false;
    }
  };
  const env = key ? keyEnv(key.value) : null;
  return (
    <Dialog open onClose={hide} title="Export the vault key" width={520}>
      <div className="flex flex-col gap-3 p-4">
        <p className="m-0 text-[17px] font-semibold">Export the vault key</p>
        <p className="m-0 leading-[1.5] text-text2">{KEY_ABOUT}</p>
        {key ? (
          <>
            <p className={EYEBROW}>
              Key {key.id}
              {env ? ` · ${env}` : ""} · shown once
            </p>
            <section aria-label="The vault key, shown once">
              <pre className="m-0 overflow-x-auto rounded-[6px] border border-line bg-sunk p-3 font-mono text-[12px] leading-[17px] break-all whitespace-pre-wrap">
                {key.value}
              </pre>
            </section>
            <p role="status" className="m-0 text-[12px] text-text3">
              {hidesIn(until, now)}
            </p>
          </>
        ) : (
          <label className="flex flex-col gap-1 text-[12px] text-text3">
            Password
            <input
              data-autofocus
              autoComplete="current-password"
              type="password"
              placeholder="Your password"
              className={FIELD}
              value={password}
              onChange={(e) => {
                setPassword(e.target.value);
                setError(null);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  void run();
                }
              }}
            />
            <span>Type your password: exporting the key asks it again.</span>
          </label>
        )}
        {error ? (
          <p role="alert" className="m-0 text-red">
            {error}
          </p>
        ) : null}
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        {key ? (
          <>
            <Button onClick={() => void copy(key.value)}>Copy</Button>
            <Button onClick={() => void save()}>Save to file…</Button>
            <Button kind="primary" onClick={hide}>
              Done
            </Button>
          </>
        ) : (
          <>
            <Button onClick={hide}>Cancel</Button>
            <Button kind="primary" disabled={busy || password === ""} onClick={() => void run()}>
              {busy ? "Exporting…" : "Export key"}
            </Button>
          </>
        )}
      </div>
    </Dialog>
  );
}

function AuditLog({ projectId, all }: { projectId: string | null; all: boolean }) {
  const connection = useConnection();
  const { user } = useLinkState();
  const first = useVaultAudit(projectId, all, true);
  const [older, setOlder] = useState<VaultAuditRow[]>([]);
  const [more, setMore] = useState<boolean | null>(null);
  const rows = useMemo(() => mergeAudit(first.data?.rows ?? [], older, -1), [first.data, older]);
  const load = async () => {
    const before = rows.at(-1)?.id;
    if (before === undefined) return;
    const page = expectBody(
      await connection.call("vault.audit", { ...(projectId ? { project_id: projectId } : {}), all, before, limit: 50 }),
      "vault_audit",
    ).value;
    setOlder((now) => [...now, ...page.rows]);
    setMore(page.more);
  };
  const grid = "grid grid-cols-[110px_170px_130px_150px_1fr] items-center gap-3 px-[22px]";
  return (
    <div className="min-w-[700px]">
      <div className={`${grid} h-8 border-b border-line2 text-[12px] text-text3`}>
        <span>Time</span>
        <span>Who</span>
        <span>Action</span>
        <span>Item</span>
        <span>Via</span>
      </div>
      {first.isPending ? <p className="m-0 px-[22px] py-4 text-text3">Loading…</p> : null}
      {first.data && rows.length === 0 ? (
        <p className="m-0 px-[22px] py-4 text-text3">Nothing in the audit log yet.</p>
      ) : null}
      {rows.map((r) => (
        <div key={r.id} className={`${grid} min-h-[34px] border-b border-line2 text-[12px]`}>
          <span className="text-text3">
            {shortDay(r.at_ms)} {hhmm(r.at_ms)}
          </span>
          <span className="truncate">{who(r.actor, user?.id ?? null)}</span>
          <span className="truncate">{actionText(r)}</span>
          <span className="truncate">{r.item_name ?? "—"}</span>
          <span className="truncate text-text3">{via(r)}</span>
        </div>
      ))}
      {(more ?? first.data?.more) ? (
        <Button className={`${SMALL} m-3`} onClick={() => void load()}>
          Load older
        </Button>
      ) : null}
      <p className="m-0 px-[22px] py-3 text-[12px] text-text3">The audit log is read-only and never shows a value.</p>
    </div>
  );
}

export function VaultBody({ projectId, scopeName }: { projectId: string | null; scopeName: string }) {
  const connection = useConnection();
  const platform = usePlatform();
  const copyNote = usePlatformCopyNote();
  const { user, hostName } = useLinkState();
  const list = useVault(projectId);
  const [tab, setTab] = useState<"items" | "audit">("items");
  const [editing, setEditing] = useState<VaultItemView | "new" | null>(null);
  const [shown, setShown] = useState<{ id: string; value: string; until: number } | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [companyOpen, setCompanyOpen] = useState(false);
  const asked = useRef(0);
  const hide = useCallback(() => {
    asked.current++;
    setShown(null);
  }, []);
  // Hidden when its 30 seconds are over, on another scope or member, when leaving, and when the window loses focus.
  // biome-ignore lint/correctness/useExhaustiveDependencies: a change of scope or member is what hides it.
  useEffect(() => hide, [hide, projectId, user?.id, hostName]);
  useEffect(() => {
    window.addEventListener("blur", hide);
    const onVisible = () => {
      if (document.visibilityState !== "visible") hide();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      window.removeEventListener("blur", hide);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [hide]);
  useEffect(() => {
    if (!shown) return;
    const tick = setInterval(() => {
      setNow(Date.now());
      if (Date.now() >= shown.until) hide();
    }, 500);
    return () => clearInterval(tick);
  }, [shown, hide]);
  const show = async (item: VaultItemView) => {
    const ticket = ++asked.current;
    try {
      const body = expectBody(
        await connection.call("vault.reveal", { item_id: item.id, purpose: "show" }),
        "vault_value",
      ).value;
      // An answer that comes after leaving is dropped.
      if (ticket !== asked.current) return;
      setNow(Date.now());
      setShown({ id: item.id, value: body.value, until: Date.now() + SHOW_MS });
    } catch (e) {
      toast(failureText(e));
    }
  };
  const copy = async (item: VaultItemView) => {
    try {
      const body = expectBody(
        await connection.call("vault.reveal", { item_id: item.id, purpose: "copy" }),
        "vault_value",
      ).value;
      await copyNote(body.value);
    } catch (e) {
      toast(failureText(e));
    }
  };
  const items = list.data?.items ?? [];
  const company = list.data?.company_items ?? [];
  const small =
    "h-[22px] cursor-pointer rounded-[5px] border border-btn-line bg-btn px-[7px] text-[11.5px] text-text hover:bg-sel";
  const grid = "grid grid-cols-[1.2fr_1fr_100px_1.2fr_120px] items-center gap-3 px-[22px]";
  const row = (item: VaultItemView, readOnly = false) => {
    const open = shown?.id === item.id ? shown : null;
    return (
      <div key={item.id} className={`${grid} min-h-[38px] border-b border-line2 text-[13px]`}>
        <button
          type="button"
          aria-label={`Edit ${item.name}`}
          disabled={readOnly}
          title={item.description ?? undefined}
          onClick={() => setEditing(item)}
          className={`min-w-0 truncate border-0 bg-transparent p-0 text-left font-medium text-text ${readOnly ? "" : "cursor-pointer hover:underline"}`}
        >
          {item.name}
          {item.overrides_company ? (
            <span className="ml-2 text-[11px] font-normal text-text3">overrides Company</span>
          ) : null}
        </button>
        <code className="truncate font-mono text-[12px] text-text2">{item.env_name}</code>
        <span className="text-[12px] text-text3">{KIND_TAG[item.kind]}</span>
        <span className="flex min-w-0 flex-wrap items-center gap-2 py-[6px]">
          {item.undecryptable ? (
            <span className="text-[12px] text-red">Cannot be decrypted</span>
          ) : item.kind === "url" && item.value && /^https?:\/\//i.test(item.value) ? (
            <a
              href={item.value}
              target="_blank"
              rel="noreferrer"
              className="truncate font-mono text-[12px] text-acc-text"
            >
              {item.value}
            </a>
          ) : item.kind !== "secret" ? (
            <span className="truncate font-mono text-[12px] text-text2">{(item.value ?? "").split("\n")[0]}</span>
          ) : (
            <>
              <span className={`min-w-0 font-mono text-[12px] ${open ? "w-full break-all" : "truncate"}`}>
                {open ? open.value : MASK}
              </span>
              <button
                type="button"
                aria-label={`${open ? "Hide" : "Show"} ${item.name}`}
                title={SHOW_TIP}
                className={small}
                onClick={() => (open ? hide() : void show(item))}
              >
                {open ? "Hide" : "Show"}
              </button>
              <button
                type="button"
                aria-label={`Copy ${item.name}`}
                title="Copies without showing it, and clears the clipboard after 60 s. This is logged."
                className={small}
                onClick={() => void copy(item)}
              >
                Copy
              </button>
              {open ? (
                <span className="whitespace-nowrap text-[11px] text-text3">{hidesIn(open.until, now)}</span>
              ) : null}
            </>
          )}
        </span>
        <span className="truncate text-[12px] text-text3">
          {item.updated_by?.id === user?.id ? "You" : (item.updated_by?.username ?? "")} ·{" "}
          {shortDay(item.updated_at_ms)}
        </span>
      </div>
    );
  };
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <div className="flex items-center gap-3 px-[22px] pt-[18px]">
        <h2 className="m-0 text-[18px] font-semibold">{scopeName}</h2>
        <span className="text-[12px] text-text3">
          · {items.length} item{items.length === 1 ? "" : "s"}
        </span>
        <Button kind="primary" className={`${SMALL} ml-auto`} onClick={() => setEditing("new")}>
          New item
        </Button>
      </div>
      <div role="tablist" aria-label="Vault" className="flex gap-[18px] border-b border-line2 px-[22px] pt-[10px]">
        {(["items", "audit"] as const).map((t) => (
          <button
            key={t}
            type="button"
            role="tab"
            aria-selected={tab === t}
            onClick={() => setTab(t)}
            className={`-mb-px h-[34px] cursor-pointer border-0 border-b-2 bg-transparent px-0 text-[13px] font-medium ${tab === t ? "border-text text-text" : "border-transparent text-text2"}`}
          >
            {t === "items" ? "Items" : "Audit log"}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {tab === "audit" ? (
          <AuditLog projectId={projectId} all={false} />
        ) : list.isPending ? (
          <p className="m-0 p-6 text-text3">Loading…</p>
        ) : list.error ? (
          <p role="alert" className="m-0 p-6 text-red">
            {failureText(list.error)}
          </p>
        ) : (
          <div className="min-w-[760px]">
            {items.length > 0 ? (
              <>
                <div className={`${grid} h-8 border-b border-line2 text-[12px] text-text3`}>
                  <span>Name</span>
                  <span>Key</span>
                  <span>Type</span>
                  <span>Value</span>
                  <span>Updated</span>
                </div>
                {items.map((i) => row(i))}
              </>
            ) : (
              <div className="mx-auto flex max-w-[420px] flex-col items-center gap-3 p-10 text-center">
                <p className="m-0 text-[15px] font-semibold">No items in {scopeName} yet</p>
                <p className="m-0 leading-[1.6] text-text2">
                  {projectId ? "Employees of this project" : "Every employee"} can read every item here.
                </p>
                <Button kind="primary" onClick={() => setEditing("new")}>
                  New item
                </Button>
              </div>
            )}
            {company.length > 0 ? (
              <>
                <button
                  type="button"
                  aria-expanded={companyOpen}
                  onClick={() => setCompanyOpen(!companyOpen)}
                  className="cursor-pointer border-0 bg-transparent px-[22px] py-3 text-[12px] text-text3 hover:text-text"
                >
                  {companyOpen ? "▾" : "▸"} Company items also available here: {company.length}
                </button>
                {companyOpen ? company.map((i) => row(i, true)) : null}
              </>
            ) : null}
            <ul className="mx-[22px] my-[18px] flex list-none flex-col gap-1 rounded-[8px] border border-amber-line bg-amber-soft p-0 px-[14px] py-3 text-[12px] text-text2">
              <li>{PLUGINS_SEE_SECRETS}</li>
              {platform.kind === "electron" ? <li>{LINUX_KEY_NOTE}</li> : null}
            </ul>
          </div>
        )}
      </div>
      {editing ? (
        <Editor
          item={editing === "new" ? null : ((items.find((i) => i.id === editing.id) ?? editing) as VaultItemView)}
          projectId={projectId}
          scopeName={scopeName}
          onClose={() => setEditing(null)}
        />
      ) : null}
      <p role="status" className="sr-only">
        {shown ? "Shown for 30 seconds" : "Hidden"}
      </p>
    </div>
  );
}

export function VaultScreen() {
  const { features } = useLinkState();
  const projects = useProjects();
  const [scope, setScope] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const counts = useVault(null);
  const all = useMemo(() => scopes(projects.data ?? [], counts.data?.counts ?? []), [projects.data, counts.data]);
  const current = all.find((s) => s.id === scope) ?? all[0];
  if (!features.includes("vault")) {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <PageHead title="Knowledge" tabs={<KnowledgeTabs />} />
        <p className="m-0 p-6 text-text2">This server does not have the vault yet. Update the server.</p>
      </div>
    );
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHead title="Knowledge" tabs={<KnowledgeTabs />}>
        <Button className={SMALL} onClick={() => setExporting(true)}>
          Export key…
        </Button>
      </PageHead>
      <div className="flex min-h-0 flex-1">
        <nav
          aria-label="Scope"
          className="flex w-[220px] flex-none flex-col gap-px overflow-y-auto border-r border-line bg-sunk px-2 py-3"
        >
          <p className="m-0 mb-1 px-2 text-[12px] font-medium text-text3">Scope</p>
          {all.map((s) => (
            <button
              key={s.id ?? "company"}
              type="button"
              aria-current={s.id === (current?.id ?? null) ? "true" : undefined}
              onClick={() => setScope(s.id)}
              className={`flex h-[30px] cursor-pointer items-center gap-[9px] rounded-[6px] border-0 px-2 text-left ${s.id === (current?.id ?? null) ? "bg-sel text-text" : "bg-transparent text-text2 hover:text-text"}`}
            >
              <Mark letter={initial(s.name)} color={s.id ? markColor(s.id) : "#3A3D44"} size={15} radius={4} />
              <span className="min-w-0 flex-1 truncate">{s.name}</span>
              <span className="text-[12px] text-text3">{s.count}</span>
            </button>
          ))}
        </nav>
        <VaultBody
          key={current?.id ?? "company"}
          projectId={current?.id ?? null}
          scopeName={current?.name ?? "Company"}
        />
      </div>
      {exporting ? <KeyExport onClose={() => setExporting(false)} /> : null}
    </div>
  );
}
