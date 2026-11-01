import 'package:aurelius_mobile/data/channels.dart';
import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:flutter_test/flutter_test.dart';

// AUR-CHAT-001-M1: the pure helpers of the Slack-style Team chat (ui-ontwerp
// §1–§2, §6). Neutral names: board members ada and lena, employees Forge, Orion.

const _ada = UserView(id: 'u-1', username: 'ada', createdAtMs: 0);
const _lena = UserView(id: 'u-2', username: 'lena', createdAtMs: 0);

ChannelView _channel(String id, String name, {String? projectId, ChannelKind? kind, int? number, int? at, bool archived = false, ColumnKind? column}) => ChannelView(
      archived: archived,
      conversationId: id,
      kind: kind,
      lastActivityMs: at,
      mentions: 0,
      name: name,
      projectId: projectId,
      ticketColumn: column,
      ticketId: number == null ? null : 'k$number',
      ticketNumber: number,
      unread: 0,
    );

ChannelView _ticket(int number, {int? at, bool archived = false, ColumnKind column = ColumnKind.doing}) =>
    _channel('c-$number', '#$number-t', projectId: 'p1', kind: ChannelKind.ticket, number: number, at: at, archived: archived, column: column);

final _base = DateTime(2026, 11, 1, 10);
int _ms(Duration after) => _base.add(after).millisecondsSinceEpoch;

MessageView _post(String id, {UserView? by, String? author, required int at, int ordinal = 0, SystemLine? system, String? text}) => MessageView(
      activity: const [],
      author: author,
      authorUser: by,
      createdAtMs: at,
      files: const [],
      id: id,
      ordinal: ordinal,
      queued: false,
      role: by != null ? MessageRole.user : MessageRole.assistant,
      sources: const [],
      status: MessageStatus.complete,
      system: system,
      text: text ?? id,
      update: by != null || system != null ? null : 'group',
      uploads: const [],
    );

List<String> _shape(List<ChannelRow> rows) => [
      for (final r in rows)
        switch (r) {
          DayRow(:final label) => 'day $label',
          NewMessagesRow() => 'new',
          MessageRow(:final message, :final grouped) => grouped ? '  ${message.id}' : message.id,
        },
    ];

