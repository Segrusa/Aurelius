import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:markdown/markdown.dart' as md;
import 'package:webview_flutter/webview_flutter.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';

/// The page that draws one diagram with the bundled mermaid.min.js, so
/// diagrams work offline. It reports on the `MermaidChannel`.
const mermaidPage = 'assets/mermaid/mermaid.html';

/// The tallest a diagram is drawn in a message; tap it to see it whole.
const mermaidInlineMaxHeight = 420.0;

/// A closed ```` ```mermaid ```` fence becomes a `mermaid` element (its
/// source in the `source` attribute) instead of a code block. An unclosed
/// fence, as while an answer streams, stays a plain code block.
class MermaidBlockSyntax extends md.FencedCodeBlockSyntax {
  const MermaidBlockSyntax();

  static final _open = RegExp(r'^ {0,3}(`{3,}|~{3,})[ \t]*mermaid[ \t]*$', caseSensitive: false);

  @override
  bool canParse(md.BlockParser parser) {
    final open = _open.firstMatch(parser.current.content);
    if (open == null) return false;
    final fence = open.group(1)!;
    final close = RegExp('^ {0,3}${RegExp.escape(fence[0])}{${fence.length},}[ \\t]*\$');
    for (var i = 1; parser.peek(i) != null; i++) {
      if (close.hasMatch(parser.peek(i)!.content)) return true;
    }
    return false;
  }

  @override
  md.Node parse(md.BlockParser parser) {
    final pre = super.parse(parser) as md.Element;
    final source = pre.children!.first.textContent.trimRight();
    return md.Element.empty('mermaid')..attributes['source'] = source;
  }
}

/// Draws the `mermaid` elements of [MermaidBlockSyntax] as [MermaidBlock]s.
class MermaidBuilder extends MarkdownElementBuilder {
  MermaidBuilder({required this.styleSheet, this.selectable = false});

  final MarkdownStyleSheet styleSheet;
  final bool selectable;

  @override
  bool isBlockElement() => true;

  @override
  Widget? visitElementAfterWithContext(BuildContext context, md.Element element, TextStyle? preferredStyle, TextStyle? parentStyle) {
    final source = element.attributes['source'] ?? '';
    return MermaidBlock(key: ValueKey('mermaid-${Tokens.dark}-$source'), source: source, styleSheet: styleSheet, selectable: selectable);
  }
}

/// What the page said about a diagram: drawn at [height], or not valid
/// Mermaid ([failed]).
@immutable
class MermaidOutcome {
  const MermaidOutcome.drawn(double this.height);
  const MermaidOutcome.failed() : height = null;

  final double? height;
  bool get failed => height == null;

  @override
  bool operator ==(Object other) => other is MermaidOutcome && other.height == height;

  @override
  int get hashCode => height.hashCode;
}

/// A channel message from the page (`ok:<height>` or `error`); null for
/// anything else.
MermaidOutcome? parseMermaidMessage(String message) {
  if (message == 'error') return const MermaidOutcome.failed();
  if (!message.startsWith('ok:')) return null;
  final height = double.tryParse(message.substring(3));
  return height == null || height <= 0 || !height.isFinite ? const MermaidOutcome.failed() : MermaidOutcome.drawn(height);
}

/// The JavaScript that draws [source] in the page. The source goes in as a
/// JSON string, so it cannot break out of the call.
String mermaidRenderCall(String source, {required bool dark, double maxHeight = 0, bool zoom = false}) =>
    'aureliusRender(${jsonEncode(source)}, $dark, ${maxHeight.round()}, $zoom);';

/// One diagram to draw.
@immutable
class MermaidRequest {
  const MermaidRequest({required this.source, required this.dark, required this.onOutcome, this.maxHeight = 0, this.zoom = false});

  final String source;
  final bool dark;

  /// Caps the drawn height (0: none); the diagram shrinks to fit.
  final double maxHeight;

  /// Full screen: the page scrolls and pinch-zooms itself.
  final bool zoom;
  final ValueChanged<MermaidOutcome> onOutcome;
}

