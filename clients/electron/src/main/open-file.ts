// Opening a file a run wrote. A run chooses those files, so a click must never start a program:
// what could run is shown in its folder instead of opened.

import { extname } from "node:path";

/** Extensions the operating system runs (or hands to an interpreter) when the file is opened. */
const RUNNABLE_EXTENSIONS: ReadonlySet<string> = new Set([
  // Windows
  ".exe",
  ".com",
  ".bat",
  ".cmd",
  ".msi",
  ".msc",
  ".scr",
  ".pif",
  ".cpl",
  ".lnk",
  ".url",
  ".reg",
  ".ps1",
  ".psm1",
  ".vbs",
  ".vbe",
  ".js",
  ".jse",
  ".wsf",
  ".wsh",
  ".hta",
  ".jar",
  ".appref-ms",
  // macOS
  ".app",
  ".command",
  ".tool",
  ".pkg",
  ".terminal",
  ".workflow",
  // Linux
  ".desktop",
  ".sh",
  ".run",
  ".appimage",
  ".bin",
  ".deb",
  ".rpm",
]);

/** A program that opens a file in its default app, when the platform needs one instead of `shell.openPath`. */
export interface OpenCommand {
  program: string;
  args: string[];
}

/**
 * On Linux, `shell.openPath` goes through `xdg-open`, which outside GNOME and KDE ignores
 * `Terminal=true`: a terminal editor (nvim, helix) is started without a window and the click
 * seems to do nothing. `gio open` starts those in a terminal and reports a failure.
 */
export function openCommand(path: string, platform: NodeJS.Platform, hasGio: boolean): OpenCommand | null {
  if (platform !== "linux" || !hasGio) return null;
  return { program: "gio", args: ["open", "--", path] };
}

/** Whether opening the file could run something: a runnable extension, or an executable bit outside Windows. */
export function runnable(path: string, mode: number, platform: NodeJS.Platform): boolean {
  if (RUNNABLE_EXTENSIONS.has(extname(path).toLowerCase())) return true;
  return platform !== "win32" && (mode & 0o111) !== 0;
}
