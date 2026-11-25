import 'package:intl/intl.dart';

import '../core/net/protocol.dart';
import 'views.dart';

// Phase 2 (docs/product/fase-2): a ticket's discussion and a ticket made from
// a chat message. What the screens decide from the wire, kept apart so it is
// tested without widgets.

/// The server has a discussion per ticket (`ticket.post`, `MessageView.system`).
const ticketDiscussionFeature = 'ticket_discussion';

/// Why nobody can write in [ticket]'s discussion any more (B7): Done or
/// archived, as the server refuses a post there; null while it is open.
String? discussionReadOnly(TicketView ticket) {
  if (ticket.column == ColumnKind.done) return 'Done: the discussion can only be read. Follow-up work is a new ticket.';
  if (ticket.archivedAtMs != null) return 'Archived: the discussion can only be read.';
  return null;
}

/// The main task of [ticket] ended (or is a draft): a message to the owner
/// reaches nobody until Start makes a new one (R1 m1, as the server).
bool mainTaskEnded(TicketView ticket) => switch (ticket.rootTaskStatus) {
      TaskStatus.failed || TaskStatus.cancelled || TaskStatus.succeeded || TaskStatus.draft => true,
      _ => false,
    };

/// Who gets a message without `@Name` in [ticket]'s discussion (B3, P-21): the
/// owner while it is in Todo with an owner, Doing, Plan review, Testing or Review, not archived
/// and with a main task that still works; otherwise nobody.
String? discussionAddressee(TicketView ticket, {String? owner}) {
  if (ticket.archivedAtMs != null || mainTaskEnded(ticket)) return null;
  return switch (ticket.column) {
    ColumnKind.todo when ticket.ownerEmployeeId != null => owner ?? ticket.ownerName ?? 'the owner',
    ColumnKind.doing || ColumnKind.planReview || ColumnKind.testing || ColumnKind.review => owner ?? ticket.ownerName ?? 'the owner',
    _ => null,
  };
}

/// The composer's hint in a ticket's discussion (ui-ontwerp §1 "Zonder @").
String discussionHint(TicketView ticket, {String? owner}) {
  final to = discussionAddressee(ticket, owner: owner);
  if (to != null) return 'Message $to (owner) · @ to mention someone else';
  if (ticket.rootTaskStatus == TaskStatus.failed && ticket.column != ColumnKind.done) return 'The main task failed: no one is woken. Start makes a new one · @mention to ask someone.';
  return 'No one is woken. @mention to ask someone.';
}

/// What the user sees when a message to a discussion was not taken. A server
/// without the discussion engine answers `ticket.post` with `unknown_method`.
String discussionPostFailure(Object e) => switch (e) {
      ProtocolException(code: ErrorCode.unknownMethod) => 'This server cannot take messages in a ticket discussion yet. Update or restart the server, then try again.',
      ProtocolException() => e.shown,
      _ => '$e',
    };

/// What the user sees when trying the summary again failed (MU2-12 AC4).
String summaryRetryFailure(Object e) => switch (e) {
      ProtocolException(code: ErrorCode.unknownMethod) => 'This server cannot try the summary again yet. Update or restart the server, then try again.',
      ProtocolException() => e.shown,
      _ => '$e',
    };

/// An `@name` in a message: not inside a word (an email address); group 1 is the name.
final mentionPattern = RegExp(r'(?<![\p{L}\p{N}._])@([\p{L}\p{N}][\p{L}\p{N}._-]*)', unicode: true);

/// The name of an `@` match without a trailing dot or dash (those belong to the sentence).
String trimmedMention(String token) => token.replaceFirst(RegExp(r'[._-]+$'), '');

/// The names written with `@` in [text], in order, each once (a trailing dot
/// or dash belongs to the sentence, not the name).
List<String> mentionedNames(String text) {
  final out = <String>[];
  for (final m in mentionPattern.allMatches(text)) {
    final name = trimmedMention(m.group(1)!);
    if (name.isNotEmpty && !out.any((n) => n.toLowerCase() == name.toLowerCase())) out.add(name);
  }
  return out;
}

/// Whether `@[token]` names [name]: the whole name, or its first word (a name
/// with a space is written `@First`).
bool mentionNames(String token, String name) {
  final t = token.toLowerCase();
  final n = name.trim().toLowerCase();
  return n == t || n.split(RegExp(r'\s+')).first == t;
}

