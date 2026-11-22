// What the Artifact page loads into its webview: the artifact's HTML with a
// policy in front of it. A webview has no per-response headers and no isolated
// session here, so the policy goes in as `<meta>` tags and the CSP `sandbox`
// directive (which a meta tag cannot carry) is missing. See
// docs/product/design-artifacts/mobile.md for which channels that leaves.

import 'dart:math';

/// The desktop's artifact policy (`ARTIFACT_CSP` in clients/electron) without
/// `sandbox`, which only a response header can set.
const artifactCsp =
    "default-src 'none'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; connect-src 'none'; form-action 'none'; base-uri 'none'; frame-src 'none'; object-src 'none'";

/// The second policy: script elements run only inline (no `src` from the web or
/// `data:`, which a nonce alone would allow). A script must pass both.
const artifactInlineOnlyCsp = "script-src 'unsafe-inline'";

/// What the page's first script takes out of the document before the
/// artifact's code runs: WebRTC resolves a TURN server's host name even when
/// nothing else reaches the network. The same names as the desktop preload.
const artifactWebRtcNames = [
  'RTCPeerConnection',
  'webkitRTCPeerConnection',
  'RTCDataChannel',
  'RTCIceCandidate',
  'RTCIceTransport',
  'RTCSessionDescription',
];

/// The bits of a nonce: 128, as on the desktop.
const artifactNonceBytes = 16;

final _hex = RegExp(r'^[0-9a-f]{16,}$');
final _scriptTag = RegExp(r'<script\b', caseSensitive: false);
final _doctype = RegExp(r'^﻿?\s*<!doctype[^>]*>', caseSensitive: false);

/// A fresh nonce for one load: 128 random bits, hex.
String artifactNonce([Random? random]) {
  final r = random ?? Random.secure();
  return [for (var i = 0; i < artifactNonceBytes; i++) r.nextInt(256).toRadixString(16).padLeft(2, '0')].join();
}

/// The first policy of one load: [artifactCsp] plus scripts with [nonce] and
/// `on…=` attributes. Without `'unsafe-inline'` for script elements the engine
/// refuses `javascript:` URLs, whose about:blank document would get WebRTC back.
String artifactScriptCsp(String nonce) => "$artifactCsp; script-src 'nonce-$nonce'; script-src-attr 'unsafe-inline'";

/// The script that runs first, in the main document only (the platform gives
/// no hook for every frame): WebRTC's constructors removed for good.
String artifactPrelude(String nonce) {
  final names = artifactWebRtcNames.map((n) => "'$n'").join(',');
  return '<script nonce=$nonce>(function(){for(const n of [$names]){'
      'try{delete window[n];}catch(e){}'
      'try{Object.defineProperty(window,n,{value:undefined,writable:false,configurable:false,enumerable:false});}catch(e){}'
      '}})();</script>';
}

/// The document the webview loads: the artifact's [html] with both policies
/// and the WebRTC prelude in front of it (after its doctype, so the page keeps
/// its standards mode), and [nonce] on every `<script` of the artifact.
///
/// The insertion only adds characters after a literal `<script`: a script
/// element gets exactly the nonce only when that is a real start tag, and a
/// script with a nonce of its own gets it twice, which the engine refuses.
/// Throws [ArgumentError] for a nonce that is not hex, so nothing can be put
/// into the policy through it.
String artifactDocument(String html, String nonce) {
  if (!_hex.hasMatch(nonce)) throw ArgumentError.value(nonce, 'nonce', 'An artifact nonce is hex.');
  final head = '<meta http-equiv="Content-Security-Policy" content="${artifactScriptCsp(nonce)}">'
      '<meta http-equiv="Content-Security-Policy" content="$artifactInlineOnlyCsp">'
      // CSP has no directive for DNS prefetch; this switch cannot be turned back on.
      '<meta http-equiv="x-dns-prefetch-control" content="off">'
      '<meta name="referrer" content="no-referrer">'
      '${artifactPrelude(nonce)}';
  final body = html.replaceAll(_scriptTag, '<script nonce=$nonce');
  final doctype = _doctype.firstMatch(body);
  return doctype == null ? '$head$body' : '${body.substring(0, doctype.end)}$head${body.substring(doctype.end)}';
}
