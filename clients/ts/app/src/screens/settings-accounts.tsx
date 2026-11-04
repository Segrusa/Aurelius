// The Claude login of this board member, and what signs a board member in.

import { expectBody } from "@aurelius/client";
import type { ClaudeLoginPrompt } from "@aurelius/protocol";
import { useEffect, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { Button } from "../ui/kit";
import { Dialog } from "../ui/overlay";
import { FIELD } from "./org-kit";
import {
  ConfirmButton,
  Group,
  Muted,
  openWeb,
  Problem,
  Row,
  SmallButton,
  TextInput,
  useChange,
  when,
} from "./settings-kit";

export const CONNECT_TITLE = "Connect your own Claude account (at Anthropic)";
export const PASSWORD_MIN = 12;

/** Why a new password cannot be sent yet, or null. */
export function passwordProblem(password: string, repeat: string): string | null {
  if (password.length < PASSWORD_MIN) return "At least 12 characters.";
  return password === repeat ? null : "The two passwords are not the same.";
}

function Connect({ onDone }: { onDone: () => void }) {
  const connection = useConnection();
  const [prompt, setPrompt] = useState<ClaudeLoginPrompt | null>(null);
  const [link, setLink] = useState<string | null>(null);
  const [code, setCode] = useState("");
  const [sent, setSent] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const start = async () => {
    setProblem(null);
    setPrompt(null);
    setCode("");
    setSent(false);
    try {
      const receipt = await connection.write("claude_login.link", undefined, { retry: false });
      const { result } = receipt;
      if (result.kind === "host" && result.value.type === "claude_login_link") setLink(result.value.link_id);
    } catch (e) {
      setProblem(e instanceof Error ? e.message : "This did not work.");
    }
  };
  // biome-ignore lint/correctness/useExhaustiveDependencies: the link starts once, on opening.
  useEffect(() => {
    void start();
  }, []);
  useEffect(() => connection.claudeLoginPrompts.on(setPrompt), [connection]);
  const failed = prompt?.error ?? null;
  const send = async () => {
    if (!prompt || code.trim() === "") return;
    setSent(true);
    try {
      await connection.call("claude_login.input", { link_id: prompt.link_id, text: code.trim() });
      setCode("");
    } catch (e) {
      setSent(false);
      setProblem(e instanceof Error ? e.message : "This did not work.");
    }
  };
  const cancel = () => {
    const id = prompt?.link_id ?? link;
    if (id) void connection.write("claude_login.cancel", { link_id: id }, { retry: false }).catch(() => undefined);
    onDone();
  };
  return (
    <Dialog open onClose={cancel} title={CONNECT_TITLE} width={520}>
      <div className="flex flex-col gap-3 p-4">
        <p className="m-0 text-[15px] font-semibold">{CONNECT_TITLE}</p>
        {!prompt && !problem ? <p className="m-0 text-text3">Starting…</p> : null}
        {prompt?.url ? (
          <div className="flex items-center gap-3">
            <span className="grid size-5 flex-none place-items-center rounded-full bg-sel text-[12px]">1</span>
            <Button kind="primary" onClick={() => openWeb(prompt.url as string)}>
              Open sign-in page ↗
            </Button>
          </div>
        ) : null}
        {prompt?.asks_code && !failed ? (
          <label className="flex flex-col gap-2">
            <span className="flex items-center gap-3">
              <span className="grid size-5 flex-none place-items-center rounded-full bg-sel text-[12px]">2</span>
              Paste the code Claude shows you.
            </span>
            <span className="flex gap-2">
              <input
                data-autofocus
                aria-label="Code"
                autoComplete="off"
                placeholder="Paste the code Claude shows you"
                className={`${FIELD} font-mono`}
                value={code}
                onChange={(e) => setCode(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void send();
                  }
                }}
              />
              <Button disabled={sent || code.trim() === ""} onClick={() => void send()}>
                {sent ? "Checking…" : "Connect"}
              </Button>
            </span>
          </label>
        ) : null}
        {failed ? (
          <p role="alert" className="m-0 text-red">
            {failed}
          </p>
        ) : null}
        <Problem error={problem} />
        <Muted>Aurelius never sees your Claude password.</Muted>
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        {failed || problem ? <Button onClick={() => void start()}>Try again</Button> : null}
        <Button onClick={cancel}>Cancel</Button>
      </div>
    </Dialog>
  );
}

const STATUS = {
  linked: "Connected",
  expired: "Expired. Claude asked to sign in again.",
  missing: "Not connected. Your tickets wait until you connect.",
} as const;

