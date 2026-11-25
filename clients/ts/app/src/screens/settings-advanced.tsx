import { Button } from "@astryxdesign/core/Button";
import { CheckboxInput } from "@astryxdesign/core/CheckboxInput";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { ToggleButton } from "@astryxdesign/core/ToggleButton";
import { expectBody } from "@aurelius/client";
import type { LoadLimitsUpdate, McpLogin, McpTransport, ProviderKind } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, Fragment, useEffect, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useAction, useKeyContext } from "../keys/provider";
import { FormDialog } from "./org-kit";
import {
  bytes,
  ConfirmButton,
  Group,
  Keyed,
  Muted,
  modelName,
  openWeb,
  Part,
  Problem,
  Row,
  SmallButton,
  Switch,
  TextInput,
  useChange,
  useSetting,
  when,
} from "./settings-kit";
import { compactFieldOf, compactThresholdOf } from "./settings-logic";

const TEXT3 = "var(--precision-text3)";

const styles = stylex.create({
  pick: {
    height: "auto",
    whiteSpace: "normal",
    justifyContent: "flex-start",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 8,
    padding: 12,
    textAlign: "left",
    color: "var(--color-text-primary)",
  },
  pickOn: { borderColor: "var(--color-text-accent)", backgroundColor: "var(--color-overlay-hover)" },
  pickName: { display: "block", fontWeight: 600 },
  pickAbout: { display: "block", fontSize: 12, fontWeight: 400, color: TEXT3 },
  picks: { display: "flex", flexDirection: "column", gap: 8 },
  form: { display: "flex", alignItems: "center", gap: 8 },
  open: {
    cursor: "pointer",
    borderWidth: 0,
    padding: 0,
    backgroundColor: "transparent",
    fontWeight: 500,
    textAlign: "left",
    color: "var(--color-text-primary)",
  },
  details: { paddingBlockStart: 0, paddingInline: 16, paddingBlockEnd: 14 },
  facts: { margin: 0, display: "grid", gridTemplateColumns: "110px 1fr", columnGap: 12, rowGap: 4, fontSize: 12 },
  fact: { color: TEXT3 },
  value: { margin: 0 },
  mono: { margin: 0, fontFamily: "var(--font-family-code)" },
  monoTool: { fontFamily: "var(--font-family-code)", fontSize: 12 },
  start: { alignSelf: "flex-start" },
  status: { fontSize: 12, color: "var(--color-text-secondary)" },
  quiet: { margin: 0, color: TEXT3 },
  contents: { display: "contents" },
});

const PROVIDERS: { key: ProviderKind; label: string; about: string }[] = [
  {
    key: "claude_code",
    label: "Claude Code",
    about: "Your own, unmodified Claude Code. You sign in within Claude Code itself; Aurelius never sees your login.",
  },
  { key: "anthropic_api", label: "Anthropic API", about: "Your own API key, pay per use." },
  { key: "openai_api", label: "OpenAI API", about: "Your own API key, pay per use." },
];

