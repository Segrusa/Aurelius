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

/// Where a `rel` attribute may start as the tokenizer reads it: after whitespace,
/// `/` or the quote that ended the attribute before it. The value, quoted or not,
/// is read in a lookahead so the scan never consumes it: a `rel='` in a comment,
/// a text or another attribute's value must not hide a later, real `rel` (SEC5 M1).
final _relAttribute = RegExp(r'''(?<=[\s/"'])rel\s*=\s*(?=("[^"]*"|'[^']*'|[^\s>]+))''', caseSensitive: false);

final _space = RegExp(r'\s+');
final _quote = RegExp('["\']');

final _doctype = RegExp(r'^﻿?\s*<!doctype[^>]*>', caseSensitive: false);

const _quietRel = 'rel="nofollow"';

/// A `rel` value names a hint. A value with a character reference counts too,
/// since the engine decodes it (`pre&#99;onnect`). Quotes are dropped rather
/// than cut, so an unclosed quote cannot hide a keyword.
bool _hinted(String value) {
  final bare = value.replaceAll(_quote, '');
  return bare.contains('&') || bare.toLowerCase().split(_space).any(artifactHintRels.contains);
}

/// The `[start, end)` spans of every `rel` that names a hint, in text order.
/// Each `rel=` is decided on its own, also one inside another's value.
Iterable<(int, int)> _hintSpans(String html) sync* {
  for (final m in _relAttribute.allMatches(html)) {
    final value = m[1]!;
    if (_hinted(value)) yield (m.start, m.end + value.length);
  }
}

/// [html] with every `rel` that names one of [artifactHintRels] turned into
/// `rel="nofollow"`, on any element, in any case and quoting. Where two such
/// spans overlap, the whole stretch they cover becomes one `rel="nofollow"`, so
/// the outer one goes and the inner one with it.
String artifactQuietRels(String html) {
  final out = StringBuffer();
  var at = 0;
  for (final (start, end) in _hintSpans(html)) {
    if (start < at) {
      if (end > at) at = end;
      continue;
    }
    out
      ..write(html.substring(at, start))
      ..write(_quietRel);
    at = end;
  }
  out.write(html.substring(at));
  return out.toString();
}

/// [html] still has a `rel` that names a hint, by the same per-occurrence scan
/// as [artifactQuietRels]: the check after the rewrite.
bool artifactHasHints(String html) => _hintSpans(html).isNotEmpty;

/// The rewrite left a resource hint in the artifact, so the app does not show
/// it: failing closed beats a lookup to a host the artifact chose.
class ArtifactHintLeft implements Exception {
  const ArtifactHintLeft();

  /// What the Artifact page says, with Try again.
  static const message = 'It names a host the phone would look up, and the app could not stop that, so it is not shown.';

  @override
  String toString() => message;
}

/// The document the webview loads: the artifact's [html] with the policy in
/// front of it (after its doctype, so the page keeps its standards mode) and its
/// resource hints made quiet ([artifactQuietRels], or [quiet] in a test). Throws
/// [ArtifactHintLeft] when a hint is still there after that.
String artifactDocument(String html, {String Function(String) quiet = artifactQuietRels}) {
  const head = '<meta http-equiv="Content-Security-Policy" content="$artifactCsp">'
      // CSP has no directive for DNS prefetch; this switch cannot be turned back on.
      '<meta http-equiv="x-dns-prefetch-control" content="off">'
      '<meta name="referrer" content="no-referrer">';
  final body = quiet(html);
  if (artifactHasHints(body)) throw const ArtifactHintLeft();
  final doctype = _doctype.firstMatch(body);
  return doctype == null ? '$head$body' : '${body.substring(0, doctype.end)}$head${body.substring(doctype.end)}';
}
