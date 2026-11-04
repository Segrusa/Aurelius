import { expectBody } from "@aurelius/client";
import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useEffect, useState } from "react";
import { type Appearance, useAppearance } from "../appearance";
import { useConnection, useLinkState } from "../connection";
import { usePlatform } from "../platform";
import { parseScreenUse, SCREEN_USE_KEY } from "../screen-use";
import { Segments } from "../ui/kit";
import { ClaudeLogin, passwordProblem } from "./settings-accounts";
import {
  ConfirmButton,
  Group,
  Muted,
  Problem,
  Row,
  SmallButton,
  Switch,
  TextInput,
  useChange,
  useSetting,
  when,
} from "./settings-kit";

function ChangePassword() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [oldPassword, setOldPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const [done, setDone] = useState(false);
  const problem = passwordProblem(newPassword, repeat);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setDone(false);
    if (problem) return;
    const ok = await run(() => connection.setPassword(oldPassword, newPassword));
    setOldPassword("");
    setNewPassword("");
    setRepeat("");
    setDone(ok);
  };
  return (
    <div className="flex flex-col gap-2 px-4 py-[14px]">
      <div className="font-medium">Password</div>
      <form className="flex flex-wrap items-center gap-2" onSubmit={(e) => void submit(e)}>
        <TextInput
          label="Current password"
          placeholder="Current password"
          type="password"
          autoComplete="current-password"
          required
          value={oldPassword}
          onChange={(e) => setOldPassword(e.target.value)}
        />
        <TextInput
          label="New password"
          placeholder="New password"
          type="password"
          autoComplete="new-password"
          required
          value={newPassword}
          onChange={(e) => setNewPassword(e.target.value)}
        />
        <TextInput
          label="Repeat password"
          placeholder="Repeat password"
          type="password"
          autoComplete="new-password"
          required
          value={repeat}
          onChange={(e) => setRepeat(e.target.value)}
        />
        <SmallButton type="submit" disabled={busy || problem !== null}>
          Change password
        </SmallButton>
      </form>
      <Muted>{newPassword && problem ? problem : "At least 12 characters."}</Muted>
      {done ? <Muted>Your password is changed.</Muted> : null}
      <Problem error={error} />
    </div>
  );
}

function Sessions() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const { user } = useLinkState();
  const sessions = useSetting(
    "sessions",
    async () => expectBody(await connection.call("session.list", {}), "sessions").value.sessions,
  );
  return (
    <Group title="Signed in on">
      {(sessions.data ?? []).map((s) => (
        <Row key={s.id} title={s.device_name} hint={s.current ? "This device" : `Last seen ${when(s.last_seen_ms)}`}>
          {s.current ? null : (
            <ConfirmButton
              label="Sign out"
              question={`Sign out ${s.device_name}?`}
              disabled={busy}
              onConfirm={() =>
                void run(() => connection.write("session.revoke", { session_id: s.id }, { retry: false }))
              }
            />
          )}
        </Row>
      ))}
      {sessions.data?.length === 0 ? <Row title={user?.username ?? ""} hint="No other devices." /> : null}
      {(sessions.data ?? []).some((s) => !s.current) ? (
        <Row title="Sign out all other devices" hint="This device stays signed in.">
          <ConfirmButton
            label="Sign out all"
            question="Sign out all other devices?"
            disabled={busy}
            onConfirm={() =>
              void run(async () => {
                for (const s of sessions.data ?? []) {
                  if (!s.current) await connection.write("session.revoke", { session_id: s.id }, { retry: false });
                }
              })
            }
          />
        </Row>
      ) : null}
      <Problem error={sessions.error ?? error} />
    </Group>
  );
}

export function AccountSection() {
  const connection = useConnection();
  const { user } = useLinkState();
  return (
    <>
      <Group>
        <Row title={user?.username ?? "Nobody"} hint="The board member signed in on this device.">
          <SmallButton onClick={() => void connection.signOut()}>Sign out</SmallButton>
        </Row>
        <ChangePassword />
      </Group>
      <ClaudeLogin />
      <Sessions />
    </>
  );
}

const APPEARANCES: { key: Appearance; label: string }[] = [
  { key: "system", label: "System" },
  { key: "light", label: "Light" },
  { key: "dark", label: "Dark" },
];

