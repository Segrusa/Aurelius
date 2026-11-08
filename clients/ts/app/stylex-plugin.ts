// StyleX for our own components, shared by both shells. Astryx ships its CSS prebuilt, so its package
// is never compiled here: the plugin has no option for that, hence the wrapper around `transform`.

import { fileURLToPath } from "node:url";
import stylex from "@stylexjs/unplugin/vite";

const PREBUILT = /node_modules[\\/](?:\.pnpm[\\/][^\\/]*[\\/]node_modules[\\/])?@astryxdesign[\\/]/;
const ROOT = fileURLToPath(new URL(".", import.meta.url));
/** What the app runs on: Electron's Chromium and current browsers. lightningcss keeps `light-dark()` as is. */
const TARGETS = { chrome: 125 << 16, safari: 26 << 16, firefox: 147 << 16 };
/** For Vite's own CSS step, which would otherwise rewrite `light-dark()` and the layers. */
export const CSS_TARGET = ["chrome125", "safari26", "firefox147"];

interface HookObject {
  handler: (...args: unknown[]) => unknown;
}
type Hook = HookObject | ((...args: unknown[]) => unknown);

export function ownStylex<Plugin>(): Plugin[] {
  const made = stylex({
    runtimeInjection: false,
    useCSSLayers: false,
    unstable_moduleResolution: { type: "commonJS", rootDir: ROOT },
    lightningcssOptions: { targets: TARGETS },
  });
  const plugins: { transform?: Hook }[] = Array.isArray(made) ? made : [made];
  for (const plugin of plugins) {
    const original = plugin.transform;
    if (!original) continue;
    const handler = typeof original === "function" ? original : original.handler;
    const wrapped = function (this: unknown, code: unknown, id: unknown, ...rest: unknown[]) {
      if (typeof id === "string" && PREBUILT.test(id)) return null;
      return handler.call(this, code, id, ...rest);
    };
    plugin.transform = typeof original === "function" ? wrapped : { ...original, handler: wrapped };
  }
  return plugins as Plugin[];
}
