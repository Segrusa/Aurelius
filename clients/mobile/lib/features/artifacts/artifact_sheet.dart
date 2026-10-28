import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:webview_flutter/webview_flutter.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import '../chat/chat_view.dart' show markdownStyle;
import '../markdown/mermaid.dart';
import 'artifacts.dart';

/// Open an artifact: in the bottom sheet when the server kept its file
/// ([ArtifactView.hasContent]), else in the browser as before.
Future<void> openArtifact(BuildContext context, ArtifactView artifact) {
  if (!artifact.hasContent) return openExternalLink(context, artifact.url);
  final repository = context.read<AureliusRepository>();
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    backgroundColor: Colors.transparent,
    builder: (_) => RepositoryProvider.value(value: repository, child: ArtifactSheet(artifact: artifact)),
  );
}

/// Draws the HTML of an artifact; tests replace it (a platform webview cannot run there).
Widget Function(String html) artifactHtmlView = (html) => ArtifactWebView(html: html);

/// The artifact's kept content in a draggable sheet: title, version (a picker
/// when several are kept), Open in browser, Copy link and close. Markdown uses
/// the message renderer, HTML an isolated webview, anything else plain text.
/// Follows the newest version on a republish unless an older one was picked.
class ArtifactSheet extends StatefulWidget {
  const ArtifactSheet({super.key, required this.artifact});

  final ArtifactView artifact;

  @override
  State<ArtifactSheet> createState() => _ArtifactSheetState();
}

class _ArtifactSheetState extends State<ArtifactSheet> {
  late final AureliusRepository _repository = context.read<AureliusRepository>();
  final _sheet = DraggableScrollableController();
  late Future<ArtifactContent> _content = _load();
  late final StreamSubscription<Event> _published;

  /// The version picked; null follows the newest.
  int? _version;

  @override
  void initState() {
    super.initState();
    _published = _repository.changes.where((e) => e.kind == EventKind.artifactPublished).listen((_) {
      if (_version == null) _reload();
    });
  }

  @override
  void dispose() {
    _published.cancel();
    _sheet.dispose();
    super.dispose();
  }

  Future<ArtifactContent> _load() => _repository.artifactContent(widget.artifact.url, version: _version);

  void _reload() {
    if (!mounted) return;
    final next = _load();
    setState(() {
      _content = next;
    });
  }

  void _pick(int version, int newest) {
    _version = version == newest ? null : version;
    _reload();
  }

  /// The handle drags the whole sheet (the body scrolls on its own, also the webview).
  void _drag(DragUpdateDetails d) {
    final height = MediaQuery.sizeOf(context).height;
    if (!_sheet.isAttached || height == 0) return;
    _sheet.jumpTo((_sheet.size - (d.primaryDelta ?? 0) / height).clamp(0.0, 1.0));
  }

  void _dragEnd(DragEndDetails d) {
    if (!_sheet.isAttached) return;
    if (_sheet.size < 0.45 || (d.primaryVelocity ?? 0) > 900) {
      Navigator.of(context).maybePop();
    } else if ((d.primaryVelocity ?? 0) < -900) {
      _sheet.animateTo(1, duration: const Duration(milliseconds: 200), curve: Curves.easeOut);
    }
  }

