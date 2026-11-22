// What the Artifact page loads into its webview: the artifact's HTML with a
// policy in front of it. The webview runs no JavaScript at all, so what is left
// is static markup. A webview has no per-response headers here, so the policy
// goes in as `<meta>` tags. See docs/product/design-artifacts/mobile.md for which
// channels that leaves.

/// The desktop's artifact policy (`ARTIFACT_CSP` in clients/electron) without
/// `sandbox`, which only a response header can set, and with no scripts at all:
/// the webview has JavaScript off, and the policy says the same.
const artifactCsp =
    "default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; connect-src 'none'; form-action 'none'; base-uri 'none'; frame-src 'none'; object-src 'none'";

final _doctype = RegExp(r'^﻿?\s*<!doctype[^>]*>', caseSensitive: false);

/// The document the webview loads: the artifact's [html] with the policy in
/// front of it (after its doctype, so the page keeps its standards mode).
String artifactDocument(String html) {
  const head = '<meta http-equiv="Content-Security-Policy" content="$artifactCsp">'
      // CSP has no directive for DNS prefetch; this switch cannot be turned back on.
      '<meta http-equiv="x-dns-prefetch-control" content="off">'
      '<meta name="referrer" content="no-referrer">';
  final doctype = _doctype.firstMatch(html);
  return doctype == null ? '$head$html' : '${html.substring(0, doctype.end)}$head${html.substring(doctype.end)}';
}
