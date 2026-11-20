import 'discussion.dart';
import 'views.dart';

/// The server lists a channel per ticket (`channel.list` with `ticket_channels`
/// in `HelloParams.accepts`, `channel.search`, the code `channel_archived`).
const ticketChannelsFeature = 'ticket_channels';

/// The fixed reactions of the picker (§5): no emoji library, exactly these.
const reactionChoices = ['✓', '👀', '👍', '🎉', '❓', '❗', '🙏', '🚀'];

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

/// [reactions] after the reader's own [emoji] went [on] or off (§5), as the
/// PC will have them: shown at once, the next read confirms it. [me] joins or
/// leaves `by`; an emoji nobody has any more goes.
List<ReactionView> reactedLocally(List<ReactionView> reactions, String emoji, {required bool on, String? me}) {
  final at = reactions.indexWhere((r) => r.emoji == emoji);
  if (at < 0) {
    if (!on) return reactions;
    return [...reactions, ReactionView(emoji: emoji, count: 1, mine: true, by: [?me])];
  }
  final r = reactions[at];
  if (r.mine == on) return reactions;
  final count = r.count + (on ? 1 : -1);
  final by = on ? [...r.by, ?me] : [for (final b in r.by) if (b != me) b];
  final out = [...reactions];
  if (count <= 0) {
    out.removeAt(at);
  } else {
    out[at] = ReactionView(emoji: emoji, count: count, mine: on, by: by);
  }
  return out;
}

/// Who reacted with [r] (§5, a long press on its pill): "✓ · Alexandra, Forge".
String reactionWho(ReactionView r) {
  final names = r.by.where((b) => b.trim().isNotEmpty).toList();
  if (names.isEmpty) return '${r.emoji} · ${r.count == 1 ? '1 reaction' : '${r.count} reactions'}';
  final rest = r.count - names.length;
  return '${r.emoji} · ${names.join(', ')}${rest > 0 ? ' +$rest' : ''}';
}

/// A reaction pill's label for screen readers: "✓, 2 reactions, including yours".
String reactionSemantics(ReactionView r) => '${r.emoji}, ${r.count == 1 ? '1 reaction' : '${r.count} reactions'}${r.mine ? ', including yours' : ''}';
