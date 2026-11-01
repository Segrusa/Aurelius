import 'package:flutter/material.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:markdown/markdown.dart' as md;
import 'package:url_launcher/url_launcher.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../chat/chat_view.dart' show markdownStyle;

/// The Markdown a guideline or a proposal may use on the phone (E14): no HTML
/// (block or inline, it stays text), no bare-URL autolinks, and only headings,
/// lists, code, tables and emphasis besides.
final guidelineExtensions = md.ExtensionSet(
  const [
    md.FencedCodeBlockSyntax(),
    md.TableSyntax(),
    md.UnorderedListWithCheckboxSyntax(),
    md.OrderedListWithCheckboxSyntax(),
  ],
  [md.StrikethroughSyntax()],
);

/// The HTML-block syntax off: `<img …>` or `<script>` on a line of its own
/// is a paragraph of text, never markup.
final _noHtmlBlocks = <md.BlockSyntax>[const _HtmlAsText()];

class _HtmlAsText extends md.BlockSyntax {
  const _HtmlAsText();

  // Matches what HtmlBlockSyntax would take, so that syntax never sees it.
  @override
  RegExp get pattern => RegExp(r'^ {0,3}<');

  @override
  bool canParse(md.BlockParser parser) => pattern.hasMatch(parser.current.content);

  @override
  md.Node parse(md.BlockParser parser) {
    final lines = <String>[];
    while (!parser.isDone && parser.current.content.trim().isNotEmpty) {
      lines.add(parser.current.content);
      parser.advance();
    }
    // Plain text: never parsed for links or images, and the widget shows it as is.
    return md.Element('p', [md.Text(lines.join('\n'))]);
  }
}

/// A guideline's text as the team reads it, safely (E14): HTML shows as text,
/// an image is never loaded (its address shows instead), and a link opens only
/// after a question that shows its full address.
class GuidelineMarkdown extends StatelessWidget {
  const GuidelineMarkdown(this.data, {super.key, this.selectable = true});

  final String data;
  final bool selectable;

  @override
  Widget build(BuildContext context) => MarkdownBody(
        data: data,
        selectable: selectable,
        styleSheet: markdownStyle(context),
        extensionSet: guidelineExtensions,
        blockSyntaxes: _noHtmlBlocks,
        imageBuilder: (uri, title, alt) => NotLoadedImage(uri: uri, alt: alt),
        onTapLink: (text, href, title) {
          if (href != null) openGuidelineLink(context, href);
        },
      );
}

/// An image in a guideline: its alt text and address, never the picture (E14).
class NotLoadedImage extends StatelessWidget {
  const NotLoadedImage({super.key, required this.uri, this.alt});

  final Uri uri;
  final String? alt;

  @override
  Widget build(BuildContext context) => Text(
        '[image${(alt ?? '').isEmpty ? '' : ': $alt'} · $uri · not loaded]',
        key: const ValueKey('guideline-image-not-loaded'),
        style: TextStyle(fontSize: 13, color: Tokens.label2, fontFamily: Tokens.mono),
      );
}

/// A link in a guideline opens only after the board member saw its address (E14).
Future<void> openGuidelineLink(BuildContext context, String href) async {
  final uri = Uri.tryParse(href);
  final web = uri != null && (uri.scheme == 'https' || uri.scheme == 'http') && uri.host.isNotEmpty;
  if (!web) {
    await confirm(context, title: 'This link does not open', body: href);
    return;
  }
  if (!await confirm(context, title: 'Open this link?', body: href, action: 'Open')) return;
  await launchUrl(uri, mode: LaunchMode.externalApplication);
}
