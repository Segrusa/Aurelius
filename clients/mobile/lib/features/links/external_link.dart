import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../core/widgets/widgets.dart';

/// Opens a link in the phone's browser; tests replace it.
Future<bool> Function(Uri uri) launchExternal = (uri) => launchUrl(uri, mode: LaunchMode.externalApplication);

/// Opens a page in the system's in-app browser: SFSafariViewController on
/// iOS, Custom Tabs on Android. Never a plain WebView, where Google refuses
/// sign-in; this one keeps the claude.ai login between opens. Tests replace it.
Future<bool> Function(Uri uri) launchInAppBrowser = (uri) => launchUrl(uri, mode: LaunchMode.inAppBrowserView);

/// Whether [uri] is a claude.ai page.
bool isClaudeAiLink(Uri uri) => uri.scheme == 'https' && (uri.host == 'claude.ai' || uri.host.endsWith('.claude.ai'));

Future<bool> _tryLaunch(Future<bool> Function(Uri uri) launch, Uri uri) async {
  try {
    return await launch(uri);
  } on Object {
    return false;
  }
}

/// Open [url]: claude.ai pages in the in-app browser (so the claude.ai login
/// works), others and a failed in-app open in the browser. When that fails
/// too, the link is copied instead.
Future<void> openExternalLink(BuildContext context, String url) async {
  final uri = Uri.tryParse(url);
  var opened = false;
  if (uri != null) {
    if (isClaudeAiLink(uri)) opened = await _tryLaunch(launchInAppBrowser, uri);
    if (!opened) opened = await _tryLaunch(launchExternal, uri);
  }
  if (opened) return;
  await Clipboard.setData(ClipboardData(text: url));
  if (context.mounted) toast(context, 'Could not open the link; it is copied.');
}
