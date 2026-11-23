import { expectBody } from "@aurelius/client";
import type { PairingRequestView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { usePlatform } from "../platform";
import { QrCode } from "./qr-code";
import {
  ConfirmButton,
  Group,
  Keyed,
  Muted,
  Problem,
  Row,
  SmallButton,
  useChange,
  useSetting,
  when,
} from "./settings-kit";
import { deviceHint, OWN_DEVICES } from "./settings-logic";

const styles = stylex.create({
  code: { fontFamily: "var(--font-family-code)", fontSize: 15, fontWeight: 500, letterSpacing: "0.08em" },
});

/**
 * "Show code" and the QR for a new device. The server remembers who asked for the code:
 * the device that enters it becomes theirs, and only they are asked to allow it.
 */
export function PairDevice({ title }: { title: string }) {
  const platform = usePlatform();
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [code, setCode] = useState<{ code: string; expires: number; text: string; qr: string } | null>(null);
  const startPairing = () =>
    run(async () => {
      const receipt = await connection.write("remote.start_pairing", undefined, { retry: false });
      const { result } = receipt;
      if (result.kind === "host" && result.value.type === "pairing_code")
        setCode({
          code: result.value.code,
          expires: result.value.expires_at_ms,
          text: result.value.pairing_text,
          qr: result.value.qr,
        });
    });
  return (
    <>
      <Row
        title={title}
        hint={code ? `Valid until ${when(code.expires)}.` : "Shows a one-time code to enter on the other device."}
      >
        {code ? <span {...stylex.props(styles.code)}>{code.code}</span> : null}
        <Keyed action="SettingsNew" chord="N">
          <SmallButton disabled={busy} onClick={() => void startPairing()}>
            {code ? "New code" : "Show code"}
          </SmallButton>
        </Keyed>
      </Row>
      {code ? (
        <Row
          title="On a phone: scan this"
          hint={code.text ? "Another computer: copy the pairing text and paste it there." : undefined}
        >
          {code.text ? (
            <Keyed action="SettingsCopyPairing" chord="Y">
              <SmallButton onClick={() => void platform.copy(code.text)}>Copy pairing text</SmallButton>
            </Keyed>
          ) : null}
          {code.qr ? <QrCode payload={code.qr} label="Pairing code for a phone" /> : null}
        </Row>
      ) : null}
      <Problem error={error} />
    </>
  );
}

/** Devices that entered one of your codes and wait for your answer. */
export function PendingPairings({ pending }: { pending: PairingRequestView[] }) {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  if (pending.length === 0) return null;
  const decide = (pairingId: string, approve: boolean, owner: boolean) =>
    void run(() =>
      connection.write("remote.decide_pairing", { pairing_id: pairingId, approve, owner }, { retry: false }),
    );
  return (
    <Group title="Waiting for your answer">
      {pending.map((p) => (
        <Row
          key={p.pairing_id}
          title={p.device_name}
          hint={`${p.platform} · asked ${when(p.requested_at_ms)}${p.owner ? ". It asks for full access: it can do everything this app does here, including API keys, folders on this PC, backups and updates." : ""}`}
        >
          <Keyed action="SettingsUse" chord="Enter">
            <SmallButton kind="primary" disabled={busy} onClick={() => decide(p.pairing_id, true, false)}>
              Allow
            </SmallButton>
          </Keyed>
          {p.owner ? (
            <SmallButton disabled={busy} onClick={() => decide(p.pairing_id, true, true)}>
              Allow as owner
            </SmallButton>
          ) : null}
          <Keyed action="SettingsRefusePairing" chord="X">
            <SmallButton kind="danger" disabled={busy} onClick={() => decide(p.pairing_id, false, false)}>
              Refuse
            </SmallButton>
          </Keyed>
        </Row>
      ))}
      <Problem error={error} />
    </Group>
  );
}

/**
 * Account → Your devices: the devices you paired, never another board member's. Adding one
 * needs remote access on and a device that may manage it (the server's computer or an owner).
 */
export function YourDevices() {
  const platform = usePlatform();
  const connection = useConnection();
  const { features } = useLinkState();
  const { busy, error, run } = useChange();
  const devices = useSetting(
    "devices",
    async () => expectBody(await connection.call("device.list"), "devices").value.devices,
  );
  const remote = useSetting(
    "remote",
    async () => expectBody(await connection.call("remote.get_status"), "remote").value,
  );
  if (!features.includes(OWN_DEVICES)) return null;
  // A browser never allows other devices; a phone or other non-owner device cannot read the status.
  const canPair = platform.kind !== "web" && remote.data !== undefined;
  return (
    <>
      <Group title="Your devices">
        {(devices.data ?? []).map((d) => (
          <Row key={d.id} title={d.name} hint={deviceHint(d, when)}>
            <Keyed action="SettingsRemove" chord="Del">
              <ConfirmButton
                label="Revoke"
                question={`Revoke ${d.name}? It is signed out and must be paired again.`}
                disabled={busy}
                onConfirm={() =>
                  void run(() => connection.write("device.revoke", { client_id: d.id }, { retry: false }))
                }
              />
            </Keyed>
          </Row>
        ))}
        {devices.data?.length === 0 ? (
          <Row title="No paired devices." hint="Phones and computers you pair show here." />
        ) : null}
        {canPair && remote.data?.enabled ? <PairDevice title="Add a device" /> : null}
        {canPair && remote.data?.enabled === false ? (
          <Row title="Add a device" hint="Turn on remote access first, under Workspace → Remote access." />
        ) : null}
        <Problem error={devices.error ?? error} />
      </Group>
      {canPair && remote.data ? <PendingPairings pending={remote.data.pending} /> : null}
      {devices.data && devices.data.length > 0 ? (
        <Muted>Only you see these devices. Revoking one signs out everyone on it.</Muted>
      ) : null}
    </>
  );
}
