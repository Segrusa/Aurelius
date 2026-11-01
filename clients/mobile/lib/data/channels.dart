import 'package:flutter/material.dart' show DateUtils;
import 'package:intl/intl.dart';

import 'discussion.dart';
import 'views.dart';

// AUR-CHAT-001 (docs/product/aur-chat-001/ui-ontwerp.md): the Slack-style Team
// chat with a channel per ticket. What the screens decide from the wire, kept
// apart so it is tested without widgets.

/// The server lists a channel per ticket (`channel.list` with `ticket_channels`
/// in `HelloParams.accepts`, `channel.search`, the code `channel_archived`).
const ticketChannelsFeature = 'ticket_channels';

/// How many rows Tickets and Archived show before "Show more" (§8 O-5).
const channelSectionMax = 15;

/// Messages of one author closer together than this share one header (§2).
const messageGroupWindowMs = 5 * 60 * 1000;

/// From this width the channel list and the chat stand side by side (§6).
const channelSplitWidth = 700.0;

/// The channel list's width beside the chat (§6).
const channelListWidth = 300.0;

/// A ticket's channel; an older server leaves `kind` out, but only a ticket's has a ticket.
bool isTicketChannel(ChannelView c) => c.kind == ChannelKind.ticket || c.ticketId != null;

/// The channel list's sections (§1, §6): Channels = General, then the active
/// projects A–Z; Tickets = the open ticket channels, latest activity first
/// (same time: the higher number first); Archived = the archived projects
/// A–Z, then the closed tickets, latest activity first.
({List<ChannelView> channels, List<ChannelView> tickets, List<ChannelView> archived}) channelSections(List<ChannelView> all) {
  int byName(ChannelView a, ChannelView b) => a.name.toLowerCase().compareTo(b.name.toLowerCase());
  int byActivity(ChannelView a, ChannelView b) {
    final at = (b.lastActivityMs ?? 0).compareTo(a.lastActivityMs ?? 0);
    return at != 0 ? at : (b.ticketNumber ?? 0).compareTo(a.ticketNumber ?? 0);
  }

  bool project(ChannelView c) => c.projectId != null && !isTicketChannel(c);
  final general = [for (final c in all) if (c.projectId == null && !isTicketChannel(c)) c];
  final projects = [for (final c in all) if (project(c) && !c.archived) c]..sort(byName);
  final tickets = [for (final c in all) if (isTicketChannel(c) && !c.archived) c]..sort(byActivity);
  final archivedProjects = [for (final c in all) if (project(c) && c.archived) c]..sort(byName);
  final archivedTickets = [for (final c in all) if (isTicketChannel(c) && c.archived) c]..sort(byActivity);
  return (channels: [...general, ...projects], tickets: tickets, archived: [...archivedProjects, ...archivedTickets]);
}

/// The rows of a section that show: all of them when [all], else the first
/// [channelSectionMax]; `hidden` is how many "Show more" brings.
({List<ChannelView> shown, int hidden}) sectionRows(List<ChannelView> rows, {required bool all}) {
  if (all || rows.length <= channelSectionMax) return (shown: rows, hidden: 0);
  return (shown: rows.take(channelSectionMax).toList(), hidden: rows.length - channelSectionMax);
}

/// The ticket's status as its column's default name ("Review").
String columnName(ColumnKind column) => switch (column) {
      ColumnKind.backlog => 'Backlog',
      ColumnKind.todo => 'Todo',
      ColumnKind.doing => 'Doing',
      ColumnKind.planReview => 'Plan review',
      ColumnKind.testing => 'Testing',
      ColumnKind.review => 'Review',
      ColumnKind.done => 'Done',
    };

/// The read-only bar of an archived channel (§3): why nobody can write here.
String channelReadOnlyNote(ChannelView c) {
  if (!isTicketChannel(c)) return 'This project is archived · read-only.';
  final number = c.ticketNumber == null ? 'This ticket' : '#${c.ticketNumber}';
  return c.ticketColumn == ColumnKind.done ? '$number is Done · read-only.' : '$number is archived · read-only.';
}

