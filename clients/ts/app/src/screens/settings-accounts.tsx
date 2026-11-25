// The Claude login of this board member, and what signs a board member in.

import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { expectBody } from "@aurelius/client";
import type { ClaudeLoginPrompt } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { Icon } from "../ui/icons";
import { FormDialog, Loading } from "./org-kit";
import {
  Chip,
  ConfirmButton,
  Group,
  IconBox,
  Muted,
  openWeb,
  Problem,
  Row,
  SmallButton,
  TextInput,
  useChange,
  when,
} from "./settings-kit";

const styles = stylex.create({
  quiet: { margin: 0, color: "var(--precision-text3)" },
  step: { display: "flex", alignItems: "center", gap: 12 },
  number: {
    display: "grid",
    width: 20,
    height: 20,
    flexShrink: 0,
    placeItems: "center",
    borderRadius: "50%",
    backgroundColor: "var(--color-overlay-hover)",
    fontSize: 12,
  },
  code: { display: "flex", flexDirection: "column", gap: 8 },
  send: { display: "flex", gap: 8 },
  grow: { minWidth: 0, flex: 1 },
  session: { display: "flex", alignItems: "center", gap: 8 },
  device: { minWidth: 0, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  seen: { display: "block", fontSize: 12, color: "var(--precision-text3)" },
  temporary: {
    display: "flex",
    flexDirection: "column",
    gap: 8,
    borderBlockStartWidth: 1,
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--precision-line2)",
    paddingBlockStart: 12,
  },
  label: { fontWeight: 500 },
  start: { alignSelf: "flex-start" },
});

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
    <FormDialog
      title={CONNECT_TITLE}
      onClose={cancel}
      footer={
        <>
          {failed || problem ? <Button label="Try again" onClick={() => void start()} /> : null}
          <Button label="Cancel" onClick={cancel} />
        </>
      }
    >
      {!prompt && !problem ? <p {...stylex.props(styles.quiet)}>Starting…</p> : null}
      {prompt?.url ? (
        <div {...stylex.props(styles.step)}>
          <span {...stylex.props(styles.number)}>1</span>
          <Button variant="primary" label="Open sign-in page ↗" onClick={() => openWeb(prompt.url as string)} />
        </div>
      ) : null}
      {prompt?.asks_code && !failed ? (
        <div {...stylex.props(styles.code)}>
          <span {...stylex.props(styles.step)}>
            <span {...stylex.props(styles.number)}>2</span>
            Paste the code Claude shows you.
          </span>
          <span {...stylex.props(styles.send)}>
            <span {...stylex.props(styles.grow)}>
              <TextInput
                label="Code"
                mono
                autoFocus
                autoComplete="off"
                placeholder="Paste the code Claude shows you"
                value={code}
                onChange={(e) => setCode(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void send();
                  }
                }}
              />
            </span>
            <Button
              label={sent ? "Checking…" : "Connect"}
              isDisabled={sent || code.trim() === ""}
              onClick={() => void send()}
            />
          </span>
        </div>
      ) : null}
      {failed ? <Banner status="error" role="alert" title={failed} /> : null}
      <Problem error={problem} />
      <Muted>Aurelius never sees your Claude password.</Muted>
    </FormDialog>
  );
}

const STATUS = {
  linked: { title: "Connected", chip: "Linked", tone: "ok" },
  expired: { title: "Expired", chip: "Sign in again", tone: "warn" },
  missing: { title: "Not connected", chip: "Your tickets wait", tone: "bad" },
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
    <>
      <Group title="Claude login">
        <Row
          start={
            <IconBox>
              <Icon name="plug" />
            </IconBox>
          }
          title={
            claudeLogin ? (
              <>
                {STATUS[claudeLogin.status].title}
                <Chip tone={STATUS[claudeLogin.status].tone} dot={claudeLogin.status === "linked"}>
                  {STATUS[claudeLogin.status].chip}
                </Chip>
              </>
            ) : (
              "Unknown"
            )
          }
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
      </Group>
      {connecting ? <Connect onDone={() => setConnecting(false)} /> : null}
    </>
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
    <FormDialog title={`Sessions · ${name}`} onClose={onClose} footer={<Button label="Close" onClick={onClose} />}>
      {sessions === null ? <Loading rows={2} /> : null}
      {sessions?.length === 0 ? <p {...stylex.props(styles.quiet)}>Not signed in anywhere.</p> : null}
      {sessions?.map((s) => (
        <div key={s.id} {...stylex.props(styles.session)}>
          <span {...stylex.props(styles.device)}>
            {s.device_name}
            <span {...stylex.props(styles.seen)}>Last seen {when(s.last_seen_ms)}</span>
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
        {...stylex.props(styles.temporary)}
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
        <span {...stylex.props(styles.label)}>Temporary password</span>
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
        <span {...stylex.props(styles.start)}>
          <SmallButton type="submit" disabled={busy || problem !== null}>
            Set temporary password
          </SmallButton>
        </span>
      </form>
      <Problem error={error} />
    </FormDialog>
  );
}
