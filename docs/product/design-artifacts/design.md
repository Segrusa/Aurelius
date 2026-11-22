# Ticket #79, phase A: design artifacts in the desktop app (spec, Titan)

## What exists already (no server change needed)
- A run that names a full path in its answer has that file kept as a blob with the message (`crates/application/src/conversation/message_files.rs`, `run/lifecycle.rs:613`): max 8 files, `UPLOAD_MAX_BYTES`, `.html` → `text/html`. Clients read `MessageView.files` and fetch with `file.get`.
- Desktop: `KeptFiles` in `clients/ts/app/src/screens/run-tools.tsx:432` shows images in a dialog and **downloads** everything else, so HTML is downloaded today.
- Electron serves the app on `app://aurelius` with a strict CSP (`clients/electron/src/main/index.ts:60`, test `csp.test.ts`).
- `design-logic.ts` holds links to Claude Design artifacts (feature `design_links`). That stays as it is.

## Artifact = an HTML file kept with a message
- An artifact is a kept file with `media_type == "text/html"`. Its versions are all kept files **with the same name** in the same conversation, in message order (v1, v2, …). This is pure client logic: no protocol, no migration.
- The authoring rules for employees are in `docs/product/design-artifacts/authoring.md`: one self-contained HTML file, CSS and JS inline, no external URLs, and images as data: URIs.

## Viewer (desktop, Electron)
1. A new privileged scheme `artifact:` that main serves. The renderer hands the HTML to main over IPC (`artifact:serve`, checked with `fromOurPage`), and main returns `artifact://view/<random token>`. Tokens live in memory, are dropped when the viewer closes, and are capped by count and size.
2. Every response from `artifact:` gets its own CSP: `default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; connect-src 'none'; form-action 'none'; base-uri 'none'; frame-src 'none'; object-src 'none'`, plus `X-Content-Type-Options: nosniff`. It also carries `sandbox allow-scripts`, so an artifact is sandboxed where it is not in the viewer's frame too (the hidden export window).
3. The app CSP gains only `frame-src artifact:` (update `csp.test.ts`).
4. The iframe has `sandbox="allow-scripts"`. Never `allow-same-origin`, `allow-top-navigation`, `allow-popups` or `allow-forms`.
5. Main blocks every navigation of a subframe away from its artifact URL (`will-frame-navigate`) and every `window.open`. The preload runs only in the main frame, and no IPC is reachable from the artifact.
6. UI: opening an HTML chip (and an "Artifact" card under the message) opens an artifact view. It is a large dialog or panel with a title, a version picker (v1…vN with the time), a reload button, a Desktop / Tablet / Mobile width toggle, and the exports. The chips of non-HTML files keep working as before.

## Export
- HTML: save the file (the existing `saveBlob`).
- PDF and PNG: main loads the artifact URL in a hidden `BrowserWindow` (sandbox, no preload, the same session rules), then calls `printToPDF` or `capturePage`, and the user picks the location through a save dialog.

## Out of phase A
Comments on an element, sliders/knobs, a side-by-side compare, mobile, PPTX, React/Tailwind runtime injection, storage, AI calls from the artifact and public links.
