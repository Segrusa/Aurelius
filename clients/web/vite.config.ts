import react from "@vitejs/plugin-react";
import { defineConfig, type PluginOption } from "vite";
import { CSS_TARGET, ownStylex } from "../stylex-plugin";

// `AURELIUS_GATEWAY`: in dev, the gateway behind this server. In production the gateway serves
// these files itself, so everything is one origin.
const gateway = process.env.AURELIUS_GATEWAY ?? "http://127.0.0.1:47613";

export default defineConfig({
  plugins: [...ownStylex<PluginOption>(), react()],
  // No data: URLs: the page's CSP allows only its own origin.
  base: "/",
  build: { outDir: "dist", emptyOutDir: true, assetsInlineLimit: 0, cssTarget: CSS_TARGET },
  server: { proxy: { "/v1": { target: gateway, ws: true, changeOrigin: false } } },
});