export function ClaudeLogin() {
  const connection = useConnection();
  const { claudeLogin } = useLinkState();
  const { busy, error, run } = useChange();
  const [connecting, setConnecting] = useState(false);
  // A login that worked comes as an event: the dialog then has nothing left to ask.
  const linked = claudeLogin?.status === "linked";
  useEffect(() => {
    if (linked) setConnecting(false);
  }, [linked]);
  const check = () =>
    run(async () => {
      await connection.refreshClaudeLogin();
      await connection.call("subscription.get", { refresh: true });
    });
  return (
    <Group title="Claude login">
      <Row
        title={claudeLogin ? STATUS[claudeLogin.status] : "Unknown"}
        hint={
          [claudeLogin?.email, claudeLogin?.plan].filter(Boolean).join(" · ") ||
          "Tickets you create run on your own Claude login."
        }
      >
        {linked ? (
          <SmallButton disabled={busy} onClick={() => void check()}>
            {busy ? "Checking…" : "Check now"}
          </SmallButton>
        ) : null}
        {claudeLogin?.status === "expired" ? (
          <SmallButton kind="primary" onClick={() => setConnecting(true)}>
            Reconnect
          </SmallButton>
        ) : null}
        {claudeLogin?.status === "missing" ? (
          <SmallButton kind="primary" onClick={() => setConnecting(true)}>
            Connect
          </SmallButton>
        ) : null}
        {claudeLogin && claudeLogin.status !== "missing" ? (
          <ConfirmButton
            label="Disconnect"
            question="Disconnect your Claude login? Your tickets wait until you connect again."
            disabled={busy}
            onConfirm={() =>
              void run(async () => {
                await connection.write("claude_login.unlink", undefined, { retry: false });
                await connection.refreshClaudeLogin();
              })
            }
          />
        ) : null}
      </Row>
      <Problem error={error} />
      {connecting ? <Connect onDone={() => setConnecting(false)} /> : null}
    </Group>
  );
}

/** A board member's devices, and a temporary password for someone who lost theirs. */
export function MemberAccess({ user, name, onClose }: { user: string; name: string; onClose: () => void }) {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [sessions, setSessions] = useState<{ id: string; device_name: string; last_seen_ms: number }[] | null>(null);
  const [password, setPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const [done, setDone] = useState(false);
  const read = () =>
    connection
      .call("session.list", { user_id: user })
      .then((body) => setSessions(expectBody(body, "sessions").value.sessions))
      .catch(() => setSessions([]));
  // biome-ignore lint/correctness/useExhaustiveDependencies: read once for this member.
  useEffect(() => {
    void read();
  }, [user]);
  const problem = passwordProblem(password, repeat);
  return (
    <Dialog open onClose={onClose} title={`Sessions · ${name}`} width={520}>
      <div className="flex flex-col gap-3 p-4">
        <p className="m-0 text-[15px] font-semibold">Sessions · {name}</p>
        {sessions === null ? <p className="m-0 text-text3">Loading…</p> : null}
        {sessions?.length === 0 ? <p className="m-0 text-text3">Not signed in anywhere.</p> : null}
        {sessions?.map((s) => (
          <div key={s.id} className="flex items-center gap-2">
            <span className="min-w-0 flex-1 truncate">
              {s.device_name}
              <span className="block text-[12px] text-text3">Last seen {when(s.last_seen_ms)}</span>
            </span>
            <SmallButton
              disabled={busy}
              onClick={() =>
                void run(() => connection.write("session.revoke", { session_id: s.id }, { retry: false })).then(read)
              }
            >
              Sign out
            </SmallButton>
          </div>
        ))}
        <form
          className="flex flex-col gap-2 border-t border-line2 pt-3"
          onSubmit={(e) => {
            e.preventDefault();
            if (problem) return;
            void run(() =>
              connection.write("user.set_password", { user_id: user, new_password: password }, { retry: false }),
            ).then((ok) => {
              setPassword("");
              setRepeat("");
              setDone(ok);
            });
          }}
        >
          <span className="font-medium">Temporary password</span>
          <TextInput
            label="Temporary password"
            type="password"
            autoComplete="new-password"
            placeholder="Temporary password"
            value={password}
            onChange={(e) => {
              setPassword(e.target.value);
              setDone(false);
            }}
          />
          <TextInput
            label="Repeat password"
            type="password"
            autoComplete="new-password"
            placeholder="Repeat password"
            value={repeat}
            onChange={(e) => setRepeat(e.target.value)}
          />
          <Muted>At least 12 characters. They must choose a new one at their next sign-in.</Muted>
          {password && problem ? <Muted>{problem}</Muted> : null}
          {done ? <Muted>The temporary password is set.</Muted> : null}
          <SmallButton type="submit" className="self-start" disabled={busy || problem !== null}>
            Set temporary password
          </SmallButton>
        </form>
        <Problem error={error} />
      </div>
      <div className="flex justify-end border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Close</Button>
      </div>
    </Dialog>
  );
}
