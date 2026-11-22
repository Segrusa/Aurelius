import react from "@vitejs/plugin-react";
import { defineConfig } from "electron-vite";
import type { PluginOption } from "vite";
import { CSS_TARGET, ownStylex } from "../stylex-plugin";

// The workspace packages are TypeScript sources: they go into the bundle, and so does what the
// main process uses, because the packaged app has no node_modules. So does the preload's
// everything, because a sandboxed preload can only be one CommonJS file.
const own = ["@aurelius/client", "@aurelius/protocol", "ws"];

export default defineConfig({
  main: { build: { externalizeDeps: { exclude: own }, rollupOptions: { input: "src/main/index.ts" } } },
  preload: {
    build: {
      // The app's preload, and the artifact's (which only takes WebRTC out of every frame).
      rollupOptions: {
        input: { index: "src/preload/index.ts", artifact: "src/preload/artifact.ts" },
        output: { format: "cjs", entryFileNames: "[name].cjs" },
      },
    },
  },
  renderer: {
    root: "src/renderer",
    base: "/",
    plugins: [
      ...ownStylex<PluginOption>(),
      react(),
      // electron-vite makes these relative whatever `base` says, and a reload on /settings/app would
      // then look for its script under /settings.
      {
        name: "absolute-assets",
        enforce: "post",
        transformIndexHtml: (html) => html.replaceAll('"./assets/', '"/assets/'),
      },
    ],
    build: { assetsInlineLimit: 0, cssTarget: CSS_TARGET, rollupOptions: { input: "src/renderer/index.html" } },
  },
});