/// Draws a diagram; tests replace it (a platform webview cannot run there).
Widget Function(MermaidRequest request) mermaidView = (request) => MermaidWebView(request: request);

/// Outcomes per theme and source, so a diagram scrolled back into view gets
/// its size at once and invalid sources go straight to the code block.
final _outcomes = <String, MermaidOutcome>{};

String _outcomeKey(bool dark, String source) => '$dark\n$source';

void _remember(String key, MermaidOutcome outcome) {
  _outcomes.remove(key);
  _outcomes[key] = outcome;
  while (_outcomes.length > 200) {
    _outcomes.remove(_outcomes.keys.first);
  }
}

@visibleForTesting
void clearMermaidOutcomes() => _outcomes.clear();

/// A Mermaid diagram in a message. Until the page has drawn it, and when the
/// source is not valid Mermaid, it shows as the plain code block it is. Tap
/// the diagram to open it full screen.
class MermaidBlock extends StatefulWidget {
  const MermaidBlock({super.key, required this.source, required this.styleSheet, this.selectable = false});

  final String source;
  final MarkdownStyleSheet styleSheet;
  final bool selectable;

  @override
  State<MermaidBlock> createState() => _MermaidBlockState();
}

class _MermaidBlockState extends State<MermaidBlock> {
  late final bool _dark = Tokens.dark;
  late final String _key = _outcomeKey(_dark, widget.source);
  late MermaidOutcome? _outcome = _outcomes[_key];

  void _onOutcome(MermaidOutcome outcome) {
    _remember(_key, outcome);
    if (mounted && outcome != _outcome) setState(() => _outcome = outcome);
  }