/// The composer's hint in a channel (§4): "Message #42-login-per… · @ to mention".
String channelHint(ChannelView c) => 'Message ${channelLabel(c)} · @ to mention';

/// The day separator's label (§2): "Today", "Yesterday", else "Mon 27 Oct",
/// with the year when it is not this year.
String dayLabel(DateTime day, DateTime now) {
  final d = DateUtils.dateOnly(day);
  final today = DateUtils.dateOnly(now);
  final days = today.difference(d).inDays;
  if (days == 0) return 'Today';
  if (days == 1) return 'Yesterday';
  return DateFormat(d.year == today.year ? 'EEE d MMM' : 'EEE d MMM y').format(d);
}

/// One row of a channel's message list (§2).
sealed class ChannelRow {
  const ChannelRow();
}

/// The line between two days.
class DayRow extends ChannelRow {
  const DayRow(this.label, this.day);

  final String label;

  /// The day, for the row's key.
  final DateTime day;
}

/// "New messages": what came after the reader's last read.
class NewMessagesRow extends ChannelRow {
  const NewMessagesRow();
}

/// A message; [grouped] goes under the previous one without its own header.
class MessageRow extends ChannelRow {
  const MessageRow(this.message, {this.grouped = false});

  final MessageView message;
  final bool grouped;
}

/// Who wrote [m], as grouping compares it: a board member or an employee,
/// never one for the other; null (a system line, no author) never groups.
String? _authorKey(MessageView m) {
  if (m.system != null) return null;
  if (m.isUser) {
    final user = m.authorUser;
    return user == null ? 'me' : 'user:${user.id}';
  }
  final author = m.author?.trim();
  return author == null || author.isEmpty ? null : 'employee:$author';
}

/// Where "New messages" goes (§2): before the first message after
/// [readThrough] that someone else wrote; null when there is none or the
/// channel was never read.
int? newMessagesAt(List<MessageView> messages, int? readThrough, {String? me}) {
  if (readThrough == null) return null;
  for (final (i, m) in messages.indexed) {
    if (m.ordinal <= readThrough) continue;
    final mine = m.isUser && (m.authorUser == null || (me != null && m.authorUser!.username == me));
    if (!mine) return i;
  }
  return null;
}

/// The rows of a channel (§2): a [DayRow] before each new day, the
/// [NewMessagesRow] after [readThrough], and each message grouped with the
/// one before when it has the same author, came within [messageGroupWindowMs]
/// and nothing lies between them (no day, New line or system line).
List<ChannelRow> channelRows(List<MessageView> messages, {int? readThrough, String? me, required DateTime now}) {
  final rows = <ChannelRow>[];
  final newAt = newMessagesAt(messages, readThrough, me: me);
  DateTime? day;
  MessageView? previous;
  for (final (i, m) in messages.indexed) {
    final at = DateTime.fromMillisecondsSinceEpoch(m.createdAtMs);
    final today = DateUtils.dateOnly(at);
    var broken = false;
    if (day != today) {
      rows.add(DayRow(dayLabel(at, now), today));
      day = today;
      broken = true;
    }
    if (i == newAt) {
      rows.add(const NewMessagesRow());
      broken = true;
    }
    final key = _authorKey(m);
    final gap = previous == null ? null : m.createdAtMs - previous.createdAtMs;
    final grouped = !broken && key != null && previous != null && _authorKey(previous) == key && gap! >= 0 && gap < messageGroupWindowMs;
    rows.add(MessageRow(m, grouped: grouped));
    previous = m;
  }
  return rows;
}

/// Variation selector 15: the glyph before it is drawn as text, never as a
/// colour emoji (↩ ▶ ⚠ ⏸ have both forms).
const textPresentation = '\uFE0E';