function AddConnectionDialog({ onClose }: { onClose: () => void }) {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [provider, setProvider] = useState<ProviderKind | null>(null);
  const [path, setPath] = useState("");
  const chosen = PROVIDERS.find((p) => p.key === provider);
  useKeyContext("AddConnection", true);
  PROVIDERS.slice(0, 3).forEach((p, i) => {
    // biome-ignore lint/correctness/useHookAtTopLevel: a fixed list: the same hooks in the same order.
    useAction(`Provider${i + 1}`, () => setProvider(p.key));
  });
  useEffect(() => {
    if (provider !== "claude_code") return;
    connection
      .call("connection.detect_runtime")
      .then((body) => {
        const found = expectBody(body, "runtime_detected").value.claude_code;
        if (found) setPath(found);
      })
      .catch(() => undefined);
  }, [provider, connection]);
  const add = () =>
    chosen
      ? run(() =>
          connection.write(
            "connection.add",
            {
              display_name: chosen.label,
              provider: chosen.key,
              ...(chosen.key === "claude_code" && path.trim() ? { runtime_path: path.trim() } : {}),
            },
            { retry: false },
          ),
        ).then((ok) => {
          if (ok) onClose();
        })
      : undefined;
  return (
    <FormDialog
      title="Add connection"
      onClose={onClose}
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button variant="primary" label="Add connection" isDisabled={busy || !chosen} onClick={() => void add()} />
        </>
      }
    >
      <Muted>
        Choose a runtime. Nothing is connected automatically and there is never a silent switch to a paid service.
      </Muted>
      <div {...stylex.props(styles.picks)}>
        {PROVIDERS.map((p) => (
          <ToggleButton
            key={p.key}
            label={`${p.label}. ${p.about}`}
            isPressed={provider === p.key}
            onPressedChange={() => setProvider(p.key)}
            xstyle={[styles.pick, provider === p.key && styles.pickOn]}
          >
            <span {...stylex.props(styles.pickName)}>{p.label}</span>
            <span {...stylex.props(styles.pickAbout)}>{p.about}</span>
          </ToggleButton>
        ))}
      </div>
      {provider === "claude_code" ? (
        <TextInput
          label="Program"
          placeholder="Path to claude"
          mono
          value={path}
          onChange={(e) => setPath(e.target.value)}
        />
      ) : null}
      <Problem error={error} />
    </FormDialog>
  );
}

function ApiKey({ id, has }: { id: string; has: boolean }) {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [open, setOpen] = useState(false);
  const [secret, setSecret] = useState("");
  if (!open)
    return (
      <Keyed action="SettingsKey" chord="S">
        <SmallButton onClick={() => setOpen(true)}>{has ? "Replace API key" : "API key"}</SmallButton>
      </Keyed>
    );
  const close = () => {
    setSecret("");
    setOpen(false);
  };
  return (
    <form
      {...stylex.props(styles.form)}
      onSubmit={(e) => {
        e.preventDefault();
        // Never kept for a reconnect: the parameters hold the key.
        void run(() => connection.write("secret.set", { connection_id: id, secret }, { retry: false })).then((ok) => {
          if (ok) close();
          else setSecret("");
        });
      }}
    >
      <TextInput
        label="API key"
        autoFocus
        type="password"
        autoComplete="off"
        placeholder="API key (Enter saves, Esc cancels)"
        width={240}
        value={secret}
        onChange={(e) => setSecret(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            close();
          }
        }}
      />
      <SmallButton type="submit" kind="primary" disabled={busy || secret === ""}>
        Save
      </SmallButton>
      <Problem error={error} />
    </form>
  );
}

