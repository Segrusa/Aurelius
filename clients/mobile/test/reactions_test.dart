import 'package:aurelius_mobile/data/channels.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:flutter_test/flutter_test.dart';

// AUR-CHAT-002-M1: the pure helpers of reactions
// (ui-ontwerp §5). Neutral names: board members ada and lena, employees Forge, Orion.

const _ada = UserView(id: 'u-1', username: 'ada', createdAtMs: 0);

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
 

void main() {
  group('the reaction picker (§5)', () {
    test('the picker offers exactly the eight fixed reactions, in order', () {
      expect(reactionChoices, ['✓', '👀', '👍', '🎉', '❓', '❗', '🙏', '🚀']);
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
