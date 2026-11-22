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
The security boundary is **an isolated session without network**, not CSP and the sandbox. Chromium lets an artifact past CSP and the sandbox with WebRTC (STUN, TURN), DNS prefetch and speculation rules (security review AUR-T79-SEC1). So an artifact never runs in the app's own session or page:

1. Main serves a privileged scheme `artifact:` only in its own in-memory session `session.fromPartition("artifact")` (no `persist:`). The renderer hands the HTML to main over IPC (`artifact:serve`, checked with `fromOurPage`), and main returns `artifact://view/<random token>`. Tokens live in memory, belong to the window that asked, are dropped when the viewer closes, and are capped per window by count and over all windows by size; a window only makes room from its own artifacts.
2. The artifact session has no network: a `webRequest` filter cancels every request that is not `artifact://view/<token>`, a proxy that is not there (`127.0.0.1:9` for http, https and socks, loopback included) gets what slips past, every permission is denied, downloads are refused. Every WebContents in it gets `setWebRTCIPHandlingPolicy("disable_non_proxied_udp")`, a deny window-open handler, and `will-navigate`/`will-redirect`/`will-frame-navigate` guards that allow only its own artifact URL.
3. Chromium still resolves a TURN server's host name before it would use the proxy (a DNS channel), and Electron has no per-session resolver or WebRTC switch. So the artifact's view and export window get a preload of their own, in every frame: it opens no bridge and no IPC, and only removes `RTCPeerConnection` and its siblings from the page before the artifact's code runs.
4. DNS prefetch is off for the whole app (`blink-settings=dnsPrefetchingEnabled=false`, before ready): CSP has no directive for it.
5. On top of that, every response from `artifact:` gets its own CSP: `sandbox allow-scripts; default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; connect-src 'none'; form-action 'none'; base-uri 'none'; frame-src 'none'; object-src 'none'`, plus `X-Content-Type-Options: nosniff`. That keeps it in an opaque origin without storage, popups, forms or top navigation; it is not what keeps it off the network.
6. The viewer is a `WebContentsView` (artifact session, sandbox, context isolation, no Node, the preload of point 3) that main lays over the viewer's stage. The page reports the stage's bounds whenever they change (`artifact:show {url, bounds}`, `artifact:bounds`, `artifact:hide`): resize, scroll, the width toggle. A view follows only the page that showed it, and main takes it away when that page navigates (also a route change), reloads, crashes or its window closes; the page hides it when the dialog closes. The app's own CSP does not frame anything for it.
7. The view lies above the page. It steps aside while an open menu of the page overlaps it (the version picker); a toast that falls over the stage is hidden under it.
8. UI: opening an HTML chip (and an "Artifact" card under the message) opens an artifact view. It is a large dialog with a title, a version picker (v1…vN with the time), a reload button, a Desktop / Tablet / Mobile width toggle, and the exports. The chips of non-HTML files keep working as before.

## Export
- HTML: save the file (the existing `saveBlob`).
- PDF and PNG: main loads the artifact URL in a hidden `BrowserWindow` in the same artifact session with the same guards and preload, then calls `printToPDF` or `capturePage`, and the user picks the location through a save dialog. Only the window that holds the artifact may export it.
- Load and print/capture together get `EXPORT_TIMEOUT_MS` (20 s); after that the export fails with "This artifact did not finish rendering.". The hidden window's renderer is always ended (`forcefullyCrashRenderer`) before the window is destroyed, because a busy one does not stop.
- The PNG height is read in an isolated world without a user gesture, and kept within 16,384 px and within 64 Mpx of device pixels (width × height × scale factor²).

## Out of phase A
Comments on an element, sliders/knobs, a side-by-side compare, mobile, PPTX, React/Tailwind runtime injection, storage, AI calls from the artifact and public links.