function ScreenUse() {
  const { storage } = usePlatform();
  const [use, setUse] = useState(() => parseScreenUse(storage.get(SCREEN_USE_KEY)));
  const rows = Object.entries(use).sort((a, b) => b[1].opens - a[1].opens);
  return (
    <Group title="Screen use (this device only)">
      {rows.length === 0 ? <Row title="Nothing counted yet." /> : null}
      {rows.map(([key, u]) => (
        <Row key={key} title={key} hint={`Last opened ${when(u.last_ms)}`}>
          <span className="font-mono text-[12px] text-text2">{u.opens}×</span>
        </Row>
      ))}
      {rows.length > 0 ? (
        <Row title="Reset counts" hint="Counted here only; nothing goes to a server.">
          <SmallButton
            onClick={() => {
              storage.set(SCREEN_USE_KEY, "{}");
              setUse({});
            }}
          >
            Reset counts
          </SmallButton>
        </Row>
      ) : null}
    </Group>
  );
}

/** The service goes on when the window closes; whether it starts by itself is this computer's. */
function Background() {
  const { startup, kind } = usePlatform();
  const [on, setOn] = useState<boolean | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    if (startup) startup.get().then(setOn, () => setOn(null));
  }, [startup]);
  if (kind === "web") return null;
  return (
    <Group title="Background">
      {startup ? (
        <Row title="Start at sign-in" hint="The background service starts when you sign in to this computer.">
          <Switch
            label="Start at sign-in"
            on={on === true}
            disabled={on === null}
            onChange={(next) => {
              setProblem(null);
              startup
                .set(next)
                .then(setOn, (e: unknown) => setProblem(e instanceof Error ? e.message : "This did not work."));
            }}
          />
        </Row>
      ) : null}
      <Row title="Closing" hint="Closing the window: background work continues.">
        <span className="text-[12px] text-text2">Keeps running</span>
      </Row>
      <Problem error={problem} />
    </Group>
  );
}

/** Only for this computer's own install: a paired server can't update the app you look at. */
function AppUpdate() {
  const connection = useConnection();
  const client = useQueryClient();
  const platform = usePlatform();
  const { remote } = useLinkState();
  const { busy, error, run } = useChange();
  const status = useSetting(
    "update",
    async () => expectBody(await connection.call("update.get", { check: false }), "update").value,
  );
  const app = status.data?.app;
  if (platform.quit === null || remote || !app) return null;
  const check = () =>
    run(async () => {
      const body = await connection.call("update.get", { check: true });
      client.setQueryData(["settings", "update"], expectBody(body, "update").value);
    });
  const install = () =>
    run(async () => {
      const prepared = await connection.write("update.prepare_app", undefined, { retry: false, timeoutMs: 600_000 });
      const { result } = prepared;
      if (result.kind === "host" && result.value.type === "update" && result.value.error) {
        throw new Error(result.value.error);
      }
      await connection.write("update.apply_app", undefined, { retry: false });
      // The service replaces the app once it has closed, then opens the new one.
      platform.quit?.();
    });
  return (
    <Group title="App update">
      <Row title="Latest" hint={app.checked_at_ms ? `Checked ${when(app.checked_at_ms)}` : undefined}>
        <span className="font-mono text-[12px] text-text2">{app.latest ?? "Unknown"}</span>
        <SmallButton disabled={busy} onClick={() => void check()}>
          Check now
        </SmallButton>
      </Row>
      <Row title="Status" hint={app.error ?? app.reason ?? undefined}>
        <span className="text-[12px] text-text2">
          {!app.enabled ? "Updates are off" : app.available ? "An update is available" : "Up to date"}
        </span>
        {app.enabled && app.available ? (
          <ConfirmButton
            label="Update app"
            question="The app closes and comes back as the new version. Update now?"
            disabled={busy}
            onConfirm={() => void install()}
          />
        ) : null}
      </Row>
      {app.notes ? <Row title="What's new" hint={<span className="whitespace-pre-wrap">{app.notes}</span>} /> : null}
      <Problem error={error} />
    </Group>
  );
}

export function AppSection() {
  const { appearance, choose } = useAppearance();
  const platform = usePlatform();
  const { daemonVersion } = useLinkState();
  return (
    <>
      <Group>
        <Row title="Appearance" hint="Kept on this device.">
          <Segments label="Appearance" value={appearance} options={APPEARANCES} onChange={choose} />
        </Row>
      </Group>
      <ScreenUse />
      <Background />
      <AppUpdate />
      <Group title="Version">
        <Row title="This app" hint={platform.kind === "web" ? "In a browser" : "Desktop"}>
          <span className="font-mono text-[12px] text-text2">{platform.version}</span>
        </Row>
        <Row title="The server">
          <span className="font-mono text-[12px] text-text2">{daemonVersion ?? "Unknown"}</span>
        </Row>
      </Group>
    </>
  );
}