/// The `@names` in [text] that are nobody in [known] (active employees and
/// board members): they wake no one, and the app says so (MU2-04 AC5).
List<String> unknownMentions(String text, Iterable<String> known) => [
      for (final token in mentionedNames(text))
        if (!known.any((k) => mentionNames(token, k))) token,
    ];

/// Whether [text] names the board member [username] with `@` (B9: marked).
bool mentionsName(String text, String? username) {
  if (username == null || username.trim().isEmpty) return false;
  return mentionedNames(text).any((t) => mentionNames(t, username));
}

/// What a screen reader says for a system line (ui-ontwerp §5).
String systemLineLabel(MessageView m) => 'System: ${m.text.trim()}';

/// The ticket a system line names by number, if it does: the server writes
/// "ada created #52 from a message" (`SystemLine::sentence`); a name before it
/// may hold a `#` of its own.
int? ticketNumberIn(String text) {
  final m = RegExp(r'created #(\d+)\b').firstMatch(text) ?? RegExp(r'#(\d+)\b').firstMatch(text);
  return m == null ? null : int.tryParse(m.group(1)!);
}

/// A ticket made from a chat message: its id and, when the line says it, its number.
class TicketRef {
  const TicketRef(this.ticketId, this.number);

  final String ticketId;
  final int? number;

  String get label => number == null ? 'ticket' : '#$number';
}

/// The tickets made from the messages of a chat, by source message: the
/// system lines `ticket_created` name both (MU2-10 AC3, the "→ #52" pill).
Map<String, TicketRef> ticketsFromMessages(List<MessageView> messages) => {
      for (final m in messages)
        if (m.system case SystemLine(kind: SystemLineKind.ticketCreated, :final messageId?, :final ticketId?)) messageId: TicketRef(ticketId, ticketNumberIn(m.text)),
    };

/// Where a chat is, for a ticket made from one of its messages: how the
/// description names it ("Discussion of #42") and the project the ticket
/// goes to (none in an employee's chat: the form asks).
class TicketSource {
  const TicketSource({required this.from, this.projectId});

  final String from;
  final String? projectId;
}

/// What "New ticket" starts with when it is made from a message (MU2-10 AC1, Fase 1 §10.3).
class TicketDraft {
  const TicketDraft({required this.title, required this.description, this.projectId, this.fromMessage, this.labels = '', this.ownerId, this.quick = false});

  final String title;
  final String description;
  final String? projectId;

  /// The message it is made from (`ticket.create.from_message`).
  final String? fromMessage;

  /// Labels, comma separated.
  final String labels;

  /// The owner chosen beforehand (Give work).
  final String? ownerId;

  /// Opens as a quick task (Give work's Quick task, ticket #53); only on a server with `quick_task`.
  final bool quick;
}

/// The longest title a ticket takes (MU1-07).
const _titleMax = 160;

/// The draft for a ticket from [message]: the title is its first line (no
/// markdown heading or quote marks, at most 160 characters), the description
/// quotes it and says where it came from.
TicketDraft draftFromMessage(MessageView message, TicketSource source, {String? author, DateTime? at}) {
  final text = message.text.trim();
  final first = text.split('\n').map((l) => l.trim().replaceFirst(RegExp(r'^(#+|>+|[-*]\s)\s*'), '').trim()).firstWhere((l) => l.isNotEmpty, orElse: () => '');
  final title = first.length > _titleMax ? '${first.substring(0, _titleMax - 1).trimRight()}…' : first;
  final when = DateFormat.MMMd().add_Hm().format(at ?? DateTime.fromMillisecondsSinceEpoch(message.createdAtMs));
  final quote = text.split('\n').map((l) => l.isEmpty ? '>' : '> $l').join('\n');
  final who = author == null || author.trim().isEmpty ? '' : ' · $author';
  return TicketDraft(title: title, description: '$quote\n\n*From: ${source.from}$who · $when*', projectId: source.projectId, fromMessage: message.id);
}

/// Who wrote [m] as a name: the board member of a discussion, the employee
/// of an answer, else null.
String? authorOf(MessageView m) {
  final user = m.authorUser?.username;
  if (user != null && user.trim().isNotEmpty) return user;
  final author = m.author?.trim();
  return author == null || author.isEmpty ? null : author;
}
