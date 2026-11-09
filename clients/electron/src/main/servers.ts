// The paired servers of this device, in the file the Rust client also reads
// (crates/client/src/servers.rs), and the pairing text a server gives out.

import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

export interface PairedServer {
  host_id: string;
  host_name: string;
  url: string;
  client_id: string;
  environment: string;
  last_used_ms: number;
}

export interface ServerList {
  /** Null: this computer's own service. */
  active: string | null;
  servers: PairedServer[];
  local_name?: string;
}

export interface PairingPayload {
  v: number;
  url: string;
  code: string;
  host_id: string;
  host_name: string;
  env: string;
}

const PAIRING_TEXT_PREFIX = "aurelius-pair:";
const LOOPBACK = new Set(["127.0.0.1", "localhost", "[::1]"]);

const file = (root: string) => join(root, "client", "servers.json");

export function loadServers(root: string): ServerList {
  try {
    const list = JSON.parse(readFileSync(file(root), "utf8")) as Partial<ServerList>;
    return { ...list, active: list.active ?? null, servers: Array.isArray(list.servers) ? list.servers : [] };
  } catch {
    return { active: null, servers: [] };
  }
}

export function saveServers(root: string, list: ServerList): void {
  const path = file(root);
  mkdirSync(dirname(path), { recursive: true });
  const temporary = `${path}.tmp`;
  writeFileSync(temporary, JSON.stringify(list, null, 2), { mode: 0o600 });
  renameSync(temporary, path);
}

/** HTTPS anywhere; plain HTTP only to this computer (dev, tests). Null: fine, otherwise why not. */
export function urlProblem(url: string): string | null {
  let parsed: URL;
  try {
    parsed = new URL(url.trim());
  } catch {
    return `"${url}" is not a server address.`;
  }
  if (parsed.protocol === "https:") return null;
  if (parsed.protocol === "http:") {
    return LOOPBACK.has(parsed.hostname) ? null : "Only https:// addresses (Tailscale Serve) are allowed.";
  }
  return `"${url}" is not a server address.`;
}

/** Accepts the text form or the QR's JSON; surrounding whitespace is ignored. */
export function parsePairing(text: string): PairingPayload | null {
  const trimmed = text.trim();
  let json = trimmed;
  if (trimmed.startsWith(PAIRING_TEXT_PREFIX)) {
    json = Buffer.from(trimmed.slice(PAIRING_TEXT_PREFIX.length).trim(), "base64url").toString("utf8");
  }
  try {
    const value = JSON.parse(json) as Partial<PairingPayload>;
    const { v, url, code, host_id, host_name, env } = value;
    if (v !== 1 || !url || !code || !host_id || !host_name || !env) return null;
    return { v, url, code, host_id, host_name, env };
  } catch {
    return null;
  }
}

/** What the gateway calls this operating system; it refuses any other word. */
export function pairingPlatform(platform: NodeJS.Platform = process.platform): string {
  return platform === "win32" ? "windows" : platform === "darwin" ? "macos" : "linux";
}

/** Adds or replaces the server of this pairing and makes it the active one. */
export function withPaired(list: ServerList, server: PairedServer): ServerList {
  return {
    ...list,
    active: server.host_id,
    servers: [...list.servers.filter((s) => s.host_id !== server.host_id), server],
  };
}

export function withoutServer(list: ServerList, hostId: string): ServerList {
  return {
    ...list,
    active: list.active === hostId ? null : list.active,
    servers: list.servers.filter((s) => s.host_id !== hostId),
  };
}