export function ConnectionsSection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [adding, setAdding] = useState(false);
  const [open, setOpen] = useState<string | null>(null);
  const list = useSetting(
    "connections",
    async () => expectBody(await connection.call("connection.list"), "connections").value,
  );
  return (
    <>
      <Group title="Connections">
        {list.data?.connections.length === 0 ? <Row title="No connections yet." /> : null}
        {(list.data ?? { connections: [] }).connections.map((c) => {
          const needsKey = c.provider === "anthropic_api" || c.provider === "openai_api";
          return (
            <Fragment key={c.id}>
              <Row
                joined={open === c.id}
                title={
                  <button
                    type="button"
                    aria-expanded={open === c.id}
                    onClick={() => setOpen(open === c.id ? null : c.id)}
                    {...stylex.props(styles.open)}
                  >
                    {open === c.id ? "▾" : "▸"} {c.display_name}
                  </button>
                }
                hint={[
                  c.status,
                  c.default_model ? modelName(c.default_model) : null,
                  `Checked ${when(c.last_verified_ms)}`,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              >
                {needsKey ? <ApiKey id={c.id} has={c.has_secret} /> : null}
                <Keyed action="SettingsVerify" chord="V">
                  <SmallButton
                    disabled={busy}
                    onClick={() =>
                      void run(() => connection.write("connection.verify", { connection_id: c.id }, { retry: false }))
                    }
                  >
                    Check
                  </SmallButton>
                </Keyed>
                <Keyed action="SettingsRemove" chord="Del">
                  <ConfirmButton
                    label="Remove"
                    question={`Remove ${c.display_name}?`}
                    disabled={busy}
                    onConfirm={() =>
                      void run(() => connection.write("connection.remove", { connection_id: c.id }, { retry: false }))
                    }
                  />
                </Keyed>
              </Row>
              {open === c.id ? (
                <Part xstyle={styles.details}>
                  <dl {...stylex.props(styles.facts)}>
                    <dt {...stylex.props(styles.fact)}>Status</dt>
                    <dd {...stylex.props(styles.value)}>
                      {c.status}
                      {c.detail ? ` · ${c.detail}` : ""}
                    </dd>
                    <dt {...stylex.props(styles.fact)}>Billing</dt>
                    <dd {...stylex.props(styles.value)}>{c.billing}</dd>
                    <dt {...stylex.props(styles.fact)}>Sign-in</dt>
                    <dd {...stylex.props(styles.value)}>
                      {needsKey
                        ? c.has_secret
                          ? "API key is in the OS credential store"
                          : "No API key yet"
                        : c.provider === "claude_code"
                          ? "In Claude Code itself (`claude auth login`)"
                          : "Not needed"}
                    </dd>
                    {c.runtime_path ? (
                      <>
                        <dt {...stylex.props(styles.fact)}>Program</dt>
                        <dd {...stylex.props(styles.mono)}>{c.runtime_path}</dd>
                      </>
                    ) : null}
                    <dt {...stylex.props(styles.fact)}>Available</dt>
                    <dd {...stylex.props(styles.value)}>{c.models.map(modelName).join(", ") || "None"}</dd>
                    <dt {...stylex.props(styles.fact)}>Id</dt>
                    <dd {...stylex.props(styles.mono)}>{c.id}</dd>
                  </dl>
                </Part>
              ) : null}
            </Fragment>
          );
        })}
      </Group>
      <div>
        <Keyed action="SettingsNew" chord="N">
          <SmallButton kind="primary" onClick={() => setAdding(true)}>
            Add connection…
          </SmallButton>
        </Keyed>
      </div>
      {adding ? <AddConnectionDialog onClose={() => setAdding(false)} /> : null}
      <Problem error={list.error ?? error} />
    </>
  );
}

const MCP_AUTH: Record<string, string> = { authorized: "Connected", needs_login: "Needs login" };

function Browser() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const browser = useSetting("browser", async () => expectBody(await connection.call("browser.get"), "browser").value);
  const data = browser.data;
  const backend = data?.backend ?? "claude_in_chrome";
  return (
    <Group title="Browser">
      <Row
        title="Drive with"
        hint={
          backend === "playwright"
            ? "A separate Aurelius browser window on the server's computer: log in there once and the logins stay. Employees work in parallel, each in its own tab. Needs Node.js."
            : "The extension in your own browser profile, with your logins. Asks before each browser action unless the mode is Auto."
        }
      >
        <SegmentedControl
          label="Drive the browser with"
          size="sm"
          value={backend}
          onChange={(next) =>
            void run(() =>
              connection.write("browser.set_backend", { backend: next as typeof backend }, { retry: false }),
            )
          }
        >
          <SegmentedControlItem value="claude_in_chrome" label="Claude in Chrome" />
          <SegmentedControlItem value="playwright" label="Playwright" />
        </SegmentedControl>
      </Row>
      {backend === "claude_in_chrome" ? (
        <Row
          title="Browser"
          hint={
            data?.env_override
              ? `AURELIUS_BROWSER on the server forces ${data.env_override}.`
              : data?.choice === "automatic"
                ? "Whichever is running, else the one that has the extension. Used from the next run."
                : "Used from the next run."
          }
        >
          <SegmentedControl
            label="Browser"
            size="sm"
            value={data?.choice ?? "automatic"}
            onChange={(next) => {
              const choice = next as NonNullable<typeof data>["choice"];
              if (!busy) void run(() => connection.write("browser.set", { choice }, { retry: false }));
            }}
          >
            <SegmentedControlItem value="automatic" label="Automatic" />
            <SegmentedControlItem value="chrome" label="Chrome" />
            <SegmentedControlItem value="edge" label="Edge" />
          </SegmentedControl>
        </Row>
      ) : null}
      <Problem error={browser.error ?? error} />
    </Group>
  );
}

