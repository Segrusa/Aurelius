// Pure helpers for Markdown links and paths in the in-app viewer.

import 'links.dart';

final _markdownExtension = RegExp(r'\.(md|markdown)$', caseSensitive: false);

/// Whether a path or link target (without `#anchor` or `?query`) names a Markdown file.
bool isMarkdownPath(String target) {
  final t = _stripFragment(target);
  return t.isNotEmpty && !t.contains('://') && !t.startsWith('mailto:') && _markdownExtension.hasMatch(t);
}

String _stripFragment(String target) {
  var t = target.trim();
  for (final c in ['#', '?']) {
    final i = t.indexOf(c);
    if (i >= 0) t = t.substring(0, i);
  }
  return t;
}

/// The path to ask the server for when a link or path [target] is opened from
/// the file [from] (relative to the work folder, `null` from a message).
/// Absolute paths are kept; relative ones resolve against [from]'s folder.
String resolveMarkdownPath(String target, {String? from}) {
  final t = _stripFragment(Uri.decodeFull(target)).replaceAll('\\', '/');
  final absolute = t.startsWith('/') || RegExp(r'^[A-Za-z]:/').hasMatch(t);
  if (absolute) return t;
  final base = from == null || !from.contains('/') ? <String>[] : from.substring(0, from.lastIndexOf('/')).split('/');
  final parts = [...base];
  for (final seg in t.split('/')) {
    if (seg.isEmpty || seg == '.') continue;
    if (seg == '..') {
      if (parts.isNotEmpty) parts.removeLast();
    } else {
      parts.add(seg);
    }
  }
  return parts.join('/');
}

final _codePath = RegExp(r'`([^`\s]+\.(?:md|markdown))`', caseSensitive: false);

final _token = RegExp(r'\S+');
final _lead = RegExp('^[("\']*');
final _trail = RegExp('[.,;:!?)"\']*\$');

/// Message text with `.md` paths (in inline code or bare in plain text) turned
/// into links, so they open in the viewer; fenced code, other inline code and
/// existing links are left alone. Bare paths count only as whitespace-free tokens.
// shortcut: indented (4-space) code blocks are not recognised, only fenced ones; upgrade if models start using them.
String linkMarkdownPaths(String text) {
  var fenced = false;
  var code = false;
  var bracket = 0;
  String bare(Match m) {
    final token = m[0]!;
    final plain = !code && bracket == 0 && !token.contains(RegExp(r'[`\[\]<>*|]'));
    if ('`'.allMatches(token).length.isOdd) code = !code;
    bracket = (bracket + '['.allMatches(token).length - ']'.allMatches(token).length).clamp(0, 1 << 30);
    final lead = _lead.firstMatch(token)![0]!;
    final rest = token.substring(lead.length);
    final tail = _trail.firstMatch(rest)![0]!;
    final core = rest.substring(0, rest.length - tail.length);
    // A closing quote without its opening one ends a quoted path with spaces: not this token alone.
    final cut = tail.contains(RegExp('["\']')) && !lead.contains(RegExp('["\']'));
    if (!plain || cut || !isMarkdownPath(core) || core.contains('#') || core.contains('?')) return token;
    return '$lead[${core.replaceAll('\\', '\\\\')}](${Uri.encodeFull(core.replaceAll('\\', '/'))})$tail';
  }

  final out = <String>[];
  for (final line in text.split('\n')) {
    if (line.trimLeft().startsWith('```') || line.trimLeft().startsWith('~~~')) {
      fenced = !fenced;
      out.add(line);
      continue;
    }
    out.add(fenced
        ? line
        : line.replaceAllMapped(_codePath, (m) {
            final before = m.start > 0 ? line[m.start - 1] : '';
            final after = m.end < line.length ? line[m.end] : '';
            if (before == '[' || after == ']') return m[0]!;
            return '[`${m[1]}`](${Uri.encodeFull(m[1]!)})';
          }).replaceAllMapped(_token, bare));
  }
  return out.join('\n');
}

/// A link found in plain text: characters [start] to [end] open [target].
typedef TextLink = ({int start, int end, String target});

final _url = RegExp(r'''(?:https?://|www\.)[^\s<>"'`]+''', caseSensitive: false);
final _urlTrail = RegExp(r'''[.,;:!?'"*_]+$''');
final _inlineCode = RegExp(r'`[^`]*`');
final _pathLead = RegExp('^[("\'`]*');
final _pathTrail = RegExp('[.,;:!?)"\'`]*\$');

/// The links in plain (not rendered) text, in order: http(s) and `www.` URLs,
/// Aurelius links and `.md` paths, like rendered messages get them. URLs inside
/// code (fenced or inline) stay plain; `.md` paths in inline code are links, as in messages.
List<TextLink> plainTextLinks(String text) {
  final links = <TextLink>[for (final l in bareAureliusLinks(text)) (start: l.start, end: l.end, target: l.raw)];
  var offset = 0;
  var fenced = false;
  for (final line in text.split('\n')) {
    final trimmed = line.trimLeft();
    if (trimmed.startsWith('```') || trimmed.startsWith('~~~')) {
      fenced = !fenced;
    } else if (!fenced) {
      _lineLinks(line, offset, links);
    }
    offset += line.length + 1;
  }
  links.sort((a, b) => a.start.compareTo(b.start));
  return links;
}

void _lineLinks(String line, int offset, List<TextLink> out) {
  final code = [for (final m in _inlineCode.allMatches(line)) (m.start, m.end)];
  bool inCode(int i) => code.any((c) => i > c.$1 && i < c.$2);
  final urls = <(int, int)>[];
  for (final m in _url.allMatches(line)) {
    if (inCode(m.start)) continue;
    final url = _trimUrl(m[0]!);
    if (!url.contains('.')) continue;
    urls.add((m.start, m.start + url.length));
    out.add((start: offset + m.start, end: offset + m.start + url.length, target: url.contains('://') ? url : 'https://$url'));
  }
  for (final m in _token.allMatches(line)) {
    final token = m[0]!;
    final lead = _pathLead.firstMatch(token)![0]!;
    final rest = token.substring(lead.length);
    final core = rest.substring(0, rest.length - _pathTrail.firstMatch(rest)![0]!.length);
    if (!isMarkdownPath(core) || core.contains('#') || core.contains('?')) continue;
    final start = m.start + lead.length;
    if (urls.any((u) => start < u.$2 && start + core.length > u.$1)) continue;
    out.add((start: offset + start, end: offset + start + core.length, target: core.replaceAll('\\', '/')));
  }
}

/// [url] without trailing punctuation, and without a closing parenthesis it
/// does not open itself (`(see https://x.io)`).
String _trimUrl(String url) {
  var u = url;
  while (true) {
    final before = u;
    u = u.replaceFirst(_urlTrail, '');
    if (u.endsWith(')') && ')'.allMatches(u).length > '('.allMatches(u).length) u = u.substring(0, u.length - 1);
    if (u == before) return u;
  }
}
