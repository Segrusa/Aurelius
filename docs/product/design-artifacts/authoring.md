# Design artifacts: how an employee makes one

An artifact is a design that the board member opens and tries out in the Aurelius app: a screen, a prototype, a dashboard or a slide.

- Make **one self-contained HTML file**: `<!doctype html>`, with all CSS in `<style>` and all JS in `<script>`. Do not load anything from the internet (no CDN, fonts or images by URL), because the viewer blocks the network. Put images in as `data:` URIs or inline SVG. Use system fonts.
- Scripts: inline `<script>` elements and `on…=` attributes (`onclick="…"`) run. A `<script>` element that your script makes (`createElement('script')`, `document.write('<script>')`) and `javascript:` links do not run: use event listeners instead. Write plain `<script>` tags: no `nonce` attribute, no `src`, no `<script/>`, no custom elements named `script-…`. Frames and WebRTC do not work.
- Save it in your work folder as `design/<kebab-name>.html`. To publish a new version, overwrite the same file and keep the same name: the app counts versions by name.
- Name the full path in your answer. Aurelius keeps the file with your message, and the board member opens it as an artifact.
- Follow the client's or project's design system. Take the tokens (colours, spacing, radius, type) from its guidelines or styleguide and put them in CSS variables at the top (`:root { --accent: …; --space: … }`).
- Make it responsive for desktop, tablet and mobile widths, with realistic content and no lorem ipsum. Build interactions (tabs, dialogs, states) in plain JS.
- Before you publish, check it: open the file in a headless browser (Playwright) and take a screenshot at 1280 and 390 px wide.
- Keep it under 5 MB. Never put secrets or real customer data in an artifact.
