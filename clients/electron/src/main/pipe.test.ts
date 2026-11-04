import { describe, expect, it } from "vitest";
import { daemonEndpoint, dataDir, encodeFrame, FrameReader, MAX_FRAME_BYTES } from "./pipe";

describe("frames", () => {
  it("come out whole however the chunks fall", () => {
    const bytes = Buffer.concat([encodeFrame('{"a":1}'), encodeFrame('{"b":"é👋"}')]);
    for (const size of [1, 3, 5, bytes.length]) {
      const reader = new FrameReader();
      const frames: string[] = [];
      for (let at = 0; at < bytes.length; at += size) {
        frames.push(...(reader.push(bytes.subarray(at, at + size)) as string[]));
      }
      expect(frames).toEqual(['{"a":1}', '{"b":"é👋"}']);
    }
  });

  it("refuse an empty or oversized length before reading the body", () => {
    const header = (n: number) => {
      const b = Buffer.alloc(4);
      b.writeUInt32BE(n);
      return b;
    };
    expect(new FrameReader().push(header(0))).toBeNull();
    expect(new FrameReader().push(header(MAX_FRAME_BYTES + 1))).toBeNull();
    expect(() => encodeFrame("x".repeat(MAX_FRAME_BYTES + 1))).toThrow(/too large/);
  });
});

describe("the daemon's endpoint", () => {
  it("is the socket in its data folder on Unix", () => {
    expect(daemonEndpoint("/data", "linux")).toBe("/data/ipc/daemon.sock");
  });

  it("is a pipe named after the folder's hash on Windows, as the Rust side computes it", () => {
    // The first 16 hex digits of sha256("C:\\data"), computed apart with Python's hashlib.
    expect(daemonEndpoint("C:\\data", "win32")).toBe(`\\\\.\\pipe\\aurelius-3d2b498a929b466d`);
  });
});

describe("the data folder", () => {
  it("follows the Rust side per platform and environment", () => {
    expect(dataDir({ HOME: "/h" }, "dev", "linux")).toBe("/h/.local/share/Aurelius-Dev");
    expect(dataDir({ HOME: "/h", XDG_DATA_HOME: "/x" }, "production", "linux")).toBe("/x/Aurelius");
    expect(dataDir({ HOME: "/h" }, "production", "darwin")).toBe("/h/Library/Application Support/Aurelius");
    expect(dataDir({ LOCALAPPDATA: "C:\\L" }, "dev", "win32")).toBe("C:\\L\\Aurelius-Dev");
    expect(dataDir({ AURELIUS_DATA_DIR: "/d", HOME: "/h" }, "dev", "linux")).toBe("/d");
    expect(dataDir({}, "dev", "linux")).toBeNull();
  });
});