function AddMcp({ onClose }: { onClose: () => void }) {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [transport, setTransport] = useState<McpTransport>("stdio");
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");
  const [url, setUrl] = useState("");
  const [enable, setEnable] = useState(true);
  const local = transport === "stdio";
  const ready = name.trim() !== "" && (local ? command.trim() !== "" : url.trim() !== "");
  const add = () => {
    const [program, ...args] = command.trim().split(/\s+/);
    return run(() =>
      connection.write(
        "mcp.add",
        { name: name.trim(), transport, enable, ...(local ? { command: program ?? "", args } : { url: url.trim() }) },
        { retry: false },
      ),
    ).then((ok) => {
      if (ok) onClose();
    });
  };
  return (
    <FormDialog
      title="Add MCP server"
      onClose={onClose}
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button
            variant="primary"
            label={busy ? "Adding…" : "Add server"}
            isDisabled={busy || !ready}
            onClick={() => void add()}
          />
        </>
      }
    >
      <Muted>Adds the server to Claude Code on the server PC (user scope).</Muted>
      <SegmentedControl
        label="Transport"
        size="sm"
        value={transport}
        onChange={(next) => setTransport(next as McpTransport)}
      >
        <SegmentedControlItem value="stdio" label="Command" />
        <SegmentedControlItem value="http" label="HTTP" />
        <SegmentedControlItem value="sse" label="SSE" />
      </SegmentedControl>
      <TextInput label="Name" placeholder="Name" value={name} onChange={(e) => setName(e.target.value)} />
      {local ? (
        <TextInput
          label="Command"
          placeholder="Command and arguments"
          mono
          value={command}
          onChange={(e) => setCommand(e.target.value)}
        />
      ) : (
        <TextInput label="URL" placeholder="https://…" mono value={url} onChange={(e) => setUrl(e.target.value)} />
      )}
      <CheckboxInput label="Turn on now" value={enable} onChange={(checked) => setEnable(checked)} />
      <Problem error={error} />
    </FormDialog>
  );
}

