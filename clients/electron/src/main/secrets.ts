// The device secret of a paired server and the session token of a sign-in. Encrypted by the OS where
// it can; on Linux without a keychain in the file the Rust client keeps it in
// (crates/host/src/secrets.rs: 0600 in 0700).

import { chmodSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export interface Crypt {
  available(): boolean;
  encrypt(text: string): Buffer;
  decrypt(data: Buffer): string;
}

export const validHostId = (id: unknown): id is string => typeof id === "string" && /^[A-Za-z0-9-]{1,64}$/.test(id);

type SecretRef = (id: string) => string;

const deviceSecretRef: SecretRef = (hostId) => `server/${hostId}`;

/** The name the Rust client keeps a session under: a sign-in survives the move to this app. */
export const sessionRef: SecretRef = (server) => `desktop-session-${server}`;

/** `<data>/secrets/<target>` with everything but letters, digits, dots and dashes as `_`. */
export function rustSecretFile(
  root: string,
  environment: "dev" | "production",
  hostId: string,
  secretRef: SecretRef = deviceSecretRef,
): string {
  const target = `${environment === "dev" ? "Aurelius-Dev" : "Aurelius"}/${secretRef(hostId)}`;
  return join(root, "secrets", target.replace(/[^A-Za-z0-9.-]/g, "_"));
}

export class DeviceSecrets {
  private readonly memory = new Map<string, string>();

  constructor(
    private readonly folder: string,
    private readonly root: string,
    private readonly environment: "dev" | "production",
    private readonly crypt: Crypt,
    private readonly platform: NodeJS.Platform = process.platform,
    private readonly secretRef: SecretRef = deviceSecretRef,
  ) {}

  private rustFile(hostId: string): string {
    return rustSecretFile(this.root, this.environment, hostId, this.secretRef);
  }

  private own(hostId: string): string {
    return join(this.folder, `${hostId}.bin`);
  }

  get(hostId: string): string | null {
    const kept = this.memory.get(hostId);
    if (kept !== undefined) return kept;
    if (this.crypt.available() && existsSync(this.own(hostId))) {
      try {
        return this.crypt.decrypt(readFileSync(this.own(hostId)));
      } catch {
        return null;
      }
    }
    return this.platform === "linux" ? this.fromRustFile(hostId) : null;
  }

  private fromRustFile(hostId: string): string | null {
    try {
      const secret = readFileSync(this.rustFile(hostId), "utf8").trim();
      return secret === "" ? null : secret;
    } catch {
      return null;
    }
  }

  set(hostId: string, secret: string): void {
    if (this.crypt.available()) {
      mkdirSync(this.folder, { recursive: true, mode: 0o700 });
      writeFileSync(this.own(hostId), this.crypt.encrypt(secret), { mode: 0o600 });
    } else if (this.platform === "linux") {
      const file = this.rustFile(hostId);
      const folder = join(this.root, "secrets");
      mkdirSync(folder, { recursive: true, mode: 0o700 });
      chmodSync(folder, 0o700);
      // `mode` only counts for a new file.
      writeFileSync(file, secret, { mode: 0o600 });
      chmodSync(file, 0o600);
    } else {
      // No safe place: it lasts until the app closes.
      this.memory.set(hostId, secret);
    }
  }

  delete(hostId: string): void {
    this.memory.delete(hostId);
    rmSync(this.own(hostId), { force: true });
    if (this.platform === "linux") rmSync(this.rustFile(hostId), { force: true });
  }
}
