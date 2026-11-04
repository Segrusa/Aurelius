// Makes the app folder that an install or an update puts in `app/` of the install folder:
// `release/<platform>-<arch>/`, with `aurelius(.exe)` and `resources/app.asar` at its root.
// Everything the app needs is in the bundle in `out/`, so no node_modules goes along.

import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { FuseV1Options, FuseVersion, flipFuses } from "@electron/fuses";
import { packager } from "@electron/packager";

const here = dirname(fileURLToPath(import.meta.url));
const electronDir = resolve(here, "..");
const repo = resolve(electronDir, "../..");
const require = createRequire(import.meta.url);

const platform = process.env.AURELIUS_PACKAGE_PLATFORM ?? process.platform;
const arch = process.env.AURELIUS_PACKAGE_ARCH ?? process.arch;

// The app's version is the workspace's, which `scripts/release.ps1` raises.
const cargo = readFileSync(join(repo, "Cargo.toml"), "utf8");
const version = cargo.match(/^version = "([^"]+)"/m)?.[1];
if (!version) throw new Error("No version in the root Cargo.toml.");
if (!existsSync(join(electronDir, "out/main/index.js"))) throw new Error("Build first: pnpm build.");

const staging = mkdtempSync(join(tmpdir(), "aurelius-package-"));
const release = join(electronDir, "release");
const target = join(release, `${platform}-${arch}`);
try {
  cpSync(join(electronDir, "out"), join(staging, "out"), { recursive: true });
  writeFileSync(
    join(staging, "package.json"),
    JSON.stringify({ name: "aurelius", productName: "Aurelius", version, type: "module", main: "out/main/index.js" }),
  );
  const assets = join(repo, "clients/desktop/assets");
  const [made] = await packager({
    dir: staging,
    out: join(staging, "made"),
    name: "Aurelius",
    executableName: "aurelius",
    appVersion: version,
    electronVersion: require("electron/package.json").version,
    platform,
    arch,
    asar: true,
    prune: false,
    overwrite: true,
    quiet: true,
    appBundleId: "com.segrusa.aurelius.desktop",
    protocols: [{ name: "Aurelius link", schemes: ["aurelius"] }],
    ...(platform === "win32" ? { icon: join(assets, "aurelius.ico") } : {}),
    ...(platform === "darwin" ? { icon: join(assets, "aurelius.icns") } : {}),
    extraResource: [join(assets, "aurelius-512.png")],
    win32metadata: { CompanyName: "Segrusa", ProductName: "Aurelius", FileDescription: "Aurelius" },
  });
  if (!made) throw new Error("The packager made nothing.");
  const program =
    platform === "win32"
      ? join(made, "aurelius.exe")
      : platform === "darwin"
        ? join(made, "Aurelius.app")
        : join(made, "aurelius");
  await flipFuses(program, {
    version: FuseVersion.V1,
    [FuseV1Options.RunAsNode]: false,
    [FuseV1Options.EnableCookieEncryption]: true,
    [FuseV1Options.EnableNodeOptionsEnvironmentVariable]: false,
    [FuseV1Options.EnableNodeCliInspectArguments]: false,
    [FuseV1Options.OnlyLoadAppFromAsar]: true,
  });
  rmSync(target, { recursive: true, force: true });
  cpSync(made, target, { recursive: true, verbatimSymlinks: true });
  console.log(`Aurelius ${version}: ${target}`);
} finally {
  rmSync(staging, { recursive: true, force: true });
}