function McpLoginDialog({ name, onClose }: { name: string; onClose: () => void }) {
  const connection = useConnection();
  const client = useQueryClient();
  const [login, setLogin] = useState<McpLogin | null>(null);
  const [redirect, setRedirect] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    connection
      .write("mcp.login_start", { name }, { retry: false })
      .then((receipt) => {
        const { result } = receipt;
        if (result.kind === "host" && result.value.type === "mcp_login") {
          setLogin(result.value);
          openWeb(result.value.authorization_url);
        }
      })
      .catch((e: unknown) => setProblem(e instanceof Error ? e.message : "This did not work."));
  }, [connection, name]);
  // It ends in the browser: look for that here.
  const id = login?.login_id;
  useEffect(() => {
    if (!id) return;
    const tick = setInterval(() => {
      connection
        .call("mcp.login_get", { login_id: id })
        .then((body) => {
          const next = expectBody(body, "mcp_login").value;
          setLogin(next);
          if (next.state === "succeeded") {
            void client.invalidateQueries({ queryKey: ["settings"] });
            onClose();
          }
        })
        .catch(() => undefined);
    }, 2000);
    return () => clearInterval(tick);
  }, [id, connection, client, onClose]);
  const finish = () =>
    id
      ? connection
          .write("mcp.login_complete", { login_id: id, redirect_url: redirect.trim() }, { retry: false })
          .catch((e: unknown) => setProblem(e instanceof Error ? e.message : "This did not work."))
      : undefined;
  return (
    <FormDialog
      title={`Log in to ${name}`}
      onClose={onClose}
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button variant="primary" label="Finish" isDisabled={redirect.trim() === ""} onClick={() => void finish()} />
        </>
      }
    >
      {login ? (
        <>
          <Muted>
            Log in in the browser. It then ends on a page that does not load: copy that page's address and paste it
            here.
          </Muted>
          <span {...stylex.props(styles.start)}>
            <SmallButton onClick={() => openWeb(login.authorization_url)}>Open sign-in page ↗</SmallButton>
          </span>
          <TextInput
            label="Page address"
            placeholder={login.redirect_uri}
            mono
            value={redirect}
            onChange={(e) => setRedirect(e.target.value)}
          />
        </>
      ) : !problem ? (
        <p {...stylex.props(styles.quiet)}>Starting the login…</p>
      ) : null}
      <Problem error={problem ?? login?.error} />
    </FormDialog>
  );
}

function AlwaysAllowed() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const list = useSetting(
    "always_allow",
    async () => expectBody(await connection.call("mcp_always_allow.list"), "mcp_always_allow").value.tools,
  );
  return (
    <Group title="Always allowed tools">
      <Row title="These MCP tools run without asking, in every project." hint="Plan mode still blocks them." />
      {list.data?.length === 0 ? (
        <Row title="No tools yet." hint="Choose 'Always allow' on an MCP tool's approval to add one." />
      ) : null}
      {(list.data ?? []).map((tool) => (
        <Row key={tool} title={<span {...stylex.props(styles.monoTool)}>{tool}</span>}>
          <SmallButton
            disabled={busy}
            onClick={() => void run(() => connection.write("mcp_always_allow.remove", { tool }, { retry: false }))}
          >
            Remove
          </SmallButton>
        </Row>
      ))}
      <Problem error={list.error ?? error} />
    </Group>
  );
}

export function McpSection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const list = useSetting("mcp", async () => expectBody(await connection.call("mcp.list"), "mcp_servers").value);
  const auto = useSetting(
    "auto_allow",
    async () => expectBody(await connection.call("auto_allow.get_setting"), "auto_allow_setting").value,
  );
  const servers = list.data?.servers ?? [];
  const [adding, setAdding] = useState(false);
  const [login, setLogin] = useState<string | null>(null);
  const setEnabled = (name: string, on: boolean) => {
    const enabled = servers.filter((s) => (s.name === name ? on : s.enabled)).map((s) => s.name);
    return run(() => connection.write("mcp.set", { enabled }, { retry: false }));
  };
  return (
    <>
      <Group title="Tools">
        <Row title="Allow safe tools without asking" hint={auto.data?.tools?.join(", ")}>
          <Switch
            label="Allow safe tools without asking"
            on={auto.data?.enabled ?? false}
            disabled={busy || !auto.data}
            onChange={(enabled) =>
              void run(() => connection.write("auto_allow.set_enabled", { enabled }, { retry: false }))
            }
          />
        </Row>
      </Group>
      <Browser />
      <Group title="MCP servers">
        {servers.length === 0 && (list.data?.missing ?? []).length === 0 ? <Row title="No MCP servers yet." /> : null}
        {(list.data?.missing ?? []).map((name) => (
          <Row key={name} title={name} hint="Claude Code no longer has this server." />
        ))}
        {servers.map((s) => (
          <Row
            key={s.name}
            title={s.name}
            hint={`${s.summary}${MCP_AUTH[s.auth ?? ""] ? ` · ${MCP_AUTH[s.auth ?? ""]}` : ""}`}
          >
            {s.auth === "needs_login" ? <SmallButton onClick={() => setLogin(s.name)}>Log in</SmallButton> : null}
            {s.auth === "authorized" ? (
              <SmallButton
                disabled={busy}
                onClick={() => void run(() => connection.write("mcp.logout", { name: s.name }, { retry: false }))}
              >
                Log out
              </SmallButton>
            ) : null}
            <Switch
              label={`Use ${s.name}`}
              on={s.enabled}
              disabled={busy}
              onChange={(on) => void setEnabled(s.name, on)}
            />
            <Keyed action="SettingsRemove" chord="Del">
              <ConfirmButton
                label="Remove"
                question={`Remove ${s.name}?`}
                disabled={busy}
                onConfirm={() => void run(() => connection.write("mcp.remove", { name: s.name }, { retry: false }))}
              />
            </Keyed>
          </Row>
        ))}
      </Group>
      <div>
        <Keyed action="SettingsNew" chord="N">
          <SmallButton kind="primary" onClick={() => setAdding(true)}>
            Add MCP server…
          </SmallButton>
        </Keyed>
      </div>
      <AlwaysAllowed />
      {adding ? <AddMcp onClose={() => setAdding(false)} /> : null}
      {login ? <McpLoginDialog name={login} onClose={() => setLogin(null)} /> : null}
      <Problem error={list.error ?? auto.error ?? error} />
    </>
  );
}

