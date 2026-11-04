import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "electron-vite";

// The workspace packages are TypeScript sources: they go into the bundle, and so does what the
// main process uses, because the packaged app has no node_modules. So does the preload's
// everything, because a sandboxed preload can only be one CommonJS file.
const own = ["@aurelius/client", "@aurelius/protocol", "ws"];

export default defineConfig({
  main: { build: { externalizeDeps: { exclude: own }, rollupOptions: { input: "src/main/index.ts" } } },
  preload: {
    build: {
      rollupOptions: { input: "src/preload/index.ts", output: { format: "cjs", entryFileNames: "[name].cjs" } },
    },
  },
  renderer: {
    root: "src/renderer",
    resolve: {
      alias: {
        "react-style-singleton": fileURLToPath(new URL("../ts/app/src/ui/no-style-singleton.ts", import.meta.url)),
      },
    },
    base: "/",
    plugins: [
      react(),
      tailwindcss(),
      // electron-vite makes these relative whatever `base` says, and a reload on /settings/app would
      // then look for its script under /settings.
      {
        name: "absolute-assets",
        enforce: "post",
        transformIndexHtml: (html) => html.replaceAll('"./assets/', '"/assets/'),
      },
    ],
    build: { assetsInlineLimit: 0, rollupOptions: { input: "src/renderer/index.html" } },
  },
});
