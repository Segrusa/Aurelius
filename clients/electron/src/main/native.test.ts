import { mkdirSync, mkdtempSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { connectOrStart, daemonProgram } from "./daemon";
import { linkInArguments } from "./links";
import { placement } from "./placement";
import { type Crypt, DeviceSecrets, rustSecretFile, validHostId } from "./secrets";
import {
  loadServers,
  pairingPlatform,
  parsePairing,
  saveServers,
  urlProblem,
  withoutServer,
  withPaired,
} from "./servers";

const payload = { v: 1, url: "https://pc.ts.net", code: "12345678", host_id: "h1", host_name: "PC", env: "production" };
const server = {
  host_id: "h1",
  host_name: "PC",
  url: "https://pc.ts.net",
  client_id: "c1",
  environment: "production",
  last_used_ms: 1,
};

describe("pairing text", () => {
  it("reads the text form, as the server writes it, and the QR's JSON", () => {
    const text = `aurelius-pair:${Buffer.from(JSON.stringify(payload)).toString("base64url")}`;
    expect(parsePairing(`  ${text}\n`)).toEqual(payload);
    expect(parsePairing(JSON.stringify(payload))).toEqual(payload);
  });

  it("refuses anything else", () => {
    expect(parsePairing("hello")).toBeNull();
    expect(parsePairing("aurelius-pair:!!!")).toBeNull();
    expect(parsePairing(JSON.stringify({ ...payload, v: 2 }))).toBeNull();
    expect(parsePairing(JSON.stringify({ ...payload, code: "" }))).toBeNull();
  });
});

describe("server addresses", () => {
  it("takes https anywhere and http only on this computer", () => {
    expect(urlProblem("https://pc.ts.net")).toBeNull();
    expect(urlProblem("http://127.0.0.1:9000")).toBeNull();
    expect(urlProblem("http://localhost:9000")).toBeNull();
    expect(urlProblem("http://[::1]:9000")).toBeNull();
    expect(urlProblem("http://pc.ts.net")).toMatch(/https/);
    expect(urlProblem("ftp://pc")).toMatch(/not a server/);
    expect(urlProblem("nonsense")).toMatch(/not a server/);
  });

  it("names the platform as the gateway wants it", () => {
    expect(pairingPlatform("win32")).toBe("windows");
    expect(pairingPlatform("darwin")).toBe("macos");
    expect(pairingPlatform("linux")).toBe("linux");
  });
});

describe("the server list", () => {
  it("survives a missing file and a broken one", () => {
    const root = mkdtempSync(join(tmpdir(), "aur-servers-"));
    expect(loadServers(root)).toEqual({ active: null, servers: [] });
    saveServers(root, { active: "h1", servers: [server], local_name: "Mine" });
    expect(loadServers(root)).toEqual({ active: "h1", servers: [server], local_name: "Mine" });
    writeFileSync(join(root, "client", "servers.json"), "{");
    expect(loadServers(root)).toEqual({ active: null, servers: [] });
  });

  it("pairing replaces and activates; forgetting the active one goes back to this computer", () => {
    const paired = withPaired({ active: null, servers: [server] }, { ...server, client_id: "c2" });
    expect(paired.active).toBe("h1");
    expect(paired.servers.map((s) => s.client_id)).toEqual(["c2"]);
    expect(withoutServer(paired, "h1")).toEqual({ active: null, servers: [] });
    expect(withoutServer(paired, "other").active).toBe("h1");
  });
});

describe("starting the service", () => {
  it("starts it only when nothing answers, and waits for it", async () => {
    let started = 0;
    let tries = 0;
    const open = async () => {
      tries += 1;
      if (tries < 4) throw new Error("no");
      return "link";
    };
    const link = await connectOrStart(
      open,
      async () => {
        started += 1;
      },
      async () => undefined,
    );
    expect(link).toBe("link");
    expect(started).toBe(1);
  });

  it("gives the first failure where there is nothing to start", async () => {
    await expect(connectOrStart(() => Promise.reject(new Error("off")), null)).rejects.toThrow("off");
  });

  it("does not start a service on a client-only install or on macOS", () => {
    const dir = mkdtempSync(join(tmpdir(), "aur-role-"));
    expect(daemonProgram({}, null, "linux")).toBeNull();
    expect(daemonProgram({}, dir, "darwin")).toBeNull();
    expect(daemonProgram({}, dir, "linux")).toBe(join(dir, "aurelius-daemon"));
    writeFileSync(join(dir, "role.txt"), "client\n");
    expect(daemonProgram({}, dir, "linux")).toBeNull();
    expect(daemonProgram({ AURELIUS_DAEMON_PATH: "/x" }, dir, "linux")).toBe("/x");
  });
});

describe("links from the system", () => {
  it("finds the link among the arguments", () => {
    expect(linkInArguments(["aurelius", "--flag", "aurelius://h/ticket/t"])).toBe("aurelius://h/ticket/t");
    expect(linkInArguments(["aurelius", "https://x"])).toBeNull();
    expect(linkInArguments([`aurelius://${"x".repeat(3000)}`])).toBeNull();
  });
});

describe("where a project window comes back", () => {
  const screen = { x: 0, y: 0, width: 1920, height: 1080 };
  const left = { x: -1920, y: 0, width: 1920, height: 1080 };

  it("stays where it was", () => {
    const saved = { x: 100, y: 50, width: 1200, height: 800 };
    expect(placement(saved, [screen], screen)).toEqual({ bounds: saved, own: true });
  });

  it("moves back in from a screen that is gone, without staying maximized", () => {
    const saved = { x: -1500, y: 50, width: 1200, height: 800 };
    expect(placement(saved, [left, screen], screen).bounds).toEqual(saved);
    const { bounds, own } = placement(saved, [screen], screen);
    expect(own).toBe(false);
    expect(bounds).toEqual({ x: 360, y: 140, width: 1200, height: 800 });
  });

  it("is made to fit a smaller screen", () => {
    const small = { x: 0, y: 0, width: 1280, height: 720 };
    const { bounds } = placement({ x: 100, y: 100, width: 1600, height: 1000 }, [small], small);
    expect(bounds).toEqual({ x: 0, y: 0, width: 1280, height: 720 });
  });
});

describe("device secrets", () => {
  const room = () => mkdtempSync(join(tmpdir(), "aur-secrets-"));
  const crypt = (available: boolean): Crypt => ({
    available: () => available,
    encrypt: (text) => Buffer.from(`enc:${text}`),
    decrypt: (data) => data.toString().replace(/^enc:/, ""),
  });

  it("never lets a host id become a path", () => {
    expect(validHostId("0190-ab")).toBe(true);
    expect(validHostId("../../x")).toBe(false);
    expect(validHostId("a/b")).toBe(false);
    expect(validHostId(1)).toBe(false);
  });

  it("keeps it encrypted, not in the clear, where the OS can", () => {
    const folder = room();
    const secrets = new DeviceSecrets(folder, room(), "production", crypt(true), "linux");
    secrets.set("h1", "s3cret");
    expect(readFileSync(join(folder, "h1.bin"), "utf8")).toBe("enc:s3cret");
    expect(secrets.get("h1")).toBe("s3cret");
    secrets.delete("h1");
    expect(secrets.get("h1")).toBeNull();
  });

  it("reads what the Rust client stored on Linux, so a paired server stays paired", () => {
    const root = room();
    mkdirSync(join(root, "secrets"));
    expect(rustSecretFile(root, "production", "h1")).toBe(join(root, "secrets", "Aurelius_server_h1"));
    writeFileSync(rustSecretFile(root, "production", "h1"), "from-rust\n");
    expect(new DeviceSecrets(room(), root, "production", crypt(true), "linux").get("h1")).toBe("from-rust");
    expect(new DeviceSecrets(room(), root, "production", crypt(true), "win32").get("h1")).toBeNull();
  });

  it("without a keychain on Linux uses a file only this user can read; elsewhere only memory", () => {
    const root = room();
    const linux = new DeviceSecrets(room(), root, "dev", crypt(false), "linux");
    linux.set("h1", "s");
    expect(statSync(rustSecretFile(root, "dev", "h1")).mode & 0o777).toBe(0o600);
    expect(linux.get("h1")).toBe("s");
    const other = new DeviceSecrets(room(), room(), "dev", crypt(false), "win32");
    other.set("h1", "s");
    expect(other.get("h1")).toBe("s");
    expect(new DeviceSecrets(room(), room(), "dev", crypt(false), "win32").get("h1")).toBeNull();
  });
});
