// The Vault. A secret's value lives only in `shown` below, for at most 30 seconds, and in the editor's
// field while it is open. It goes through no query cache, no log and no retry.

import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { List, ListItem } from "@astryxdesign/core/List";
import { Tab, TabList } from "@astryxdesign/core/TabList";
import { Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow } from "@astryxdesign/core/Table";
import { TextArea } from "@astryxdesign/core/TextArea";
import { TextInput } from "@astryxdesign/core/TextInput";
import { ToggleButton, ToggleButtonGroup } from "@astryxdesign/core/ToggleButton";
import { expectBody, ProtocolError } from "@aurelius/client";
import type { VaultAuditRow, VaultItemView, VaultKind } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useProjects, useVault, useVaultAudit } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { initial, Mark, markColor } from "../ui/marks";
import { toast } from "../ui/toast";
import {
  Confirm,
  Empty,
  Eyebrow,
  FormDialog,
  Hint,
  hhmm,
  KnowledgeTabs,
  Loading,
  Page,
  PageHead,
  Problem,
  shortDay,
} from "./org-kit";
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
  MCP_SEES_SECRETS,
  mergeAudit,
  NOT_SECRET,
  SHOW_MS,
  SHOW_TIP,
  scopes,
  updateParams,
  via,
  who,
} from "./vault-logic";

const PANEL = "vault-panel";

