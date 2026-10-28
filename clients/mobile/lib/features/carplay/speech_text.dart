/// What CarPlay reads aloud: an employee's markdown as plain spoken sentences,
/// and a long answer cut to its first part (the driver can read the rest on
/// the phone later).
library;

/// About half a minute of speech.
const spokenLimit = 600;

const restOnPhone = 'That was the first part; the rest is on your phone.';
const codeLeftOut = '(code left out)';

/// Markdown, code and links removed; one line of sentences.
String speakable(String markdown) {
  var text = markdown.replaceAll('\r\n', '\n');

  // Blocks first: code, HTML, images, tables, rules.
  text = text.replaceAll(RegExp(r'(```|~~~)[^\n]*\n[\s\S]*?(\n\1[^\n]*|$(?![\s\S]))', multiLine: true), '\n$codeLeftOut\n');
  text = text.replaceAll(RegExp(r'<[^>\n]+>'), '');
  text = text.replaceAll(RegExp(r'!\[[^\]]*\]\([^)]*\)'), '');
  text = text.replaceAllMapped(RegExp(r'\[([^\]]+)\]\([^)]*\)'), (m) => m[1]!);
  text = text.replaceAll(RegExp(r'https?://\S+'), 'a link');

  final lines = <String>[];
  for (var line in text.split('\n')) {
    line = line.trim();
    if (line.isEmpty) {
      lines.add('');
      continue;
    }
    if (RegExp(r'^(-{3,}|\*{3,}|_{3,})$').hasMatch(line)) continue;
    if (RegExp(r'^\|?[\s:|-]+\|[\s:|-]*$').hasMatch(line) && line.contains('-')) continue;
    var block = false;
    if (line.startsWith('|')) {
      line = line.split('|').map((c) => c.trim()).where((c) => c.isNotEmpty).join(', ');
      block = true;
    }
    final heading = RegExp(r'^#{1,6}\s+').firstMatch(line);
    if (heading != null) {
      line = line.substring(heading.end);
      block = true;
    }
    line = line.replaceFirst(RegExp(r'^(>\s*)+'), '');
    final item = RegExp(r'^([-*+]|\d{1,3}[.)])\s+(\[[ xX]\]\s+)?').firstMatch(line);
    if (item != null) {
      line = line.substring(item.end);
      block = true;
    }
    line = _inline(line);
    if (line.isEmpty) continue;
    // Headings, list items and table rows become sentences of their own.
    if (block && !RegExp(r'[.!?:;…]$').hasMatch(line)) line = '$line.';
    lines.add(line);
  }

  // Paragraph breaks end a sentence too.
  final out = StringBuffer();
  for (final line in lines) {
    if (line.isEmpty) {
      final soFar = out.toString().trimRight();
      if (soFar.isNotEmpty && !RegExp(r'[.!?:;…)]$').hasMatch(soFar)) out.write('.');
      continue;
    }
    if (out.isNotEmpty) out.write(' ');
    out.write(line);
  }
  return out.toString().replaceAll(RegExp(r'\s+'), ' ').replaceAll(RegExp(r'\s+([.,;:!?])'), r'$1').replaceAll(RegExp(r'(?<!\.)\.\.(?!\.)'), '.').trim();
}

String _inline(String line) {
  var text = line;
  // Short inline code is read as words; longer is skipped.
  text = text.replaceAllMapped(RegExp(r'`([^`]*)`'), (m) => m[1]!.length <= 40 ? m[1]! : 'code');
  text = text.replaceAllMapped(RegExp(r'(\*\*|__)(.+?)\1'), (m) => m[2]!);
  text = text.replaceAllMapped(RegExp(r'~~(.+?)~~'), (m) => m[1]!);
  text = text.replaceAllMapped(RegExp(r'(^|[^\w*])\*(?!\s)([^*]+?)\*(?!\w)'), (m) => '${m[1]}${m[2]}');
  text = text.replaceAllMapped(RegExp(r'(^|\W)_(?!\s)([^_]+?)_(?!\w)'), (m) => '${m[1]}${m[2]}');
  text = text.replaceAll(RegExp(r'[`*#]'), '');
  text = text.replaceAll('→', ' then ').replaceAll('&nbsp;', ' ').replaceAll('&amp;', '&');
  return text.replaceAll(RegExp(r'\s+'), ' ').trim();
}

/// [text] when short enough; otherwise its first sentences up to [limit]
/// characters and [restOnPhone].
String shortenForSpeech(String text, {int limit = spokenLimit}) {
  if (text.length <= limit) return text;
  final sentences = text.split(RegExp(r'(?<=[.!?…])\s+'));
  final kept = StringBuffer();
  for (final sentence in sentences) {
    if (kept.length + sentence.length + 1 > limit) break;
    if (kept.isNotEmpty) kept.write(' ');
    kept.write(sentence);
  }
  if (kept.isEmpty) {
    // One very long sentence: stop at a word.
    final cut = text.lastIndexOf(' ', limit);
    kept.write('${text.substring(0, cut > limit ~/ 2 ? cut : limit).trimRight()}…');
  }
  return '$kept $restOnPhone';
}

/// An employee's answer as it is read aloud.
String spokenAnswer(String markdown, {int limit = spokenLimit}) => shortenForSpeech(speakable(markdown), limit: limit);
