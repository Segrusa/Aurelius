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

/// The `rel` keywords that make the engine look up or fetch a host before
/// anything is shown: CSP covers the fetch, but not every lookup.
const artifactHintRels = {'preconnect', 'dns-prefetch', 'prefetch', 'prerender', 'preload', 'modulepreload'};

/// A `rel` attribute as the tokenizer reads it: after whitespace, `/` or the
/// quote that ended the attribute before it, with a quoted or unquoted value.
final _relAttribute = RegExp(r'''(?<=[\s/"'])rel\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)''', caseSensitive: false);

final _space = RegExp(r'\s+');
final _quote = RegExp('["\']');

final _doctype = RegExp(r'^﻿?\s*<!doctype[^>]*>', caseSensitive: false);

/// [html] with every `rel` that names one of [artifactHintRels] turned into
/// `rel="nofollow"`, on any element, in any case and quoting. A value with a
/// character reference counts too, since the engine decodes it (`pre&#99;onnect`).
String artifactQuietRels(String html) => html.replaceAllMapped(_relAttribute, (m) {
      // Quotes dropped rather than cut, so an unclosed quote cannot hide a keyword.
      final value = m[1]!.replaceAll(_quote, '');
      final hinted = value.contains('&') || value.toLowerCase().split(_space).any(artifactHintRels.contains);
      return hinted ? 'rel="nofollow"' : m[0]!;
    });

/// The document the webview loads: the artifact's [html] with the policy in
/// front of it (after its doctype, so the page keeps its standards mode) and its
/// resource hints made quiet ([artifactQuietRels]).
String artifactDocument(String html) {
  const head = '<meta http-equiv="Content-Security-Policy" content="$artifactCsp">'
      // CSP has no directive for DNS prefetch; this switch cannot be turned back on.
      '<meta http-equiv="x-dns-prefetch-control" content="off">'
      '<meta name="referrer" content="no-referrer">';
  final body = artifactQuietRels(html);
  final doctype = _doctype.firstMatch(body);
  return doctype == null ? '$head$body' : '${body.substring(0, doctype.end)}$head${body.substring(doctype.end)}';
}
