# Design artifacts on mobile (ticket #79, phase C)

The mobile app opens a kept `text/html` file as a live artifact, like the desktop's viewer (`design.md`). No server or protocol change: the file comes with `file.get` (`AureliusRepository.file`), and the only write is an ordinary chat message.

## Page
- `lib/features/artifact/`: `artifact_versions.dart` (pure: versions, labels, widths, the feedback text), `artifact_html.dart` (pure: the document with the policy and the nonce), `artifact_bloc.dart`, `artifact_page.dart`, `artifact_webview.dart`.
- The chip of a kept HTML file in a chat (session, task, ticket discussion) opens the **Artifact page**. Other files keep the File page.
- **Versions**: every kept HTML file with the same name in the conversation, in message order, newest first in the picker (`v3 · 14:05`). The same rule as the desktop's `artifact-logic.ts`. A new version that comes in while the page is open joins the picker.
- **Desktop / Tablet / Mobile**: the artifact is laid out at 1280, 768 or 390 CSS px and scaled down to the stage's width (never up).
- **Reload** loads the same HTML again with a fresh nonce. **Share HTML** saves the file as the server keeps it through the platform's save dialog (`file_picker`'s `saveFile`; the app has no share sheet and gets no new dependency).
- **Ask for changes** asks for a text and posts one message, `Feedback on pricing.html v3: <text>`, through the chat's own send (`ChatMessageSent`, without the composer's pending attachments), so it lands in the conversation the file came from, with the chat's retry and secret check. It is offered only where the chat can be written to. The file name is the kept name (the server keeps no folder). No element picking on mobile.
- A failed load shows "This artifact could not be loaded: <reason>" with **Try again**.

## Security
The desktop's boundary is an isolated session without network. A mobile webview has no such session, no per-response headers and no hook for every frame, so the phone takes the layers the platform offers and the rest is written down in `target/ticket-79/c/report.md` (per channel, Android and iOS).

1. The HTML is loaded with `loadHtmlString` and no base URL: an `about:blank` document with an opaque origin of its own.
2. In front of it (after its doctype) come two CSP `<meta>` tags: the desktop's policy without `sandbox` (a meta tag cannot carry it) plus `script-src 'nonce-<n>'; script-src-attr 'unsafe-inline'`, and the second policy `script-src 'unsafe-inline'`. The nonce is 128 random bits, fresh per load, and is put on every `<script` of the artifact. So `javascript:` URLs never run, and script elements run only inline.
3. `x-dns-prefetch-control: off` as a meta tag: CSP has no directive for DNS prefetch.
4. A first script (with the nonce) removes WebRTC's constructors from the main document, as the desktop preload does. The plugin offers no document-start script for every frame, so frames the artifact makes itself are not covered (see the report).
5. Navigation: only the main-frame `about:blank` the page itself just loaded is allowed; links, forms, `window.open`, redirects and frame navigations are refused (`artifactNavigationAllowed`).
6. Every permission request (camera, microphone, …) is denied. Android: file and content access off, geolocation off, no file chooser, media needs a gesture. iOS: link previews off, not inspectable, media needs a gesture, no automatic windows.
7. No JavaScript channel: the page never hears from the artifact.

### What stays open (Android WebView 124, emulator probe; iOS not testable here)
Measured against local sinks with a bare webview as the positive control (`target/ticket-79/c/report.md`):
- Blocked: fetch, XHR, beacon, WebSocket, images, CSS, fonts, frames by URL, `<link rel=prefetch>`, every navigation (link, form, `location`, meta refresh, `target=_blank`, `window.open`) and `javascript:` URLs and frames.
- **Open: WebRTC through frames.** The prelude takes WebRTC out of the main document, but an empty `<iframe>` (about:blank) and a `srcdoc` frame have the page's own origin, so the artifact reaches their `RTCPeerConnection`: STUN packets go out and TURN host names are resolved. Without the CSP `sandbox` (header only) and without a document-start script for every frame (the plugin has none), the platform cannot close this.
- **Open: `<link rel=preconnect>`** resolves its host name. CSP has no directive for it and `x-dns-prefetch-control` does not cover it.
- Not done by the platform at all (also not in the control): `<link rel=dns-prefetch>`, anchor prefetch, speculation rules.

Which of these the phone may keep is Titan's decision; nothing is worked around in the page.
