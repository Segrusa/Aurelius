import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/links.dart';
import '../../data/markdown_paths.dart';
import '../../data/views.dart';
import '../chat/chat_view.dart' show markdownStyle;
import '../links/link_actions.dart' show AureliusLinkLabels;
import 'markdown_page.dart';
import 'mention_markdown.dart';
import 'mermaid.dart';

/// Tells a link handler that this activation is a long-press: it copies
/// [href] and returns true, so the handler does not open it.
typedef LinkCopied = bool Function(String href);

/// The link span (a [TextSpan] with a tap recognizer) under [position].
TapGestureRecognizer? linkAt(Offset position, int viewId) {
  final result = HitTestResult();
  WidgetsBinding.instance.hitTestInView(result, position, viewId);
  for (final entry in result.path) {
    final target = entry.target;
    if (target is TextSpan && target.recognizer is TapGestureRecognizer) return target.recognizer! as TapGestureRecognizer;
  }
  return null;
}

/// A long press that only starts on a link, a bit sooner than text selection's
/// own long press so it wins on links and leaves selection alone elsewhere.
class _LinkLongPressRecognizer extends LongPressGestureRecognizer {
  _LinkLongPressRecognizer({super.debugOwner}) : super(duration: const Duration(milliseconds: 400));

  @override
  bool isPointerAllowed(PointerDownEvent event) => super.isPointerAllowed(event) && linkAt(event.position, event.viewId) != null;
}

/// Long-press on a link in what [builder] builds copies the link instead of
/// opening it. The builder's link handlers check the [LinkCopied] they get first.
class LinkGestures extends StatefulWidget {
  const LinkGestures({super.key, required this.builder});

  final Widget Function(BuildContext context, LinkCopied copied) builder;

  @override
  State<LinkGestures> createState() => _LinkGesturesState();
}

class _LinkGesturesState extends State<LinkGestures> {
  bool _copying = false;

  bool _copied(String href) {
    if (!_copying) return false;
    final text = isMarkdownPath(href) ? Uri.decodeFull(href) : href;
    Clipboard.setData(ClipboardData(text: text));
    HapticFeedback.selectionClick();
    toast(context, 'Link copied.');
    return true;
  }

  void _longPress(LongPressStartDetails details) {
    final link = linkAt(details.globalPosition, View.of(context).viewId);
    if (link?.onTap == null) return;
    _copying = true;
    try {
      link!.onTap!();
    } finally {
      _copying = false;
    }
  }

  @override
  Widget build(BuildContext context) {
    return RawGestureDetector(
      gestures: {
        _LinkLongPressRecognizer: GestureRecognizerFactoryWithHandlers<_LinkLongPressRecognizer>(
          () => _LinkLongPressRecognizer(debugOwner: this),
          (r) => r.onLongPressStart = _longPress,
        ),
      },
      child: widget.builder(context, _copied),
    );
  }
}

/// Message or file Markdown: `.md` paths become links to the viewer, other
/// links (also autolinks and bare URLs) open in the browser, an Aurelius link
/// in the app under its #42, long-press copies a link. Code is never a link. Closed ```` ```mermaid ```` blocks draw as
/// diagrams unless [diagrams] is off (while an answer streams).
class MessageMarkdown extends StatelessWidget {
  const MessageMarkdown({super.key, required this.data, required this.scope, this.from, this.selectable = false, this.styleSheet, this.diagrams = true});

  final String data;

  /// Where `.md` links open; `null` leaves them inert.
  final FileScope? scope;

  /// The file [data] comes from, for relative links.
  final String? from;
  final bool selectable;
  final MarkdownStyleSheet? styleSheet;

  /// Draw mermaid blocks; off, they stay code blocks.
  final bool diagrams;

  @override
  Widget build(BuildContext context) {
    if (data.contains('aurelius://')) {
      return AureliusLinkLabels(text: data, builder: (context, label) => _body(context, labelAureliusLinks(data, label)));
    }
    return _body(context, data);
  }

  Widget _body(BuildContext context, String data) {
    final style = styleSheet ?? markdownStyle(context);
    final mermaid = diagrams ? mermaidMarkdown(style, selectable: selectable) : null;
    // The Slack-style Team chat marks its @names (AUR-CHAT-001 §2).
    final mentions = MentionScope.maybeOf(context);
    return LinkGestures(
      builder: (context, copied) => MarkdownBody(
        // MarkdownBody only parses again when text or style change, so a
        // finished answer needs a new one to draw its diagrams, and one whose
        // names became known one to mark them.
        key: ValueKey((diagrams, mentions?.me, mentions?.known.length)),
        data: linkMarkdownPaths(data),
        selectable: selectable,
        styleSheet: style,
        blockSyntaxes: mermaid?.blockSyntaxes,
        inlineSyntaxes: mentions == null ? null : [MentionSyntax(mentions)],
        builders: {...?mermaid?.builders, if (mentions != null) 'mention': MentionBuilder()},
        onTapLink: markdownLinkTap(context, scope, from: from, copied: copied),
      ),
    );
  }
}

/// Plain text (the user's own message) with its URLs and `.md` paths as links,
/// like [MessageMarkdown] but without rendering the rest.
class LinkedText extends StatefulWidget {
  const LinkedText(this.text, {super.key, required this.scope, this.style});

  final String text;
  final FileScope? scope;
  final TextStyle? style;

  @override
  State<LinkedText> createState() => _LinkedTextState();
}

class _LinkedTextState extends State<LinkedText> {
  final _recognizers = <TapGestureRecognizer>[];

  void _dropRecognizers() {
    for (final r in _recognizers) {
      r.dispose();
    }
    _recognizers.clear();
  }

  @override
  void dispose() {
    _dropRecognizers();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final text = widget.text;
    final links = plainTextLinks(text);
    if (links.isEmpty) return Text(text, style: widget.style);
    if (text.contains('aurelius://')) return AureliusLinkLabels(text: text, builder: (context, label) => _linked(text, links, label));
    return _linked(text, links, null);
  }

  Widget _linked(String text, List<TextLink> links, String Function(AureliusLink)? labelOf) {
    return LinkGestures(builder: (context, copied) {
      _dropRecognizers();
      final tap = markdownLinkTap(context, widget.scope, copied: copied);
      final linkStyle = TextStyle(color: Tokens.accentText);
      final spans = <InlineSpan>[];
      var at = 0;
      for (final link in links) {
        if (link.start > at) spans.add(TextSpan(text: text.substring(at, link.start)));
        final written = text.substring(link.start, link.end);
        final ours = labelOf == null ? null : parseAureliusLink(link.target);
        final label = ours == null ? written : labelOf!(ours);
        final recognizer = TapGestureRecognizer()..onTap = () => tap(label, link.target, '');
        _recognizers.add(recognizer);
        spans.add(TextSpan(text: label, style: linkStyle, recognizer: recognizer));
        at = link.end;
      }
      if (at < text.length) spans.add(TextSpan(text: text.substring(at)));
      return Text.rich(TextSpan(children: spans), style: widget.style);
    });
  }
}