  @override
  Widget build(BuildContext context) {
    return DraggableScrollableSheet(
      controller: _sheet,
      initialChildSize: 0.92,
      minChildSize: 0.3,
      maxChildSize: 1,
      expand: false,
      snap: true,
      snapSizes: const [0.6, 0.92],
      builder: (context, _) => Container(
        key: const Key('artifact-sheet'),
        decoration: BoxDecoration(
          color: Tokens.bg,
          borderRadius: const BorderRadius.vertical(top: Radius.circular(16)),
          border: Border(top: BorderSide(color: Tokens.border)),
        ),
        clipBehavior: Clip.antiAlias,
        child: FutureBuilder<ArtifactContent>(
          future: _content,
          builder: (context, snapshot) {
            final content = snapshot.data;
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _header(content),
                Divider(height: 1, color: Tokens.border),
                Expanded(child: _body(snapshot)),
              ],
            );
          },
        ),
      ),
    );
  }

  Widget _header(ArtifactContent? content) {
    final title = content?.artifact.title ?? widget.artifact.title;
    final snap = content?.snapshot;
    final versions = content?.versions ?? const <int>[];
    final shown = snap?.version ?? _version;
    final muted = TextStyle(fontSize: 12, color: Tokens.muted);
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onVerticalDragUpdate: _drag,
      onVerticalDragEnd: _dragEnd,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 8, 8, 8),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Center(
              child: Container(
                key: const Key('artifact-sheet-handle'),
                width: 36,
                height: 4,
                decoration: BoxDecoration(color: Tokens.border, borderRadius: BorderRadius.circular(2)),
              ),
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                Icon(Icons.public, size: 18, color: Tokens.accentText),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(title.isEmpty ? widget.artifact.url : title,
                      maxLines: 2, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600, color: Tokens.text)),
                ),
                IconButton(
                  key: const Key('artifact-sheet-close'),
                  tooltip: 'Close',
                  icon: const Icon(Icons.close),
                  onPressed: () => Navigator.of(context).maybePop(),
                ),
              ],
            ),
            Text(
              [
                if (shown != null) 'Version $shown',
                if (snap != null) snap.fileName,
                if (snap != null) Fmt.ago(snap.publishedAtMs),
              ].join(' · '),
              style: muted,
            ),
            if (versions.length > 1) ...[
              const SizedBox(height: 6),
              SizedBox(
                height: 32,
                child: ListView(
                  key: const Key('artifact-versions'),
                  scrollDirection: Axis.horizontal,
                  children: [
                    for (final v in versions)
                      Padding(
                        padding: const EdgeInsets.only(right: 6),
                        child: ChoiceChip(
                          key: ValueKey('artifact-version-$v'),
                          label: Text('v$v'),
                          selected: v == shown,
                          visualDensity: VisualDensity.compact,
                          onSelected: (_) => _pick(v, versions.first),
                        ),
                      ),
                  ],
                ),
              ),
            ],
            const SizedBox(height: 4),
            Row(
              children: [
                TextButton.icon(
                  key: const Key('artifact-sheet-open'),
                  style: TextButton.styleFrom(visualDensity: VisualDensity.compact),
                  icon: const Icon(Icons.open_in_new, size: 16),
                  label: const Text('Open in browser'),
                  onPressed: () => openExternalLink(context, widget.artifact.url),
                ),
                CopyButton(key: const Key('artifact-sheet-copy'), text: widget.artifact.url, tooltip: 'Copy link'),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _body(AsyncSnapshot<ArtifactContent> snapshot) {
    final muted = TextStyle(color: Tokens.muted);
    if (snapshot.hasError) {
      final e = snapshot.error;
      final text = e is ProtocolException && e.code == ErrorCode.notFound
          ? 'This artifact is no longer on the server (its conversation may have been deleted).'
          : e is ProtocolException && e.message.isNotEmpty
              ? e.message
              : 'The artifact could not be loaded.';
      return ListView(padding: const EdgeInsets.all(24), children: [
        Text(text, key: const Key('artifact-sheet-error'), textAlign: TextAlign.center, style: muted),
        const SizedBox(height: 16),
        Center(child: OutlinedButton(onPressed: _reload, child: const Text('Try again'))),
      ]);
    }
    final content = snapshot.data;
    if (content == null || snapshot.connectionState == ConnectionState.waiting) return const Center(child: CircularProgressIndicator());
    final snap = content.snapshot;
    if (snap == null) {
      return ListView(padding: const EdgeInsets.all(24), children: [
        Text('The server kept no copy of this version, so it can only be shown on claude.ai. Open it in the browser.',
            key: const Key('artifact-sheet-none'), textAlign: TextAlign.center, style: muted),
      ]);
    }
    final style = markdownStyle(context);
    final mermaid = mermaidMarkdown(style, selectable: true);
    return switch (snap.kind) {
      ArtifactKind.html => KeyedSubtree(key: ValueKey('artifact-html-${snap.version}'), child: artifactHtmlView(snap.content)),
      ArtifactKind.markdown => ListView(
          key: const Key('artifact-markdown'),
          padding: const EdgeInsets.all(16),
          children: [
            MarkdownBody(
              data: snap.content,
              selectable: true,
              styleSheet: style,
              blockSyntaxes: mermaid.blockSyntaxes,
              builders: mermaid.builders,
              onTapLink: (text, href, title) {
                if (href != null && isExternalArtifactLink(href)) openExternalLink(context, href);
              },
            ),
          ],
        ),
      ArtifactKind.text => ListView(
          key: const Key('artifact-text'),
          padding: const EdgeInsets.all(16),
          children: [SelectableText(snap.content, style: TextStyle(fontFamily: Tokens.mono, fontSize: 12.5, height: 1.45, color: Tokens.text))],
        ),
    };
  }
}

/// http(s) and mailto: what may leave for the phone's browser.
bool isExternalArtifactLink(String url) {
  final lower = url.trim().toLowerCase();
  return lower.startsWith('https://') || lower.startsWith('http://') || lower.startsWith('mailto:');
}

/// Whether the webview may go to [url]: only the page itself (loaded from a
/// string, so about:blank or data:). Anything else opens outside.
bool artifactNavigationStays(String url) => url.startsWith('about:') || url.startsWith('data:');

/// The page with a Content-Security-Policy that blocks every connection
/// (fetch, XHR, WebSocket: so no daemon, gateway or local server) and form
/// posts. Scripts and styles from the page (or a CDN) still run.
String withArtifactCsp(String html) {
  const meta = '<meta http-equiv="Content-Security-Policy" content="connect-src \'none\'; form-action \'none\'">';
  final head = RegExp(r'<head(\s[^>]*)?>', caseSensitive: false).firstMatch(html);
  if (head != null) return html.replaceRange(head.end, head.end, meta);
  final open = RegExp(r'<html(\s[^>]*)?>', caseSensitive: false).firstMatch(html);
  if (open != null) return html.replaceRange(open.end, open.end, '<head>$meta</head>');
  final doctype = RegExp(r'<!doctype[^>]*>', caseSensitive: false).firstMatch(html);
  if (doctype != null) return html.replaceRange(doctype.end, doctype.end, meta);
  return '$meta$html';
}

/// The HTML artifact in a platform webview, isolated: no JavaScript channel
/// to the app, connections blocked by CSP, and every navigation away from the
/// page (links, target=_blank, redirects) opens in the phone's browser instead.
class ArtifactWebView extends StatefulWidget {
  const ArtifactWebView({super.key, required this.html});

  final String html;

  @override
  State<ArtifactWebView> createState() => _ArtifactWebViewState();
}

class _ArtifactWebViewState extends State<ArtifactWebView> {
  late final WebViewController _controller = WebViewController()
    ..setJavaScriptMode(JavaScriptMode.unrestricted)
    ..setBackgroundColor(Colors.white)
    ..setNavigationDelegate(NavigationDelegate(
      onNavigationRequest: (request) {
        if (artifactNavigationStays(request.url)) return NavigationDecision.navigate;
        if (isExternalArtifactLink(request.url) && mounted) openExternalLink(context, request.url);
        return NavigationDecision.prevent;
      },
    ))
    ..loadHtmlString(withArtifactCsp(widget.html));

  @override
  Widget build(BuildContext context) => WebViewWidget(
        controller: _controller,
        // The page scrolls and zooms itself inside the sheet.
        gestureRecognizers: {Factory<OneSequenceGestureRecognizer>(() => EagerGestureRecognizer())},
      );
}
