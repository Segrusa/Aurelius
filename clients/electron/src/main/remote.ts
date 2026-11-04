// A server elsewhere, over the gateway: a session from the device secret, then a WebSocket with
// that session as Bearer. The page never sees either; it gets frames, as from the local pipe.

import { type Link, LinkError } from "@aurelius/client";
import WebSocket from "ws";
import type { PairedServer, PairingPayload } from "./servers";

const PING_EVERY_MS = 20_000;
const SILENT_AFTER_MS = 45_000;
/** A device the server revoked (crates/client/src/transport.rs). */
const REVOKED = 4401;

type Fetch = typeof globalThis.fetch;

async function errorText(response: Response, fallback: string): Promise<string> {
  try {
    return ((await response.json()) as { error?: string }).error || fallback;
  } catch {
    return fallback;
  }
}

export async function openSession(
  server: Pick<PairedServer, "url" | "client_id" | "host_name">,
  secret: string,
  fetcher: Fetch,
): Promise<string> {
  const basic = Buffer.from(`${server.client_id}:${secret}`).toString("base64");
  let response: Response;
  try {
    response = await fetcher(new URL("/v1/session", server.url), {
      method: "POST",
      headers: { Authorization: `Basic ${basic}` },
      signal: AbortSignal.timeout(10_000),
    });
  } catch {
    throw new LinkError("unreachable", `${server.host_name} is not reachable.`);
  }
  if (response.status === 401) {
    throw new LinkError("revoked", `${server.host_name} no longer knows this computer. Pair it again.`);
  }
  if (!response.ok) throw new LinkError("unreachable", await errorText(response, "The server did not answer."));
  return ((await response.json()) as { token: string }).token;
}

export function openRemote(url: string, token: string): Promise<Link> {
  const address = new URL("/v1/ws", url);
  address.protocol = address.protocol === "https:" ? "wss:" : "ws:";
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(address, { headers: { Authorization: `Bearer ${token}` }, handshakeTimeout: 10_000 });
    let lastHeard = Date.now();
    socket.once("error", () => reject(new LinkError("unreachable", "The server is not reachable.")));
    socket.once("unexpected-response", (_request, response) => {
      reject(
        response.statusCode === 401
          ? new LinkError("session", "The server refused this session.")
          : new LinkError("unreachable", "The server did not accept the connection."),
      );
      socket.terminate();
    });
    socket.once("open", () => {
      socket.removeAllListeners("error");
      socket.on("error", () => socket.terminate());
      socket.on("pong", () => {
        lastHeard = Date.now();
      });
      const pinging = setInterval(() => {
        if (Date.now() - lastHeard > SILENT_AFTER_MS) socket.terminate();
        else if (socket.readyState === WebSocket.OPEN) socket.ping();
      }, PING_EVERY_MS);
      socket.once("close", () => clearInterval(pinging));
      resolve({
        send: (text) => socket.send(text),
        close: () => socket.close(),
        listen: (onFrame, onClosed) => {
          socket.on("message", (data, isBinary) => {
            lastHeard = Date.now();
            if (!isBinary) onFrame(data.toString());
          });
          socket.once("close", (code) => onClosed(code === REVOKED ? REVOKED : code));
        },
      });
    });
  });
}

/** Sends one file to this device's inbox on the server; a ticket then names it by that path. */
export async function uploadFile(
  url: string,
  token: string,
  name: string,
  bytes: Uint8Array,
  fetcher: Fetch,
): Promise<{ path: string; name: string }> {
  const body = new FormData();
  body.append("file", new Blob([Buffer.from(bytes)]), name);
  const response = await fetcher(new URL("/v1/upload", url), {
    method: "POST",
    headers: { Authorization: `Bearer ${token}` },
    body,
  });
  if (!response.ok) throw new Error(await errorText(response, `${name} did not upload.`));
  return (await response.json()) as { path: string; name: string };
}

export interface Approved {
  client_id: string;
  device_secret: string;
  host_id: string;
  host_name: string;
  environment: string;
}

/** Asks the server for owner access and waits for its answer; the server's person decides there. */
export async function requestPairing(
  payload: PairingPayload,
  deviceName: string,
  platform: string,
  signal: AbortSignal,
  fetcher: Fetch,
): Promise<Approved> {
  const started = await fetcher(new URL("/v1/pair", payload.url), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code: payload.code, device_name: deviceName, platform, role: "owner" }),
    signal,
  });
  if (!started.ok) throw new Error(await errorText(started, "The server refused this pairing."));
  const { pairing_id: id } = (await started.json()) as { pairing_id: string };
  for (;;) {
    const answer = await fetcher(new URL(`/v1/pair/${encodeURIComponent(id)}`, payload.url), { signal });
    if (answer.status === 403) throw new Error("The server refused this computer.");
    if (answer.status === 404) throw new Error("This pairing code expired. Make a new one on the server.");
    if (!answer.ok) throw new Error(await errorText(answer, "The server did not answer."));
    const body = (await answer.json()) as { status: string } & Approved;
    if (body.status !== "approved") continue;
    if (body.host_id !== payload.host_id) throw new Error("Another server answered than the pairing text names.");
    return body;
  }
}
