import { expectBody } from "@aurelius/client";
import { type FormEvent, useState } from "react";
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