void main() {
  group('channel sections (§1, §6)', () {
    test('general comes first, then the active projects by name, then the open tickets by latest activity', () {
      final s = channelSections([
        _ticket(41, at: 100),
        _channel('c-web', 'Web', projectId: 'p2', kind: ChannelKind.project),
        _channel('c-g', 'General', kind: ChannelKind.general),
        _channel('c-atlas', 'atlas', projectId: 'p1', kind: ChannelKind.project),
        _ticket(52, at: 300),
        _ticket(47, at: 100),
      ]);
      expect([for (final c in s.channels) c.conversationId], ['c-g', 'c-atlas', 'c-web']);
      expect([for (final c in s.tickets) c.ticketNumber], [52, 47, 41], reason: 'the same time: the higher number first');
      expect(s.archived, isEmpty);
    });

    test('archived holds the archived projects by name, then the closed tickets by latest activity', () {
      final s = channelSections([
        _ticket(3, at: 10, archived: true, column: ColumnKind.done),
        _channel('c-zz', 'Zulu', projectId: 'p9', archived: true),
        _ticket(9, at: 90, archived: true, column: ColumnKind.done),
        _channel('c-aa', 'alpha', projectId: 'p8', archived: true),
      ]);
      expect([for (final c in s.archived) c.conversationId], ['c-aa', 'c-zz', 'c-9', 'c-3']);
      expect(s.tickets, isEmpty);
    });

    test('a ticket channel from a server without kind is still a ticket by its ticket id', () {
      final s = channelSections([_channel('c-7', '#7-x', projectId: 'p1', number: 7)]);
      expect(s.tickets.single.ticketNumber, 7);
      expect(s.channels, isEmpty);
    });

    test('a section of more than fifteen rows shows fifteen and says how many more there are', () {
      final rows = [for (var i = 0; i < channelSectionMax + 4; i++) _ticket(i)];
      final cut = sectionRows(rows, all: false);
      expect(cut.shown, hasLength(channelSectionMax));
      expect(cut.hidden, 4);
      expect(sectionRows(rows, all: true).shown, hasLength(channelSectionMax + 4));
      expect(sectionRows(rows.take(channelSectionMax).toList(), all: false).hidden, 0);
    });

    test("a ticket's channel is labelled with the server's name and read out with its number", () {
      final c = _ticket(42);
      expect(channelLabel(c), '#42-t');
      expect(channelLabel(_channel('c', '42-login', projectId: 'p', kind: ChannelKind.ticket, number: 42)), '#42-login');
      expect(channelSemantics(c), 'Ticket 42, 42-t');
    });

    test('the read-only bar says whether the ticket is done or archived, or the project archived', () {
      expect(channelReadOnlyNote(_ticket(42, archived: true, column: ColumnKind.done)), '#42 is Done · read-only.');
      expect(channelReadOnlyNote(_ticket(42, archived: true, column: ColumnKind.review)), '#42 is archived · read-only.');
      expect(channelReadOnlyNote(_channel('c', 'Old', projectId: 'p', archived: true)), 'This project is archived · read-only.');
    });
  });

  group('day separators (§2)', () {
    final now = DateTime(2026, 11, 1, 18);

    test('today and yesterday are named, an earlier day by weekday and date', () {
      expect(dayLabel(DateTime(2026, 11, 1, 0, 5), now), 'Today');
      expect(dayLabel(DateTime(2026, 10, 31, 23, 59), now), 'Yesterday');
      expect(dayLabel(DateTime(2026, 10, 27, 9), now), 'Tue 27 Oct');
    });

    test('a day of another year carries its year', () {
      expect(dayLabel(DateTime(2025, 12, 30), now), 'Tue 30 Dec 2025');
    });
  });

  group('rows: grouping, days and New messages (§2)', () {
    test('messages of one author within five minutes are grouped under one header', () {
      final rows = channelRows([
        _post('a1', by: _ada, at: _ms(Duration.zero)),
        _post('a2', by: _ada, at: _ms(const Duration(minutes: 4, seconds: 59))),
        _post('a3', by: _ada, at: _ms(const Duration(minutes: 10))),
      ], now: _base);
      expect(_shape(rows), ['day Today', 'a1', '  a2', 'a3'], reason: 'a3 is more than five minutes after a2');
    });

    test('exactly five minutes apart is no longer grouped', () {
      final rows = channelRows([
        _post('a1', by: _ada, at: _ms(Duration.zero)),
        _post('a2', by: _ada, at: _ms(const Duration(milliseconds: messageGroupWindowMs))),
      ], now: _base);
      expect(_shape(rows), ['day Today', 'a1', 'a2']);
    });

    test('another author, a board member and an employee of the same name, or a system line between break a group', () {
      final rows = channelRows([
        _post('a1', by: _ada, at: _ms(Duration.zero)),
        _post('l1', by: _lena, at: _ms(const Duration(minutes: 1))),
        _post('f1', author: 'Forge', at: _ms(const Duration(minutes: 2))),
        _post('f2', author: 'Forge', at: _ms(const Duration(minutes: 2))),
        _post('s1', author: 'Aurelius', at: _ms(const Duration(minutes: 3)), system: const SystemLine(kind: SystemLineKind.moved)),
        _post('f3', author: 'Forge', at: _ms(const Duration(minutes: 3))),
        _post('ada', author: 'ada', at: _ms(const Duration(minutes: 3))),
      ], now: _base);
      expect(_shape(rows), ['day Today', 'a1', 'l1', 'f1', '  f2', 's1', 'f3', 'ada']);
    });

    test('a new day starts with its separator and a new header', () {
      final rows = channelRows([
        _post('a1', by: _ada, at: DateTime(2026, 10, 31, 23, 58).millisecondsSinceEpoch),
        _post('a2', by: _ada, at: DateTime(2026, 11, 1, 0, 1).millisecondsSinceEpoch),
      ], now: _base);
      expect(_shape(rows), ['day Yesterday', 'a1', 'day Today', 'a2']);
    });

    test('New messages goes before the first message after read_through and breaks the group', () {
      final rows = channelRows([
        _post('f1', author: 'Forge', at: _ms(Duration.zero), ordinal: 1),
        _post('f2', author: 'Forge', at: _ms(const Duration(minutes: 1)), ordinal: 2),
        _post('f3', author: 'Forge', at: _ms(const Duration(minutes: 2)), ordinal: 3),
      ], readThrough: 2, now: _base);
      expect(_shape(rows), ['day Today', 'f1', '  f2', 'new', 'f3']);
    });

    test('no New messages line when the channel was never read or nothing came after', () {
      final messages = [_post('f1', author: 'Forge', at: _ms(Duration.zero), ordinal: 1)];
      expect(newMessagesAt(messages, null), isNull);
      expect(newMessagesAt(messages, 1), isNull);
      expect(newMessagesAt(messages, 0), 0);
    });

    test("the reader's own messages after read_through are not new to them", () {
      final messages = [
        _post('a1', by: _ada, at: _ms(Duration.zero), ordinal: 3),
        _post('f1', author: 'Forge', at: _ms(Duration.zero), ordinal: 4),
      ];
      expect(newMessagesAt(messages, 2, me: 'ada'), 1);
      expect(newMessagesAt(messages, 2, me: 'lena'), 0);
    });
  });

  group('system lines by action, else by kind (§2, §8 O-4)', () {
    ({String shape, bool negative}) shape(SystemLineKind kind, {String? action, String text = ''}) => systemLineShape(SystemLine(kind: kind, action: action), text);

    test('an action decides the shape before the kind', () {
      expect(shape(SystemLineKind.done, action: 'closed').shape, '■');
      expect(shape(SystemLineKind.ticketCreated, action: 'opened').shape, '+');
      expect(shape(SystemLineKind.moved, action: 'reopened').shape, '↺');
      expect(shape(SystemLineKind.plan, action: 'approved').shape, '✓');
      expect(shape(SystemLineKind.plan, action: 'adjusted').shape, '↩');
      expect(shape(SystemLineKind.review, action: 'sent_back').shape, '↩');
      expect(shape(SystemLineKind.moved, action: 'tested_passed'), (shape: '✓', negative: false));
      expect(shape(SystemLineKind.moved, action: 'tested_failed'), (shape: '✕', negative: true));
    });

    test('without an action, or with one this app does not know, the kind decides', () {
      expect(shape(SystemLineKind.moved).shape, '→');
      expect(shape(SystemLineKind.done).shape, '→');
      expect(shape(SystemLineKind.moved, action: 'something_new').shape, '→');
      expect(shape(SystemLineKind.summaryFailed), (shape: '⚠', negative: true));
    });

    test('a reported run result is a run line, and a failed one is negative', () {
      expect(shape(SystemLineKind.reported, text: 'Forge reported on task 12'), (shape: '▶', negative: false));
      expect(shape(SystemLineKind.reported, text: 'Forge failed task 12'), (shape: '✕', negative: true));
    });

    test('a compact line is the shape, the text and the time', () {
      final at = DateTime(2026, 11, 1, 10, 41).millisecondsSinceEpoch;
      final line = _post('s', author: 'Aurelius', at: at, system: const SystemLine(kind: SystemLineKind.moved), text: 'Forge moved #42 Doing → Review');
      expect(compactSystemLine(line), '→ Forge moved #42 Doing → Review · 10:41');
    });
  });

  group('@mentions (§2)', () {
    const known = ['Forge', 'Orion', 'ada', 'lena', 'Alexandra Smith'];

    test('the signed-in member is marked stronger than someone else', () {
      expect(mentionSegments('Done. @ada the cost is in config, @Forge.', known: known, me: 'ada'), [
        ('Done. ', null),
        ('@ada', MentionMark.me),
        (' the cost is in config, ', null),
        ('@Forge', MentionMark.other),
        ('.', null),
      ]);
    });

    test('an @ that names nobody, or sits inside an email address, stays plain', () {
      expect(mentionSegments('mail ada@example.com or @nobody', known: known, me: 'ada'), [('mail ada@example.com or @nobody', null)]);
    });

    test('a name with a space is mentioned by its first word, case not counting', () {
      expect(mentionMarkOf('alexandra', known: known), MentionMark.other);
      expect(mentionMarkOf('ADA', known: known, me: 'ada'), MentionMark.me);
      expect(mentionMarkOf('Forge.', known: known), MentionMark.other, reason: 'a trailing dot belongs to the sentence');
    });
  });
}
