import 'package:aurelius_mobile/data/channels.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('@mentions', () {
    const known = ['Forge', 'Orion', 'ada', 'lena', 'Alexandra Smith'];

    test('the signed-in member is marked stronger than someone else', () {
      expect(mentionMarkOf('ada', known: known, me: 'ada'), MentionMark.me);
      expect(mentionMarkOf('Forge', known: known, me: 'ada'), MentionMark.other);
    });

    test('an @ that names nobody is not marked', () {
      expect(mentionMarkOf('nobody', known: known, me: 'ada'), isNull);
    });

    test('a name with a space is mentioned by its first word, case not counting', () {
      expect(mentionMarkOf('alexandra', known: known), MentionMark.other);
      expect(mentionMarkOf('ADA', known: known, me: 'ada'), MentionMark.me);
      expect(mentionMarkOf('Forge.', known: known), MentionMark.other, reason: 'a trailing dot belongs to the sentence');
    });
  });
}
