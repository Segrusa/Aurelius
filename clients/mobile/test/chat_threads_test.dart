import 'package:aurelius_mobile/data/channels.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:flutter_test/flutter_test.dart';

// AUR-CHAT-002-M1: the pure helpers of threads and reactions in the Team chat
// (ui-ontwerp §5). Neutral names: board members ada and lena, employees Forge, Orion.

const _ada = UserView(id: 'u-1', username: 'ada', createdAtMs: 0);
const _lena = UserView(id: 'u-2', username: 'lena', createdAtMs: 0);

final _now = DateTime(2026, 11, 1, 12);
int _ms(DateTime at) => at.millisecondsSinceEpoch;

MessageView _post(String id, {UserView? by, String? author, int at = 0, String? threadParent, int? replies, int? lastMs, List<String> repliers = const [], SystemLine? system, String? update, MessageStatus status = MessageStatus.complete, List<ReactionView> reactions = const []}) => MessageView(
      activity: const [],
      author: author,
      authorUser: by,
      createdAtMs: at,
      files: const [],
      id: id,
      ordinal: 0,
      queued: false,
      reactions: reactions,
      role: by != null ? MessageRole.user : MessageRole.assistant,
      sources: const [],
      status: status,
      system: system,
      text: 'text',
      threadLastReplyMs: lastMs,
      threadParent: threadParent,
      threadRepliers: repliers,
      threadReplyCount: replies,
      update: by != null || system != null ? null : update,
      uploads: const [],
    );

ChannelView _channel(String name) => ChannelView(archived: false, conversationId: 'c-42', kind: ChannelKind.ticket, mentions: 0, name: name, projectId: 'p1', ticketId: 'k42', ticketNumber: 42, unread: 0);