/// The shape before a system line's text (§2), so colour is never the only
/// signal, and whether it tells of a failure (NEGATIVE). By `action` when the
/// server sends one, else by `kind` (§8 O-4).
({String shape, bool negative}) systemLineShape(SystemLine line, String text) {
  switch (line.action) {
    case 'opened' || 'joined':
      return (shape: '+', negative: false);
    case 'closed':
      return (shape: '■', negative: false);
    case 'reopened':
      return (shape: '↺', negative: false);
    case 'approved' || 'tested_passed':
      return (shape: '✓', negative: false);
    case 'adjusted' || 'sent_back':
      return (shape: '↩$textPresentation', negative: false);
    case 'tested_failed':
      return (shape: '✕', negative: true);
  }
  return switch (line.kind) {
    SystemLineKind.moved || SystemLineKind.done || SystemLineKind.delegated || SystemLineKind.createdFrom || SystemLineKind.ticketCreated => (shape: '→', negative: false),
    // A run result: the text says reported or failed (§8 O-4).
    SystemLineKind.reported => _failed(text) ? (shape: '✕', negative: true) : (shape: '▶$textPresentation', negative: false),
    SystemLineKind.plan || SystemLineKind.review => (shape: '◆', negative: false),
    SystemLineKind.summary => (shape: '✓', negative: false),
    SystemLineKind.summaryFailed => (shape: '⚠$textPresentation', negative: true),
    SystemLineKind.limit => (shape: '⏸$textPresentation', negative: false),
  };
}

bool _failed(String text) => RegExp(r'\bfailed\b', caseSensitive: false).hasMatch(text);

/// A system line as a channel shows it (§2): "→ Forge moved #42 Doing → Review · 10:41".
String compactSystemLine(MessageView m) {
  final text = m.text.trim().isEmpty ? '…' : m.text.trim();
  final shape = m.system == null ? null : systemLineShape(m.system!, text).shape;
  final time = DateFormat.Hm().format(DateTime.fromMillisecondsSinceEpoch(m.createdAtMs));
  return '${shape == null ? '' : '$shape '}$text · $time';
}

/// How an `@name` in a message is marked (§2).
enum MentionMark {
  /// Someone else: ACCENT_TEXT.
  other,

  /// The signed-in board member: stronger (ACCENT_HALO pill, SEMIBOLD).
  me,
}

/// How `@[token]` is marked: [MentionMark.me] for the signed-in board member
/// [me], [MentionMark.other] for another name of [known], null for nobody.
MentionMark? mentionMarkOf(String token, {required Iterable<String> known, String? me}) {
  final name = trimmedMention(token);
  if (name.isEmpty) return null;
  if (me != null && me.trim().isNotEmpty && mentionNames(name, me)) return MentionMark.me;
  return known.any((k) => mentionNames(name, k)) ? MentionMark.other : null;
}

/// [text] cut into its plain parts and its `@names` (§2): a name of [known]
/// is marked, [me] the strongest; an `@` that names nobody stays plain.
List<(String, MentionMark?)> mentionSegments(String text, {required Iterable<String> known, String? me}) {
  final out = <(String, MentionMark?)>[];
  var at = 0;
  for (final m in mentionPattern.allMatches(text)) {
    final mark = mentionMarkOf(m.group(1)!, known: known, me: me);
    if (mark == null) continue;
    final end = m.start + 1 + trimmedMention(m.group(1)!).length;
    if (m.start > at) out.add((text.substring(at, m.start), null));
    out.add((text.substring(m.start, end), mark));
    at = end;
  }
  if (at < text.length) out.add((text.substring(at), null));
  return out;
}

/// Who works on a ticket, as its channel's header names them (§3): the owner
/// first, then every other employee of its tasks, each once.
List<String> ticketAssignees(TicketDetail detail) {
  final out = <String>[];
  void add(String? name) {
    final n = name?.trim();
    if (n != null && n.isNotEmpty && !out.contains(n)) out.add(n);
  }

  add(detail.ticket.ownerName);
  add(detail.rootTask?.employeeName);
  for (final t in detail.tasks) {
    add(t.employeeName);
  }
  return out;
}

/// The assignees as the header's line says them (§3): at most two names,
/// then "+n" for the rest.
String assigneesLine(List<String> names) => names.length <= 2 ? names.join(', ') : '${names.take(2).join(', ')} +${names.length - 2}';