export function BackupsSection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const list = useSetting("backups", async () => expectBody(await connection.call("backup.list"), "backups").value);
  const data = list.data;
  return (
    <>
      <Group>
        <Row title="Back up now" hint={data?.folder}>
          <Keyed action="SettingsBackupNow" chord="B">
            <SmallButton
              disabled={busy}
              onClick={() =>
                void run(() => connection.write("backup.create", undefined, { retry: false, timeoutMs: 600_000 }))
              }
            >
              {busy ? "Working…" : "Back up now"}
            </SmallButton>
          </Keyed>
        </Row>
        {data?.paused ? (
          <Row
            title="Restored from a backup. Nothing runs (tasks, chats, schedules) until you resume."
            hint={data.paused}
          >
            <Keyed action="SettingsResume" chord="P">
              <SmallButton
                disabled={busy}
                onClick={() => void run(() => connection.write("backup.resume", undefined, { retry: false }))}
              >
                Resume
              </SmallButton>
            </Keyed>
          </Row>
        ) : null}
        {data?.restore_pending ? <Row title="A restore waits for the next start" hint={data.restore_pending} /> : null}
      </Group>
      <Group title="Backups">
        {data?.backups.length === 0 ? <Row title="No backups yet." /> : null}
        {(data?.backups ?? []).map((b) => (
          <Row
            key={b.id}
            title={when(b.created_at_ms)}
            hint={`${b.kind === "daily" ? "daily" : "manual"} · ${bytes(b.bytes)} · ${b.files} files · version ${b.app_version}`}
          >
            <Keyed action="SettingsRestore" chord="R">
              <ConfirmButton
                label="Restore"
                question={`Restore the backup of ${when(b.created_at_ms)}? Aurelius restarts and today's data is replaced.`}
                disabled={busy}
                onConfirm={() => void run(() => connection.write("backup.restore", { id: b.id }, { retry: false }))}
              />
            </Keyed>
          </Row>
        ))}
      </Group>
      <Problem error={list.error ?? data?.error ?? error} />
    </>
  );
}

const LIMITS: { key: "cpu_high" | "cpu_low" | "mem_min_mb" | "max_runs"; label: string; hint: string }[] = [
  { key: "max_runs", label: "Max runs at once", hint: "At most." },
  { key: "cpu_high", label: "Hold from CPU", hint: "Percent." },
  { key: "cpu_low", label: "Release below CPU", hint: "Percent." },
  { key: "mem_min_mb", label: "Min free memory", hint: "GB." },
];

