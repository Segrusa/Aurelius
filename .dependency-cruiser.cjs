// The shared client code runs in a browser too: only the Electron shell may touch Electron or
// Node, and the layers point one way (app -> client -> protocol).
module.exports = {
  forbidden: [
    {
      name: "shared-code-runs-in-a-browser",
      comment: "clients/ts may not use Node or Electron; put it behind the Platform contract.",
      severity: "error",
      from: { path: "^clients/(ts|web|electron/src/renderer)/" },
      to: { dependencyTypes: ["core"] },
    },
    {
      name: "shared-code-without-electron",
      severity: "error",
      from: { path: "^clients/(ts|web|electron/src/renderer)/" },
      to: { path: "node_modules/electron" },
    },
    {
      name: "protocol-stands-alone",
      severity: "error",
      from: { path: "^clients/ts/protocol/" },
      to: { path: "^clients/ts/(client|app)/" },
    },
    {
      name: "client-has-no-ui",
      severity: "error",
      from: { path: "^clients/ts/client/" },
      to: { path: "^clients/ts/app/" },
    },
  ],
  options: {
    doNotFollow: { path: "node_modules" },
    exclude: { path: ["\\.test\\.ts$", "vite\\.config\\.ts$", "/dist/"] },
    tsPreCompilationDeps: true,
    tsConfig: { fileName: "tsconfig.base.json" },
  },
};