const styles = stylex.create({
  pushApart: { marginInlineEnd: "auto" },
  type: { display: "flex", flexDirection: "column", gap: 4 },
  typeLabel: { fontSize: 12, color: "var(--precision-text3)" },
  warning: { margin: 0, fontSize: 12, color: "var(--color-text-warning)" },
  secret: { display: "flex", alignItems: "flex-end", gap: 4 },
  grow: { minWidth: 0, flex: 1 },
  mono: { fontFamily: "var(--font-family-code)" },
  keyValue: {
    margin: 0,
    overflowX: "auto",
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    padding: 12,
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    lineHeight: "17px",
    wordBreak: "break-all",
    whiteSpace: "pre-wrap",
  },
  about: { margin: 0, lineHeight: 1.5, color: "var(--color-text-secondary)" },
  status: { margin: 0, fontSize: 12, color: "var(--precision-text3)" },
  auditTable: { minWidth: 700 },
  itemsTable: { minWidth: 760 },
  headCell: { fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
  timeColumn: { width: 110, minWidth: 110, maxWidth: 110 },
  whoColumn: { width: 170, minWidth: 170, maxWidth: 170 },
  actionColumn: { width: 130, minWidth: 130, maxWidth: 130 },
  itemColumn: { width: 150, minWidth: 150, maxWidth: 150 },
  typeColumn: { width: 100, minWidth: 100, maxWidth: 100 },
  updatedColumn: { width: 120, minWidth: 120, maxWidth: 120 },
  cellText: { display: "block", overflow: "hidden", fontSize: 12, whiteSpace: "nowrap", textOverflow: "ellipsis" },
  dim: { color: "var(--precision-text3)" },
  more: { margin: 12 },
  note: { margin: 0, paddingInline: 22, paddingBlock: 12, fontSize: 12, color: "var(--precision-text3)" },
  empty: { margin: 0, paddingInline: 22, paddingBlock: 16, color: "var(--precision-text3)" },
  name: {
    display: "block",
    maxWidth: "100%",
    overflow: "hidden",
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    textAlign: "start",
    fontWeight: 500,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--color-text-primary)",
  },
  nameEditable: { cursor: "pointer", textDecoration: { default: "none", ":hover": "underline" } },
  overrides: { marginInlineStart: 8, fontSize: 11, fontWeight: 400, color: "var(--precision-text3)" },
  key: {
    display: "block",
    overflow: "hidden",
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--color-text-secondary)",
  },
  valueCell: { display: "flex", minWidth: 0, flexWrap: "wrap", alignItems: "center", gap: 8, paddingBlock: 6 },
  error: { fontSize: 12, color: "var(--color-text-error)" },
  url: {
    overflow: "hidden",
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--color-text-accent)",
  },
  plain: {
    overflow: "hidden",
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--color-text-secondary)",
  },
  masked: {
    minWidth: 0,
    overflow: "hidden",
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
  },
  revealed: { width: "100%", minWidth: 0, fontFamily: "var(--font-family-code)", fontSize: 12, wordBreak: "break-all" },
  hides: { fontSize: 11, whiteSpace: "nowrap", color: "var(--precision-text3)" },
  message: { margin: 0, padding: 24, color: "var(--color-text-secondary)" },
  body: { display: "flex", minWidth: 0, flex: 1, flexDirection: "column" },
  title: { display: "flex", alignItems: "center", gap: 12, paddingInline: 22, paddingBlockStart: 18 },
  heading: { margin: 0, fontSize: 18, fontWeight: 600 },
  count: { fontSize: 12, color: "var(--precision-text3)" },
  new: { marginInlineStart: "auto" },
  tabs: { paddingInline: 22, paddingBlockStart: 10 },
  scroll: { minHeight: 0, flex: 1, overflow: "auto" },
  companyToggle: { paddingInline: 22, paddingBlock: 12 },
  notes: { marginBlock: 18, marginInline: 22 },
  visuallyHidden: {
    position: "absolute",
    width: 1,
    height: 1,
    overflow: "hidden",
    clipPath: "inset(50%)",
    whiteSpace: "nowrap",
  },
  split: { display: "flex", minHeight: 0, flex: 1 },
  scopes: {
    display: "flex",
    width: 220,
    flexShrink: 0,
    flexDirection: "column",
    overflowY: "auto",
    borderInlineEndWidth: 1,
    borderInlineEndStyle: "solid",
    borderInlineEndColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    paddingInline: 8,
    paddingBlock: 12,
  },
  scopeLabel: {
    margin: 0,
    marginBlockEnd: 4,
    paddingInline: 8,
    fontSize: 12,
    fontWeight: 500,
    color: "var(--precision-text3)",
  },
  scopeCount: { fontSize: 12, color: "var(--precision-text3)" },
});

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
  return (
    <>
      <FormDialog
        title={title}
        width={560}
        onClose={close}
        footer={
          <>
            {item ? (
              <span {...stylex.props(styles.pushApart)}>
                <Button variant="destructive" label="Delete…" onClick={() => setDeleting(true)} />
              </span>
            ) : null}
            <Button label="Cancel" onClick={close} />
            <Button variant="primary" label={busy ? "Saving…" : "Save"} isDisabled={busy} onClick={() => void save()} />
          </>
        }
      >
        <div {...stylex.props(styles.type)}>
          <span {...stylex.props(styles.typeLabel)}>Type</span>
          <ToggleButtonGroup
            label="Type"
            size="sm"
            value={form.kind}
            onChange={(v) => v && change({ kind: v as VaultKind })}
          >
            {kinds.map((k: VaultKind) => (
              <ToggleButton key={k} value={k} label={KIND_LABEL[k]} />
            ))}
          </ToggleButtonGroup>
          {item?.kind === "secret" ? (
            <Hint>A secret stays a secret. Make a new item to change its type.</Hint>
          ) : item ? (
            <Hint>Text and URL trade places; a secret needs a new item.</Hint>
          ) : null}
        </div>
        <TextInput
          hasAutoFocus
          label="Name *"
          placeholder="GitHub token"
          value={form.name}
          onChange={(name) => change({ name })}
        />
        <TextInput
          label="Key *"
          placeholder="From the name"
          description={`${envHint}A–Z, 0–9 and _ · unique in this scope`}
          controlXstyle={styles.mono}
          value={form.env}
          onChange={(env) => change({ env })}
        />
        {item && form.env.trim() !== "" && form.env.trim() !== item.env_name ? (
          <p {...stylex.props(styles.warning)}>Runs that use {item.env_name} will no longer find it.</p>
        ) : null}
        {isSecret ? (
          <div {...stylex.props(styles.secret)}>
            <span {...stylex.props(styles.grow)}>
              <TextInput
                width="100%"
                label="Value *"
                description="At least 8 characters. It is never shown again here."
                autoComplete="off"
                hasSpellCheck={false}
                type={reveal ? "text" : "password"}
                placeholder={item ? "Leave empty to keep the current secret" : ""}
                controlXstyle={styles.mono}
                value={form.value}
                onChange={(value) => change({ value })}
              />
            </span>
            <Button label={reveal ? "Hide what you type" : "Show what you type"} onClick={() => setReveal(!reveal)}>
              {reveal ? "Hide" : "Show"}
            </Button>
          </div>
        ) : form.kind === "url" ? (
          <TextInput
            label="Value *"
            description={`https://… · ${NOT_SECRET}`}
            controlXstyle={styles.mono}
            value={form.value}
            onChange={(value) => change({ value })}
          />
        ) : (
          <TextArea
            label="Value *"
            description={NOT_SECRET}
            rows={3}
            value={form.value}
            onChange={(value) => change({ value })}
          />
        )}
        <TextInput
          label="Note"
          placeholder="Employees see this; never a secret"
          value={form.note}
          onChange={(note) => change({ note })}
        />
        <Hint>{ALL_READ}</Hint>
        {error ? <Problem>{error}</Problem> : null}
      </FormDialog>
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
    </>
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
    <FormDialog
      title="Export the vault key"
      onClose={hide}
      footer={
        key ? (
          <>
            <Button label="Copy" onClick={() => void copy(key.value)} />
            <Button label="Save to file…" onClick={() => void save()} />
            <Button variant="primary" label="Done" onClick={hide} />
          </>
        ) : (
          <>
            <Button label="Cancel" onClick={hide} />
            <Button
              variant="primary"
              label={busy ? "Exporting…" : "Export key"}
              isDisabled={busy || password === ""}
              onClick={() => void run()}
            />
          </>
        )
      }
    >
      <p {...stylex.props(styles.about)}>{KEY_ABOUT}</p>
      {key ? (
        <>
          <Eyebrow>
            Key {key.id}
            {env ? ` · ${env}` : ""} · shown once
          </Eyebrow>
          <section aria-label="The vault key, shown once">
            <pre {...stylex.props(styles.keyValue)}>{key.value}</pre>
          </section>
          <p role="status" {...stylex.props(styles.status)}>
            {hidesIn(until, now)}
          </p>
        </>
      ) : (
        <TextInput
          hasAutoFocus
          label="Password"
          description="Type your password: exporting the key asks it again."
          autoComplete="current-password"
          type="password"
          placeholder="Your password"
          value={password}
          onChange={(value) => {
            setPassword(value);
            setError(null);
          }}
          onEnter={() => void run()}
        />
      )}
      {error ? <Problem>{error}</Problem> : null}
    </FormDialog>
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
  return (
    <>
      <Table density="compact" xstyle={styles.auditTable}>
        <TableHeader>
          <TableRow isHeaderRow>
            <TableHeaderCell xstyle={[styles.headCell, styles.timeColumn]}>Time</TableHeaderCell>
            <TableHeaderCell xstyle={[styles.headCell, styles.whoColumn]}>Who</TableHeaderCell>
            <TableHeaderCell xstyle={[styles.headCell, styles.actionColumn]}>Action</TableHeaderCell>
            <TableHeaderCell xstyle={[styles.headCell, styles.itemColumn]}>Item</TableHeaderCell>
            <TableHeaderCell xstyle={styles.headCell}>Via</TableHeaderCell>
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((r) => (
            <TableRow key={r.id}>
              <TableCell>
                <span {...stylex.props(styles.cellText, styles.dim)}>
                  {shortDay(r.at_ms)} {hhmm(r.at_ms)}
                </span>
              </TableCell>
              <TableCell>
                <span {...stylex.props(styles.cellText)}>{who(r.actor, user?.id ?? null)}</span>
              </TableCell>
              <TableCell>
                <span {...stylex.props(styles.cellText)}>{actionText(r)}</span>
              </TableCell>
              <TableCell>
                <span {...stylex.props(styles.cellText)}>{r.item_name ?? "—"}</span>
              </TableCell>
              <TableCell>
                <span {...stylex.props(styles.cellText, styles.dim)}>{via(r)}</span>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
      {first.isPending ? <Loading /> : null}
      {first.data && rows.length === 0 ? <p {...stylex.props(styles.empty)}>Nothing in the audit log yet.</p> : null}
      {(more ?? first.data?.more) ? (
        <div {...stylex.props(styles.more)}>
          <Button size="sm" label="Load older" onClick={() => void load()} />
        </div>
      ) : null}
      <p {...stylex.props(styles.note)}>The audit log is read-only and never shows a value.</p>
    </>
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
  const row = (item: VaultItemView, readOnly = false) => {
    const open = shown?.id === item.id ? shown : null;
    return (
      <TableRow key={item.id}>
        <TableCell>
          <button
            type="button"
            aria-label={`Edit ${item.name}`}
            disabled={readOnly}
            title={item.description ?? undefined}
            onClick={() => setEditing(item)}
            {...stylex.props(styles.name, !readOnly && styles.nameEditable)}
          >
            {item.name}
            {item.overrides_company ? <span {...stylex.props(styles.overrides)}>overrides Company</span> : null}
          </button>
        </TableCell>
        <TableCell>
          <code {...stylex.props(styles.key)}>{item.env_name}</code>
        </TableCell>
        <TableCell>
          <span {...stylex.props(styles.cellText, styles.dim)}>{KIND_TAG[item.kind]}</span>
        </TableCell>
        <TableCell>
          <span {...stylex.props(styles.valueCell)}>
            {item.undecryptable ? (
              <span {...stylex.props(styles.error)}>Cannot be decrypted</span>
            ) : item.kind === "url" && item.value && /^https?:\/\//i.test(item.value) ? (
              <a href={item.value} target="_blank" rel="noreferrer" {...stylex.props(styles.url)}>
                {item.value}
              </a>
            ) : item.kind !== "secret" ? (
              <span {...stylex.props(styles.plain)}>{(item.value ?? "").split("\n")[0]}</span>
            ) : (
              <>
                <span {...stylex.props(open ? styles.revealed : styles.masked)}>{open ? open.value : MASK}</span>
                <Button
                  size="sm"
                  label={`${open ? "Hide" : "Show"} ${item.name}`}
                  tooltip={SHOW_TIP}
                  onClick={() => (open ? hide() : void show(item))}
                >
                  {open ? "Hide" : "Show"}
                </Button>
                <Button
                  size="sm"
                  label={`Copy ${item.name}`}
                  tooltip="Copies without showing it, and clears the clipboard after 60 s. This is logged."
                  onClick={() => void copy(item)}
                >
                  Copy
                </Button>
                {open ? <span {...stylex.props(styles.hides)}>{hidesIn(open.until, now)}</span> : null}
              </>
            )}
          </span>
        </TableCell>
        <TableCell>
          <span {...stylex.props(styles.cellText, styles.dim)}>
            {item.updated_by?.id === user?.id ? "You" : (item.updated_by?.username ?? "")} ·{" "}
            {shortDay(item.updated_at_ms)}
          </span>
        </TableCell>
      </TableRow>
    );
  };
  const table = (rows: React.ReactNode) => (
    <Table density="compact" xstyle={styles.itemsTable}>
      <TableHeader>
        <TableRow isHeaderRow>
          <TableHeaderCell xstyle={styles.headCell}>Name</TableHeaderCell>
          <TableHeaderCell xstyle={styles.headCell}>Key</TableHeaderCell>
          <TableHeaderCell xstyle={[styles.headCell, styles.typeColumn]}>Type</TableHeaderCell>
          <TableHeaderCell xstyle={styles.headCell}>Value</TableHeaderCell>
          <TableHeaderCell xstyle={[styles.headCell, styles.updatedColumn]}>Updated</TableHeaderCell>
        </TableRow>
      </TableHeader>
      <TableBody>{rows}</TableBody>
    </Table>
  );
  return (
    <div {...stylex.props(styles.body)}>
      <div {...stylex.props(styles.title)}>
        <Heading level={2} xstyle={styles.heading}>
          {scopeName}
        </Heading>
        <span {...stylex.props(styles.count)}>
          · {items.length} item{items.length === 1 ? "" : "s"}
        </span>
        <span {...stylex.props(styles.new)}>
          <Button size="sm" variant="primary" label="New item" onClick={() => setEditing("new")} />
        </span>
      </div>
      <div {...stylex.props(styles.tabs)}>
        <TabList role="tablist" aria-label="Vault" hasDivider value={tab} onChange={(v) => setTab(v as typeof tab)}>
          <Tab value="items" label="Items" panelId={PANEL} />
          <Tab value="audit" label="Audit log" panelId={PANEL} />
        </TabList>
      </div>
      <div id={PANEL} role="tabpanel" {...stylex.props(styles.scroll)}>
        {tab === "audit" ? (
          <AuditLog projectId={projectId} all={false} />
        ) : list.isPending ? (
          <Loading />
        ) : list.error ? (
          <Problem>{failureText(list.error)}</Problem>
        ) : (
          <>
            {items.length > 0 ? (
              table(items.map((i) => row(i)))
            ) : (
              <Empty
                title={`No items in ${scopeName} yet`}
                description={`${projectId ? "Employees of this project" : "Every employee"} can read every item here.`}
              >
                <Button variant="primary" label="New item" onClick={() => setEditing("new")} />
              </Empty>
            )}
            {company.length > 0 ? (
              <>
                <div {...stylex.props(styles.companyToggle)}>
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-expanded={companyOpen}
                    label={`${companyOpen ? "▾" : "▸"} Company items also available here: ${company.length}`}
                    onClick={() => setCompanyOpen(!companyOpen)}
                  />
                </div>
                {companyOpen ? table(company.map((i) => row(i, true))) : null}
              </>
            ) : null}
            <div {...stylex.props(styles.notes)}>
              <Banner
                status="warning"
                title={MCP_SEES_SECRETS}
                {...(platform.kind === "electron" ? { description: LINUX_KEY_NOTE } : {})}
              />
            </div>
          </>
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
      <p role="status" {...stylex.props(styles.visuallyHidden)}>
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
      <Page>
        <PageHead title="Knowledge" tabs={<KnowledgeTabs />} />
        <p {...stylex.props(styles.message)}>This server does not have the vault yet. Update the server.</p>
      </Page>
    );
  }
  return (
    <Page>
      <PageHead title="Knowledge" tabs={<KnowledgeTabs />}>
        <Button size="sm" label="Export key…" onClick={() => setExporting(true)} />
      </PageHead>
      <div {...stylex.props(styles.split)}>
        <nav aria-label="Scope" {...stylex.props(styles.scopes)}>
          <p {...stylex.props(styles.scopeLabel)}>Scope</p>
          <List density="compact">
            {all.map((s) => (
              <ListItem
                key={s.id ?? "company"}
                isSelected={s.id === (current?.id ?? null)}
                onClick={() => setScope(s.id)}
                startContent={
                  <Mark letter={initial(s.name)} color={s.id ? markColor(s.id) : "#3A3D44"} size={15} radius={4} />
                }
                label={s.name}
                endContent={<span {...stylex.props(styles.scopeCount)}>{s.count}</span>}
              />
            ))}
          </List>
        </nav>
        <VaultBody
          key={current?.id ?? "company"}
          projectId={current?.id ?? null}
          scopeName={current?.name ?? "Company"}
        />
      </div>
      {exporting ? <KeyExport onClose={() => setExporting(false)} /> : null}
    </Page>
  );
}
