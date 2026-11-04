// The local daemon's named pipe or Unix socket, in the main process: a 4-byte big-endian length,
// then UTF-8 JSON (crates/host/src/frame.rs). Access is the OS user's, so no pairing is needed.

import { createHash } from "node:crypto";
import { connect } from "node:net";
import { join, posix, win32 } from "node:path";
import { type Link, LinkError } from "@aurelius/client";

/** The largest frame either side accepts. */
export const MAX_FRAME_BYTES = 4 * 1024 * 1024;

/** Reads whole frames from chunks that may split or join them. Null: a frame the protocol forbids. */
export class FrameReader {
  private buffer: Buffer = Buffer.alloc(0);

  push(chunk: Buffer): string[] | null {
    this.buffer = this.buffer.length === 0 ? chunk : Buffer.concat([this.buffer, chunk]);
    const frames: string[] = [];
    while (this.buffer.length >= 4) {
      const length = this.buffer.readUInt32BE(0);
      if (length === 0 || length > MAX_FRAME_BYTES) return null;
      if (this.buffer.length < 4 + length) break;
      frames.push(this.buffer.toString("utf8", 4, 4 + length));
      this.buffer = this.buffer.subarray(4 + length);
    }
    return frames;
  }
}

export function encodeFrame(text: string): Buffer {
  const body = Buffer.from(text, "utf8");
  if (body.length > MAX_FRAME_BYTES) throw new Error("This message is too large to send.");
  const header = Buffer.alloc(4);
  header.writeUInt32BE(body.length);
  return Buffer.concat([header, body]);
}

/** Where the daemon of this data folder listens (`aurelius_host::endpoint`). */
export function daemonEndpoint(root: string, platform: NodeJS.Platform = process.platform): string {
  if (platform !== "win32") return join(root, "ipc", "daemon.sock");
  const short = createHash("sha256").update(root).digest("hex").slice(0, 16);
  return `\\\\.\\pipe\\aurelius-${short}`;
}

/** `aurelius_host::data_dir`: the folder the daemon of this user and environment works in. */
export function dataDir(
  env: NodeJS.ProcessEnv,
  environment: "dev" | "production",
  platform: NodeJS.Platform = process.platform,
): string | null {
  if (env.AURELIUS_DATA_DIR) return env.AURELIUS_DATA_DIR;
  const paths = platform === "win32" ? win32 : posix;
  const base =
    platform === "win32"
      ? env.LOCALAPPDATA
      : platform === "darwin"
        ? env.HOME && paths.join(env.HOME, "Library/Application Support")
        : env.XDG_DATA_HOME || (env.HOME && paths.join(env.HOME, ".local/share"));
  return base ? paths.join(base, environment === "dev" ? "Aurelius-Dev" : "Aurelius") : null;
}

export function openPipe(endpoint: string): Promise<Link> {
  return new Promise((resolve, reject) => {
    const socket = connect(endpoint);
    socket.once("error", () => reject(new LinkError("unreachable", "The Aurelius service is not running.")));
    socket.once("connect", () => {
      socket.removeAllListeners("error");
      socket.on("error", () => socket.destroy());
      resolve({
        send: (text) => socket.write(encodeFrame(text)),
        close: () => socket.destroy(),
        listen: (onFrame, onClosed) => {
          const reader = new FrameReader();
          socket.on("data", (chunk) => {
            const frames = reader.push(chunk);
            if (frames === null) {
              socket.destroy();
              return;
            }
            for (const frame of frames) onFrame(frame);
          });
          socket.once("close", () => onClosed(null));
        },
      });
    });
  });
}