  @override
  Widget build(BuildContext context) {
    final outcome = _outcome;
    final code = MermaidCodeBlock(source: widget.source, styleSheet: widget.styleSheet, selectable: widget.selectable);
    if (outcome != null && outcome.failed) return code;
    final view = IgnorePointer(
      child: mermaidView(MermaidRequest(source: widget.source, dark: _dark, maxHeight: mermaidInlineMaxHeight, onOutcome: _onOutcome)),
    );
    final height = outcome?.height;
    // While drawing, the code shows and the page gets one transparent pixel of
    // height (not Opacity 0, which would stop painting the platform view). The
    // keyed diagram keeps its place in the tree, so the page is not reloaded.
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        if (height == null) code,
        Semantics(
          key: const ValueKey('mermaid-view'),
          label: height == null ? null : 'Diagram',
          button: height != null,
          child: GestureDetector(
            key: const Key('mermaid-diagram'),
            behavior: HitTestBehavior.opaque,
            onTap: height == null ? null : () => MermaidPage.open(context, widget.source),
            // The same widgets either way: a changed tree would recreate the page.
            child: DecoratedBox(
              decoration: height == null ? const BoxDecoration() : widget.styleSheet.codeblockDecoration ?? const BoxDecoration(),
              child: ClipRRect(
                borderRadius: BorderRadius.circular(Tokens.radius),
                child: SizedBox(height: height ?? 1, child: view),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

/// The diagram's source as the Markdown renderer shows a code block.
class MermaidCodeBlock extends StatelessWidget {
  const MermaidCodeBlock({super.key, required this.source, required this.styleSheet, this.selectable = false});

  final String source;
  final MarkdownStyleSheet styleSheet;
  final bool selectable;

  @override
  Widget build(BuildContext context) {
    return Container(
      key: const Key('mermaid-code'),
      clipBehavior: Clip.hardEdge,
      decoration: styleSheet.codeblockDecoration,
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        padding: styleSheet.codeblockPadding,
        child: selectable ? SelectableText(source, style: styleSheet.code) : Text(source, style: styleSheet.code),
      ),
    );
  }
}

/// A diagram full screen: pinch to zoom, copy its source, or show the code.
class MermaidPage extends StatefulWidget {
  const MermaidPage({super.key, required this.source});

  final String source;

  static Future<void> open(BuildContext context, String source) =>
      Navigator.of(context).push(MaterialPageRoute<void>(builder: (_) => MermaidPage(source: source)));

  @override
  State<MermaidPage> createState() => _MermaidPageState();
}

class _MermaidPageState extends State<MermaidPage> {
  bool _code = false;
  bool _failed = false;

  @override
  Widget build(BuildContext context) {
    final style = MarkdownStyleSheet.fromTheme(Theme.of(context)).copyWith(
      code: TextStyle(fontFamily: Tokens.mono, fontSize: 13, height: 1.45, color: Tokens.text),
      codeblockDecoration: BoxDecoration(color: Tokens.surface, borderRadius: BorderRadius.circular(Tokens.radius), border: Border.all(color: Tokens.border)),
    );
    final code = _code || _failed;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Diagram'),
        actions: [
          if (!_failed)
            IconButton(
              key: const Key('mermaid-toggle-code'),
              tooltip: code ? 'Show diagram' : 'Show code',
              icon: Icon(code ? Icons.account_tree_outlined : Icons.code),
              onPressed: () => setState(() => _code = !_code),
            ),
          CopyButton(key: const Key('mermaid-copy'), text: widget.source, tooltip: 'Copy source'),
          const SizedBox(width: 8),
        ],
      ),
      body: code
          ? ListView(padding: const EdgeInsets.all(16), children: [MermaidCodeBlock(source: widget.source, styleSheet: style, selectable: true)])
          : mermaidView(MermaidRequest(
              source: widget.source,
              dark: Tokens.dark,
              zoom: true,
              onOutcome: (o) {
                if (o.failed && mounted) setState(() => _failed = true);
              },
            )),
    );
  }
}

/// The bundled page in a platform webview: no network (CSP), no navigation,
/// and only `ok:<height>` / `error` come back to the app.
class MermaidWebView extends StatefulWidget {
  const MermaidWebView({super.key, required this.request});

  final MermaidRequest request;

  @override
  State<MermaidWebView> createState() => _MermaidWebViewState();
}

class _MermaidWebViewState extends State<MermaidWebView> {
  bool _started = false;

  late final WebViewController _controller = WebViewController()
    ..setJavaScriptMode(JavaScriptMode.unrestricted)
    ..setBackgroundColor(Colors.transparent)
    ..addJavaScriptChannel('MermaidChannel', onMessageReceived: (message) {
      final outcome = parseMermaidMessage(message.message);
      if (outcome != null && mounted) widget.request.onOutcome(outcome);
    })
    ..setNavigationDelegate(NavigationDelegate(
      onPageFinished: (_) => _render(),
      onWebResourceError: (error) {
        if (error.isForMainFrame ?? true) widget.request.onOutcome(const MermaidOutcome.failed());
      },
      // Only the bundled page itself; diagrams have no links in strict mode.
      onNavigationRequest: (request) =>
          request.url.startsWith('file:') || request.url.startsWith('about:') ? NavigationDecision.navigate : NavigationDecision.prevent,
    ))
    ..loadFlutterAsset(mermaidPage);

  void _render() {
    if (_started || !mounted) return;
    _started = true;
    final r = widget.request;
    _controller.runJavaScript(mermaidRenderCall(r.source, dark: r.dark, maxHeight: r.maxHeight, zoom: r.zoom)).catchError((Object _) {
      if (mounted) r.onOutcome(const MermaidOutcome.failed());
    });
  }

  @override
  Widget build(BuildContext context) => WebViewWidget(
        controller: _controller,
        gestureRecognizers: widget.request.zoom ? {Factory<OneSequenceGestureRecognizer>(() => EagerGestureRecognizer())} : const {},
      );
}

/// The Markdown hooks that turn closed mermaid fences into diagrams.
({List<md.BlockSyntax> blockSyntaxes, Map<String, MarkdownElementBuilder> builders}) mermaidMarkdown(MarkdownStyleSheet styleSheet, {bool selectable = false}) => (
      blockSyntaxes: const [MermaidBlockSyntax()],
      builders: {'mermaid': MermaidBuilder(styleSheet: styleSheet, selectable: selectable)},
    );
