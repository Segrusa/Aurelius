import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// `AURELIUS_GATEWAY`: in dev, the gateway behind this server. In production the gateway serves
// these files itself, so everything is one origin.
const gateway = process.env.AURELIUS_GATEWAY ?? "http://127.0.0.1:47613";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "react-style-singleton": fileURLToPath(new URL("../ts/app/src/ui/no-style-singleton.ts", import.meta.url)),
    },
  },
  // No data: URLs: the page's CSP allows only its own origin.
  base: "/",
  build: { outDir: "dist", emptyOutDir: true, assetsInlineLimit: 0 },
  server: { proxy: { "/v1": { target: gateway, ws: true, changeOrigin: false } } },
});