/** Memory is kept in MB on the wire and written in GB. */
const shownLimit = (key: string, v: number | undefined) =>
  v === undefined ? "" : key === "mem_min_mb" ? String(v / 1024) : String(v);
const sentLimit = (key: string, v: number) => (key === "mem_min_mb" ? Math.round(v * 1024) : v);

/** What the server is doing or about to do; the sidebar's Live only knows tasks. */
function RunningNow() {
  const connection = useConnection();
  const jobs = useSetting("jobs", async () => expectBody(await connection.call("job.list"), "jobs").value.jobs);
  const open = (jobs.data ?? []).filter((j) => j.state === "leased" || j.state === "ready");
  if (open.length === 0) return null;
  return (
    <Group title="Running now">
      {open.map((j) => (
        <Row key={j.id} title={j.label} hint={j.reason ?? j.kind}>
          <span {...stylex.props(styles.status)}>{j.state === "leased" ? "Running" : "Waiting"}</span>
        </Row>
      ))}
    </Group>
  );
}

export function LoadSection() {
  const connection = useConnection();
  const client = useQueryClient();
  const { busy, error, run } = useChange();
  const load = useSetting("load", async () => expectBody(await connection.call("load.get"), "load").value);
  const [draft, setDraft] = useState<Partial<Record<(typeof LIMITS)[number]["key"], string>>>({});
  useEffect(
    () => connection.loads.on((snapshot) => client.setQueryData(["settings", "load"], snapshot)),
    [connection, client],
  );
  const data = load.data;
  const changes: LoadLimitsUpdate = {};
  for (const { key } of LIMITS) {
    const value = Number(draft[key]);
    if (
      draft[key] !== undefined &&
      draft[key] !== "" &&
      Number.isFinite(value) &&
      sentLimit(key, value) !== data?.limits[key]
    )
      changes[key] = sentLimit(key, value);
  }
  const low = changes.cpu_low ?? data?.limits.cpu_low ?? 0;
  const high = changes.cpu_high ?? data?.limits.cpu_high ?? 100;
  const invalid = low >= high ? "CPU low must be below CPU high." : null;
  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (invalid) return;
    if (await run(() => connection.write("load.set_limits", changes, { retry: false }))) setDraft({});
  };
  return (
    <>
      <RunningNow />
      <Group title="Now">
        <Row
          title={data?.gate === "held" ? "New runs are waiting" : "New runs start right away"}
          hint={data ? `${data.held} waiting · ${data.running} running` : undefined}
        >
          {data ? (
            <span {...stylex.props(styles.status)}>
              CPU {Math.round(data.cpu_pct)}% · {bytes(data.mem_free_mb * 1024 * 1024)} free of{" "}
              {bytes(data.mem_total_mb * 1024 * 1024)}
            </span>
          ) : null}
        </Row>
      </Group>
      <form onSubmit={(e) => void save(e)} {...stylex.props(styles.contents)}>
        <Group title="CPU and memory">
          <Row title="Hold runs when the computer is busy">
            <Keyed action="LoadToggleEnabled" chord="Alt E">
              <Switch
                label="Hold runs when the computer is busy"
                on={data?.limits.enabled ?? false}
                disabled={busy || !data}
                onChange={(enabled) =>
                  void run(() => connection.write("load.set_limits", { enabled }, { retry: false }))
                }
              />
            </Keyed>
          </Row>
          {LIMITS.map(({ key, label, hint }) => (
            <Row key={key} title={label} hint={hint}>
              <TextInput
                label={label}
                type="number"
                min={0}
                step="any"
                width={90}
                value={draft[key] ?? shownLimit(key, data?.limits[key])}
                onChange={(e) => setDraft((d) => ({ ...d, [key]: e.target.value }))}
              />
            </Row>
          ))}
        </Group>
        {Object.keys(changes).length > 0 ? (
          <div>
            <Keyed action="LoadSave" chord="Ctrl S">
              <SmallButton kind="primary" type="submit" disabled={busy || invalid !== null}>
                Save limits
              </SmallButton>
            </Keyed>
          </div>
        ) : null}
      </form>
      <Problem error={load.error ?? invalid ?? error} />
    </>
  );
}

