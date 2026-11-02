// Links to a ticket or a message (A5a, freeze §2): `aurelius://<host_id>/ticket/<id>`
// and `aurelius://<host_id>/message/<id>`. Parsed strictly, the same on both
// clients; a link holds no token or address and only ever navigates.

/// The server can say where a message is (`message.locate`): only then does this
/// client offer or open links.
const messageLocateFeature = 'message_locate';

/// What any link that does not parse says.
const invalidLinkText = "This isn't a valid Aurelius link.";

/// A parsed link; [hostId] is the server it belongs to.
sealed class AureliusLink {
  const AureliusLink(this.hostId, this.id);

  final String hostId;

  /// The ticket's or message's id.
  final String id;
}

class TicketLink extends AureliusLink {
  const TicketLink(super.hostId, super.id);
}

class MessageLink extends AureliusLink {
  const MessageLink(super.hostId, super.id);
}

const _uuid = '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}';
final _link = RegExp('^aurelius://($_uuid)/(ticket|message)/($_uuid)\$');

/// [raw] as a link: the scheme, a host id, exactly one kind and a valid id, and
/// nothing else (no query, fragment, port, user or extra segment); null otherwise.
AureliusLink? parseAureliusLink(String raw) {
  final match = _link.firstMatch(raw);
  if (match == null) return null;
  return match[2] == 'ticket' ? TicketLink(match[1]!, match[3]!) : MessageLink(match[1]!, match[3]!);
}

String ticketLinkOf(String hostId, String ticketId) => 'aurelius://$hostId/ticket/$ticketId';

String messageLinkOf(String hostId, String messageId) => 'aurelius://$hostId/message/$messageId';

/// Whether a link target claims to be ours: it never goes to the browser, valid or not.
bool isAureliusHref(String href) => href.toLowerCase().startsWith('aurelius:');

final _bareLink = RegExp(r'''aurelius://[^\s<>()\[\]`"']+''');
final _bareTrail = RegExp(r'[.,;:!?*_]+$');
final _inlineCode = RegExp(r'`[^`]*`');

/// The valid links written out in [line] outside inline code, with where they stand.
Iterable<({int start, int end, AureliusLink link, String raw})> _inLine(String line) sync* {
  final code = [for (final m in _inlineCode.allMatches(line)) (m.start, m.end)];
  for (final m in _bareLink.allMatches(line)) {
    if (code.any((c) => m.start > c.$1 && m.start < c.$2)) continue;
    final raw = m[0]!.replaceFirst(_bareTrail, '');
    final link = parseAureliusLink(raw);
    if (link != null) yield (start: m.start, end: m.start + raw.length, link: link, raw: raw);
  }
}

/// [each] for the lines of [text] outside fenced code, with where each line starts.
void _plainLines(String text, void Function(String line, int offset) each) {
  var offset = 0;
  var fenced = false;
  for (final line in text.split('\n')) {
    final trimmed = line.trimLeft();
    if (trimmed.startsWith('```') || trimmed.startsWith('~~~')) {
      fenced = !fenced;
    } else if (!fenced) {
      each(line, offset);
    }
    offset += line.length + 1;
  }
}

/// The valid links written out in [text]; code is never a link.
List<({int start, int end, AureliusLink link, String raw})> bareAureliusLinks(String text) {
  final out = <({int start, int end, AureliusLink link, String raw})>[];
  _plainLines(text, (line, offset) {
    for (final l in _inLine(line)) {
      out.add((start: offset + l.start, end: offset + l.end, link: l.link, raw: l.raw));
    }
  });
  return out;
}

/// Markdown [text] with the links written out in it as `[label](link)`, so the
/// reader sees #42 and not an id (A5a). One already in a Markdown link stays as it is.
String labelAureliusLinks(String text, String Function(AureliusLink) label) {
  final found = bareAureliusLinks(text);
  if (found.isEmpty) return text;
  final out = StringBuffer();
  var at = 0;
  for (final l in found) {
    if (l.start < at) continue;
    out.write(text.substring(at, l.start));
    final inMarkdownLink = l.start >= 2 && text.substring(l.start - 2, l.start) == '](';
    out.write(inMarkdownLink ? l.raw : '[${label(l.link)}](${l.raw})');
    at = l.end;
  }
  out.write(text.substring(at));
  return out.toString();
}
