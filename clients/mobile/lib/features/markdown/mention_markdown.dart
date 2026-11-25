import 'package:flutter/material.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:markdown/markdown.dart' as md;

import '../../core/theme/tokens.dart';
import '../../data/channels.dart';
import '../../data/discussion.dart';

/// Who an `@name` in the messages below can be (AUR-CHAT-001 §2): a
/// [MessageMarkdown] under it marks the names of [known], [me] the strongest.
/// Only a ticket's session (a group conversation) puts one up.
class MentionScope extends InheritedWidget {
  const MentionScope({super.key, required this.known, this.me, required super.child});

  /// The employees and board members a mention can name.
  final List<String> known;

  /// The signed-in board member.
  final String? me;

  static MentionScope? maybeOf(BuildContext context) => context.dependOnInheritedWidgetOfExactType<MentionScope>();

  @override
  bool updateShouldNotify(MentionScope old) => old.me != me || old.known.length != known.length || !old.known.every(known.contains);
}

/// The tag of a marked `@name` in the Markdown tree.
const _mentionTag = 'mention';

/// Parses `@name` (as [mentionPattern]) into a [_mentionTag] element when it
/// names someone of [scope]; code spans never get here (their syntax runs first).
class MentionSyntax extends md.InlineSyntax {
  MentionSyntax(this.scope) : super(mentionPattern.pattern, startCharacter: 0x40);

  final MentionScope scope;

  /// As [md.InlineSyntax.tryMatch], with [mentionPattern] itself: the base
  /// class compiles its pattern without `unicode`, so `\p{L}` would match nothing.
  @override
  bool tryMatch(md.InlineParser parser, [int? startMatchPos]) {
    final at = startMatchPos ?? parser.pos;
    if (parser.source.codeUnitAt(at) != 0x40) return false;
    final match = mentionPattern.matchAsPrefix(parser.source, at);
    if (match == null) return false;
    parser.writeText();
    if (onMatch(parser, match)) parser.consume(match[0]!.length);
    return true;
  }

  @override
  bool onMatch(md.InlineParser parser, Match match) {
    final token = match[1]!;
    final mark = mentionMarkOf(token, known: scope.known, me: scope.me);
    final name = trimmedMention(token);
    if (mark == null || name.isEmpty) {
      parser.addNode(md.Text(match[0]!));
      return true;
    }
    parser.addNode(md.Element.text(_mentionTag, '@$name')..attributes['mark'] = mark.name);
    // The trailing dot or dash belongs to the sentence.
    final rest = token.substring(name.length);
    if (rest.isNotEmpty) parser.addNode(md.Text(rest));
    return true;
  }
}

/// Draws a marked `@name` in the paragraph's text: ACCENT_TEXT, and for the
/// signed-in member SEMIBOLD on ACCENT_HALO (§2).
class MentionBuilder extends MarkdownElementBuilder {
  @override
  Widget? visitElementAfterWithContext(BuildContext context, md.Element element, TextStyle? preferredStyle, TextStyle? parentStyle) {
    final me = element.attributes['mark'] == MentionMark.me.name;
    final style = (parentStyle ?? const TextStyle()).copyWith(
      color: Tokens.accentText,
      fontWeight: me ? FontWeight.w600 : null,
      backgroundColor: me ? Tokens.accentHalo : null,
    );
    return Text.rich(TextSpan(text: element.textContent, style: style));
  }
}
