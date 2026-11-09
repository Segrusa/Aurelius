import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { expectBody } from "@aurelius/client";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useEffect, useState } from "react";
import { type Appearance, useAppearance } from "../appearance";
import { useConnection, useLinkState } from "../connection";
import { useAction } from "../keys/provider";
import { usePlatform } from "../platform";
import { parseScreenUse, SCREEN_USE_KEY } from "../screen-use";
import { ClaudeLogin, passwordProblem } from "./settings-accounts";
import {
  ConfirmButton,
  Group,
  HINTS_KEY,
  Keyed,
  Muted,
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
import { screenUseRows } from "./settings-logic";

const styles = stylex.create({
  label: { fontWeight: 500 },
  form: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 8 },
  value: { fontFamily: "var(--font-family-code)", fontSize: 12, color: "var(--color-text-secondary)" },
  status: { fontSize: 12, color: "var(--color-text-secondary)" },
  notes: { whiteSpace: "pre-wrap" },
});

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
    <Part>
      <div {...stylex.props(styles.label)}>Password</div>
      <form {...stylex.props(styles.form)} onSubmit={(e) => void submit(e)}>
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
    </Part>
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
  const rows = screenUseRows(use);
  return (
    <Group title="Screen use (this device only)">
      {rows.length === 0 ? <Row title="Nothing counted yet." /> : null}
      {rows.map(([key, u]) => (
        <Row key={key} title={key} hint={`Last opened ${when(u.last_ms)}`}>
          <span {...stylex.props(styles.value)}>{u.opens}×</span>
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
          <Keyed action="SettingsLoginStart" chord="L">
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
          </Keyed>
        </Row>
      ) : null}
      <Row title="Closing" hint="Closing the window: background work continues.">
        <span {...stylex.props(styles.status)}>Keeps running</span>
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
        <span {...stylex.props(styles.value)}>{app.latest ?? "Unknown"}</span>
        <Keyed action="SettingsCheckUpdate" chord="C">
          <SmallButton disabled={busy} onClick={() => void check()}>
            Check now
          </SmallButton>
        </Keyed>
      </Row>
      <Row title="Status" hint={app.error ?? app.reason ?? undefined}>
        <span {...stylex.props(styles.status)}>
          {!app.enabled ? "Updates are off" : app.available ? "An update is available" : "Up to date"}
        </span>
        {app.enabled && app.available ? (
          <Keyed action="SettingsInstallAppUpdate" chord="I">
            <ConfirmButton
              label="Update app"
              question="The app closes and comes back as the new version. Update now?"
              disabled={busy}
              onConfirm={() => void install()}
            />
          </Keyed>
        ) : null}
      </Row>
      {app.notes ? <Row title="What's new" hint={<span {...stylex.props(styles.notes)}>{app.notes}</span>} /> : null}
      <Problem error={error} />
    </Group>
  );
}

export function AppSection() {
  const { appearance, choose } = useAppearance();
  const platform = usePlatform();
  const { daemonVersion } = useLinkState();
  const { storage } = platform;
  const [hints, setHints] = useState(() => storage.get(HINTS_KEY) === "true");
  useAction("SettingsAppearance", () => {
    const at = APPEARANCES.findIndex((a) => a.key === appearance);
    const next = APPEARANCES[(at + 1) % APPEARANCES.length];
    if (next) choose(next.key);
  });
  return (
    <>
      <Group>
        <Row title="Appearance" hint="Kept on this device.">
          <SegmentedControl
            label="Appearance"
            size="sm"
            value={appearance}
            onChange={(next) => choose(next as Appearance)}
          >
            {APPEARANCES.map((a) => (
              <SegmentedControlItem key={a.key} value={a.key} label={a.label} />
            ))}
          </SegmentedControl>
        </Row>
      </Group>
      <Group>
        <Row title="Always show shortcut hints" hint="The keys of this page, beside its buttons.">
          <Keyed action="SettingsShowHints" chord="H">
            <Switch
              label="Always show shortcut hints"
              on={hints}
              onChange={(on) => {
                setHints(on);
                storage.set(HINTS_KEY, String(on));
                document.documentElement.toggleAttribute("data-hints", on);
              }}
            />
          </Keyed>
        </Row>
      </Group>
      <ScreenUse />
      <Background />
      <AppUpdate />
      <Group title="Version">
        <Row title="This app" hint={platform.kind === "web" ? "In a browser" : "Desktop"}>
          <span {...stylex.props(styles.value)}>{platform.version}</span>
        </Row>
        <Row title="The server">
          <span {...stylex.props(styles.value)}>{daemonVersion ?? "Unknown"}</span>
        </Row>
      </Group>
    </>
  );
}
