// The link of a page served by the gateway: everything on its own origin, the session in
// `HttpOnly` cookies this code never sees.

import type { GatewayError, GatewaySession, PairAccepted, PairPoll, PairRequest } from "@aurelius/protocol";
import type { Link, OpenLink } from "./connection";
import { LinkError } from "./errors";

async function errorText(response: Response, fallback: string): Promise<string> {
  try {
    return ((await response.json()) as GatewayError).error || fallback;
  } catch {
    return fallback;
  }
}

/** A new session cookie. A failed WebSocket upgrade tells a browser nothing, so this comes first. */
export async function refreshSession(origin: string): Promise<GatewaySession> {
  let response: Response;
  try {
    response = await fetch(`${origin}/v1/session`, { method: "POST", credentials: "same-origin" });
  } catch {
    throw new LinkError("unreachable", "The server is not reachable.");
  }
  if (response.status === 401)
    throw new LinkError("revoked", await errorText(response, "This browser is not paired with this server."));
  if (!response.ok) throw new LinkError("unreachable", await errorText(response, "The server did not answer."));
  return (await response.json()) as GatewaySession;
}

export function openWebSocket(url: string): Promise<Link> {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    let opened = false;
    socket.onopen = () => {
      opened = true;
      resolve({
        send: (text) => socket.send(text),
        close: () => socket.close(),
        listen: (onFrame, onClosed) => {
          socket.onmessage = (event) => {
            if (typeof event.data === "string") onFrame(event.data);
          };
          socket.onclose = (event) => onClosed(event.code);
        },
      });
    };
    socket.onclose = () => {
      if (!opened) reject(new LinkError("session", "The connection was refused."));
    };
  });
}

/** How a browser opens: a session cookie while it is good for a while, then the WebSocket of its own origin. */
export function webLink(origin: string): OpenLink {
  let validUntil = 0;
  return async () => {
    if (Date.now() >= validUntil) {
      const session = await refreshSession(origin);
      validUntil = session.expires_at_ms - 60_000;
    }
    try {
      return await openWebSocket(`${origin.replace(/^http/, "ws")}/v1/ws`);
    } catch (error) {
      validUntil = 0; // Perhaps the session ended: the next try asks, and that answer says why.
      throw error;
    }
  };
}

/** Pairs this browser; the secret arrives as an `HttpOnly` cookie. Resolves once it is allowed. */
export async function pairBrowser(origin: string, code: string, deviceName: string): Promise<void> {
  const body: PairRequest = { code: code.trim(), device_name: deviceName.trim(), platform: "web" };
  const started = await fetch(`${origin}/v1/pair`, {
    method: "POST",
    credentials: "same-origin",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (started.status !== 202) throw new Error(await errorText(started, "Pairing failed."));
  const { pairing_id } = (await started.json()) as PairAccepted;
  for (;;) {
    const polled = await fetch(`${origin}/v1/pair/${encodeURIComponent(pairing_id)}`, { credentials: "same-origin" });
    if (polled.status === 403) throw new Error("The pairing was refused.");
    if (!polled.ok) throw new Error(await errorText(polled, "This pairing request expired."));
    if (((await polled.json()) as PairPoll).status === "approved") return;
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
}