void main() {
  group('the features (§5)', () {
    test('the picker offers exactly the eight fixed reactions, in order', () {
      expect(reactionChoices, ['✓', '👀', '👍', '🎉', '❓', '❗', '🙏', '🚀']);
    });

    test('the feature names are the ones the server advertises and the hello accepts', () {
      expect(chatThreadsFeature, 'chat_threads');
      expect(chatReactionsFeature, 'chat_reactions');
    });
  });

  group('threads (§5)', () {
    test('a message of the channel can start a thread; a reply, a system line or a streaming answer cannot', () {
      expect(canStartThread(_post('m1', by: _ada)), isTrue);
      expect(canStartThread(_post('m2', author: 'Forge', update: 'group')), isTrue);
      expect(canStartThread(_post('r1', by: _lena, threadParent: 'm1')), isFalse, reason: 'no nested threads');
      expect(canStartThread(_post('s1', system: const SystemLine(kind: SystemLineKind.moved))), isFalse);
      expect(canStartThread(_post('m3', author: 'Forge', update: 'group', status: MessageStatus.partial)), isFalse);
      expect(canStartThread(_post('u1', author: 'Forge', update: 'finding')), isFalse, reason: 'a task update is no channel message');
    });

    test('the line under a root counts the replies and says when the last came: a time today, a date before', () {
      expect(threadLine(_post('m1', by: _ada), _now), isNull, reason: 'no replies, no line');
      expect(threadLine(_post('m1', by: _ada, replies: 0), _now), isNull);
      expect(threadLine(_post('m1', by: _ada, replies: 1, lastMs: _ms(DateTime(2026, 11, 1, 11, 2))), _now), '↳ 1 reply · last 11:02');
      expect(threadLine(_post('m1', by: _ada, replies: 3, lastMs: _ms(DateTime(2026, 11, 1, 9, 5))), _now), '↳ 3 replies · last 09:05');
      expect(threadLine(_post('m1', by: _ada, replies: 3, lastMs: _ms(DateTime(2026, 10, 27, 9))), _now), '↳ 3 replies · last 27 Oct');
      expect(threadLine(_post('m1', by: _ada, replies: 2, lastMs: _ms(DateTime(2025, 10, 27, 9))), _now), '↳ 2 replies · last 27 Oct 2025');
      expect(threadLine(_post('m1', by: _ada, replies: 2), _now), '↳ 2 replies');
    });

    test("the repliers are the root's own, in its order, at most three; a name that is no employee is a board member", () {
      const employees = ['Orion', 'Forge'];
      final root = _post('m1', by: _ada, replies: 5, repliers: const ['ada', 'Orion', 'lena', 'Forge']);
      expect(threadRepliers(root, employees), [('ada', true), ('Orion', false), ('lena', true)]);
      expect(threadRepliers(_post('m2', by: _ada, replies: 1, repliers: const ['Forge']), employees), [('Forge', false)]);
      expect(threadRepliers(_post('m3', by: _ada, replies: 2), employees), isEmpty, reason: 'an older server sends no repliers: no monograms');
      expect(threadRepliers(_post('m4', by: _ada, replies: 1, repliers: const ['Pulse']), const []), [('Pulse', true)], reason: 'a name the company does not know counts as a board member');
    });

    test("a reaction keeps the root's repliers", () {
      final root = _post('m1', by: _ada, replies: 2, repliers: const ['lena', 'Forge']);
      expect(root.withReactions(const [ReactionView(emoji: '✓', count: 1, mine: true, by: ['ada'])]).threadRepliers, ['lena', 'Forge']);
    });

    test('the thread screen is titled after its channel', () {
      expect(threadTitle(_channel('#42-login-per-board-member')), 'Thread · #42-login-per-board-member');
    });
  });

  group('reactions (§5)', () {
    const tick = ReactionView(emoji: '✓', count: 2, mine: false, by: ['lena', 'Forge']);

    test('turning yours on joins an emoji others used, or adds it at the end', () {
      final joined = reactedLocally(const [tick], '✓', on: true, me: 'ada');
      expect([for (final r in joined) (r.emoji, r.count, r.mine, r.by.join(', '))], [('✓', 3, true, 'lena, Forge, ada')]);
      final added = reactedLocally(const [tick], '🚀', on: true, me: 'ada');
      expect([for (final r in added) (r.emoji, r.count, r.mine, r.by.join(', '))], [('✓', 2, false, 'lena, Forge'), ('🚀', 1, true, 'ada')]);
    });

    test('turning yours off leaves the others, and an emoji nobody has any more goes', () {
      final mine = const ReactionView(emoji: '✓', count: 3, mine: true, by: ['lena', 'Forge', 'ada']);
      final off = reactedLocally([mine], '✓', on: false, me: 'ada');
      expect([for (final r in off) (r.emoji, r.count, r.mine, r.by.join(', '))], [('✓', 2, false, 'lena, Forge')]);
      final only = const ReactionView(emoji: '👀', count: 1, mine: true, by: ['ada']);
      expect(reactedLocally([only], '👀', on: false, me: 'ada'), isEmpty);
    });

    test('asking for what already is changes nothing (the write is idempotent too)', () {
      expect(identical(reactedLocally(const [tick], '✓', on: false, me: 'ada').single, tick), isTrue);
      expect(reactedLocally(const [tick], '🎉', on: false, me: 'ada'), hasLength(1));
    });

    test('a long press says who reacted; more than the names tell is "+n"', () {
      expect(reactionWho(tick), '✓ · lena, Forge');
      expect(reactionWho(const ReactionView(emoji: '👀', count: 3, mine: false, by: ['lena'])), '👀 · lena +2');
      expect(reactionWho(const ReactionView(emoji: '🙏', count: 1, mine: false)), '🙏 · 1 reaction');
      expect(reactionSemantics(const ReactionView(emoji: '✓', count: 2, mine: true)), '✓, 2 reactions, including yours');
    });

    test('withReactions keeps every other field of the message', () {
      final m = _post('m1', by: _ada, replies: 3, lastMs: 5);
      final next = m.withReactions(const [tick]);
      expect(next.reactions.single.emoji, '✓');
      expect((next.authorUser?.username, next.threadReplyCount, next.threadLastReplyMs, next.id, next.role), ('ada', 3, 5, 'm1', MessageRole.user));
    });
  });
}