export function TraceSection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [asking, setAsking] = useState(false);
  const trace = useSetting(
    "trace",
    async () => expectBody(await connection.call("trace.get_setting"), "trace_setting").value,
  );
  const set = (enabled: boolean) => run(() => connection.write("trace.set_enabled", { enabled }, { retry: false }));
  return (
    <>
      <Group>
        <Row
          title="Trace mode"
          hint={
            trace.data
              ? `Keeps a trace of every run, for ${trace.data.keep_days} days and at most ${trace.data.keep_runs} runs.`
              : undefined
          }
        >
          {asking ? (
            <>
              <span {...stylex.props(styles.status)}>Turn trace mode off? Every kept trace is deleted.</span>
              <SmallButton
                kind="danger"
                onClick={() => {
                  setAsking(false);
                  void set(false);
                }}
              >
                Turn off and delete
              </SmallButton>
              <SmallButton onClick={() => setAsking(false)}>Keep on</SmallButton>
            </>
          ) : (
            <Switch
              label="Trace mode"
              on={trace.data?.enabled ?? false}
              disabled={busy || !trace.data}
              onChange={(enabled) => (enabled ? void set(true) : setAsking(true))}
            />
          )}
        </Row>
      </Group>
      <Problem error={trace.error ?? error} />
    </>
  );
}

export function CompactionSection() {
  const { features } = useLinkState();
  return features.includes("compaction_setting") ? (
    <CompactionThreshold />
  ) : (
    <Group>
      <Muted>This server compacts a session above 120k tokens. Update the server to choose when.</Muted>
    </Group>
  );
}

function CompactionThreshold() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const setting = useSetting(
    "compaction",
    async () => expectBody(await connection.call("compaction.get_setting"), "compaction_setting").value,
  );
  const [draft, setDraft] = useState<string | null>(null);
  const data = setting.data;
  const field = draft ?? (data ? compactFieldOf(data.threshold_tokens) : "");
  const parsed = data && draft !== null ? compactThresholdOf(draft, data) : null;
  const invalid = typeof parsed === "string" ? parsed : null;
  const save = async (tokens: number) => {
    if (await run(() => connection.write("compaction.set_threshold", { threshold_tokens: tokens }, { retry: false })))
      setDraft(null);
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (typeof parsed === "number") void save(parsed);
  };
  return (
    <>
      <form onSubmit={submit} {...stylex.props(styles.contents)}>
        <Group>
          <Row
            title="Compact the context above (thousands of tokens)"
            hint={
              data
                ? `Before its next turn a session whose context grew past this is summarised. Default ${compactFieldOf(data.default_tokens)}, from ${compactFieldOf(data.min_tokens)} to ${compactFieldOf(data.max_tokens)}; 0: never. Claude Code still compacts by itself when the model's context window is full.`
                : undefined
            }
          >
            <TextInput
              label="Compact the context above (thousands of tokens)"
              type="number"
              min={0}
              step={1}
              width={90}
              value={field}
              disabled={busy || !data}
              onChange={(e) => setDraft(e.target.value)}
            />
            {data && data.threshold_tokens !== data.default_tokens && draft === null ? (
              <SmallButton disabled={busy} onClick={() => void save(data.default_tokens)}>
                Use the default
              </SmallButton>
            ) : null}
          </Row>
        </Group>
        {draft !== null && typeof parsed === "number" && parsed !== data?.threshold_tokens ? (
          <div>
            <SmallButton kind="primary" type="submit" disabled={busy}>
              Save
            </SmallButton>
          </div>
        ) : null}
      </form>
      <Problem error={setting.error ?? invalid ?? error} />
    </>
  );
}
