import { expectBody } from "@aurelius/client";
import type { LoadLimitsUpdate } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useEffect, useState } from "react";
import { useConnection } from "../connection";
import {
  bytes,
  ConfirmButton,
  Group,
  Muted,
  modelName,
  Problem,
  Row,
  SmallButton,
  Switch,
  TextInput,
  useChange,
  useSetting,
  when,
} from "./settings-kit";

export function ConnectionsSection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const list = useSetting(
    "connections",
    async () => expectBody(await connection.call("connection.list"), "connections").value,
  );
  return (
    <>
      <Group title="Connections">
        {list.data?.connections.length === 0 ? <Row title="No connections yet." /> : null}
        {(list.data?.connections ?? []).map((c) => (
          <Row
            key={c.id}
            title={c.display_name}
            hint={[c.status, c.default_model ? modelName(c.default_model) : null, `Checked ${when(c.last_verified_ms)}`]
              .filter(Boolean)
              .join(" · ")}
          >
            <SmallButton
              disabled={busy}
              onClick={() =>
                void run(() => connection.write("connection.verify", { connection_id: c.id }, { retry: false }))
              }
            >
              Check
            </SmallButton>
          </Row>
        ))}
      </Group>
      <Muted>Adding or removing a connection still goes through the desktop app.</Muted>
      <Problem error={list.error ?? error} />
    </>
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
      <Group title="MCP servers">
        {servers.length === 0 ? <Row title="No MCP servers yet." /> : null}
        {servers.map((s) => (
          <Row key={s.name} title={s.name} hint={s.summary}>
            <Switch
              label={`Use ${s.name}`}
              on={s.enabled}
              disabled={busy}
              onChange={(on) => void setEnabled(s.name, on)}
            />
            <ConfirmButton
              label="Remove"
              question={`Remove ${s.name}?`}
              disabled={busy}
              onConfirm={() => void run(() => connection.write("mcp.remove", { name: s.name }, { retry: false }))}
            />
          </Row>
        ))}
      </Group>
      <Muted>Adding a server or signing in to one still goes through the desktop app.</Muted>
      <Problem error={list.error ?? auto.error ?? error} />
    </>
  );
}

export function SchedulesSection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const list = useSetting(
    "schedules",
    async () => expectBody(await connection.call("recurring_task.list", {}), "task_schedules").value.schedules,
  );
  return (
    <>
      <Group title="Recurring schedules">
        {list.data?.length === 0 ? <Row title="No recurring tasks yet." /> : null}
        {(list.data ?? []).map((s) => (
          <Row
            key={s.id}
            title={s.title}
            hint={`${s.employee_name} · ${s.paused ? "Paused" : `Next ${when(s.next_run_ms)}`} · Last ${when(s.last_run_ms)}`}
          >
            <SmallButton
              disabled={busy}
              onClick={() =>
                void run(() => connection.write("recurring_task.run_now", { schedule_id: s.id }, { retry: false }))
              }
            >
              Run now
            </SmallButton>
            <Switch
              label={`${s.title} is active`}
              on={!s.paused}
              disabled={busy}
              onChange={(on) =>
                void run(() =>
                  connection.write("recurring_task.set_paused", { schedule_id: s.id, paused: !on }, { retry: false }),
                )
              }
            />
          </Row>
        ))}
      </Group>
      <Muted>Making or editing a recurring task still goes through the desktop app.</Muted>
      <Problem error={list.error ?? error} />
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
          <SmallButton
            disabled={busy}
            onClick={() =>
              void run(() => connection.write("backup.create", undefined, { retry: false, timeoutMs: 600_000 }))
            }
          >
            {busy ? "Working…" : "Back up now"}
          </SmallButton>
        </Row>
        {data?.paused ? (
          <Row title="Automatic backups are paused" hint={data.paused}>
            <SmallButton
              disabled={busy}
              onClick={() => void run(() => connection.write("backup.resume", undefined, { retry: false }))}
            >
              Resume
            </SmallButton>
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
            hint={`${b.kind} · ${bytes(b.bytes)} · ${b.files} files · version ${b.app_version}`}
          >
            <ConfirmButton
              label="Restore"
              question="Replace today's data with this?"
              disabled={busy}
              onConfirm={() => void run(() => connection.write("backup.restore", { id: b.id }, { retry: false }))}
            />
          </Row>
        ))}
      </Group>
      <Problem error={list.error ?? data?.error ?? error} />
    </>
  );
}

const LIMITS: { key: "cpu_high" | "cpu_low" | "mem_min_mb" | "max_runs"; label: string; hint: string }[] = [
  { key: "cpu_high", label: "Hold new runs above", hint: "CPU percentage." },
  { key: "cpu_low", label: "Start again below", hint: "CPU percentage." },
  { key: "mem_min_mb", label: "Keep memory free", hint: "Megabytes." },
  { key: "max_runs", label: "Runs at once", hint: "At most." },
];

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
    if (draft[key] !== undefined && draft[key] !== "" && Number.isFinite(value) && value !== data?.limits[key])
      changes[key] = value;
  }
  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (await run(() => connection.write("load.set_limits", changes, { retry: false }))) setDraft({});
  };
  return (
    <>
      <Group title="Now">
        <Row
          title={data?.gate === "held" ? "New runs are waiting" : "New runs start right away"}
          hint={data ? `${data.held} waiting · ${data.running} running` : undefined}
        >
          {data ? (
            <span className="text-[12px] text-text2">
              CPU {Math.round(data.cpu_pct)}% · {bytes(data.mem_free_mb * 1024 * 1024)} free of{" "}
              {bytes(data.mem_total_mb * 1024 * 1024)}
            </span>
          ) : null}
        </Row>
      </Group>
      <form onSubmit={(e) => void save(e)} className="contents">
        <Group title="Limits">
          <Row title="Hold runs when the computer is busy">
            <Switch
              label="Hold runs when the computer is busy"
              on={data?.limits.enabled ?? false}
              disabled={busy || !data}
              onChange={(enabled) => void run(() => connection.write("load.set_limits", { enabled }, { retry: false }))}
            />
          </Row>
          {LIMITS.map(({ key, label, hint }) => (
            <Row key={key} title={label} hint={hint}>
              <TextInput
                label={label}
                type="number"
                min={0}
                className="w-[90px] text-right"
                value={draft[key] ?? data?.limits[key] ?? ""}
                onChange={(e) => setDraft((d) => ({ ...d, [key]: e.target.value }))}
              />
            </Row>
          ))}
        </Group>
        {Object.keys(changes).length > 0 ? (
          <div>
            <SmallButton kind="primary" type="submit" disabled={busy}>
              Save limits
            </SmallButton>
          </div>
        ) : null}
      </form>
      <Problem error={load.error ?? error} />
    </>
  );
}

export function TraceSection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const trace = useSetting(
    "trace",
    async () => expectBody(await connection.call("trace.get_setting"), "trace_setting").value,
  );
  return (
    <>
      <Group>
        <Row
          title="Keep a trace of every run"
          hint={trace.data ? `Kept for ${trace.data.keep_days} days, at most ${trace.data.keep_runs} runs.` : undefined}
        >
          <Switch
            label="Keep a trace of every run"
            on={trace.data?.enabled ?? false}
            disabled={busy || !trace.data}
            onChange={(enabled) => void run(() => connection.write("trace.set_enabled", { enabled }, { retry: false }))}
          />
        </Row>
      </Group>
      <Problem error={trace.error ?? error} />
    </>
  );
}
