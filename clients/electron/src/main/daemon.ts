// Finding and starting this computer's own service. The packaged app sits in `app/` of the
// install folder, beside the service (crates/host/src/updater.rs, `app_program`).

import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";

const PROGRAM = process.platform === "win32" ? "aurelius-daemon.exe" : "aurelius-daemon";

/** The folder with the service's programs; null when not installed that way (a dev run). */
export function installDir(execPath: string, packaged: boolean): string | null {
  if (!packaged) return null;
  const dir = dirname(dirname(execPath));
  return existsSync(join(dir, PROGRAM)) ? dir : null;
}

/** The service to start; null where this install has none (macOS, a client-only install, a dev run). */
export function daemonProgram(
  env: NodeJS.ProcessEnv,
  dir: string | null,
  platform: NodeJS.Platform = process.platform,
): string | null {
  if (env.AURELIUS_DAEMON_PATH) return env.AURELIUS_DAEMON_PATH;
  if (dir === null || platform === "darwin") return null;
  try {
    if (readFileSync(join(dir, "role.txt"), "utf8").trim() === "client") return null;
  } catch {
    // No role written: a full install.
  }
  return join(dir, PROGRAM);
}

/** Detached, so closing the app does not end it. */
export function startDaemon(program: string, root: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(program, [], {
      detached: true,
      stdio: "ignore",
      windowsHide: true,
      env: { ...process.env, AURELIUS_DATA_DIR: root },
    });
    child.once("error", reject);
    child.once("spawn", () => {
      child.unref();
      resolve();
    });
  });
}

/** Tries `open`; when nothing listens starts the service once, then waits up to five seconds. */
export async function connectOrStart<T>(
  open: () => Promise<T>,
  start: (() => Promise<void>) | null,
  wait: (ms: number) => Promise<void> = (ms) => new Promise((r) => setTimeout(r, ms)),
): Promise<T> {
  try {
    return await open();
  } catch (error) {
    if (start === null) throw error;
    await start();
    let last = error;
    for (let attempt = 0; attempt < 50; attempt += 1) {
      await wait(100);
      try {
        return await open();
      } catch (next) {
        last = next;
      }
    }
    throw last;
  }
}
